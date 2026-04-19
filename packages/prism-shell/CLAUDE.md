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
  As of the B1 builder unification (2026-04-18), it imports Slint's
  `std-widgets.slint` (`Button`, `LineEdit`, `Switch`, `ComboBox`,
  `ListView`, `ScrollView`, `GroupBox`, `Palette`, etc.) instead of
  hand-rolling every widget from `Rectangle` + `Text` + `TouchArea`.
  Six custom components: `NavButton` (activity bar icon with
  selection indicator, uses `Image` + `colorize`), `IconButton`
  (28px icon-only button for toolbars/actions), `ToolbarSeparator`,
  `FieldEditor` (property row using `LineEdit` / `Switch` per kind),
  `InspectorRow` (indented tree node with icon move buttons), and
  `BuilderBlock` (WYSIWYG component preview with inline editing).
  All color references use `Palette.*` for native Slint theming.
  Activity bar uses hardcoded `NavButton` instances with
  `@image-url("icons/*.svg")` instead of a dynamic model.
  Four-panel switcher layout unchanged.

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
- `Shell` — owning wrapper around `Rc<RefCell<ShellInner>>` +
  the root Slint `AppWindow`. `ShellInner` holds the `Store<AppState>`,
  `ComponentRegistry`, `KeyboardModel`, `CommandRegistry`, and undo
  stacks. Callbacks capture `Rc::clone` of the inner state so Slint
  closures can mutate the store directly. Build one with `Shell::new()`,
  call `Shell::run()` to hand control to Slint's event loop.
  `sync_ui_from_shared` pushes current state into Slint properties
  after every mutation; `Shell::dispatch_input`, `subscribe` /
  `unsubscribe`, and `snapshot` / `restore` keep the §7 hot-reload
  story alive. `Shell::add_notification` pushes toast notifications.
- `FirstPaint` — boot telemetry slot (`src/telemetry.rs`).
- `AppState`, `ActivePanel` — the reloadable root state. Phase 4
  replaced `selected_node: Option<NodeId>` with `SelectionModel`
  (multi-select with focus depth). Added `tabs`, `toasts`,
  `command_palette_open`, `command_palette_query`, `search_query`.
  The component registry is **not** part of `AppState` — it's an
  `Arc<ComponentRegistry>` on `ShellInner`, rebuilt from scratch on
  every boot via `prism_builder::register_builtins`.
- `SelectionModel` (`src/selection.rs`) — multi-select with focus
  index and depth. Methods: `select`, `toggle`, `extend`, `clear`,
  `primary`, `contains`, `is_multi`, `deepen`, `shallow`.
- `KeyboardModel` (`src/keyboard.rs`) — `KeyCombo` (parsed from
  "ctrl+shift+p" strings), `KeyBinding` (combo + command + optional
  `when` context predicate), `KeyboardModel` (register bindings,
  resolve key events to command ids, context-aware matching with
  negation support). `with_defaults()` registers standard shortcuts.
- `CommandRegistry` (`src/command.rs`) — `CommandEntry` (id, label,
  category, shortcut) and `CommandRegistry` (register, get, list,
  filter by fuzzy multi-word query). `with_builtins()` registers
  standard commands (undo, redo, palette, panels, search, etc.).
- `SearchIndex` (`src/search.rs`) — TF-IDF full-text search over
  `BuilderDocument` node properties. `SearchIndex::build(doc)` indexes
  all text/number/boolean fields; `query(str)` returns ranked
  `SearchResult` items (node_id, component, field, snippet, score).
- `SelectPanel(ActivePanel)` — reducer-side action for switching
  panels.
- `InputAction` — serialisable `Action<AppState>` wrapper around
  `InputEvent`.
- `panels` — data-provider structs (one per panel) that feed Slint
  properties. Four panels: Identity, Builder, Inspector, Properties.

## Phase 4 status
The shell is now an interactive editor. All MVP and Should Ship
shell framework features are implemented:
- **Interactive property editing**: editable `FieldRowView` with
  `TextInput` for text/number/color/select fields and toggle switch
  for booleans. Edits commit on Enter and dispatch through
  `MutateNodeProp` with type-aware JSON value coercion.
- **Icon library**: 26 SVG icons in `ui/icons/` for navigation,
  toolbar actions, and component palette. Icons are loaded via
  `@image-url()` in Slint and tinted with `colorize` for theming.
- **Builder toolbar**: Icon-button toolbar above the canvas with
  undo/redo, copy/cut/paste/duplicate, move up/down, and delete.
- **Clipboard**: Internal clipboard buffer on `ShellInner` for
  copy (Ctrl+C), cut (Ctrl+X), paste (Ctrl+V), and duplicate
  (Ctrl+D). Paste generates new node IDs via `clone_node_with_new_ids`.
  Duplicate inserts the clone as the next sibling.
- **Expanded inline editing**: Heading, text, link, button, card,
  code, and accordion components are all inline-editable when
  selected in the builder canvas (press Enter to commit).
- **Tabbed MDI**: 48px `ActivityBar` icon strip (left edge) +
  `TabBar` above content for open documents.
- **Command palette**: Ctrl+Shift+P overlay with fuzzy command
  filtering, keyboard navigation, and shortcut labels.
- **Keyboard binding system**: `KeyCombo` parsing, context-aware
  `KeyboardModel` resolution, standard shortcuts wired to command
  dispatch.
- **Notification toasts**: bottom-right toast overlay with
  kind-colored indicators (info/success/warning/error) and dismiss.
- **Selection model**: `SelectionModel` replaces `selected_node`,
  supports multi-select, focus index, and depth levels.
- **Drag-and-drop**: node move-up/move-down reordering in the
  inspector panel via `move_node_in_siblings`.
- **Undo/redo UI**: StatusBar at bottom with undo/redo buttons,
  description labels, and document snapshot stack (100-entry limit).
- **Search**: TF-IDF full-text search in sidebar with live results.

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
