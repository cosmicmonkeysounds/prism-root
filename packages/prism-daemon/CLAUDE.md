# prism-daemon

Rust library + standalone binary — the local physics engine. Transport-agnostic
kernel of composable modules assembled via a fluent builder. Runs anywhere Rust
runs (desktop via `prism-daemond` sidecar spawned by `prism-studio`, mobile via
`cargo-mobile2` staticlib + C ABI on iOS/Android, headless via `prism-daemond`,
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
  The single transport-agnostic entry point. Every adapter (local IPC,
  UniFFI, stdio CLI, HTTP) funnels through `kernel.invoke(name, payload)`.
- `src/module.rs` — `DaemonModule` trait. Self-registers commands into the
  builder's shared `CommandRegistry`.
- `src/initializer.rs` — `DaemonInitializer` trait + `InitializerHandle` for
  post-boot side effects. Torn down in reverse order on `dispose()`.
- `src/builder.rs` — `DaemonBuilder`: fluent `with_crdt/with_luau/with_build/
  with_watcher/with_vfs/with_crypto/with_actors/with_whisper/with_conferencing/
  with_module/with_initializer/with_defaults/build`. Equivalent of
  `createStudioKernel({ lensBundles, initializers })`.
- `src/kernel.rs` — `DaemonKernel`: the assembled runtime. Cheaply cloneable
  (Arc interior). Exposes `invoke`, `capabilities`, `installed_modules`,
  `doc_manager`, `watcher_manager`, `vfs_manager`, `actors_manager`,
  `whisper_manager`, `conferencing_manager`, `dispose`.
- `src/doc_manager.rs` — Loro-backed CRDT service. Injectable via
  `builder.set_doc_manager(...)` so hosts can preload docs from disk.
- `src/modules/` — built-in modules, each behind a feature flag:
  - `crdt_module.rs` → `prism.crdt` → `crdt.{write,read,export,import}`
  - `luau_module.rs` → `prism.luau` → `luau.exec`
  - `build_module.rs` → `prism.build` → `build.run_step` (+ `BuildStep`,
    `BuildStepOutput`, `run_build_step` kept as free fn for hot paths)
  - `watcher_module.rs` → `prism.watcher` → `watcher.{watch,poll,stop}`
  - `vfs_module.rs` + `vfs_module/s3.rs` → `prism.vfs` →
    `vfs.{put,get,has,delete,list,stats}` — content-addressed blob store
    (SHA-256 keys, atomic write-temp+rename) layered over a pluggable
    `VfsBackend` trait. The default `LocalVfsBackend` writes to disk;
    `InMemoryVfsBackend` is a test fixture; `S3VfsBackend` (feature
    `vfs-s3`) and `GcsVfsBackend` (feature `vfs-gcs`) talk to S3 / GCS via
    a hand-rolled SigV4 signer over a blocking `ureq` HTTP transport (no
    tokio runtime in the kernel hot path). Hosts inject a `VfsManager`
    via `builder.set_vfs_manager(...)`; the module lazily creates a
    local-fs one if no host plugged one in.
  - `crypto_module.rs` → `prism.crypto` → `crypto.{keypair,derive_public,
    shared_secret,encrypt,decrypt,random_bytes}` — X25519 ECDH +
    XChaCha20-Poly1305 AEAD + CSPRNG. Pure-Rust RustCrypto (no libsodium-sys)
    so iOS/Android/emscripten all compile without a C toolchain dep. Every
    byte field on the wire is lowercase hex.
  - `actors_module.rs` → `prism.actors` → `actors.{spawn,send,recv,status,
    list,stop}` — sandboxed Luau actor pool, thread-per-actor with
    inbox/outbox mailboxes. First supported actor kind is a Luau script;
    `python` / `llm_sidecar` kinds will land later behind their own
    sub-features. Depends on `luau`.
  - `debug_module.rs` → `prism.debug` → `luau.debug.{launch,
    set_breakpoints,continue,step_in,step_over,step_out,inspect,
    evaluate,terminate}` — Luau debugger with thread-per-session
    execution, interrupt-based breakpoints via `set_interrupt`, condvar
    parking for pause/resume, call stack inspection via `inspect_stack`,
    and in-session expression evaluation while paused. Depends on `luau`.
  - `whisper_module.rs` → `prism.whisper` → batch: `whisper.{load_model,
    unload_model,list_models,transcribe_pcm,transcribe_file}` + streaming:
    `whisper.{create_session,push_audio,poll_segments,close_session}` —
    local-first STT via `whisper-rs` (whisper.cpp + GGML built from source).
    PCM input must be mono f32 @ 16 kHz. Streaming sessions accumulate audio
    via `push_audio`; `poll_segments` transcribes the full buffer each call
    (whisper.cpp resets state per `full()` — the host diffs results).
    Desktop-only — needs `cmake` on PATH; excluded from all presets; opt in
    with `cargo build --features whisper`.
  - `conferencing_module.rs` → `prism.conferencing` → data channels:
    `conferencing.{create_peer,create_data_channel,create_offer,
    create_answer,set_local_description,set_remote_description,
    local_description,add_ice_candidate,send_data,recv_data,peer_state,
    list_peers,close_peer}` + audio/video tracks: `conferencing.{add_track,
    write_sample,recv_track_data,list_tracks,remove_track}` + rooms:
    `conferencing.{create_room,join_room,leave_room,room_info,list_rooms,
    broadcast_data}` — pure-Rust WebRTC via the `webrtc` crate. Tracks
    transport pre-encoded media (Opus/VP8) — the host encodes/decodes, the
    daemon transports. Rooms group peers for multi-party calls: full-mesh
    P2P for small groups, Relay SFU for larger ones. Desktop-only.
  - `admin_module.rs` → (no feature gate, always available) →
    `daemon.admin` — returns a normalised admin snapshot matching
    `@prism/admin-kit`'s `AdminSnapshot` shape (health, uptime,
    metrics, services, activity). Installed last in `with_defaults()`
    so it captures every module installed before it.
- `src/bin/prism_daemond.rs` — standalone stdio JSON daemon binary. The
  desktop sidecar spawned by `prism-studio` and the canonical headless
  entry point. Gated on the `cli` feature.
- `src/wasm.rs` — C-ABI adapter for the browser. Gated on the `wasm`
  feature. Exposes `prism_daemon_{create,destroy,invoke,free_string}` so
  emscripten can wrap them via `ccall`/`cwrap`. Uses a hand-rolled C ABI
  (not `wasm-bindgen`) because `mlua`'s vendored Luau only compiles on
  `wasm32-unknown-emscripten`, which is incompatible with `wasm-bindgen`'s
  `wasm32-unknown-unknown` glue. One real Luau everywhere.

## Feature Flags
| Feature        | Pulls in                                    | Why                                            |
|----------------|---------------------------------------------|------------------------------------------------|
| `full`         | crdt + luau + build + watcher + vfs + crypto + actors + cli (default) | Desktop/server                                 |
| `mobile`       | crdt + luau + vfs + crypto + actors         | iOS bans process spawning; no notify; still needs E2EE + blob store + on-device sidecars |
| `embedded`     | crdt                                        | Minimum viable kernel (ESP32-class)            |
| `wasm`         | crdt + luau + vfs + crypto + C-ABI          | Browser (emscripten); no notify, no spawn      |
| `whisper`        | strictly opt-in                             | whisper.cpp via `whisper-rs` — needs `cmake` on PATH; desktop only |
| `conferencing`   | strictly opt-in                             | `webrtc` crate + tokio bridge; desktop only    |
| `vfs-s3`         | vfs + ureq                                  | S3-compatible blob store backend (SigV4)       |
| `vfs-gcs`        | vfs + ureq                                  | GCS blob store via S3-interop endpoint         |
| `transport-http` | axum + tokio + tower                        | HTTP adapter: `POST /invoke/:command`          |
| `transport-grpc` | tonic + prost + tokio                       | gRPC adapter: hand-rolled `DaemonService/Invoke` |
| `transport-uniffi` | uniffi                                    | Typed Swift/Kotlin bindings                    |
| `transport-ipc`  | interprocess + postcard                     | Local IPC adapter: length-prefixed postcard frames over unix sockets / named pipes; the Slint-based Studio ↔ daemon sidecar wire (see `docs/dev/slint-migration-plan.md`) |

Mobile/embedded/wasm builds don't contain the code they can't run.
Individual capabilities: `crdt`, `luau`, `build`, `watcher`, `vfs`,
`crypto`, `actors`, `cli`. Strictly opt-in (not in any preset):
`whisper` (needs `cmake` on PATH for the whisper.cpp build), `conferencing`
(pulls the `webrtc` crate's network stack — desktop only), `vfs-s3`,
`vfs-gcs`, `transport-http`, `transport-grpc`, `transport-uniffi`,
`transport-ipc`.

## Transport-Agnostic
`kernel.invoke(name, payload)` is the single entry point. Transport adapters
are thin wrappers:

- **CLI**: `prism-daemond` wraps `kernel.invoke()` in a stdio JSON loop.
- **Browser (WASM)**: `src/wasm.rs` wraps `kernel.invoke()` in a C ABI.
  Cross-compile to `wasm32-unknown-emscripten`; emscripten produces
  `prism_daemon.wasm` + a `prism_daemon.js` loader; JS calls
  `Module.ccall('prism_daemon_invoke', ...)`.
- **HTTP (axum)**: `src/transport/http_axum.rs` — `POST /invoke/:command`,
  `GET /capabilities`, `GET /healthz`, `GET /admin` (HTML dashboard),
  `GET /admin/api/snapshot` (JSON). Feature `transport-http`. Sync
  kernel.invoke is run on the blocking pool via `spawn_blocking`.
  Admin HTML is served via `include_str!` from split template files
  (`admin_template_head.html` + `admin_template_tail.html`).
- **gRPC (tonic)**: `src/transport/grpc_tonic.rs` — hand-rolled tonic 0.12
  server (no `tonic-build`, no `protoc`). Single unary RPC
  `prism.daemon.DaemonService/Invoke` carrying JSON-as-bytes. Feature
  `transport-grpc`.
- **UniFFI**: `src/transport/uniffi_bridge.rs` — typed Swift/Kotlin
  bindings via `uniffi` proc macros. Feature `transport-uniffi`.
  `PrismDaemonHandle` object with `.invoke(command, payloadJson)`,
  `.capabilities()`, `.installedModules()`, `.dispose()`.
- **Local IPC**: `src/transport/ipc_local.rs` — length-prefixed
  `postcard` frames over `interprocess::local_socket` (abstract
  namespace on Linux, named pipes on Windows, filesystem socket in
  `std::env::temp_dir()` on macOS/BSDs). Feature `transport-ipc`.
  Synchronous server: `serve_blocking(kernel, display)` binds a
  listener and spawns one `std::thread` per accepted connection. Wire
  types are `IpcRequest` / `IpcResponse` (re-exported from the crate
  root); payloads stay JSON-encoded strings inside postcard because
  `serde_json::Value` is `#[serde(untagged)]` and postcard's
  non-self-describing format can't round-trip it. The `prism-daemond`
  binary exposes this mode via `--ipc-socket <display>`; the
  Slint-based Studio (`prism-studio/src-tauri`, name is a
  historical artefact) is the canonical client and uses it to talk to
  the daemon sidecar over this wire (see
  `docs/dev/slint-migration-plan.md`).
- **Mobile C-ABI**: `cargo-mobile2` staticlib, same C ABI as the browser
  build — the host (UIKit on iOS, Activity on Android via `winit`) calls
  `prism_daemon_{create,invoke,destroy}` directly. No webview bridge.

## Adding a New Capability
1. Create `src/modules/my_module.rs` with a struct impl'ing `DaemonModule`.
2. Add it to `src/modules/mod.rs` (feature-gated if optional).
3. Add a `with_my()` shortcut on `DaemonBuilder` (also feature-gated).
4. Register the module in `with_defaults()`.
5. Write tests: a `registers_command` assertion + an end-to-end
   `kernel.invoke()` roundtrip.

## Tests
- **78 unit tests** across registry + modules (default feature set): 7
  registry, 4 crdt, 4 luau, 6 debug, 11 build, 5 watcher, 13 vfs, 14
  crypto, 10 actors, plus doc-manager internals.
- **107 unit tests** with all new features on (`--features vfs-s3,vfs-gcs,
  transport-http,transport-grpc,transport-uniffi`): 72 default + 17 s3 +
  5 http + 7 grpc + 6 uniffi.
- **87 unit tests under `--features conferencing`** — same as default plus
  15 conferencing tests covering peer creation, data channel setup, the
  full SDP offer/answer handshake driven through `kernel.invoke()`, audio/
  video track add/list/remove lifecycle, and room create/join/leave/info/
  list with P2P mesh topology.
- **50 unit tests under `--features wasm --lib`** — subset that excludes
  notify/process modules, plus the 6 `src/wasm.rs` tests that drive the
  C ABI from the host so the create/invoke/free/destroy ownership dance
  is exercised without needing an actual browser.
- **50 unit tests under `--features mobile`** — same subset as wasm but
  without the C ABI layer.
- **11 unit tests under `--features embedded`** — registry + CRDT only.
- **`--features whisper`** — 11 whisper tests (batch command registration,
  sample-rate validation, unknown-model errors, streaming session
  create/push/poll/close lifecycle, manager pure-API surface). Compile +
  run requires `cmake` on PATH; not part of any preset, opt in explicitly.
- **9 integration tests** in `tests/kernel_integration.rs` covering
  builder composition, custom modules, initializer ordering, kernel
  clone/share, dispose lifecycle, plus VFS and crypto end-to-end
  roundtrips through `kernel.invoke()`.
- **2 integration tests** in `tests/stdio_bin.rs` that spawn the
  `prism-daemond` binary as a subprocess and drive every built-in
  module through the stdio JSON loop — the CLI-transport analogue of
  the Playwright browser suite.
- **2 integration tests** in `tests/ipc_bin.rs` (gated on
  `cli,transport-ipc`) that spawn `prism-daemond --ipc-socket` as a
  subprocess, connect over `interprocess::local_socket`, drive the
  banner + `daemon.capabilities` + `crdt.write` + unknown-command
  error paths through length-prefixed postcard frames, and confirm
  the child reaps cleanly after a kill. The spawn/supervise/kill
  proof for the Slint-Studio ↔ daemon sidecar wire.
The Playwright-driven browser E2E suite (`e2e/wasm.spec.ts`) and the
`scripts/test-all.sh` full-matrix runner were retired 2026-04-15
alongside the Hono TS relay. The C-ABI `src/wasm.rs` tests still
exercise the create/invoke/free/destroy ownership dance from Rust,
and `prism test` drives every crate through the unified
`cargo test --workspace` path. Mobile FFI sanity checks (iOS
xcframework + per-ABI Android cdylibs) will be re-added as Phase 0
spike tasks when the `cargo-mobile2` host lands.
