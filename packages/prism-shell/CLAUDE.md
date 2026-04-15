# prism-shell

Single source of truth for the Prism UI tree. Every Studio panel,
every lens, and the page builder itself all render through
`render_app(&AppState, &mut Clay)` — downstream backends (wgpu on
native, a `<canvas>` / wasm-bindgen glue on web) only know how to
walk the returned Clay render commands and turn them into pixels.

## Build & Test
- `cargo build -p prism-shell` — default features `native` + `clay`.
- `cargo build -p prism-shell --no-default-features --features web,clay`
  — WASM target (paired with `trunk`).
- `cargo test -p prism-shell` — unit + insta snapshot tests.
- `prism dev shell` — native bin (`src/bin/native.rs`) against
  winit/wgpu without any Tauri packaging.
- `prism dev web` — WASM via `trunk serve --config Trunk.toml`.

## Crate layout
- `crate-type = ["rlib", "cdylib"]` — `rlib` for the packaged
  Studio host (`packages/prism-studio/src-tauri`) that pulls the
  crate in as a library, `cdylib` for the `wasm-pack`/`trunk`
  build that produces a `.wasm` consumable from a `<canvas>`-based
  web shell.
- `[[bin]] prism-shell` — native dev binary at `src/bin/native.rs`.
  Only present when the `native` feature is active (it pulls in
  `wgpu`/`tao`/`glyphon`).

## Features
- `native` (default) — on-desktop stack: `wgpu`, `tao`, `winit`,
  `glyphon`, `pollster`, plus `prism-daemon` with `crdt` / `luau` /
  `vfs` / `crypto`.
- `web` — WASM glue: `wasm-bindgen`, `web-sys`, `js-sys`,
  `console_error_panic_hook`.
- `clay` (default) — pulls in `clay-layout` and flips `panels::*` +
  `render` from the count-of-commands stub onto the real Clay path.

The `native` and `web` features are mutually exclusive at build time
— pick the target triple and feature set together (`cargo build`
vs. `trunk build`).

## Public surface
From `src/lib.rs`:
- `render_app(&AppState, &mut Clay) -> Vec<RenderCommand<'_, (), ()>>`
  — the one entry point every backend calls per frame. Takes a
  borrowed `&AppState` so renderers can call it with `shell.state()`.
- `Shell` — owning wrapper around `prism_core::Store<AppState>`. The
  host (native dev bin / Studio / the WASM shim) builds one of these
  on startup and routes every input event through `Shell::dispatch_input`.
  Exposes `state()`, `store_mut()`, `subscribe` / `unsubscribe`, and
  `snapshot` / `restore` for §7 hot-reload.
- `InputAction` — serialisable `Action<AppState>` wrapper around
  `InputEvent`. Exposed so hosts (or tests) that want structured
  action dispatch can hand Inputs to `store.dispatch(InputAction(evt))`
  instead of the ergonomic `Shell::dispatch_input` helper.
- `AppState`, `ActivePanel` — the reloadable root state. Everything
  reloadable must live inside `AppState`; never stash state in
  `lazy_static` / `OnceCell` (that breaks the hot-reload story).
  `AppState` is `Serialize + Deserialize` so `Shell::snapshot` /
  `restore` just works.
- `install_stub_text_measurer(&mut Clay)` — placeholder text
  measurer Phase 0 uses until glyphon is wired into the render pass.
- `panels`, `input`, `render`, `app` — the four top-level modules.
- `web` — feature-gated entry point for the WASM target (`src/web.rs`).

## Phase 0 status
The shell currently renders one hard-coded panel (`panels::identity`)
with a sidebar of three buttons and forwards mouse/keyboard into
Clay's input API. Everything past that is a deliberate TODO until
Phase 1 begins fanning out the full panel set. When adding a panel:

1. Implement `Panel` in `src/panels/<name>.rs`.
2. Wire it into `ActivePanel` and the match in `render_app`.
3. Add an insta snapshot that covers the declare path.
4. Keep per-panel runtime state inside `AppState`.

## Downstream
- `prism-studio/src-tauri` embeds this crate as a library with
  `default-features = false, features = ["native", "clay"]`.
  Studio supplies the `tao::EventLoop` + window (no Tauri, no wry —
  §4.5 Option C locked 2026-04-15); this crate supplies the tree.
- The web target (`prism dev web`) drives `web::*` through trunk.
