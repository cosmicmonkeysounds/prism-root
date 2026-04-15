# prism-studio

The packaged Prism Studio desktop shell. Pure `tao` + `wgpu` +
`prism-shell`, with `prism-daemon` spawned as a sibling process
over `interprocess`. No Tauri, no wry, no webview anywhere in
the dep tree.

> **Canonical location:** all Rust lives here in `src-tauri/`. The
> directory name is a historical artefact from the pre-2026-04-15
> Option B spike; renaming it to plain `src/` is a cleanup followup
> but has no functional impact. The parent `packages/prism-studio/`
> directory is a pnpm-workspace anchor only — it intentionally has
> no `CLAUDE.md` and no frontend source tree.

## Build & Test
- `prism build --target studio` — `cargo build -p prism-studio
  --release` under the hood. Phase 5 will add `cargo-packager` as
  the bundling step; today this just produces a bare binary.
- `prism dev studio` — `cargo run -p prism-studio`. Used by
  `prism dev all` as one of the supervised children.
- `cargo build -p prism-studio` — plain cargo path.

## Architecture
- `src/main.rs` owns a bare `tao::EventLoop`. On startup it spawns
  the `prism-daemond` sidecar, builds a single `tao::Window` (no
  webview), constructs `GraphicsContext` + `UiRenderer` + `Clay`
  from `prism-shell`, installs a glyphon-backed measure-text
  callback, and drives `event_loop.run(...)` into the same
  per-frame render pump the `prism-shell` dev bin uses.
- `prism-shell` is embedded as a library
  (`default-features = false, features = ["native", "clay"]`).
  Studio owns the `tao::EventLoop`; the shell owns the UI tree,
  the wgpu pipeline, and Clay layout state.
- Input wiring mirrors `prism-shell/src/bin/native.rs` exactly:
  `CursorMoved` → `PointerMove`, `MouseInput` → `PointerDown/Up`
  (Primary/Secondary/Middle), `MouseWheel` →
  `Wheel { dx, dy }` (line deltas pre-scaled by 18.0), and
  `Resized` / `ScaleFactorChanged` fan out to the renderer, Clay
  layout dims, and `InputEvent::Resize`. `input::pump_clay` runs
  exactly once per `RedrawRequested` tick before `render_app`.
- **Sidecar**: `prism-daemon` is spawned as a sibling process, not
  linked as a library. IPC rides `interprocess` (unix sockets /
  named pipes behind one trait) with length-prefixed `postcard`
  frames per §4.5. Resolved end-to-end by Phase 0 spike #6
  (2026-04-14) — spawn, connect, roundtrip, and kill/reap all
  validated by `tests/ipc_bin.rs` in `prism-daemon`.
- `src/sidecar.rs` — daemon sidecar lifecycle + IPC client.
  `spawn_dev()` locates the `prism-daemond` binary sitting next to
  `prism-studio` in `target/<profile>/` (via
  `std::env::current_exe`), spawns it with `--ipc-socket
  prism-daemon-<pid>.sock`, retries `connect_client` for up to 2 s
  while the server thread comes up, and reads the handshake banner
  before returning a `DaemonSidecar { child, stream, next_id,
  socket_name }`. `DaemonSidecar::invoke` sends an `IpcRequest`,
  reads an `IpcResponse`, asserts the id matches, and parses the
  JSON result. Its `Drop` impl best-effort-kills + waits the child
  so the supervise/kill path runs even on panic. The packaged
  `cargo-packager` path (shipping a signed `prism-daemond-<triple>`
  binary next to the Studio bundle as a resource) is scheduled for
  Phase 5; Phase 0 only had to prove the IPC transport works.

## §4.5 history — resolved 2026-04-15 by Phase 0 spike #5 retry
The original Option B (Tauri 2 "no-webview" via the `unstable`
feature's `tauri::window::WindowBuilder`) landed 2026-04-14 but
fell over the moment we tried to wire raw pointer input through
`RunEvent::WindowEvent`. `tauri-runtime-wry-2.10.1/src/lib.rs:552`
unconditionally drops every tao event except
`Resized`/`Moved`/`Destroyed`/`ScaleFactorChanged`/`Focused`/
`ThemeChanged` before it reaches the user callback —
`CursorMoved`, `MouseInput`, `MouseWheel`, and `KeyboardInput` are
all thrown away on the assumption that a webview is handling them.
Combined with the tao version split the Tauri dep forces
(prism-shell on `tao 0.30`, `tauri-runtime-wry` dragging in
`tao 0.34`), there was no clean way to intercept raw input.

Option C from the plan is now live: bare `tao` for windowing,
`wgpu` for rendering, `prism-shell` for the UI tree, and
`cargo-packager` (Phase 5) for the installer layer. Auto-updates
come from `self_update` or similar in Phase 5. The decision log
entry in `docs/dev/clay-migration-plan.md` was updated on
2026-04-15.

## Dependencies
- `prism-shell` (path, `features = ["native", "clay"]`) — the UI
  tree plus the wgpu render stack Studio drives into the tao window.
- `prism-core` (path) — shared types.
- `prism-daemon` (path, `features = ["full", "transport-ipc"]`) —
  kept on the path so dev builds can spawn the in-tree binary.
  Not a library link.
- `tao` (workspace) — windowing + event loop. Single version
  across the whole workspace now that tauri/wry are gone.
- `clay-layout`, `raw-window-handle` — imported from the same
  workspace versions `prism-shell/src/bin/native.rs` uses, so
  both shells share one render pipeline.
- `interprocess`, `postcard` — sidecar IPC.

## Phase 5 TODOs
- `cargo-packager` config (icons, bundle IDs, signing identity,
  `.dmg`/`.msi`/`.AppImage`/`.deb` targets).
- `self_update` or an equivalent auto-updater against a static
  release feed.
- `tray-icon`, `notify-rust`, `rfd`, `arboard`, `keyring` wiring
  where the old Tauri plugin system would have handled it.
- Rename `src-tauri/` → `src/` and drop the vestigial directory
  layout.
