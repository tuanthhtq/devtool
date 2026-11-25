# Windows Process Handling

## Scope

- Start an Android Virtual Device without opening a console window.
- Keep the Tauri window responsive while resolving the process bound to a port.

## Findings

`start_avd` spawned `emulator.exe` with redirected standard streams, but stream
redirection does not suppress Windows console allocation. The spawned process
needs the Windows `CREATE_NO_WINDOW` creation flag.

`list_processes_by_port` was a synchronous Tauri command. It ran `netstat` and
then launched a separate filtered `tasklist` command for every matching row, so
the native UI thread remained blocked until every child process completed.

## Implementation

- `src-tauri/src/lib.rs`: apply `CREATE_NO_WINDOW` when spawning an AVD.
- `src-tauri/src/lib.rs`: make port lookup asynchronous and move its blocking
  process calls to Tauri's blocking worker pool.
- `src-tauri/src/lib.rs`: load the Windows task list once and map all matching
  PIDs from that single result.
- `src/main.js`: expose a busy state on the port form and prevent overlapping
  lookups while leaving the rest of the application interactive.
