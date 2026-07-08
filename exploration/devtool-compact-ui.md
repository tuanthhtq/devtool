# DevTool Compact UI

## Source

- Pencil design: `design/v1.pen`
- Main frame: `I9BMOk` (`DevTool Compact Window`)
- Window width: fixed at 330 px
- Window height: starts at 390 px and remains vertically resizable

## Structure

The interface is a compact dark vertical tool surface with flat sections for
Android AVDs, custom apps, Nginx, ports, and startup settings. All sections
share one background instead of using card containers. Content padding is kept
tight at 10 px, and the compact viewport scrolls when dynamic rows exceed the
available height. The native Tauri titlebar remains enabled with maximize
disabled. Equal minimum and maximum widths lock horizontal resizing while
preserving vertical resizing.

An action console sits at the bottom with a 64 px minimum height and grows with
the window. It keeps the latest 50 timestamped frontend action, status, and error
messages and scrolls to the most recent entry.

Primary actions use borderless emoji controls placed beside each section title.
Reduced section, list, and row spacing keeps the full utility surface compact.
Heading rows align title and action emojis as one left-side group instead of
pushing actions to the far edge.
App paths use a native file picker with `.exe`, `.bat`, `.cmd`, `.jar`, and
`.vbs` support, while Android SDK configuration uses a native folder picker.
Native launch routes scripts through the right Windows host, including
`wscript.exe` for `.vbs` files.

After a file is selected, its filename seeds the display name and the new row
immediately enters inline edit mode. App rows show only the display name; the
stored path remains hidden. The pencil action edits the name in place with
Save/Cancel controls, Enter/Escape shortcuts, and no file picker or path change.

Settings expose only Start Windows and Start minimized as compact checkboxes.
Closing or minimizing always hides the main window to the native system tray. A
left click restores the window, while the tray menu provides ADV, Apps, and
Nginx quick actions plus Open and Quit. Tray ADV lists AVDs without add/scan
actions, Apps lists saved apps without add/edit actions, and Nginx exposes only
Start, Stop, Reload, and Edit.

## Implementation

- `src/index.html`: semantic controls and action console.
- `src/styles.css`: Pencil-aligned compact styling.
- `src/main.js`: rendering, command invocation, and action logging.
- `src-tauri/src/lib.rs`: Tauri commands plus native tray lifecycle behavior.
- `src-tauri/tauri.conf.json`: fixed compact window with native decorations.
