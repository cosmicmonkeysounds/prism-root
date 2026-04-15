# prism-studio

The packaged Prism Studio desktop shell. Thin launcher around
`prism-shell` — Slint owns windowing/layout/rendering via the
library's `Shell::run()`, and Studio's only extra job is spawning
the `prism-daemond` sibling process over `interprocess` and holding
its handle for the lifetime of the event loop.

> **Canonical location:** all Rust lives here in `src-tauri/`. The
> directory name is a historical artefact from the pre-Option-C
> spike; renaming it to plain `src/` is a cleanup followup but has
> no functional impact. The parent `packages/prism-studio/` directory
> is a pnpm-workspace anchor only — it intentionally has no
> `CLAUDE.md` and no frontend source tree.

## Build & Test
- `prism build --target studio` — `cargo build -p prism-studio
  --release` under the hood. Phase 5 will add `cargo-packager` as
  the bundling step; today this just produces a bare binary.
- `prism dev studio` — `cargo run -p prism-studio`. Used by
  `prism dev all` as one of the supervised children.
- `cargo build -p prism-studio` — plain cargo path.

## Architecture
- `src/main.rs` is ~30 lines: spawn the daemon sidecar (best-effort,
  log and continue on failure), build a `prism_shell::Shell`, and
  call `Shell::run()`. Slint drives the winit event loop from
  there; every panel, every lens, and every pointer/keyboard event
  flows through the shell library the same way the standalone
  `prism-shell` dev binary does.
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
  Phase 5.

## §4.5 history — resolved 2026-04-15 by Clay → Slint pivot
Phase 0 went through two abandoned attempts before landing on
today's shape. Option B (Tauri 2 "no-webview" via the `unstable`
feature's `tauri::window::WindowBuilder`) landed 2026-04-14 but
fell over when we tried to wire raw pointer input through
`RunEvent::WindowEvent`. `tauri-runtime-wry-2.10.1/src/lib.rs:552`
unconditionally drops every tao event except
`Resized`/`Moved`/`Destroyed`/`ScaleFactorChanged`/`Focused`/
`ThemeChanged` before it reaches the user callback —
`CursorMoved`, `MouseInput`, `MouseWheel`, and `KeyboardInput` are
all thrown away on the assumption that a webview is handling them.
Option C (bare `tao` + `wgpu` + hand-vendored `UiRenderer` + Clay)
then landed on 2026-04-15 as a stopgap, but the `clay-layout` +
wgpu pipeline it relied on was itself retired the same day in the
Clay → Slint pivot (see `docs/dev/slint-migration-plan.md`).

The final resolution: Slint owns windowing + layout + rendering via
its native winit + femtovg backend, `prism-shell` exposes a thin
`Shell::run()` entry point, and Studio is a ~30-line launcher on
top.

## Dependencies
- `prism-shell` (path, `features = ["native"]`) — the UI tree + the
  Slint event loop Studio runs.
- `prism-core` (path) — shared types.
- `slint` (workspace) — only used for `slint::PlatformError` at the
  `main` return type; the real Slint invocation lives in the shell
  library.
- `prism-daemon` (path, `features = ["full", "transport-ipc"]`) —
  kept on the path so dev builds can spawn the in-tree binary.
  Not a library link.
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
