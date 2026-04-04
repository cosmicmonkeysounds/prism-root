# prism-daemon

Rust library — the local physics engine for CRDT, Lua, VFS, and hardware.

## Build
- `cargo build` / `cargo test` / `cargo clippy`
- `cargo fmt` before every commit

## Architecture
- Loro CRDT for document merging
- mlua (lua54 + vendored) for scripting
- Commands in `src/commands/` — one file per domain
- Used as a dependency by prism-studio's Tauri shell
