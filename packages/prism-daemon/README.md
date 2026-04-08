# prism-daemon

Rust library — the local physics engine for sovereign hardware. Runs inside the Tauri shell (`prism-studio/src-tauri/`) as managed state.

## What It Does

| Domain | Crate | Purpose |
|--------|-------|---------|
| CRDT | `loro` | Multi-document CRDT management. Write/read/export/import/merge. |
| Scripting | `mlua` (Lua 5.4, vendored) | Lua VM with JSON arg injection. Same scripts as browser (wasmoon). |
| File Watching | `notify` | Filesystem change detection (create/modify/remove events). |
| Build Pipeline | `std::process` + `std::fs` | Executes Studio's self-replicating `BuildStep`s: write files, spawn CLI tools (`pnpm`, `vite`, `tauri`, `cap`), capture stdout/stderr. |

## Source Layout

```
src/
  lib.rs              Library root — exports DocManager, commands
  commands/
    crdt.rs           crdt_write, crdt_read, crdt_export, crdt_import
    lua.rs            lua_exec with JSON↔Lua value conversion
    watcher.rs        filesystem watch subscriptions
    build.rs          run_build_step — executes one BuildStep from Studio's
                      BuilderManager (emit-file / run-command / invoke-ipc)
                      with path resolution against workingDir and env
                      propagation to child processes
```

## Build Pipeline — `run_build_step`

Studio's `BuilderManager` composes a deterministic `BuildPlan` from an `AppProfile` + `BuildTarget` and dispatches each step to the daemon via Tauri IPC. The `run_build_step` command is the daemon side of that loop:

```rust
pub fn run_build_step(
    step: &BuildStep,
    working_dir: &Path,
    env: &HashMap<String, String>,
) -> Result<BuildStepOutput, String>
```

- `BuildStep::EmitFile` — creates parent dirs and writes contents to `working_dir/path` (or an absolute path).
- `BuildStep::RunCommand` — spawns `command` with `args`, inheriting the plan's `env` on top of the current process env, failing on non-zero exit and returning `stdout`/`stderr`.
- `BuildStep::InvokeIpc` — reserved for cross-command chaining; currently returns an error.

The wire shape (`#[serde(tag = "kind", rename_all = "kebab-case")]`) mirrors `@prism/core/builder` exactly so plans serialized by Studio deserialize cleanly on the Rust side. Tauri's `rename_all = "camelCase"` wrapper lets JS send `workingDir` → Rust receives `working_dir`.

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
- Richer build pipeline: streaming step progress via Tauri events, artifact verification, signed-binary flows for Tauri/Capacitor targets
