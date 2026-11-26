# DevTool Compact UI

## Source

- Pencil design: `design/v1.pen`
- Main frame: `I9BMOk` (`DevTool Compact Window`)
- Window width: starts at 350 px (min 320 px, max 460 px)
- Window height: dynamically auto-fits `.app-shell` content height via `ResizeObserver` + `set_content_height` (clamped between 220 px and screen work area)

## Structure

The interface is a compact dark vertical utility window on a single flat
surface (`#0c1016`) separated by hairline horizontal dividers for Android AVD,
Apps, Nginx, Ports, Secrets (`API Token` + `Rand Pwd`), Password Hash
(`scrypt` / `pbkdf2` / `sha256`), and Settings (no nested cards). Section headers
pair subtle uppercase labels on the left with right-aligned Lucide inline SVG
icon buttons on the right. There is no permanent bottom log console; transient
status messages appear as an auto-dismissing toast while errors use
`role="alert"` and persist until dismissed by button or `Escape`.
App paths use a native file picker with `.exe`, `.bat`, `.cmd`, `.jar`, and
`.vbs` support, while Android SDK configuration uses a native folder picker.
Native launch routes scripts through the right Windows host, including
`wscript.exe` for `.vbs` files.

After a file is selected, its filename seeds the display name and the new row
immediately enters inline edit mode. App rows show the display name (with the
full path in the tooltip). The edit action opens an inline form for editing both
the display name and executable path (including a native Browse picker), with
Save/Cancel controls and Enter/Escape shortcuts.

Settings expose only Start Windows and Start minimized as compact checkboxes.
Closing or minimizing always hides the main window to the native system tray. A
left click restores the window, while the tray menu provides ADV, Apps, and
Nginx quick actions plus Open and Quit. Tray ADV lists AVDs without add/scan
actions, Apps lists saved apps without add/edit actions, and Nginx exposes only
Start, Stop, Reload, and Edit.

## Implementation

- `src/index.html`: flat compact controls (AVD create/delete, Apps, Nginx, 3x wider Port input, Secrets with configurable byte length) and floating toast container.
- `src/styles.css`: tokenized compact dark styling and semantic icon button states.
- `src/icons.js`: zero-dependency inline Lucide-style SVG icon definitions and DOM hydration.
- `src/secrets.js`: CSPRNG API token (`encodeURIComponent(base64)`) and password (`base64url`) generators with configurable byte length (`1..256`, default `32`).
- `src/main.js`: rendering, AVD inline creation with Root/No-Root & ABI selection + auto-download, command invocation, clipboard copy, and toast feedback.
- `src-tauri/src/lib.rs`: Tauri commands (including Android SDK/JBR/CLI auto-detection, system image download, AVD create/delete, and Windows clipboard write) plus native tray lifecycle behavior.
- `src-tauri/tauri.conf.json`: fixed compact window with native decorations.

## Application icons

The app, Tauri window, taskbar, tray, and installer icons use the transparent
`>_` developer emblem generated as a 1024x1024 RGBA PNG (centered within
880x880). `pnpm tauri icon` generates the checked-in assets under
`src-tauri/icons`; `src-tauri/tauri.conf.json` bundles the 32x32, 128x128,
128x128@2x, ICNS, and ICO outputs.
