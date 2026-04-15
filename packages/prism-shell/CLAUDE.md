# prism-shell

Single source of truth for the Prism UI tree. Every Studio panel,
every lens, and the page builder itself all render through a single
Slint component tree (`ui/app.slint`) whose properties are bound
from the reloadable `AppState` struct at runtime. Slint owns layout,
windowing, and rendering on both native (winit + femtovg) and
browser (winit on `wasm32-unknown-unknown` + WebGL via femtovg).

The 2026-04-15 Clay → Slint pivot (see
`docs/dev/slint-migration-plan.md`) retired the old `clay-layout`
DSL, the hand-vendored wgpu renderer, the `tao` windowing path,
and the emscripten/Canvas2D web target in one stroke. License is
GPL-3.0-or-later (workspace-wide), which is what Slint's community
licence requires.

## Build & Test
- `cargo build -p prism-shell` — default feature `native`. Pulls in
  the full Slint stack + `prism-daemon` for the sidecar.
- `cargo build -p prism-shell --target wasm32-unknown-unknown
  --no-default-features --features web` — browser build. Run
  `wasm-bindgen --target web --out-dir web/ target/wasm32-unknown-unknown/<profile>/prism_shell.wasm`
  after the cargo step to emit the JS loader next to
  `web/index.html`. `prism build --target web` wires this up as
  one command.
- `cargo test -p prism-shell` — unit + insta snapshot tests. Slint
  tests that need a platform backend gracefully skip if none is
  available (CI-friendly).
- `prism dev shell` — native bin at `src/bin/native.rs`. One-line
  `main` that builds a `Shell` and runs Slint's event loop.
- `prism dev web` — builds the `wasm32-unknown-unknown` target,
  runs `wasm-bindgen` into `web/`, then serves that directory via
  `python3 -m http.server 1420`. Load `http://127.0.0.1:1420/`
  to boot the shell into `<canvas id="canvas">`.

## Crate layout
- `crate-type = ["cdylib", "rlib"]` — the `rlib` is the library
  every native consumer (the Studio host, tests) links against; the
  `cdylib` is what `wasm-bindgen` needs to produce its JS glue for
  the browser build. Emitting both unconditionally matches the
  Slint project templates and avoids per-target feature plumbing.
- `build.rs` calls `slint_build::compile("ui/app.slint")` so the
  generated Rust types land in the crate's `OUT_DIR`. `src/lib.rs`
  pulls them in via `slint::include_modules!()`.
- `ui/app.slint` is the hand-written declarative root component.
  Phase 0 ships a Phase-0-parity sidebar + three buttons + main
  content placeholder. Phase 1 grows it into the full panel set.

## Features
- `native` (default) — on-desktop stack. Pulls in `slint` (with the
  default winit + femtovg backend) plus `prism-daemon` with `crdt`
  / `luau` / `vfs` / `crypto`. The daemon is kept as a *native*
  dependency only; the browser build talks to a remote relay.
- `web` — browser target. Pulls in `wasm-bindgen`,
  `console_error_panic_hook`, and `getrandom = { features = ["js"] }`
  so Slint's wasm dep chain resolves. The `#[wasm_bindgen(start)]`
  entry point in `src/lib.rs::web_start` boots the same `Shell`
  the native binary does. Mutually exclusive with `native` at
  link time.

## Public surface
From `src/lib.rs`:
- `Shell` — owning wrapper around `prism_core::Store<AppState>` +
  the root Slint `AppWindow`. Build one with `Shell::new()`, call
  `Shell::run()` to hand control to Slint's event loop.
  `Shell::sync_ui` pushes the current state into Slint properties
  after every mutation; `Shell::dispatch_input`, `subscribe` /
  `unsubscribe`, and `snapshot` / `restore` keep the §7 hot-reload
  story alive.
- `AppState`, `ActivePanel` — the reloadable root state.
  Everything reloadable lives inside `AppState`; never stash state
  in `lazy_static` / `OnceCell`. `AppState` is `Serialize +
  Deserialize` so `Shell::snapshot` / `restore` just works.
- `InputAction` — serialisable `Action<AppState>` wrapper around
  `InputEvent`. Kept as a bridge for tests and future action-
  dispatch code paths; the live native/web pipelines route input
  through Slint directly.
- `AppWindow`, `ButtonSpec` — the Slint-generated types re-exported
  from the crate root. Downstream crates (Studio, tests) use these
  when they need a typed handle onto the UI.
- `panels` — data-provider structs (one per Phase-1 panel) that
  feed the Slint properties `Shell::sync_ui` pushes in.

## Phase 0 status
The shell renders one hard-coded panel (`panels::identity`) with a
sidebar of three buttons and a main content placeholder. Slint
delivers every input natively; the Rust side only handles
`clicked(int)` for the sidebar buttons so the Slint → Rust
round-trip is exercised end-to-end. Adding a panel means:

1. Implement `Panel` in `src/panels/<name>.rs` (data provider only).
2. Add a variant to `AppState::active_panel` (`ActivePanel`).
3. Add a branch in `Shell::sync_ui` that feeds the new panel's
   data into the Slint `AppWindow`.
4. Grow `ui/app.slint` if the new panel needs a different root
   layout — otherwise just rebind `panel_title` / `panel_hint` /
   `actions`.
5. Add tests — `cargo test -p prism-shell` runs every panel's
   pure-data assertions without requiring a platform backend.

## Downstream
- `prism-studio/src-tauri` embeds this crate as a library with
  `default-features = false, features = ["native"]`. Studio builds
  a `Shell` inside its own main, spawns the `prism-daemond`
  sidecar over `interprocess`, and runs Slint's event loop from
  there (no bare `tao::EventLoop`; no wgpu; no webview).
- `prism dev web` drives the web build via `wasm-bindgen`. The
  hand-written `web/index.html` imports the generated
  `prism_shell.js` module and calls `init()`; the
  `#[wasm_bindgen(start)]` entry point in `src/lib.rs` runs the
  Slint event loop into `<canvas id="canvas">`. No emscripten,
  no Canvas2D, no hand-rolled C ABI.
