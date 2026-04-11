# prism-daemon

Rust library + standalone binary — the local physics engine for sovereign hardware. A transport-agnostic kernel of composable modules (CRDT, Luau, build, watcher) assembled via a fluent builder, so the same engine can run behind Tauri on desktop, behind a UniFFI bridge on iOS/Android, as a headless stdio daemon on a server, **or directly in the browser via WebAssembly**.

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
    .with_luau()
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
| `prism.luau`     | `luau`    | `luau.exec`                                          |
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
| `full`    | yes     | Shorthand for `crdt + luau + build + watcher + cli` |
| `mobile`  | no      | `crdt + luau` only — no process spawning, no notify |
| `embedded`| no      | `crdt` only                                         |
| `wasm`    | no      | `crdt + luau` + C-ABI adapter (`wasm32-unknown-emscripten`) |
| `crdt`    | via `full` | Loro-backed CRDT service                         |
| `luau`    | via `full` | mlua (luau + vendored)                           |
| `build`   | via `full` | `std::process`-based build step executor       |
| `watcher` | via `full` | `notify`-based filesystem watcher                |
| `cli`     | via `full` | Enables the standalone `prism-daemond` binary    |

Mobile/embedded/wasm builds opt out at the feature level so the shipped binary literally does not contain the code it can't run.

## Browser / WebAssembly

The same kernel that Studio embeds over Tauri IPC also runs in Chrome, Firefox, and Safari as a plain WebAssembly module. Both CRDT and Luau travel along — the Luau runtime is still real, C++-vendored Luau via `mlua`, not a JavaScript interpreter.

Why `wasm32-unknown-emscripten` instead of `wasm32-unknown-unknown` + `wasm-bindgen`? Because Luau's C++ source needs a libc/libcxx, and emscripten is the only WASM triple that provides them. There's no production-ready pure-Rust Luau VM to swap in, so going through emscripten keeps one real Luau everywhere — desktop, mobile, browser.

The adapter in [`src/wasm.rs`](src/wasm.rs) exposes the kernel through a small C ABI that emscripten wraps automatically via `ccall`/`cwrap`:

```c
DaemonKernel*   prism_daemon_create(void);
void            prism_daemon_destroy(DaemonKernel*);
char*           prism_daemon_invoke(DaemonKernel*, const char* name, const char* payload_json);
void            prism_daemon_free_string(char*);
```

### Build

```bash
# one-time setup
source /path/to/emsdk/emsdk_env.sh      # activate emscripten
rustup target add wasm32-unknown-emscripten

cargo build --release \
  --target wasm32-unknown-emscripten \
  --no-default-features \
  --features wasm
```

Emscripten produces `prism_daemon.wasm` + a tiny `prism_daemon.js` loader.

### Use from JavaScript

```js
import createModule from './prism_daemon.js';

const Module = await createModule();
const invoke = Module.cwrap(
  'prism_daemon_invoke', 'number',
  ['number', 'string', 'string'],
);
const freeString = Module.cwrap('prism_daemon_free_string', null, ['number']);

const kernel = Module.ccall('prism_daemon_create', 'number', [], []);

// Round-trip through Luau:
const ptr = invoke(kernel, 'luau.exec', JSON.stringify({ script: 'return 21 * 2' }));
const response = JSON.parse(Module.UTF8ToString(ptr));
freeString(ptr);
console.log(response); // { ok: true, result: 42 }
```

The response envelope is always `{ ok: true, result }` or `{ ok: false, error }`. Two reserved commands — `daemon.capabilities` and `daemon.modules` — are available alongside every registered module command.

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
        .with_crdt().with_luau().with_build().with_watcher()
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
cargo build --release --target wasm32-unknown-emscripten \
  --no-default-features --features wasm       # browser
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt
```

## Roadmap

Module-shaped so new capabilities plug in without touching the core:
- `prism.vfs` — content-addressed blob storage module
- `prism.hardware.midi` / `.dmx` / `.osc` — hardware protocol bridges
- `prism.canto` — audio engine (lock-free Rust signal graph)
- `prism.actors` — sandboxed Luau actor execution
- Transport adapters: UniFFI (iOS/Android), axum HTTP, tonic gRPC
