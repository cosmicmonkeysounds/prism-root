# prism-daemon

Rust library — the local physics engine for sovereign hardware. Runs inside the Tauri shell (`prism-studio/src-tauri/`) as managed state.

## What It Does

| Domain | Crate | Purpose |
|--------|-------|---------|
| CRDT | `loro` | Multi-document CRDT management. Write/read/export/import/merge. |
| Scripting | `mlua` (Lua 5.4, vendored) | Lua VM with JSON arg injection. Same scripts as browser (wasmoon). |
| File Watching | `notify` | Filesystem change detection (create/modify/remove events). |

## Source Layout

```
src/
  lib.rs              Library root — exports DocManager, commands
  commands/
    crdt.rs           crdt_write, crdt_read, crdt_export, crdt_import
    lua.rs            lua_exec with JSON↔Lua value conversion
```

## Usage

The daemon is not a standalone binary — it's a library consumed by Tauri:

```rust
// In prism-studio/src-tauri/main.rs
use prism_daemon::DocManager;

fn main() {
    tauri::Builder::default()
        .manage(Mutex::new(DocManager::new()))
        .invoke_handler(tauri::generate_handler![...])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

## Build

```bash
cargo build         # Build
cargo test          # Unit + integration tests
cargo clippy        # Lint
cargo fmt           # Format (run before every commit)
```

## Future

The daemon will grow to include:
- VFS operations (content-addressed blob storage on local disk)
- Hardware protocols (MIDI, DMX, OSC)
- Audio engine (Canto — lock-free Rust signal graph)
- Actor execution (Lua VMs with capability sandboxing)
