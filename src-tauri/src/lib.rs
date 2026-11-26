use serde::{Deserialize, Serialize};
#[cfg(windows)]
use std::os::windows::{ffi::OsStringExt, process::CommandExt};
use std::{
    collections::{HashMap, HashSet},
    env,
    ffi::OsString,
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::Mutex,
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem, Submenu},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Manager, WindowEvent,
};
#[cfg(windows)]
use windows_sys::Win32::{
    Foundation::{CloseHandle, GlobalFree},
    System::{
        DataExchange::{CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData},
        Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE},
        Ole::CF_UNICODETEXT,
        Threading::{OpenProcess, QueryFullProcessImageNameW, PROCESS_QUERY_LIMITED_INFORMATION},
    },
};

type AppResult<T> = Result<T, String>;

const TRAY_ID: &str = "devtool-main-tray";
const NGINX_START_ATTEMPTS: usize = 60;
const NGINX_START_POLL_INTERVAL: Duration = Duration::from_millis(50);
#[cfg(windows)]
const CLIPBOARD_OPEN_ATTEMPTS: usize = 5;
#[cfg(windows)]
const CLIPBOARD_RETRY_INTERVAL: Duration = Duration::from_millis(10);
static NGINX_START_LOCK: Mutex<()> = Mutex::new(());
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AppSettings {
    start_with_windows: bool,
    start_minimized: bool,
    minimize_to_tray: bool,
    close_to_tray: bool,
    android_sdk_path: Option<String>,
    nginx_path: Option<String>,
    custom_apps: Vec<CustomApp>,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            start_with_windows: false,
            start_minimized: false,
            minimize_to_tray: true,
            close_to_tray: true,
            android_sdk_path: None,
            nginx_path: find_default_nginx().map(path_to_string),
            custom_apps: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CustomApp {
    id: String,
    name: String,
    path: String,
    args: Option<String>,
    cwd: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CustomAppInput {
    id: Option<String>,
    name: String,
    path: String,
    args: Option<String>,
    cwd: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct AvdInfo {
    name: String,
    status: String,
    serial: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct SystemImageOption {
    package_id: String,
    api: String,
    tag: String,
    abi: String,
    rooted: bool,
    installed: bool,
    label: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct AvdToolingStatus {
    can_create: bool,
    can_delete: bool,
    mode: String,
    missing_reason: Option<String>,
    images: Vec<SystemImageOption>,
    profiles: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateAvdInput {
    name: Option<String>,
    package_id: Option<String>,
    profile: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct NginxConfig {
    path: String,
    root: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
enum NginxStartOutcome {
    Started,
    AlreadyRunning,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct PortProcess {
    protocol: String,
    local_address: String,
    pid: u32,
    process_name: String,
}

#[tauri::command]
fn get_app_settings() -> AppResult<AppSettings> {
    read_settings()
}

#[tauri::command]
fn save_app_settings(mut settings: AppSettings) -> AppResult<AppSettings> {
    if let Ok(existing) = read_settings() {
        settings.custom_apps = existing.custom_apps;
    }
    write_settings(&settings)?;
    Ok(settings)
}

#[tauri::command]
fn set_start_with_windows(enabled: bool) -> AppResult<()> {
    let exe = env::current_exe().map_err(to_message)?;
    if enabled {
        run_command(
            "reg",
            &[
                "add",
                r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run",
                "/v",
                "devtool",
                "/t",
                "REG_SZ",
                "/d",
                &path_to_string(exe),
                "/f",
            ],
            None,
        )?;
    } else {
        let _ = run_command(
            "reg",
            &[
                "delete",
                r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run",
                "/v",
                "devtool",
                "/f",
            ],
            None,
        );
    }
    Ok(())
}

#[tauri::command]
fn get_start_with_windows_status() -> bool {
    run_command(
        "reg",
        &[
            "query",
            r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run",
            "/v",
            "devtool",
        ],
        None,
    )
    .is_ok()
}

#[tauri::command]
fn show_main_window(app: tauri::AppHandle) -> AppResult<()> {
    restore_main_window(&app)
}

fn restore_main_window(app: &tauri::AppHandle) -> AppResult<()> {
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| "main window not found".to_string())?;
    window.unminimize().map_err(to_message)?;
    window.show().map_err(to_message)?;
    window.set_focus().map_err(to_message)?;
    Ok(())
}

#[tauri::command]
fn hide_main_window(app: tauri::AppHandle) -> AppResult<()> {
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| "main window not found".to_string())?;
    window.hide().map_err(to_message)
}

#[tauri::command]
fn set_content_height(window: tauri::WebviewWindow, height: f64) -> AppResult<()> {
    if !height.is_finite() || height <= 0.0 || window.is_minimized().unwrap_or(false) {
        return Ok(());
    }

    let scale = window.scale_factor().map_err(to_message)?;
    let inner = window
        .inner_size()
        .map_err(to_message)?
        .to_logical::<f64>(scale);
    if inner.width <= 0.0 {
        return Ok(());
    }

    let max_height = window
        .current_monitor()
        .ok()
        .flatten()
        .map(|monitor| {
            let work = monitor
                .work_area()
                .size
                .to_logical::<f64>(monitor.scale_factor());
            (work.height - 64.0).max(260.0)
        })
        .unwrap_or(820.0);

    let target_height = height.ceil().clamp(220.0, max_height);
    if (inner.height - target_height).abs() < 1.0 {
        return Ok(());
    }

    window
        .set_size(tauri::LogicalSize::new(inner.width, target_height))
        .map_err(to_message)
}

#[tauri::command]
fn quit_app(app: tauri::AppHandle) {
    app.exit(0);
}

#[tauri::command]
fn refresh_tray(app: tauri::AppHandle) -> AppResult<()> {
    refresh_tray_menu(&app)
}

#[tauri::command]
fn scan_avds() -> AppResult<Vec<AvdInfo>> {
    let settings = read_settings()?;
    let sdk = detect_android_sdk(&settings)?;
    let emulator = sdk.join("emulator").join("emulator.exe");

    let output = if emulator.exists() {
        run_command_path(&emulator, &["-list-avds"], None)?
    } else if find_cmdline_tools_bin(&sdk).is_some() {
        return Ok(Vec::new());
    } else {
        return Err(format!(
            "emulator.exe not found under {}",
            path_to_string(sdk)
        ));
    };

    let running = running_avds(&sdk);
    let avds = output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(|name| {
            let serial = running.get(name).cloned();
            AvdInfo {
                name: name.to_string(),
                status: if serial.is_some() {
                    "running".to_string()
                } else {
                    "stopped".to_string()
                },
                serial,
            }
        })
        .collect();

    Ok(avds)
}

#[tauri::command]
async fn get_avd_tooling_status() -> AppResult<AvdToolingStatus> {
    tauri::async_runtime::spawn_blocking(inspect_avd_tooling_blocking)
        .await
        .map_err(to_message)?
}

#[tauri::command]
async fn create_avd(app: tauri::AppHandle, input: CreateAvdInput) -> AppResult<String> {
    let created = tauri::async_runtime::spawn_blocking(move || create_avd_blocking(input))
        .await
        .map_err(to_message)??;
    let _ = refresh_tray_menu(&app);
    Ok(created)
}

#[tauri::command]
async fn delete_avd(app: tauri::AppHandle, name: String) -> AppResult<()> {
    tauri::async_runtime::spawn_blocking(move || delete_avd_blocking(&name))
        .await
        .map_err(to_message)??;
    let _ = refresh_tray_menu(&app);
    Ok(())
}

#[tauri::command]
fn start_avd(name: String) -> AppResult<()> {
    let settings = read_settings()?;
    let sdk = detect_android_sdk(&settings)?;
    let emulator = sdk.join("emulator").join("emulator.exe");

    if !emulator.exists() {
        return Err(format!(
            "emulator.exe not found under {}",
            path_to_string(sdk)
        ));
    }

    let mut command = Command::new(emulator);
    command
        .args(["-avd", &name])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    hide_console_window(&mut command);
    command.spawn().map_err(to_message)?;

    Ok(())
}

#[tauri::command]
fn stop_avd(serial: String) -> AppResult<()> {
    let settings = read_settings()?;
    let sdk = detect_android_sdk(&settings)?;
    let adb = sdk.join("platform-tools").join("adb.exe");

    if !adb.exists() {
        return Err(format!("adb.exe not found under {}", path_to_string(sdk)));
    }

    run_command_path(&adb, &["-s", &serial, "emu", "kill"], None).map(|_| ())
}

#[tauri::command]
fn list_custom_apps() -> AppResult<Vec<CustomApp>> {
    Ok(read_settings()?.custom_apps)
}

#[tauri::command]
fn save_custom_app(app_handle: tauri::AppHandle, app: CustomAppInput) -> AppResult<CustomApp> {
    let mut settings = read_settings()?;
    let saved = CustomApp {
        id: app.id.unwrap_or_else(new_id),
        name: app.name.trim().to_string(),
        path: app.path.trim().to_string(),
        args: trim_optional(app.args),
        cwd: trim_optional(app.cwd),
    };

    if saved.name.is_empty() {
        return Err("app name is required".to_string());
    }

    if saved.path.is_empty() {
        return Err("app path is required".to_string());
    }

    if let Some(existing) = settings
        .custom_apps
        .iter_mut()
        .find(|candidate| candidate.id == saved.id)
    {
        *existing = saved.clone();
    } else {
        settings.custom_apps.push(saved.clone());
    }

    write_settings(&settings)?;
    let _ = refresh_tray_menu(&app_handle);
    Ok(saved)
}

#[tauri::command]
fn delete_custom_app(app: tauri::AppHandle, id: String) -> AppResult<()> {
    let mut settings = read_settings()?;
    let before = settings.custom_apps.len();
    settings.custom_apps.retain(|app| app.id != id);

    if settings.custom_apps.len() == before {
        return Err("app not found".to_string());
    }

    write_settings(&settings)?;
    let _ = refresh_tray_menu(&app);
    Ok(())
}

#[tauri::command]
fn launch_custom_app(id: String) -> AppResult<()> {
    let settings = read_settings()?;
    let app = settings
        .custom_apps
        .iter()
        .find(|candidate| candidate.id == id)
        .ok_or_else(|| "app not found".to_string())?;

    launch_path(app)
}

#[tauri::command]
fn get_nginx_config() -> AppResult<Option<NginxConfig>> {
    let settings = read_settings()?;
    Ok(settings
        .nginx_path
        .or_else(|| find_default_nginx().map(path_to_string))
        .and_then(|path| normalize_nginx_path(&path).ok()))
}

#[tauri::command]
fn set_nginx_path(app: tauri::AppHandle, path: String) -> AppResult<NginxConfig> {
    let config = normalize_nginx_path(&path)?;
    let mut settings = read_settings()?;
    settings.nginx_path = Some(config.path.clone());
    write_settings(&settings)?;
    let _ = refresh_tray_menu(&app);
    Ok(config)
}

#[tauri::command]
async fn start_nginx() -> AppResult<NginxStartOutcome> {
    tauri::async_runtime::spawn_blocking(start_nginx_blocking)
        .await
        .map_err(to_message)?
}

fn start_nginx_blocking() -> AppResult<NginxStartOutcome> {
    let _start_guard = NGINX_START_LOCK
        .lock()
        .map_err(|_| "nginx start lock is unavailable".to_string())?;
    let config = require_nginx_config()?;

    if running_nginx_master(&config)?.is_some() {
        return Ok(NginxStartOutcome::AlreadyRunning);
    }

    let mut command = Command::new(&config.path);
    command
        .current_dir(&config.root)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    hide_console_window(&mut command);
    let mut child = command.spawn().map_err(to_message)?;

    for _ in 0..NGINX_START_ATTEMPTS {
        if running_nginx_master(&config)?.is_some() {
            return Ok(NginxStartOutcome::Started);
        }

        if let Some(status) = child.try_wait().map_err(to_message)? {
            return Err(format!(
                "nginx exited with status {status}; check {}",
                path_to_string(Path::new(&config.root).join("logs").join("error.log"))
            ));
        }

        thread::sleep(NGINX_START_POLL_INTERVAL);
    }

    Err(format!(
        "nginx did not become ready; check {}",
        path_to_string(Path::new(&config.root).join("logs").join("error.log"))
    ))
}

#[tauri::command]
fn stop_nginx() -> AppResult<()> {
    let config = require_nginx_config()?;
    run_command_path(
        Path::new(&config.path),
        &["-s", "stop"],
        Some(Path::new(&config.root)),
    )
    .map(|_| ())
}

#[tauri::command]
fn reload_nginx() -> AppResult<()> {
    let config = require_nginx_config()?;
    run_command_path(
        Path::new(&config.path),
        &["-s", "reload"],
        Some(Path::new(&config.root)),
    )
    .map(|_| ())
}

#[tauri::command]
fn open_nginx_config() -> AppResult<()> {
    let config = require_nginx_config()?;
    let file = Path::new(&config.root).join("conf").join("nginx.conf");

    if !file.exists() {
        return Err(format!("nginx.conf not found at {}", path_to_string(file)));
    }

    open_file(&file)
}

#[tauri::command]
async fn list_processes_by_port(port: u16) -> AppResult<Vec<PortProcess>> {
    tauri::async_runtime::spawn_blocking(move || list_processes_by_port_blocking(port))
        .await
        .map_err(to_message)?
}

fn list_processes_by_port_blocking(port: u16) -> AppResult<Vec<PortProcess>> {
    let output = run_command("netstat", &["-ano"], None)?;
    let mut matching_connections = Vec::new();

    for line in output.lines() {
        let parts = line.split_whitespace().collect::<Vec<_>>();
        if parts.len() < 5 {
            continue;
        }

        let protocol = parts[0].to_string();
        if protocol != "TCP" && protocol != "UDP" {
            continue;
        }

        let local_address = parts[1].to_string();
        if !address_matches_port(&local_address, port) {
            continue;
        }

        let pid_text = parts.last().copied().unwrap_or_default();
        let pid = match pid_text.parse::<u32>() {
            Ok(value) => value,
            Err(_) => continue,
        };

        matching_connections.push((protocol, local_address, pid));
    }

    let process_names = process_names().unwrap_or_default();
    let mut rows = matching_connections
        .into_iter()
        .map(|(protocol, local_address, pid)| PortProcess {
            protocol,
            local_address,
            pid,
            process_name: process_names
                .get(&pid)
                .cloned()
                .unwrap_or_else(|| "unknown".to_string()),
        })
        .collect::<Vec<_>>();

    rows.sort_by_key(|row| row.pid);
    rows.dedup_by(|left, right| left.pid == right.pid && left.local_address == right.local_address);
    Ok(rows)
}

#[tauri::command]
async fn kill_process(pid: u32) -> AppResult<()> {
    tauri::async_runtime::spawn_blocking(move || {
        run_command("taskkill", &["/PID", &pid.to_string(), "/F"], None).map(|_| ())
    })
    .await
    .map_err(to_message)?
}

#[tauri::command]
fn copy_to_clipboard(text: String) -> AppResult<()> {
    write_clipboard_text(&text)
}

fn refresh_tray_menu(app: &tauri::AppHandle) -> AppResult<()> {
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        let menu = build_tray_menu(app).map_err(to_message)?;
        tray.set_menu(Some(menu)).map_err(to_message)?;
    }
    Ok(())
}

fn build_tray_menu(app: &tauri::AppHandle) -> tauri::Result<Menu<tauri::Wry>> {
    let menu = Menu::new(app)?;
    let avd_menu = build_avd_tray_menu(app)?;
    let apps_menu = build_apps_tray_menu(app)?;
    let nginx_menu = build_nginx_tray_menu(app)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let open_item = MenuItem::with_id(app, "open", "Open", true, None::<&str>)?;
    let quit_item = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;

    menu.append(&avd_menu)?;
    menu.append(&apps_menu)?;
    menu.append(&nginx_menu)?;
    menu.append(&separator)?;
    menu.append(&open_item)?;
    menu.append(&quit_item)?;

    Ok(menu)
}

fn build_avd_tray_menu(app: &tauri::AppHandle) -> tauri::Result<Submenu<tauri::Wry>> {
    let menu = Submenu::new(app, "ADV", true)?;

    match scan_avds() {
        Ok(avds) if avds.is_empty() => append_disabled_item(app, &menu, "avd-empty", "No AVDs")?,
        Ok(avds) => {
            for avd in avds {
                let (id, label) = if let Some(serial) = avd.serial {
                    (format!("avd-stop|{serial}"), format!("{} · stop", avd.name))
                } else {
                    (
                        format!("avd-start|{}", avd.name),
                        format!("{} · start", avd.name),
                    )
                };
                let item = MenuItem::with_id(app, id, label, true, None::<&str>)?;
                menu.append(&item)?;
            }
        }
        Err(error) => append_disabled_item(app, &menu, "avd-error", &error)?,
    }

    Ok(menu)
}

fn build_apps_tray_menu(app: &tauri::AppHandle) -> tauri::Result<Submenu<tauri::Wry>> {
    let menu = Submenu::new(app, "Apps", true)?;
    let apps = read_settings()
        .map(|settings| settings.custom_apps)
        .unwrap_or_default();

    if apps.is_empty() {
        append_disabled_item(app, &menu, "apps-empty", "No apps saved")?;
    } else {
        for custom_app in apps {
            let item = MenuItem::with_id(
                app,
                format!("app-launch|{}", custom_app.id),
                custom_app.name,
                true,
                None::<&str>,
            )?;
            menu.append(&item)?;
        }
    }

    Ok(menu)
}

fn build_nginx_tray_menu(app: &tauri::AppHandle) -> tauri::Result<Submenu<tauri::Wry>> {
    let menu = Submenu::new(app, "Nginx", true)?;
    let start_item = MenuItem::with_id(app, "nginx-start", "Start", true, None::<&str>)?;
    let stop_item = MenuItem::with_id(app, "nginx-stop", "Stop", true, None::<&str>)?;
    let reload_item = MenuItem::with_id(app, "nginx-reload", "Reload", true, None::<&str>)?;
    let edit_item = MenuItem::with_id(app, "nginx-edit", "Edit", true, None::<&str>)?;

    menu.append(&start_item)?;
    menu.append(&stop_item)?;
    menu.append(&reload_item)?;
    menu.append(&edit_item)?;

    Ok(menu)
}

fn append_disabled_item(
    app: &tauri::AppHandle,
    menu: &Submenu<tauri::Wry>,
    id: &str,
    label: &str,
) -> tauri::Result<()> {
    let item = MenuItem::with_id(app, id, label, false, None::<&str>)?;
    menu.append(&item)
}

fn handle_tray_menu_event(app: &tauri::AppHandle, id: &str) {
    let should_refresh = if id == "open" {
        let _ = restore_main_window(app);
        false
    } else if id == "quit" {
        app.exit(0);
        false
    } else if let Some(name) = id.strip_prefix("avd-start|") {
        let _ = start_avd(name.to_string());
        true
    } else if let Some(serial) = id.strip_prefix("avd-stop|") {
        let _ = stop_avd(serial.to_string());
        true
    } else if let Some(app_id) = id.strip_prefix("app-launch|") {
        let _ = launch_custom_app(app_id.to_string());
        false
    } else if id == "nginx-start" {
        tauri::async_runtime::spawn(async {
            let _ = start_nginx().await;
        });
        false
    } else if id == "nginx-stop" {
        let _ = stop_nginx();
        false
    } else if id == "nginx-reload" {
        let _ = reload_nginx();
        false
    } else if id == "nginx-edit" {
        let _ = open_nginx_config();
        false
    } else {
        false
    };

    if should_refresh {
        let _ = refresh_tray_menu(app);
    }
}

fn read_settings() -> AppResult<AppSettings> {
    let path = settings_path()?;
    if !path.exists() {
        let settings = AppSettings::default();
        write_settings(&settings)?;
        return Ok(settings);
    }

    let content = fs::read_to_string(&path).map_err(to_message)?;
    serde_json::from_str(&content).map_err(to_message)
}

fn write_settings(settings: &AppSettings) -> AppResult<()> {
    let path = settings_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(to_message)?;
    }

    let content = serde_json::to_string_pretty(settings).map_err(to_message)?;
    fs::write(path, content).map_err(to_message)
}

fn settings_path() -> AppResult<PathBuf> {
    let base = env::var_os("APPDATA")
        .or_else(|| env::var_os("LOCALAPPDATA"))
        .ok_or_else(|| "APPDATA and LOCALAPPDATA are not set".to_string())?;
    Ok(PathBuf::from(base).join("devtool").join("settings.json"))
}

fn detect_android_sdk(settings: &AppSettings) -> AppResult<PathBuf> {
    let candidates = [
        settings.android_sdk_path.clone(),
        env::var("ANDROID_HOME").ok(),
        env::var("ANDROID_SDK_ROOT").ok(),
        env::var("LOCALAPPDATA")
            .ok()
            .map(|root| path_to_string(PathBuf::from(root).join("Android").join("Sdk"))),
    ];

    candidates
        .into_iter()
        .flatten()
        .map(PathBuf::from)
        .find(|path| path.exists())
        .ok_or_else(|| {
            "Android SDK not found. Set ANDROID_HOME, ANDROID_SDK_ROOT, or SDK path in settings."
                .to_string()
        })
}

fn running_avds(sdk: &Path) -> HashMap<String, String> {
    let adb = sdk.join("platform-tools").join("adb.exe");
    if !adb.exists() {
        return HashMap::new();
    }

    let devices = match run_command_path(&adb, &["devices"], None) {
        Ok(output) => output,
        Err(_) => return HashMap::new(),
    };

    let mut running = HashMap::new();
    for line in devices.lines().skip(1) {
        let parts = line.split_whitespace().collect::<Vec<_>>();
        if parts.len() < 2 || parts[1] != "device" {
            continue;
        }

        let serial = parts[0].to_string();
        if let Some(name) = avd_name_for_serial(&adb, &serial) {
            running.insert(name, serial);
        }
    }

    running
}

fn avd_name_for_serial(adb: &Path, serial: &str) -> Option<String> {
    let output = run_command_path(adb, &["-s", serial, "emu", "avd", "name"], None).ok()?;
    output
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && *line != "OK")
        .map(str::to_string)
}

struct CmdlineTools {
    bin_dir: PathBuf,
    android_exe: Option<PathBuf>,
    avdmanager_bat: Option<PathBuf>,
    sdkmanager_bat: Option<PathBuf>,
}

fn find_cmdline_tools_bin(sdk: &Path) -> Option<CmdlineTools> {
    let mut candidate_bins = Vec::new();
    let cmdline_root = sdk.join("cmdline-tools");
    let latest_bin = cmdline_root.join("latest").join("bin");
    if latest_bin.is_dir() {
        candidate_bins.push(latest_bin);
    }

    if let Ok(entries) = fs::read_dir(&cmdline_root) {
        let mut versioned = entries
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| {
                path.is_dir() && path.file_name().and_then(|n| n.to_str()) != Some("latest")
            })
            .map(|path| path.join("bin"))
            .filter(|bin| bin.is_dir())
            .collect::<Vec<_>>();
        versioned.sort();
        versioned.reverse();
        candidate_bins.extend(versioned);
    }

    for bin_dir in candidate_bins {
        let android_exe = Some(bin_dir.join("android.exe")).filter(|p| p.exists());
        let avdmanager_bat = Some(bin_dir.join("avdmanager.bat")).filter(|p| p.exists());
        let sdkmanager_bat = Some(bin_dir.join("sdkmanager.bat")).filter(|p| p.exists());
        if android_exe.is_some() || avdmanager_bat.is_some() {
            return Some(CmdlineTools {
                bin_dir,
                android_exe,
                avdmanager_bat,
                sdkmanager_bat,
            });
        }
    }

    None
}

fn parse_java_major_version(output: &str) -> Option<u32> {
    for line in output.lines() {
        if !line.contains("version") {
            continue;
        }
        let quoted = line.split('"').nth(1)?;
        let parts = quoted
            .split(|ch: char| !ch.is_ascii_digit())
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>();
        let first = parts.first()?.parse::<u32>().ok()?;
        let major = if first == 1 {
            parts.get(1)?.parse::<u32>().ok()?
        } else {
            first
        };
        return Some(major);
    }
    None
}

fn check_java_home_17(home: &Path) -> Option<PathBuf> {
    let java_exe = home.join("bin").join("java.exe");
    if !java_exe.exists() {
        return None;
    }
    let mut cmd = Command::new(&java_exe);
    cmd.arg("-version");
    hide_console_window(&mut cmd);
    let output = cmd.output().ok()?;
    let combined = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stderr),
        String::from_utf8_lossy(&output.stdout)
    );
    if parse_java_major_version(&combined).unwrap_or(0) >= 17 {
        Some(home.to_path_buf())
    } else {
        None
    }
}

fn collect_jdk_subdirs(parent: &Path, out: &mut Vec<PathBuf>) {
    if let Ok(entries) = fs::read_dir(parent) {
        let mut dirs = entries
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| path.is_dir())
            .collect::<Vec<_>>();
        dirs.sort();
        dirs.reverse();
        out.extend(dirs);
    }
}

fn find_java_17_home() -> Option<PathBuf> {
    let mut candidates = Vec::new();

    if let Ok(java_home) = env::var("JAVA_HOME") {
        let trimmed = java_home.trim_matches('"').trim();
        if !trimmed.is_empty() {
            candidates.push(PathBuf::from(trimmed));
        }
    }

    if let Ok(where_out) = run_command("where", &["java"], None) {
        for line in where_out.lines() {
            let exe = PathBuf::from(line.trim());
            if let Some(home) = exe.parent().and_then(Path::parent) {
                candidates.push(home.to_path_buf());
            }
        }
    }

    if let Ok(program_files) = env::var("ProgramFiles") {
        let pf = PathBuf::from(&program_files);
        candidates.push(pf.join("Android").join("Android Studio").join("jbr"));
        collect_jdk_subdirs(&pf.join("Eclipse Adoptium"), &mut candidates);
        collect_jdk_subdirs(&pf.join("Microsoft"), &mut candidates);
        collect_jdk_subdirs(&pf.join("Java"), &mut candidates);
    }

    if let Ok(local_app_data) = env::var("LOCALAPPDATA") {
        let lad = PathBuf::from(&local_app_data);
        candidates.push(lad.join("Programs").join("Android Studio").join("jbr"));
        let toolbox = lad.join("JetBrains").join("Toolbox").join("apps");
        if toolbox.is_dir() {
            let mut studio_dirs = Vec::new();
            collect_jdk_subdirs(&toolbox, &mut studio_dirs);
            for dir in studio_dirs {
                candidates.push(dir.join("jbr"));
            }
        }
    }

    if let Ok(user_profile) = env::var("USERPROFILE") {
        collect_jdk_subdirs(&PathBuf::from(user_profile).join(".jdk"), &mut candidates);
    }

    candidates
        .into_iter()
        .find_map(|home| check_java_home_17(&home))
}

fn validate_avd_name(raw: &str) -> AppResult<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err("AVD name is required".to_string());
    }
    if trimmed.len() > 64 {
        return Err("AVD name must be at most 64 characters".to_string());
    }
    let mut chars = trimmed.chars();
    let first = chars.next().unwrap();
    if !first.is_ascii_alphanumeric() {
        return Err("AVD name must start with a letter or digit".to_string());
    }
    if !chars.all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-')) {
        return Err("AVD name can only contain letters, digits, '.', '_', or '-'".to_string());
    }
    Ok(trimmed.to_string())
}

fn parse_api_sort_key(api: &str) -> (u32, u32) {
    let mut parts = api.split('.');
    let major = parts
        .next()
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(0);
    let minor = parts
        .next()
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(0);
    (major, minor)
}

fn format_tag_label(tag: &str) -> &str {
    match tag {
        "google_apis" => "Google APIs",
        "google_apis_ps16k" => "Google APIs 16K",
        "google_apis_playstore" => "Google Play",
        "google_apis_playstore_ps16k" => "Google Play 16K",
        "default" => "AOSP",
        "aosp_atd" => "AOSP ATD",
        "google_atd" => "Google ATD",
        _ => tag,
    }
}

fn parse_system_image_package(raw: &str, installed: bool) -> Option<SystemImageOption> {
    let normalized = raw.trim().replace('/', ";");
    let parts = normalized.split(';').collect::<Vec<_>>();
    if parts.len() != 4 || parts[0] != "system-images" {
        return None;
    }

    let api_segment = parts[1];
    let api = api_segment.strip_prefix("android-")?;
    if api.is_empty() {
        return None;
    }

    let tag = parts[2].to_string();
    let abi = parts[3].to_string();
    if tag.is_empty() || abi.is_empty() {
        return None;
    }

    let rooted = !tag.contains("playstore");
    let root_badge = if rooted { "[Root]" } else { "[No Root]" };
    let tag_label = format_tag_label(&tag);
    let suffix = if installed { "" } else { " · Download" };
    let label = format!("{root_badge} API {api} · {tag_label} · {abi}{suffix}");

    Some(SystemImageOption {
        package_id: format!("system-images;{api_segment};{tag};{abi}"),
        api: api.to_string(),
        tag,
        abi,
        rooted,
        installed,
        label,
    })
}

fn is_curated_remote_image(option: &SystemImageOption) -> bool {
    let (major, _) = parse_api_sort_key(&option.api);
    let supported_tag = matches!(
        option.tag.as_str(),
        "google_apis"
            | "google_apis_playstore"
            | "default"
            | "google_apis_ps16k"
            | "google_apis_playstore_ps16k"
    );
    let supported_abi = matches!(option.abi.as_str(), "x86_64" | "arm64-v8a");
    major >= 30 && supported_tag && supported_abi
}

fn sort_system_images(images: &mut [SystemImageOption]) {
    let host_abi = if cfg!(target_arch = "aarch64") {
        "arm64-v8a"
    } else {
        "x86_64"
    };

    images.sort_by(|left, right| {
        right
            .installed
            .cmp(&left.installed)
            .then_with(|| parse_api_sort_key(&right.api).cmp(&parse_api_sort_key(&left.api)))
            .then_with(|| (left.abi != host_abi).cmp(&(right.abi != host_abi)))
            .then_with(|| tag_rank(&left.tag).cmp(&tag_rank(&right.tag)))
            .then_with(|| left.package_id.cmp(&right.package_id))
    });
}

fn tag_rank(tag: &str) -> u8 {
    match tag {
        "google_apis" => 0,
        "google_apis_playstore" => 1,
        "default" => 2,
        "google_apis_ps16k" => 3,
        "google_apis_playstore_ps16k" => 4,
        _ => 5,
    }
}

fn scan_installed_system_images(sdk: &Path) -> Vec<SystemImageOption> {
    let root = sdk.join("system-images");
    let mut found = Vec::new();

    let Ok(apis) = fs::read_dir(&root) else {
        return found;
    };

    for api_entry in apis.flatten() {
        let api_path = api_entry.path();
        if !api_path.is_dir() {
            continue;
        }
        let Some(api_name) = api_path.file_name().and_then(|v| v.to_str()) else {
            continue;
        };
        let Ok(tags) = fs::read_dir(&api_path) else {
            continue;
        };

        for tag_entry in tags.flatten() {
            let tag_path = tag_entry.path();
            if !tag_path.is_dir() {
                continue;
            }
            let Some(tag_name) = tag_path.file_name().and_then(|v| v.to_str()) else {
                continue;
            };
            let Ok(abis) = fs::read_dir(&tag_path) else {
                continue;
            };

            for abi_entry in abis.flatten() {
                let abi_path = abi_entry.path();
                if !abi_path.is_dir() || !abi_path.join("source.properties").exists() {
                    continue;
                }
                let Some(abi_name) = abi_path.file_name().and_then(|v| v.to_str()) else {
                    continue;
                };
                let raw = format!("system-images;{api_name};{tag_name};{abi_name}");
                if let Some(option) = parse_system_image_package(&raw, true) {
                    found.push(option);
                }
            }
        }
    }

    found
}

fn parse_sdk_list_system_images(output: &str) -> Vec<String> {
    output
        .lines()
        .filter_map(|line| line.split_whitespace().next())
        .filter(|token| {
            token.starts_with("system-images/android-")
                || token.starts_with("system-images;android-")
        })
        .map(str::to_string)
        .collect()
}

fn fetch_remote_system_images(
    sdk: &Path,
    tools: &CmdlineTools,
    java_home: Option<&Path>,
) -> Vec<SystemImageOption> {
    let raw_list = if let Some(android_exe) = &tools.android_exe {
        let sdk_arg = format!("--sdk={}", path_to_string(sdk));
        run_command_path(
            android_exe,
            &[&sdk_arg, "sdk", "list", "--all", "system-images/android-*"],
            None,
        )
        .ok()
    } else if let (Some(sdkmanager), Some(java)) = (&tools.sdkmanager_bat, java_home) {
        let sdk_arg = format!("--sdk_root={}", path_to_string(sdk));
        run_java_tool_command(sdkmanager, &[&sdk_arg, "--list"], java, None).ok()
    } else {
        None
    };

    raw_list
        .map(|output| {
            parse_sdk_list_system_images(&output)
                .into_iter()
                .filter_map(|pkg| parse_system_image_package(&pkg, false))
                .filter(is_curated_remote_image)
                .collect()
        })
        .unwrap_or_default()
}

fn collect_system_images(
    sdk: &Path,
    tools: &CmdlineTools,
    java_home: Option<&Path>,
) -> Vec<SystemImageOption> {
    let mut images = scan_installed_system_images(sdk);
    let seen = images
        .iter()
        .map(|item| item.package_id.clone())
        .collect::<HashSet<_>>();

    for candidate in fetch_remote_system_images(sdk, tools, java_home) {
        if !seen.contains(&candidate.package_id) {
            images.push(candidate);
        }
    }

    sort_system_images(&mut images);
    images
}

fn default_avd_profiles(sdk: &Path, android_exe: &Path) -> Vec<String> {
    let sdk_arg = format!("--sdk={}", path_to_string(sdk));
    if let Ok(output) = run_command_path(
        android_exe,
        &[&sdk_arg, "emulator", "create", "--list-profiles"],
        None,
    ) {
        let profiles = output
            .lines()
            .map(str::trim)
            .filter(|line| {
                !line.is_empty() && line.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
            })
            .map(str::to_string)
            .collect::<Vec<_>>();
        if !profiles.is_empty() {
            return profiles;
        }
    }

    vec![
        "medium_phone".to_string(),
        "small_phone".to_string(),
        "medium_tablet".to_string(),
        "small_desktop".to_string(),
        "medium_desktop".to_string(),
        "large_desktop".to_string(),
    ]
}

fn inspect_avd_tooling_blocking() -> AppResult<AvdToolingStatus> {
    let settings = read_settings()?;
    let sdk = match detect_android_sdk(&settings) {
        Ok(path) => path,
        Err(_) => {
            return Ok(AvdToolingStatus {
                can_create: false,
                can_delete: false,
                mode: "missing".to_string(),
                missing_reason: Some(
                    "Android SDK not found. Click the folder icon to select your SDK directory, or install Android Studio / Command-line Tools."
                        .to_string(),
                ),
                images: Vec::new(),
                profiles: Vec::new(),
            });
        }
    };

    let Some(tools) = find_cmdline_tools_bin(&sdk) else {
        return Ok(AvdToolingStatus {
            can_create: false,
            can_delete: false,
            mode: "missing".to_string(),
            missing_reason: Some(format!(
                "Android Command-line Tools missing in {}. Enable 'Android SDK Command-line Tools (latest)' in Android Studio SDK Tools, or extract commandlinetools-win into cmdline-tools\\latest.",
                path_to_string(&sdk)
            )),
            images: Vec::new(),
            profiles: Vec::new(),
        });
    };

    let java_home = find_java_17_home();

    if tools.avdmanager_bat.is_some() && java_home.is_some() {
        let images = collect_system_images(&sdk, &tools, java_home.as_deref());
        if images.is_empty() {
            return Ok(AvdToolingStatus {
                can_create: false,
                can_delete: true,
                mode: "missing".to_string(),
                missing_reason: Some(
                    "No Android system images found and SDK repository could not be reached. Check internet connection or install a system image."
                        .to_string(),
                ),
                images: Vec::new(),
                profiles: Vec::new(),
            });
        }

        return Ok(AvdToolingStatus {
            can_create: true,
            can_delete: true,
            mode: "full".to_string(),
            missing_reason: None,
            images,
            profiles: Vec::new(),
        });
    }

    if let Some(android_exe) = &tools.android_exe {
        return Ok(AvdToolingStatus {
            can_create: true,
            can_delete: true,
            mode: "profile".to_string(),
            missing_reason: None,
            images: Vec::new(),
            profiles: default_avd_profiles(&sdk, android_exe),
        });
    }

    Ok(AvdToolingStatus {
        can_create: false,
        can_delete: false,
        mode: "missing".to_string(),
        missing_reason: Some(
            "Java 17+ not found for avdmanager, and android.exe is unavailable. Install JDK 17+, Android Studio, or update Android Command-line Tools."
                .to_string(),
        ),
        images: Vec::new(),
        profiles: Vec::new(),
    })
}

fn system_image_installed_on_disk(sdk: &Path, option: &SystemImageOption) -> bool {
    sdk.join("system-images")
        .join(format!("android-{}", option.api))
        .join(&option.tag)
        .join(&option.abi)
        .join("source.properties")
        .exists()
}

fn install_sdk_packages_if_needed(
    sdk: &Path,
    tools: &CmdlineTools,
    java_home: Option<&Path>,
    image: &SystemImageOption,
) -> AppResult<()> {
    let mut semicolon_packages = Vec::new();
    if !sdk.join("emulator").join("emulator.exe").exists() {
        semicolon_packages.push("emulator".to_string());
    }
    if !sdk.join("platform-tools").join("adb.exe").exists() {
        semicolon_packages.push("platform-tools".to_string());
    }
    if !system_image_installed_on_disk(sdk, image) {
        semicolon_packages.push(image.package_id.clone());
    }

    if semicolon_packages.is_empty() {
        return Ok(());
    }

    if let Some(android_exe) = &tools.android_exe {
        let sdk_arg = format!("--sdk={}", path_to_string(sdk));
        for pkg in &semicolon_packages {
            let slash_pkg = pkg.replace(';', "/");
            run_command_path(android_exe, &[&sdk_arg, "sdk", "install", &slash_pkg], None)?;
        }
        return Ok(());
    }

    if let (Some(sdkmanager), Some(java)) = (&tools.sdkmanager_bat, java_home) {
        let sdk_arg = format!("--sdk_root={}", path_to_string(sdk));
        let mut args = vec![sdk_arg.as_str()];
        for pkg in &semicolon_packages {
            args.push(pkg.as_str());
        }
        let yes_input = "y\r\ny\r\ny\r\ny\r\ny\r\ny\r\ny\r\ny\r\n";
        run_java_tool_command(sdkmanager, &args, java, Some(yes_input))?;
        return Ok(());
    }

    Err("Cannot download Android system image: neither android.exe nor sdkmanager.bat is available.".to_string())
}

fn run_avdmanager(
    tools: &CmdlineTools,
    java_home: &Path,
    sdk: &Path,
    args: &[&str],
    stdin_text: Option<&str>,
) -> AppResult<String> {
    let cmdline_root = tools.bin_dir.parent().unwrap_or(&tools.bin_dir);
    let classpath_jar = cmdline_root.join("lib").join("avdmanager-classpath.jar");
    let java_exe = java_home.join("bin").join("java.exe");

    if classpath_jar.exists() && java_exe.exists() {
        let mut command = Command::new(&java_exe);
        command
            .arg(format!(
                "-Dcom.android.sdkmanager.toolsdir={}",
                path_to_string(cmdline_root)
            ))
            .arg("-classpath")
            .arg(&classpath_jar)
            .arg("com.android.sdklib.tool.AvdManagerCli")
            .args(args)
            .env("ANDROID_HOME", sdk)
            .env("ANDROID_SDK_ROOT", sdk)
            .env("JAVA_HOME", java_home);
        hide_console_window(&mut command);
        return command_output_with_stdin(command, stdin_text);
    }

    let avdmanager = tools
        .avdmanager_bat
        .as_ref()
        .ok_or_else(|| "avdmanager.bat not found".to_string())?;
    run_java_tool_command(avdmanager, args, java_home, stdin_text)
}

fn run_java_tool_command(
    script: &Path,
    args: &[&str],
    java_home: &Path,
    stdin_text: Option<&str>,
) -> AppResult<String> {
    let mut command = Command::new(script);
    command.args(args).env("JAVA_HOME", java_home);
    hide_console_window(&mut command);
    command_output_with_stdin(command, stdin_text)
}

fn command_output_with_stdin(mut command: Command, stdin_text: Option<&str>) -> AppResult<String> {
    if let Some(input) = stdin_text {
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = command.spawn().map_err(to_message)?;
        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(input.as_bytes());
        }
        let output = child.wait_with_output().map_err(to_message)?;
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        if output.status.success() {
            Ok(stdout)
        } else if !stderr.is_empty() {
            Err(stderr)
        } else if !stdout.is_empty() {
            Err(stdout)
        } else {
            Err(format!("command failed with status {}", output.status))
        }
    } else {
        command_output(command)
    }
}

fn create_avd_blocking(input: CreateAvdInput) -> AppResult<String> {
    let settings = read_settings()?;
    let sdk = detect_android_sdk(&settings)?;
    let tools = find_cmdline_tools_bin(&sdk).ok_or_else(|| {
        "Android Command-line Tools missing. Install cmdline-tools;latest first.".to_string()
    })?;
    let java_home = find_java_17_home();

    if tools.avdmanager_bat.is_some() && java_home.is_some() && input.package_id.is_some() {
        let java = java_home.unwrap();
        let name = validate_avd_name(input.name.as_deref().unwrap_or_default())?;
        let existing = scan_avds().unwrap_or_default();
        if existing
            .iter()
            .any(|avd| avd.name.eq_ignore_ascii_case(&name))
        {
            return Err(format!("AVD '{name}' already exists"));
        }

        let pkg_raw = input.package_id.unwrap();
        let image = parse_system_image_package(&pkg_raw, false)
            .ok_or_else(|| format!("Invalid system image package: {pkg_raw}"))?;

        install_sdk_packages_if_needed(&sdk, &tools, Some(&java), &image)?;
        run_avdmanager(
            &tools,
            &java,
            &sdk,
            &["create", "avd", "-n", &name, "-k", &image.package_id],
            Some("no\r\n"),
        )?;
        return Ok(name);
    }

    if let Some(android_exe) = &tools.android_exe {
        let profile = input
            .profile
            .as_deref()
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .unwrap_or("medium_phone");
        let sdk_arg = format!("--sdk={}", path_to_string(&sdk));
        run_command_path(
            android_exe,
            &[&sdk_arg, "emulator", "create", profile],
            None,
        )?;
        return Ok(profile.to_string());
    }

    Err(
        "Missing required Android tools (avdmanager+Java 17 or android.exe) to create AVD."
            .to_string(),
    )
}

fn delete_avd_blocking(raw_name: &str) -> AppResult<()> {
    let name = validate_avd_name(raw_name)?;
    let settings = read_settings()?;
    let sdk = detect_android_sdk(&settings)?;

    if running_avds(&sdk)
        .keys()
        .any(|running| running.eq_ignore_ascii_case(&name))
    {
        return Err(format!("Stop {name} before deleting"));
    }

    let tools = find_cmdline_tools_bin(&sdk).ok_or_else(|| {
        "Android Command-line Tools missing. Install cmdline-tools;latest to delete AVDs."
            .to_string()
    })?;

    if let Some(android_exe) = &tools.android_exe {
        let sdk_arg = format!("--sdk={}", path_to_string(&sdk));
        run_command_path(android_exe, &[&sdk_arg, "emulator", "remove", &name], None)?;
        return Ok(());
    }

    if let (Some(_), Some(java)) = (&tools.avdmanager_bat, find_java_17_home()) {
        run_avdmanager(&tools, &java, &sdk, &["delete", "avd", "-n", &name], None)?;
        return Ok(());
    }

    Err(
        "Missing required Android tools (android.exe or avdmanager+Java 17) to delete AVD."
            .to_string(),
    )
}

fn launch_path(app: &CustomApp) -> AppResult<()> {
    let path = Path::new(&app.path);
    if !path.exists() {
        return Err(format!("file not found: {}", app.path));
    }

    let cwd = app
        .cwd
        .as_ref()
        .map(PathBuf::from)
        .or_else(|| path.parent().map(Path::to_path_buf));
    let args = split_args(app.args.as_deref());
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();

    let mut command = match extension.as_str() {
        "bat" | "cmd" => {
            let mut command = Command::new("cmd");
            command.arg("/C").arg("start").arg("").arg(path);
            command
        }
        "jar" => {
            let mut command = Command::new("java");
            command.arg("-jar").arg(path);
            command
        }
        "vbs" => {
            let mut command = Command::new("wscript.exe");
            command.arg(path);
            command
        }
        _ => Command::new(path),
    };

    command.args(args);

    if let Some(cwd) = cwd {
        command.current_dir(cwd);
    }

    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(to_message)?;

    Ok(())
}

fn normalize_nginx_path(path: &str) -> AppResult<NginxConfig> {
    let raw = PathBuf::from(path.trim());
    let exe = if raw.is_dir() {
        raw.join("nginx.exe")
    } else {
        raw
    };

    if !exe.exists() {
        return Err(format!("nginx.exe not found at {}", path_to_string(exe)));
    }

    let root = exe
        .parent()
        .ok_or_else(|| "nginx root not found".to_string())?
        .to_path_buf();

    Ok(NginxConfig {
        path: path_to_string(exe),
        root: path_to_string(root),
    })
}

fn require_nginx_config() -> AppResult<NginxConfig> {
    get_nginx_config()?.ok_or_else(|| "nginx path is not configured".to_string())
}

fn running_nginx_master(config: &NginxConfig) -> AppResult<Option<u32>> {
    let Some(pid) = read_nginx_pid(config)? else {
        return Ok(None);
    };
    let Some(process_path) = process_executable_path(pid)? else {
        return Ok(None);
    };

    if paths_match(&process_path, Path::new(&config.path)) {
        Ok(Some(pid))
    } else {
        Ok(None)
    }
}

fn read_nginx_pid(config: &NginxConfig) -> AppResult<Option<u32>> {
    let pid_path = Path::new(&config.root).join("logs").join("nginx.pid");
    match fs::read_to_string(&pid_path) {
        Ok(value) => Ok(parse_nginx_pid(&value)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(format!(
            "failed to read {}: {error}",
            path_to_string(pid_path)
        )),
    }
}

fn parse_nginx_pid(value: &str) -> Option<u32> {
    value.trim().parse::<u32>().ok().filter(|pid| *pid > 0)
}

fn paths_match(left: &Path, right: &Path) -> bool {
    let left = fs::canonicalize(left).unwrap_or_else(|_| left.to_path_buf());
    let right = fs::canonicalize(right).unwrap_or_else(|_| right.to_path_buf());
    path_to_string(left).eq_ignore_ascii_case(&path_to_string(right))
}

#[cfg(windows)]
fn process_executable_path(pid: u32) -> AppResult<Option<PathBuf>> {
    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if handle.is_null() {
        let error = std::io::Error::last_os_error();
        return match error.raw_os_error() {
            Some(87) => Ok(None),
            _ => Err(format!("failed to inspect PID {pid}: {error}")),
        };
    }

    let mut buffer = vec![0_u16; 32_768];
    let mut size = buffer.len() as u32;
    let success = unsafe { QueryFullProcessImageNameW(handle, 0, buffer.as_mut_ptr(), &mut size) };
    unsafe {
        CloseHandle(handle);
    }

    if success == 0 {
        return Err(format!(
            "failed to read executable path for PID {pid}: {}",
            std::io::Error::last_os_error()
        ));
    }

    Ok(Some(PathBuf::from(OsString::from_wide(
        &buffer[..size as usize],
    ))))
}

fn find_default_nginx() -> Option<PathBuf> {
    [r"D:\nginx\nginx.exe", r"C:\nginx\nginx.exe"]
        .into_iter()
        .map(PathBuf::from)
        .find(|path| path.exists())
}

fn address_matches_port(address: &str, port: u16) -> bool {
    address
        .rsplit_once(':')
        .and_then(|(_, value)| value.parse::<u16>().ok())
        == Some(port)
}

fn process_names() -> AppResult<HashMap<u32, String>> {
    let output = run_command("tasklist", &["/FO", "CSV", "/NH"], None)?;
    let names = output
        .lines()
        .filter_map(parse_csv_line)
        .filter_map(|fields| {
            let name = fields.first()?.clone();
            let pid = fields.get(1)?.parse::<u32>().ok()?;
            Some((pid, name))
        })
        .collect();

    Ok(names)
}

fn parse_csv_line(line: &str) -> Option<Vec<String>> {
    let mut fields = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut chars = line.chars().peekable();

    while let Some(ch) = chars.next() {
        match ch {
            '"' if quoted && chars.peek() == Some(&'"') => {
                field.push('"');
                chars.next();
            }
            '"' => quoted = !quoted,
            ',' if !quoted => {
                fields.push(field.clone());
                field.clear();
            }
            _ => field.push(ch),
        }
    }

    fields.push(field);
    Some(fields)
}

fn open_file(path: &Path) -> AppResult<()> {
    run_command("cmd", &["/C", "start", "", &path_to_string(path)], None).map(|_| ())
}

fn run_command(program: &str, args: &[&str], cwd: Option<&Path>) -> AppResult<String> {
    let mut command = Command::new(program);
    command.args(args);
    if let Some(cwd) = cwd {
        command.current_dir(cwd);
    }
    hide_console_window(&mut command);
    command_output(command)
}

fn run_command_path(program: &Path, args: &[&str], cwd: Option<&Path>) -> AppResult<String> {
    let mut command = Command::new(program);
    command.args(args);
    if let Some(cwd) = cwd {
        command.current_dir(cwd);
    }
    hide_console_window(&mut command);
    command_output(command)
}

fn command_output(mut command: Command) -> AppResult<String> {
    let output = command.output().map_err(to_message)?;
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();

    if output.status.success() {
        Ok(stdout)
    } else if !stderr.is_empty() {
        Err(stderr)
    } else if !stdout.is_empty() {
        Err(stdout)
    } else {
        Err(format!("command failed with status {}", output.status))
    }
}

fn hide_console_window(command: &mut Command) {
    #[cfg(windows)]
    command.creation_flags(CREATE_NO_WINDOW);
}

fn split_args(args: Option<&str>) -> Vec<String> {
    args.unwrap_or_default()
        .split_whitespace()
        .map(str::to_string)
        .collect()
}

fn trim_optional(value: Option<String>) -> Option<String> {
    value
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
}

fn new_id() -> String {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default();
    format!("app-{millis}")
}

fn path_to_string(path: impl AsRef<Path>) -> String {
    path.as_ref().to_string_lossy().to_string()
}

fn to_message(error: impl std::fmt::Display) -> String {
    error.to_string()
}

fn encode_utf16_nul(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(windows)]
struct ClipboardGuard;

#[cfg(windows)]
impl Drop for ClipboardGuard {
    fn drop(&mut self) {
        unsafe {
            CloseClipboard();
        }
    }
}

#[cfg(windows)]
fn open_clipboard_with_retry() -> AppResult<ClipboardGuard> {
    for attempt in 0..CLIPBOARD_OPEN_ATTEMPTS {
        if unsafe { OpenClipboard(std::ptr::null_mut()) } != 0 {
            return Ok(ClipboardGuard);
        }

        if attempt + 1 < CLIPBOARD_OPEN_ATTEMPTS {
            thread::sleep(CLIPBOARD_RETRY_INTERVAL);
        }
    }

    Err(format!(
        "failed to open clipboard: {}",
        std::io::Error::last_os_error()
    ))
}

#[cfg(windows)]
fn write_clipboard_text(text: &str) -> AppResult<()> {
    let utf16 = encode_utf16_nul(text);
    let byte_len = utf16.len() * std::mem::size_of::<u16>();
    let _guard = open_clipboard_with_retry()?;

    if unsafe { EmptyClipboard() } == 0 {
        return Err(format!(
            "failed to clear clipboard: {}",
            std::io::Error::last_os_error()
        ));
    }

    let handle = unsafe { GlobalAlloc(GMEM_MOVEABLE, byte_len) };
    if handle.is_null() {
        return Err(format!(
            "failed to allocate clipboard memory: {}",
            std::io::Error::last_os_error()
        ));
    }

    let pointer = unsafe { GlobalLock(handle) };
    if pointer.is_null() {
        unsafe {
            GlobalFree(handle);
        }
        return Err(format!(
            "failed to lock clipboard memory: {}",
            std::io::Error::last_os_error()
        ));
    }

    unsafe {
        std::ptr::copy_nonoverlapping(utf16.as_ptr(), pointer.cast::<u16>(), utf16.len());
        GlobalUnlock(handle);
    }

    let result = unsafe { SetClipboardData(CF_UNICODETEXT as u32, handle) };
    if result.is_null() {
        unsafe {
            GlobalFree(handle);
        }
        return Err(format!(
            "failed to write clipboard text: {}",
            std::io::Error::last_os_error()
        ));
    }

    Ok(())
}

#[cfg(not(windows))]
fn write_clipboard_text(_text: &str) -> AppResult<()> {
    Err("clipboard copy is only supported on Windows".to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let app_handle = app.app_handle().clone();
            let menu = build_tray_menu(&app_handle)?;

            let mut tray = TrayIconBuilder::with_id(TRAY_ID)
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| {
                    let id = event.id().as_ref().to_string();
                    handle_tray_menu_event(app, &id);
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        let _ = restore_main_window(tray.app_handle());
                    }
                });

            if let Some(icon) = app.default_window_icon() {
                tray = tray.icon(icon.clone());
            }
            tray.build(app)?;

            if read_settings()
                .map(|settings| settings.start_minimized)
                .unwrap_or(false)
            {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.hide();
                }
            }

            Ok(())
        })
        .on_window_event(|window, event| match event {
            WindowEvent::CloseRequested { api, .. } => {
                api.prevent_close();
                let _ = window.hide();
            }
            WindowEvent::Resized(_) if window.is_minimized().unwrap_or(false) => {
                let _ = window.hide();
            }
            _ => {}
        })
        .invoke_handler(tauri::generate_handler![
            get_app_settings,
            save_app_settings,
            set_start_with_windows,
            get_start_with_windows_status,
            show_main_window,
            hide_main_window,
            set_content_height,
            quit_app,
            refresh_tray,
            scan_avds,
            get_avd_tooling_status,
            create_avd,
            delete_avd,
            start_avd,
            stop_avd,
            list_custom_apps,
            save_custom_app,
            delete_custom_app,
            launch_custom_app,
            get_nginx_config,
            set_nginx_path,
            start_nginx,
            stop_nginx,
            reload_nginx,
            open_nginx_config,
            list_processes_by_port,
            kill_process,
            copy_to_clipboard,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_valid_nginx_pid() {
        assert_eq!(parse_nginx_pid("15144\r\n"), Some(15144));
    }

    #[test]
    fn rejects_invalid_nginx_pid() {
        assert_eq!(parse_nginx_pid(""), None);
        assert_eq!(parse_nginx_pid("0"), None);
        assert_eq!(parse_nginx_pid("not-a-pid"), None);
    }

    #[test]
    fn compares_windows_paths_case_insensitively() {
        assert!(paths_match(
            Path::new(r"C:\Tools\Nginx\nginx.exe"),
            Path::new(r"c:\tools\nginx\NGINX.EXE")
        ));
    }

    #[test]
    fn serializes_nginx_start_outcomes_for_the_frontend() {
        assert_eq!(
            serde_json::to_string(&NginxStartOutcome::Started).unwrap(),
            r#""started""#
        );
        assert_eq!(
            serde_json::to_string(&NginxStartOutcome::AlreadyRunning).unwrap(),
            r#""alreadyRunning""#
        );
    }

    #[cfg(windows)]
    #[test]
    fn reads_current_process_executable_path() {
        let actual = process_executable_path(std::process::id())
            .unwrap()
            .expect("current process should be running");
        assert!(paths_match(&actual, &env::current_exe().unwrap()));
    }

    #[cfg(windows)]
    #[test]
    fn detects_live_master_from_pid_file_and_executable_path() {
        let root = env::temp_dir().join(format!(
            "devtool-nginx-test-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let logs = root.join("logs");
        fs::create_dir_all(&logs).unwrap();
        fs::write(logs.join("nginx.pid"), std::process::id().to_string()).unwrap();

        let config = NginxConfig {
            path: path_to_string(env::current_exe().unwrap()),
            root: path_to_string(&root),
        };

        assert_eq!(
            running_nginx_master(&config).unwrap(),
            Some(std::process::id())
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn encodes_clipboard_text_as_nul_terminated_utf16() {
        assert_eq!(encode_utf16_nul("ab"), vec![b'a' as u16, b'b' as u16, 0]);
        assert_eq!(encode_utf16_nul(""), vec![0]);
    }

    #[test]
    fn validates_avd_names() {
        assert_eq!(validate_avd_name("Pixel_Root-37").unwrap(), "Pixel_Root-37");
        assert!(validate_avd_name("").is_err());
        assert!(validate_avd_name("-bad").is_err());
        assert!(validate_avd_name("has space").is_err());
    }

    #[test]
    fn parses_and_ranks_system_images_by_root_and_installed_status() {
        let rooted =
            parse_system_image_package("system-images/android-37.0/google_apis/x86_64", true)
                .unwrap();
        assert!(rooted.rooted);
        assert!(rooted.installed);
        assert_eq!(
            rooted.package_id,
            "system-images;android-37.0;google_apis;x86_64"
        );
        assert_eq!(rooted.label, "[Root] API 37.0 · Google APIs · x86_64");

        let play = parse_system_image_package(
            "system-images;android-37.0;google_apis_playstore;x86_64",
            false,
        )
        .unwrap();
        assert!(!play.rooted);
        assert!(!play.installed);
        assert_eq!(
            play.label,
            "[No Root] API 37.0 · Google Play · x86_64 · Download"
        );

        let mut list = vec![play.clone(), rooted.clone()];
        sort_system_images(&mut list);
        assert_eq!(list[0], rooted);
        assert_eq!(list[1], play);
    }

    #[test]
    fn parses_java_major_versions() {
        assert_eq!(
            parse_java_major_version("openjdk version \"21.0.4\" 2024-07-16"),
            Some(21)
        );
        assert_eq!(
            parse_java_major_version("java version \"1.8.0_391\""),
            Some(8)
        );
    }
}
