# DevTool Compact App Plan

## Summary

Build DevTool as a small Windows utility window, not a large dashboard. The app uses Tauri v2 with vanilla HTML/CSS/JS and keeps all core actions on one compact screen.

Design direction:

- Small fixed-size window, around `560x720`.
- Minimal style, quiet colors, dense controls.
- No sidebar, no tabs, no multi-page navigation.
- All features are visible on one screen as compact sections.
- System tray is used for quick access and background running.

## One-Screen Layout

Use a single vertical window:

1. Header
   - App name: `DevTool`
   - Compact status text
   - Refresh button
2. Android AVD
   - SDK status/path line
   - Scan/refresh action
   - Compact AVD list with start/stop buttons
3. Custom Apps
   - Add app button
   - Small list of saved apps
   - Actions per app: run, rename/edit, delete
4. Nginx
   - Nginx path line
   - Start, stop, reload, config buttons
5. Ports
   - Port input
   - Search button
   - Compact process result list
   - Kill button requires confirmation
6. Startup / Tray
   - Start with Windows
   - Start minimized
   - Minimize to tray
   - Close hides to tray

## Backend Features

### Settings And Tray

- Persist settings in app config JSON:
  - `start_with_windows`
  - `start_minimized`
  - `minimize_to_tray`
  - `close_to_tray`
  - Android SDK override
  - Nginx path
  - Custom apps
- Add Tauri tray icon.
- Left-click tray icon shows/focuses the compact window.
- Right-click tray icon opens a native quick menu:
  - Open DevTool
  - Refresh AVD
  - Start Nginx
  - Stop Nginx
  - Reload Nginx
  - Open Ports
  - Quit
- Tray menu must stay short; detailed management happens in the window.

### Android AVD

- Detect SDK from `ANDROID_HOME`, `ANDROID_SDK_ROOT`, then `%LOCALAPPDATA%\Android\Sdk`.
- Scan AVDs with `emulator.exe -list-avds`.
- Check running emulators with `adb devices`.
- Start AVDs with `emulator.exe -avd <name>`.
- Stop emulators with `adb -s <serial> emu kill`.

### Custom Apps

- Store custom apps with `id`, `name`, `path`, optional `args`, optional `cwd`.
- Support `.exe`, `.bat`, `.cmd`, `.jar`, and other runnable files.
- `.jar` runs through `java -jar`.
- Support add, run, edit/rename, delete.

### Nginx

- Store nginx root or direct `nginx.exe` path.
- Normalize to executable path and root directory.
- Start with working directory set to nginx root.
- Stop/reload with `nginx.exe -s stop` and `nginx.exe -s reload`.
- Open `conf/nginx.conf` through the opener plugin.

### Ports

- Query process by port using `netstat -ano`.
- Enrich PID with process name using `tasklist`.
- Kill using `taskkill /PID <pid> /F`.
- Always confirm before killing.

## Tauri Commands

```rust
get_app_settings() -> AppSettings
save_app_settings(settings: AppSettings) -> CommandResult
set_start_with_windows(enabled: bool) -> CommandResult
get_start_with_windows_status() -> bool
show_main_window() -> CommandResult
hide_main_window() -> CommandResult
quit_app() -> CommandResult

scan_avds() -> Vec<AvdInfo>
start_avd(name: String) -> CommandResult
stop_avd(serial: String) -> CommandResult

list_custom_apps() -> Vec<CustomApp>
save_custom_app(app: CustomAppInput) -> CustomApp
delete_custom_app(id: String) -> CommandResult
launch_custom_app(id: String) -> CommandResult

get_nginx_config() -> Option<NginxConfig>
set_nginx_path(path: String) -> CommandResult
start_nginx() -> CommandResult
stop_nginx() -> CommandResult
reload_nginx() -> CommandResult
open_nginx_config() -> CommandResult

list_processes_by_port(port: u16) -> Vec<PortProcess>
kill_process(pid: u32) -> CommandResult
```

## Test Plan

- Verify the compact one-screen UI fits without scrolling at the target size.
- Verify settings persist after restart.
- Verify start with Windows can be enabled/disabled.
- Verify start minimized opens hidden/to tray.
- Verify minimize/close-to-tray behavior.
- Verify tray left-click restores the window.
- Verify tray right-click menu actions work.
- Verify AVD scan/start/stop.
- Verify custom app add/run/edit/delete.
- Verify nginx start/stop/reload/config.
- Verify port lookup and confirmed process kill.

## Assumptions

- Target platform is Windows only.
- No tabs, no sidebar, no large dashboard.
- Keep UI compact and utilitarian.
- Tray menu is quick-access only, not a replacement for the main window.
