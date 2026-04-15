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
  Phase 3 grew it into a four-panel switcher: a 200px-wide left nav
  column driven by a `[PanelNavItem]` model and a `select_panel(int)`
  callback, plus a main content column that renders one of four
  conditional surfaces — identity actions list, Builder DSL preview
  (monospaced `Text` bound to `builder-source`), Inspector tree dump
  (`inspector-tree`), or Properties field rows (`[FieldRow]` driving
  the local `FieldRowView` component). Each surface is gated by an
  `if` on the corresponding prop being non-empty, so `Shell::sync_ui`
  can clear every slot and populate only the active one.

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
- `live-preview` — opts into Slint's native hot-reload path
  (`slint/live-preview`). Additive: combine with `native` for
  `prism dev shell`'s default hot-reload experience. At compile
  time `slint-build` checks `SLINT_LIVE_PREVIEW=1`; when set it
  replaces the baked `AppWindow` codegen with a
  `LiveReloadingComponent` wrapper that parses `ui/app.slint` at
  runtime via `slint-interpreter` and reloads it whenever the file
  changes on disk. Public API (`AppWindow::new()`, getters, setters,
  callbacks, `ComponentHandle`) stays wire-compatible with the
  normal codegen, so `Shell::new()` doesn't branch on the feature.
  `prism dev shell` turns this on by default — pair it with the
  `.rs` respawn half in `prism-cli::dev_loop` to get a full
  rebuild + reload loop. Pass `prism dev shell --no-hot-reload` to
  opt out.

## Public surface
From `src/lib.rs`:
- `Shell` — owning wrapper around `prism_core::Store<AppState>` +
  the root Slint `AppWindow`. Build one with `Shell::new()`, call
  `Shell::run()` to hand control to Slint's event loop.
  `Shell::sync_ui` pushes the current state into Slint properties
  after every mutation; `Shell::dispatch_input`, `subscribe` /
  `unsubscribe`, and `snapshot` / `restore` keep the §7 hot-reload
  story alive. `Shell::run` installs a Slint rendering notifier on
  the way into the event loop so the first `AfterRendering` frame
  stamps a timing into `FirstPaint` and logs a one-shot
  `prism-shell: first-paint Nms` line.
- `FirstPaint` — Phase 1 first-paint telemetry slot
  (`src/telemetry.rs`). `FirstPaint::start` records the boot
  instant; `record_first_paint` is idempotent so repeated redraws
  never clobber the first observation. Clone is cheap
  (`Rc<Cell<_>>`) so the Slint notifier closure can capture its
  own handle; `Shell::telemetry()` exposes a clone for hosts that
  want to drive the notifier themselves (Studio, future test
  harnesses). Tests in `src/telemetry.rs` cover the pure-data
  surface without needing a Slint platform backend.
- `AppState`, `ActivePanel` — the reloadable root state.
  Everything reloadable lives inside `AppState`; never stash state
  in `lazy_static` / `OnceCell`. `AppState` is `Serialize +
  Deserialize` so `Shell::snapshot` / `restore` just works. Phase 3
  grew it with a serializable `BuilderDocument` + `selected_node:
  Option<NodeId>` so the Builder / Inspector / Properties panels
  have something to render out of the box. The component registry
  is **not** part of `AppState` — it's an `Arc<ComponentRegistry>`
  on the `Shell` itself, rebuilt from scratch on every boot via
  `prism_builder::register_builtins`, and carried through hot-reload
  by `Shell::from_state`.
- `SelectPanel(ActivePanel)` — reducer-side action for switching
  panels. Bridged to the Slint `select_panel(int)` callback via
  `ActivePanel::{as_id, from_id}` (unknown ids fall back to
  Identity so an out-of-range click can't break state).
- `InputAction` — serialisable `Action<AppState>` wrapper around
  `InputEvent`. Kept as a bridge for tests and future action-
  dispatch code paths; the live native/web pipelines route input
  through Slint directly.
- `AppWindow`, `ButtonSpec` — the Slint-generated types re-exported
  from the crate root. Downstream crates (Studio, tests) use these
  when they need a typed handle onto the UI.
- `panels` — data-provider structs (one per panel) that feed the
  Slint properties `Shell::sync_ui` pushes in. Phase 3 ships four:
  `identity::IdentityPanel` (title + hint + static action list),
  `builder::BuilderPanel` (emits `.slint` DSL from the active
  document via `prism_builder::render_document_slint_source`, plus
  a node count for the header), `inspector::InspectorPanel` (flat
  indented `<id> · <component>` dump of the tree), and
  `properties::PropertiesPanel` (walks the selected component's
  `schema() -> Vec<FieldSpec>` and emits one `FieldRowData` per
  entry using `FieldValue::read_{string,number,integer,boolean}`
  for value extraction). Each panel is a pure data provider keyed
  off `ActivePanel`; `Shell::sync_ui` dispatches on the variant and
  pushes the matching props.

## Phase 3 status
The shell renders a four-panel switcher — Identity, Builder,
Inspector, Properties. Slint delivers every input natively; the
Rust side handles `select_panel(int)` (sidebar navigation) and
`clicked(int)` (identity actions). The Builder panel shows the
live `.slint` DSL `prism_builder::render_document_slint_source`
emits from the active `BuilderDocument` + `ComponentRegistry`; the
Inspector panel shows a flat indented node-tree dump; the
Properties panel walks the selected component's `FieldSpec` list
and shows one row per entry. Adding another panel means:

1. Implement `Panel` in `src/panels/<name>.rs` (data provider only).
2. Add a variant to `ActivePanel` + a matching id constant on the
   panel struct.
3. Add a branch in `Shell::sync_ui` (and in the `push_active_panel`
   helper that Slint's `select_panel` callback calls from a weak
   window handle) to populate the right Slint slots.
4. Grow `ui/app.slint` if the panel needs a genuinely different
   root layout surface — otherwise bind new in-properties on
   `AppWindow` and gate their conditional surface with an `if`.
5. Add tests — `cargo test -p prism-shell` runs every panel's
   pure-data assertions without requiring a Slint platform backend.

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
