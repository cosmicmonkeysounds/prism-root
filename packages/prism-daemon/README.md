# prism-daemon

Rust library + standalone binary — the local physics engine for sovereign hardware. A transport-agnostic kernel of composable modules (CRDT, Lua, build, watcher) assembled via a fluent builder, so the same engine can run behind Tauri on desktop, behind a UniFFI bridge on iOS/Android, or as a headless stdio daemon on a server.

## Paradigm

Modelled one-to-one on Studio's self-replicating kernel pattern:

| Studio (TS)            | Daemon (Rust)           |
|------------------------|-------------------------|
| `createStudioKernel`   | `DaemonBuilder::build`  |
| `LensBundle`           | `DaemonModule`          |
| `StudioInitializer`    | `DaemonInitializer`     |
| `LensRegistry`         | `CommandRegistry`       |
| `kernel.dispose()`     | `kernel.dispose()`      |

Modules self-register JSON-in/JSON-out handlers into a shared registry. Every transport adapter (Tauri `#[command]`, a mobile FFI, the stdio CLI, a future HTTP/gRPC shim) funnels through the single `kernel.invoke(name, payload)` entry point.

## Quick Start

```rust
use prism_daemon::DaemonBuilder;
use serde_json::json;

let kernel = DaemonBuilder::new()
    .with_crdt()
    .with_lua()
    .with_build()
    .with_watcher()
    .build()
    .unwrap();

kernel.invoke(
    "crdt.write",
    json!({ "docId": "notes", "key": "title", "value": "Hello" }),
).unwrap();

let caps = kernel.capabilities(); // every registered command name
let modules = kernel.installed_modules(); // install order
```

`DaemonBuilder::new().with_defaults()` installs every module the current feature set allows — the daemon equivalent of `createBuiltinLensBundles()`.

## Built-In Modules

| Module ID        | Feature   | Commands registered                                  |
|------------------|-----------|------------------------------------------------------|
| `prism.crdt`     | `crdt`    | `crdt.write`, `crdt.read`, `crdt.export`, `crdt.import` |
| `prism.lua`      | `lua`     | `lua.exec`                                           |
| `prism.build`    | `build`   | `build.run_step`                                     |
| `prism.watcher`  | `watcher` | `watcher.watch`, `watcher.poll`, `watcher.stop`      |

Hot paths still have direct Rust access: `kernel.doc_manager()` / `kernel.watcher_manager()` hand back the shared `Arc<…>` for transports (like Tauri's `Vec<u8>` → `number[]` bridging) that want to skip JSON round-tripping.

## Custom Modules & Initializers

Anything implementing [`DaemonModule`] can be plugged in via `builder.with_module(…)`:

```rust
use prism_daemon::{CommandError, DaemonBuilder, DaemonModule};

struct MyModule;

impl DaemonModule for MyModule {
    fn id(&self) -> &str { "example.hello" }

    fn install(&self, builder: &mut DaemonBuilder) -> Result<(), CommandError> {
        builder.registry().register("hello.say", |payload| {
            Ok(serde_json::json!({ "echo": payload }))
        })?;
        Ok(())
    }
}

let kernel = DaemonBuilder::new().with_module(MyModule).build().unwrap();
```

Initializers are post-boot side-effect hooks that can call `kernel.invoke(...)` to seed state. They run in install order and tear down in reverse on `dispose()`.

## Feature Flags

| Feature   | Default | Purpose                                            |
|-----------|---------|----------------------------------------------------|
| `full`    | yes     | Shorthand for `crdt + lua + build + watcher + cli` |
| `mobile`  | no      | `crdt + lua` only — no process spawning, no notify |
| `embedded`| no      | `crdt` only                                        |
| `crdt`    | via `full` | Loro-backed CRDT service                        |
| `lua`     | via `full` | mlua (lua54 + vendored)                         |
| `build`   | via `full` | `std::process`-based build step executor       |
| `watcher` | via `full` | `notify`-based filesystem watcher                |
| `cli`     | via `full` | Enables the standalone `prism-daemond` binary    |

Mobile/embedded builds opt out at the feature level so the shipped binary literally does not contain the code it can't run.

## Standalone Binary — `prism-daemond`

Proof that the kernel runs detached from Tauri: a minimal stdio-JSON loop that demonstrates cross-platform execution. Each line on stdin is a JSON request; each line on stdout is the reply.

```bash
cargo run --bin prism-daemond
> { "id": 1, "command": "daemon.capabilities" }
< { "id": 1, "ok": true, "result": { "commands": [...] } }
```

Two introspection commands (`daemon.capabilities`, `daemon.modules`) sit alongside whatever module-contributed commands the current feature set loaded.

## Tauri Integration

Studio's Tauri shell constructs the kernel exactly like any other host:

```rust
let kernel: Arc<DaemonKernel> = Arc::new(
    DaemonBuilder::new()
        .with_crdt().with_lua().with_build().with_watcher()
        .build().unwrap(),
);

tauri::Builder::default()
    .manage(kernel)
    // ... tauri commands forward to kernel.invoke() or reach into
    // kernel.doc_manager() for hot paths
```

## Build

```bash
cargo build                                 # default (full)
cargo build --no-default-features --features mobile
cargo build --no-default-features --features embedded
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt
```

## Roadmap

Module-shaped so new capabilities plug in without touching the core:
- `prism.vfs` — content-addressed blob storage module
- `prism.hardware.midi` / `.dmx` / `.osc` — hardware protocol bridges
- `prism.canto` — audio engine (lock-free Rust signal graph)
- `prism.actors` — sandboxed Lua actor execution
- Transport adapters: UniFFI (iOS/Android), axum HTTP, tonic gRPC
