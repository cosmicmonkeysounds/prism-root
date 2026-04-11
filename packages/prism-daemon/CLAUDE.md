# prism-daemon

Rust library + standalone binary — the local physics engine. Transport-agnostic
kernel of composable modules assembled via a fluent builder. Runs anywhere Rust
runs (desktop via Tauri, mobile via Capacitor/FFI, headless via `prism-daemond`,
**browser via `wasm32-unknown-emscripten`**).

## Build
- `cargo build` / `cargo test` / `cargo clippy --all-targets -- -D warnings`
- `cargo fmt` before every commit
- Feature matrix: `full` (default), `mobile`, `embedded`, `wasm`
- WASM build (requires emscripten SDK + `rustup target add wasm32-unknown-emscripten`):
  `cargo build --release --target wasm32-unknown-emscripten --no-default-features --features wasm`

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
- `src/builder.rs` — `DaemonBuilder`: fluent `with_crdt/with_luau/with_build/
  with_watcher/with_vfs/with_crypto/with_module/with_initializer/with_defaults/
  build`. Equivalent of `createStudioKernel({ lensBundles, initializers })`.
- `src/kernel.rs` — `DaemonKernel`: the assembled runtime. Cheaply cloneable
  (Arc interior). Exposes `invoke`, `capabilities`, `installed_modules`,
  `doc_manager`, `watcher_manager`, `vfs_manager`, `dispose`.
- `src/doc_manager.rs` — Loro-backed CRDT service. Injectable via
  `builder.set_doc_manager(...)` so hosts can preload docs from disk.
- `src/modules/` — built-in modules, each behind a feature flag:
  - `crdt_module.rs` → `prism.crdt` → `crdt.{write,read,export,import}`
  - `luau_module.rs` → `prism.luau` → `luau.exec`
  - `build_module.rs` → `prism.build` → `build.run_step` (+ `BuildStep`,
    `BuildStepOutput`, `run_build_step` kept as free fn for hot paths)
  - `watcher_module.rs` → `prism.watcher` → `watcher.{watch,poll,stop}`
  - `vfs_module.rs` → `prism.vfs` → `vfs.{put,get,has,delete,list,stats}` —
    content-addressed blob store (SHA-256 keys, atomic write-temp+rename).
    Hosts inject a `VfsManager` rooted at the app data dir via
    `builder.set_vfs_manager(...)`; the module lazily creates one under
    the OS temp dir if no host plugged one in.
  - `crypto_module.rs` → `prism.crypto` → `crypto.{keypair,derive_public,
    shared_secret,encrypt,decrypt,random_bytes}` — X25519 ECDH +
    XChaCha20-Poly1305 AEAD + CSPRNG. Pure-Rust RustCrypto (no libsodium-sys)
    so iOS/Android/emscripten all compile without a C toolchain dep. Every
    byte field on the wire is lowercase hex.
- `src/bin/prism_daemond.rs` — standalone stdio JSON daemon binary. Proves
  the kernel runs detached from Tauri. Gated on the `cli` feature.
- `src/wasm.rs` — C-ABI adapter for the browser. Gated on the `wasm`
  feature. Exposes `prism_daemon_{create,destroy,invoke,free_string}` so
  emscripten can wrap them via `ccall`/`cwrap`. Uses a hand-rolled C ABI
  (not `wasm-bindgen`) because `mlua`'s vendored Luau only compiles on
  `wasm32-unknown-emscripten`, which is incompatible with `wasm-bindgen`'s
  `wasm32-unknown-unknown` glue. One real Luau everywhere.

## Feature Flags
| Feature    | Pulls in                                    | Why                                            |
|------------|---------------------------------------------|------------------------------------------------|
| `full`     | everything (default)                        | Desktop/server                                 |
| `mobile`   | crdt + luau + vfs + crypto                  | iOS bans process spawning; no notify; still needs E2EE + blob store |
| `embedded` | crdt                                        | Minimum viable kernel (ESP32-class)            |
| `wasm`     | crdt + luau + vfs + crypto + C-ABI          | Browser (emscripten); no notify, no spawn      |

Mobile/embedded/wasm builds don't contain the code they can't run.
Individual capabilities: `crdt`, `luau`, `build`, `watcher`, `vfs`,
`crypto`, `cli`.

## Transport-Agnostic
`kernel.invoke(name, payload)` is the single entry point. Transport adapters
are thin wrappers:

- **Tauri**: `prism-studio/src-tauri/src/{main,commands}.rs` constructs a
  `DaemonKernel` in `main()` via `DaemonBuilder::new().with_crdt().with_luau()
  .with_build().with_watcher().build()`, `.manage()`s it, and `#[tauri::command]`
  functions forward to `kernel.invoke()` or reach into `kernel.doc_manager()`
  for hot paths (CRDT byte arrays).
- **CLI**: `prism-daemond` wraps `kernel.invoke()` in a stdio JSON loop.
- **Browser (WASM)**: `src/wasm.rs` wraps `kernel.invoke()` in a C ABI.
  Cross-compile to `wasm32-unknown-emscripten`; emscripten produces
  `prism_daemon.wasm` + a `prism_daemon.js` loader; JS calls
  `Module.ccall('prism_daemon_invoke', ...)`.
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
- **60 unit tests** across registry + modules (default feature set): 7
  registry, 4 crdt, 4 luau, 11 build, 5 watcher, 13 vfs, 14 crypto,
  plus doc-manager internals.
- **50 unit tests under `--features wasm --lib`** — subset that excludes
  notify/process modules, plus the 6 `src/wasm.rs` tests that drive the
  C ABI from the host so the create/invoke/free/destroy ownership dance
  is exercised without needing an actual browser.
- **50 unit tests under `--features mobile`** — same subset as wasm but
  without the C ABI layer.
- **11 unit tests under `--features embedded`** — registry + CRDT only.
- **9 integration tests** in `tests/kernel_integration.rs` covering
  builder composition, custom modules, initializer ordering, kernel
  clone/share, dispose lifecycle, plus VFS and crypto end-to-end
  roundtrips through `kernel.invoke()`.
- **2 integration tests** in `tests/stdio_bin.rs` that spawn the
  `prism-daemond` binary as a subprocess and drive every built-in
  module through the stdio JSON loop — the CLI-transport analogue of
  the Playwright browser suite.
- **19 Playwright E2E tests** (× 2 profiles = 38 runs) in
  `e2e/wasm.spec.ts` that compile the daemon to
  `wasm32-unknown-emscripten`, load it into Chromium, and exercise
  every command (CRDT, Luau, VFS, crypto) through the real C ABI.
  Run via `pnpm test:e2e:dev` / `pnpm test:e2e:prod`.

### Full-matrix runner
`scripts/test-all.sh` orchestrates the whole gauntlet in order — host
cargo tests for every feature combo, clippy under every combo,
`cargo fmt --check`, WASM dev+prod cross-compile, Playwright dev+prod,
iOS xcframework build + C ABI symbol check, Android per-ABI cdylib
build + C ABI symbol check. Skips compose: `--skip-mobile`, `--skip-e2e`,
`--skip-wasm`.

### Mobile FFI sanity check
`scripts/build-ios.sh` produces an xcframework at
`packages/prism-capacitor-daemon/ios/Frameworks/PrismDaemon.xcframework`
with device + simulator slices; `scripts/build-android.sh` produces
per-ABI `libprism_daemon.so` files under
`packages/prism-capacitor-daemon/android/src/main/jniLibs/<abi>/`. Both
scripts are symbol-checked (`_prism_daemon_{create,destroy,invoke,
free_string}`) by the runner.
