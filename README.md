# DevTool

A compact Windows developer utility built with **Tauri v2**, **Rust**, and **vanilla HTML/CSS/JS**. DevTool keeps everyday local development controls on one lightweight screen and lives quietly in the Windows system tray.

<p align="left">
  <img src="src-tauri/icons/128x128.png" width="72" height="72" alt="DevTool icon" />
</p>

## Features

- **Android AVD Manager**
  - Scan, start, stop, create, and delete Android Virtual Devices.
  - Auto-detects Android SDK (`ANDROID_HOME`, `ANDROID_SDK_ROOT`, `%LOCALAPPDATA%\Android\Sdk`, or custom path), `cmdline-tools`, Android Studio's bundled `jbr`, standalone JDK 17+, and the native `android.exe` CLI.
  - Choose between **Root** (`google_apis` / `default` `userdebug` images with `adb root`) and **No Root** (`google_apis_playstore` production images) across architectures (`x86_64`, `arm64-v8a`), with automatic system image download when not yet installed.
- **Custom App Launcher**
  - Save, inline-rename, launch, and remove `.exe`, `.bat`, `.cmd`, `.jar`, and `.vbs` tools from the main window or system tray.
- **Nginx Controller**
  - Start (with live master PID verification), stop, reload, and open `conf/nginx.conf` directly.
- **Port & Process Inspector**
  - Inspect processes listening on or using a TCP/UDP port (`netstat -ano` + `tasklist`) and terminate processes by PID with confirmation.
- **Secret Generator & Password Hasher**
  - Generate cryptographically random **API Tokens** (`encodeURIComponent(base64)`) and **Rand Pwd** (`base64url`) directly into the native Windows clipboard with configurable byte length (`1..256`, default `32`).
  - Hash user-supplied passwords via **scrypt** (`N=16384, r=8, p=1`, default `16`-byte random hex salt, `64`-byte key length -> `scrypt:<salt_hex>:<hash_hex>`), `pbkdf2-sha256`, `pbkdf2-sha512`, or `sha256`.
- **Windows Tray & Startup**
  - Close or minimize to the system tray; left-click restores the window, right-click opens quick actions for AVDs, Custom Apps, and Nginx.
  - Optional **Start with Windows** (`HKCU\...\Run`) and **Start minimized**.

## Prerequisites

- **Windows 10 / 11** (WebView2 runtime included on modern Windows)
- **Node.js** >= 20 and **pnpm**
- **Rust** stable toolchain (MSVC target on Windows)
- *(Optional)* Android SDK / Android Studio for AVD management, Nginx for local reverse-proxy controls

## Development

Install dependencies and launch in development mode:

```powershell
pnpm install
pnpm tauri dev
```

Run backend unit tests:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml
```

## Build Installer

Build optimized Windows installers (`.msi` and NSIS `-setup.exe`):

```powershell
pnpm tauri build
```

Generated bundles are written under:
- `src-tauri/target/release/bundle/msi/`
- `src-tauri/target/release/bundle/nsis/`

## Releases (CI/CD)

Pushing a version tag matching `v*` triggers [`.github/workflows/release.yml`](.github/workflows/release.yml) on `windows-latest` to run tests, build `.msi` and `-setup.exe` installers, and publish a GitHub Release automatically:

```powershell
# 1. Bump version in package.json, src-tauri/Cargo.toml, and src-tauri/tauri.conf.json
# 2. Commit, tag, and push
git tag v0.2.0
git push origin main --tags
```

## Configuration

User settings and saved custom apps are stored locally at:

```text
%APPDATA%\devtool\settings.json
```

## Project Structure

```text
src/
  index.html      Compact single-screen UI markup
  styles.css      Tokenized dark theme styles
  main.js         UI state, Tauri command calls, and toast notifications
  icons.js        Zero-dependency inline Lucide SVG icons
  secrets.js      CSPRNG API token and password generators
src-tauri/
  src/lib.rs      Tauri commands, Win32 clipboard/process helpers, AVD/Nginx/Port logic, tray menu
  tauri.conf.json Window dimensions and bundle configuration
```
