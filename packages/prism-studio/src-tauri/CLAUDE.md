# prism-studio (src-tauri)

The packaged Prism Studio desktop shell. Tauri 2 app that uses the
Tauri runtime for **packaging / signing / auto-update / sidecars** —
everything inside the window is `prism-shell` rendering Clay commands
through wgpu.

> **Canonical location:** all Rust lives here in `src-tauri/`. The
> parent `packages/prism-studio/` directory is a pnpm-workspace
> anchor only — it intentionally has no `CLAUDE.md` and no frontend
> source tree. Don't recreate the parent `CLAUDE.md` / `README.md`
> that used to live there.

## Build & Test
- `prism build --target studio` — `cargo tauri build --config
  packages/prism-studio/src-tauri/tauri.conf.json` under the hood.
- `prism dev studio` — `cargo tauri dev` with the same config. Used
  by `prism dev all` as one of the supervised children.
- `cargo build -p prism-studio` — plain cargo path if you're
  iterating without Tauri packaging.

## Architecture
- `tauri::Builder` bootstraps the app and spawns the daemon sidecar.
- `prism-shell` is embedded as a library
  (`default-features = false, features = ["native", "clay"]`). Studio
  owns the Tauri event loop; the shell owns the UI tree, the wgpu
  pipeline, and Clay layout state.
- **Sidecar**: `prism-daemon` is spawned as a sidecar process (not
  linked as a library). IPC rides `interprocess` (unix sockets /
  named pipes behind one trait) with length-prefixed `postcard`
  frames per §4.5.
- `src/main.rs` — builds the app with `tauri::Builder::default()
  .setup(|_app| { sidecar::spawn_dev(); Ok(()) })
  .build(generate_context!())`, then calls `app.run(|handle, event|
  { ... })`. On `RunEvent::Ready` it creates a *pure tao* window via
  `tauri::window::WindowBuilder::new(handle, "main").build()?` (no
  webview attached), constructs `GraphicsContext` + `UiRenderer` +
  `Clay`, and installs a glyphon-backed measure-text callback
  through `Rc<RefCell<UiRenderer>>`. Frame pumping hangs off
  `RunEvent::MainEventsCleared` and the `WindowEvent::Resized` /
  `ScaleFactorChanged` branches. This mirrors
  `prism-shell/src/bin/native.rs`, so the packaged shell and the dev
  bin share the exact same render path.
- `src/sidecar.rs` — dev-mode sidecar spawn. In dev this falls back
  to the in-tree `cargo run -p prism-daemon` binary so the tree
  doesn't need a packaging step to hot-iterate.

## `wry` status — resolved 2026-04-14 by Phase 0 spike #5
The root `CLAUDE.md` says "wry is not loaded". Taken literally that
was always going to be a lie (Tauri's only shipping `Runtime` impl
is `tauri-runtime-wry`); the real question spike #5 resolved was
*"can Studio avoid ever creating a `WebviewWindow`?"*.

**Verdict: yes.** `Cargo.toml` has
`tauri = { workspace = true, features = ["wry", "unstable"] }`.
The `unstable` flag exposes `tauri::window::WindowBuilder`, which
builds a bare `tao::Window` with no webview attached — the opposite
of `tauri::WebviewWindowBuilder`. `tauri::Window<Wry>` implements
`raw_window_handle::HasWindowHandle + HasDisplayHandle`, so we wrap
it in `prism_shell::render::SharedWindow`, hand the
`GraphicsContext` a direct surface on it, and drive `wgpu` straight
at that surface. The `wry` crate is still *compiled in* because
`tauri-runtime-wry` is how Tauri reaches `tao`, but the webview
code path is never executed.

Do not flip `tauri = { default-features = false }` — you lose
`generate_context!`, the sidecar plugin, and the runtime. The `wry`
compile cost is the entry fee for Tauri's packaging/signing/updater
surface, and spike #5 accepted it as part of Option B.

## Dependencies
- `prism-shell` (path, `features = ["native", "clay"]`) — the UI
  tree plus the wgpu render stack Studio drives into the tao window.
- `prism-core` (path) — shared types.
- `prism-daemon` (path, `features = ["full"]`) — kept on the path so
  dev builds can spawn the in-tree binary. Not a library link.
- `tauri` (`features = ["wry", "unstable"]`), `tauri-build` —
  runtime + build script. `unstable` unlocks
  `tauri::window::WindowBuilder` (the no-webview path).
- `clay-layout`, `raw-window-handle` — the Studio host imports the
  same types as `prism-shell/src/bin/native.rs` so both shells
  share one render pipeline.
- `interprocess`, `postcard` — sidecar IPC.

## `tauri.conf.json`
Lives at `packages/prism-studio/src-tauri/tauri.conf.json`. The
`prism` CLI references it explicitly for both `dev studio` and
`build --target studio`; if you relocate it, update
`crate::workspace::Workspace::tauri_config` in `prism-cli`.
