use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    env, fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{SystemTime, UNIX_EPOCH},
};
use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem, Submenu},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Manager, WindowEvent,
};

type AppResult<T> = Result<T, String>;

const TRAY_ID: &str = "devtool-main-tray";

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

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct NginxConfig {
    path: String,
    root: String,
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

    if !emulator.exists() {
        return Err(format!(
            "emulator.exe not found under {}",
            path_to_string(sdk)
        ));
    }

    let output = run_command_path(&emulator, &["-list-avds"], None)?;
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

    Command::new(emulator)
        .args(["-avd", &name])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(to_message)?;

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
fn start_nginx() -> AppResult<()> {
    let config = require_nginx_config()?;
    let child = Command::new(&config.path)
        .current_dir(&config.root)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(to_message)?;
    drop(child);
    Ok(())
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
fn list_processes_by_port(port: u16) -> AppResult<Vec<PortProcess>> {
    let output = run_command("netstat", &["-ano"], None)?;
    let mut rows = Vec::new();

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

        rows.push(PortProcess {
            protocol,
            local_address,
            pid,
            process_name: process_name(pid).unwrap_or_else(|| "unknown".to_string()),
        });
    }

    rows.sort_by_key(|row| row.pid);
    rows.dedup_by(|left, right| left.pid == right.pid && left.local_address == right.local_address);
    Ok(rows)
}

#[tauri::command]
fn kill_process(pid: u32) -> AppResult<()> {
    run_command("taskkill", &["/PID", &pid.to_string(), "/F"], None).map(|_| ())
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
        let _ = start_nginx();
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

fn process_name(pid: u32) -> Option<String> {
    let output = run_command(
        "tasklist",
        &["/FI", &format!("PID eq {pid}"), "/FO", "CSV", "/NH"],
        None,
    )
    .ok()?;

    output
        .lines()
        .find(|line| !line.contains("No tasks are running"))
        .and_then(parse_csv_line)
        .and_then(|fields| fields.first().cloned())
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
    command_output(command)
}

fn run_command_path(program: &Path, args: &[&str], cwd: Option<&Path>) -> AppResult<String> {
    let mut command = Command::new(program);
    command.args(args);
    if let Some(cwd) = cwd {
        command.current_dir(cwd);
    }
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
            quit_app,
            refresh_tray,
            scan_avds,
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
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
