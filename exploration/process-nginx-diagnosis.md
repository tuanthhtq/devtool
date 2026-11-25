# Process and Nginx Diagnosis

## Scope

- Diagnose UI freezes while searching for or terminating a process by port.
- Diagnose repeated Nginx starts when Nginx is already running.

## Findings

The port lookup originally ran `netstat` and one filtered `tasklist` command per
matching connection inside a synchronous Tauri command. The current uncommitted
changes already move that work to the blocking worker pool and reduce the lookup
to one `tasklist` call. On this machine, one full `tasklist` call takes roughly
350 ms, so keeping it off the IPC/UI thread is significant.

`kill_process` still runs synchronous `taskkill /PID <pid> /F` through
`Command::output()`. It can therefore block the Tauri IPC/UI thread until
`taskkill` exits. Afterward the frontend waits for another port refresh, making
the whole kill action feel even longer.

`start_nginx` always spawns the configured executable and immediately returns
success. It does not inspect `logs/nginx.pid`, validate whether the recorded
master PID is alive, or distinguish an already-running instance from a new
start. Redirected output also means a newly spawned Nginx process can fail after
the app has already logged `nginx started`.

Two `nginx.exe` processes are normal for the current configuration: the PID in
`logs/nginx.pid` is the master and `worker_processes 1` creates one child worker.
That master/worker pair should count as one running Nginx service.

## Implementation

- `kill_process` is asynchronous and runs `taskkill` on the blocking worker
  pool, matching the port lookup.
- Nginx start is serialized with an in-process lock, reads the configured root's
  `logs/nginx.pid`, and uses the Windows process API to verify that the PID maps
  to the configured executable before deciding that Nginx is already running.
- Stale or malformed PID files are treated as not running. A fresh start is
  polled for up to three seconds and reports an error with the Nginx error-log
  path if the process exits or never becomes ready.
- Main-window and tray starts use the same guarded command. The frontend maps
  its structured outcome to `nginx started` or `nginx already running`.
- Port lookups share an in-flight promise. A post-kill lookup waits for any
  earlier lookup and then runs again, avoiding stale rows without overlapping
  expensive system commands.

## Verification

- `cargo check` passes on the current working tree.
- `cargo clippy --all-targets -- -D warnings` passes.
- Six Rust unit tests pass, covering PID parsing, outcome serialization,
  case-insensitive path comparison, Windows executable lookup, and live-master
  detection from a PID file.
- `node --check src/main.js` passes.
- `pnpm tauri build --debug --no-bundle` passes.
- Current runtime inspection shows master PID `15144` and worker PID `6948`,
  both from `D:\Tools\nginx-1.31.2\nginx.exe`; `logs/nginx.pid` contains `15144`.
