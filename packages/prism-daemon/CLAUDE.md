# prism-daemon

Rust library + standalone binary — the local physics engine. Transport-agnostic
kernel of composable modules assembled via a fluent builder. Runs anywhere Rust
runs (desktop via Tauri, mobile via Capacitor/FFI, headless via `prism-daemond`).

## Build
- `cargo build` / `cargo test` / `cargo clippy --all-targets -- -D warnings`
- `cargo fmt` before every commit
- Feature matrix: `full` (default), `mobile`, `embedded`

## Architecture
Mirrors Studio's `createStudioKernel` / `LensBundle` / `StudioInitializer`
paradigm, ported to Rust:

- `src/registry.rs` — `CommandRegistry`: name → `Fn(JsonValue) -> Result<JsonValue>`.
  The single transport-agnostic entry point. Every adapter (Tauri command,
  UniFFI, stdio CLI, HTTP) funnels through `kernel.invoke(name, payload)`.
- `src/module.rs` — `DaemonModule` trait. Self-registers commands into the
  builder's shared `CommandRegistry`.
- `src/initializer.rs` — `DaemonInitializer` trait + `InitializerHandle` for
  post-boot side effects. Torn down in reverse order on `dispose()`.
- `src/builder.rs` — `DaemonBuilder`: fluent `with_crdt/with_lua/with_build/
  with_watcher/with_module/with_initializer/with_defaults/build`. Equivalent
  of `createStudioKernel({ lensBundles, initializers })`.
- `src/kernel.rs` — `DaemonKernel`: the assembled runtime. Cheaply cloneable
  (Arc interior). Exposes `invoke`, `capabilities`, `installed_modules`,
  `doc_manager`, `watcher_manager`, `dispose`.
- `src/doc_manager.rs` — Loro-backed CRDT service. Injectable via
  `builder.set_doc_manager(...)` so hosts can preload docs from disk.
- `src/modules/` — built-in modules, each behind a feature flag:
  - `crdt_module.rs` → `prism.crdt` → `crdt.{write,read,export,import}`
  - `lua_module.rs` → `prism.lua` → `lua.exec`
  - `build_module.rs` → `prism.build` → `build.run_step` (+ `BuildStep`,
    `BuildStepOutput`, `run_build_step` kept as free fn for hot paths)
  - `watcher_module.rs` → `prism.watcher` → `watcher.{watch,poll,stop}`
- `src/bin/prism_daemond.rs` — standalone stdio JSON daemon binary. Proves
  the kernel runs detached from Tauri. Gated on the `cli` feature.

## Feature Flags
| Feature    | Pulls in               | Why                                       |
|------------|------------------------|-------------------------------------------|
| `full`     | everything (default)   | Desktop/server                            |
| `mobile`   | crdt + lua             | iOS bans process spawning; no notify      |
| `embedded` | crdt                   | Minimum viable kernel                     |

Mobile/embedded builds don't contain the code they can't run.

## Transport-Agnostic
`kernel.invoke(name, payload)` is the single entry point. Transport adapters
are thin wrappers:

- **Tauri**: `prism-studio/src-tauri/src/{main,commands}.rs` constructs a
  `DaemonKernel` in `main()` via `DaemonBuilder::new().with_crdt().with_lua()
  .with_build().with_watcher().build()`, `.manage()`s it, and `#[tauri::command]`
  functions forward to `kernel.invoke()` or reach into `kernel.doc_manager()`
  for hot paths (CRDT byte arrays).
- **CLI**: `prism-daemond` wraps `kernel.invoke()` in a stdio JSON loop.
- **Mobile / HTTP / gRPC**: follow the same pattern — build the kernel, wrap
  `invoke()` in whatever the platform expects.

## Adding a New Capability
1. Create `src/modules/my_module.rs` with a struct impl'ing `DaemonModule`.
2. Add it to `src/modules/mod.rs` (feature-gated if optional).
3. Add a `with_my()` shortcut on `DaemonBuilder` (also feature-gated).
4. Register the module in `with_defaults()`.
5. Write tests: a `registers_command` assertion + an end-to-end
   `kernel.invoke()` roundtrip.

## Tests
- 33 unit tests across registry + modules.
- 7 integration tests in `tests/kernel_integration.rs` covering builder
  composition, custom modules, initializer ordering, kernel clone/share,
  dispose lifecycle.
