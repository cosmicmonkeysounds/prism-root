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
- `cargo test -p prism-shell` — unit + insta snapshot tests (155+).
  Slint tests that need a platform backend gracefully skip if none is
  available (CI-friendly).
- `prism dev shell` — native bin at `src/bin/native.rs`. Supports CLI
  flags for direct scene/app navigation:
  - `--app lattice --panel builder` — skip launchpad, open Lattice builder
  - `--scene builder-grid` — load a predefined test scene
  - `--scene list` — list all available scenes
  - `--viewport tablet` — set viewport to 768px
  - `--zoom 0.5` — set initial canvas zoom
  - `--screenshot /tmp/test.png` — capture screenshot and exit
- `prism visual` — automated visual test suite. Runs all predefined
  scenes through the shell binary, captures screenshots to
  `screenshots/`. Use `prism visual --scene builder-grid` for a
  single scene, `prism visual --list` to list scenes.
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
  Seven custom components: `NavButton` (activity bar icon with
  selection indicator, uses `Image` + `colorize`), `IconButton`
  (28px icon-only button for toolbars/actions), `ToolbarSeparator`,
  `FieldEditor` (property row using `LineEdit` / `Switch` per kind),
  `InspectorRow` (indented tree node with icon move buttons),
  `BuilderBlock` (WYSIWYG component preview with inline editing),
  and `GridCanvas` (interactive page grid editor with cell rectangles,
  "+" icons in empty cells, component previews in occupied cells,
  click-to-select and click-to-add). All color references use `Palette.*` for native
  Slint theming. Activity bar uses hardcoded `NavButton` instances
  with `@image-url("icons/*.svg")` instead of a dynamic model.
  Four-panel switcher layout unchanged. Builder panel conditionally
  renders `GridCanvas` (when page has multiple grid cells) or falls
  back to the flat `BuilderBlock` list.

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
  `ComponentRegistry`, `InputManager`, `CommandRegistry`, and undo
  stacks. Callbacks capture `Rc::clone` of the inner state so Slint
  closures can mutate the store directly. Build one with `Shell::new()`,
  call `Shell::run()` to hand control to Slint's event loop.
  `sync_ui_from_shared` pushes current state into Slint properties
  after every mutation; `subscribe` / `unsubscribe`, and `snapshot` /
  `restore` keep the §7 hot-reload story alive.
  `Shell::add_notification` pushes toast notifications.
- `FirstPaint` — boot telemetry slot (`src/telemetry.rs`).
- `AppState` — the reloadable root state. `ActivePanel` was removed
  in the ADR-005 dock integration; panel/page state now lives in
  `workspace: prism_dock::DockWorkspace` on `AppState`.
  `panel_id_for_slint(&DockWorkspace) -> i32` derives the legacy
  Slint panel ID from the active workspace page.
  `page_id_for_panel(&str) -> &str` maps panel names to page IDs.
  Phase 4 replaced `selected_node: Option<NodeId>` with
  `SelectionModel` (multi-select with focus depth). Added `tabs`,
  `toasts`, `command_palette_open`, `command_palette_query`,
  `search_query`. The component registry is **not** part of
  `AppState` — it's an `Arc<ComponentRegistry>` on `ShellInner`,
  rebuilt from scratch on every boot via
  `prism_builder::register_builtins`.
- `SelectionModel` (`src/selection.rs`) — multi-select with focus
  index and depth. Methods: `select`, `toggle`, `extend`, `clear`,
  `primary`, `contains`, `is_multi`, `deepen`, `shallow`.
- `InputManager` (`src/input.rs`) — layered input scheme stack.
  Apps/editors register `InputScheme`s via the builder pattern
  (`InputScheme::builder("id").bind("ctrl+z","cmd").build()`),
  then push/pop them onto the active stack. Key events dispatch
  top-down through the stack; the first matching binding wins.
  `with_defaults()` seeds three built-in schemes: `shell.base`
  (global shortcuts + tab/panel navigation), `shell.edit` (inspector
  arrow-key navigation, delete), and `shell.search`. `FocusRegion`
  enum tracks which UI area has keyboard focus. `combo_from_slint()`
  translates Slint key events into `KeyCombo` for dispatch. Context
  flags (`hasSelection`, `hasClipboard`, `commandPaletteOpen`, and
  per-focus-region flags) are auto-synced after every mutation.
- `KeyboardModel` (`src/keyboard.rs`) — low-level `KeyCombo` parsing
  and `KeyBinding` data types (combo + command + optional `when`
  context predicate). Used internally by `InputScheme`.
- `CommandRegistry` (`src/command.rs`) — `CommandEntry` (id, label,
  category, shortcut) and `CommandRegistry` (register, get, list,
  filter by fuzzy multi-word query). `with_builtins()` registers
  standard commands (undo, redo, palette, panels, search,
  navigation, etc.).
- `SearchIndex` (`src/search.rs`) — TF-IDF full-text search over
  `BuilderDocument` node properties. `SearchIndex::build(doc)` indexes
  all text/number/boolean fields; `query(str)` returns ranked
  `SearchResult` items (node_id, component, field, snippet, score).
- `SelectPage(String)` — reducer-side action for switching workflow
  pages via `workspace.switch_page_by_id()`.
- `InputAction` — serialisable `Action<AppState>` wrapper around
  `InputEvent`.
- `panels` — data-provider structs (one per panel) that feed Slint
  properties. Four panels: Identity, Builder, Inspector, Properties.
- `help` (`src/help.rs`) — `register_help_entries(&mut HelpRegistry,
  &ComponentRegistry)` coordinates distributed help registration.
  Component entries come from `Component::help_entry()` via
  `ComponentRegistry`'s `HelpProvider` impl; panel entries from
  `Panel::help_entry()` on each panel struct; field/toolbar/shell
  entries remain in `help.rs`. New modules add help by implementing
  `HelpProvider` or `Component::help_entry` / `Panel::help_entry`.
  9 unit tests.
  The help tooltip is wired via `help-hover(id, x, y)` /
  `help-leave()` callbacks on `AppWindow`. A `slint::Timer`-based
  show delay (380ms) and auto-hide (8s idle) manages the tooltip
  lifecycle. Elements with help support call `help-hover` from
  their `moved` handler; the Rust side looks up the entry in the
  `HelpRegistry` on `ShellInner` and pushes `HelpTooltipData` to
  the Slint overlay. Escape dismisses the tooltip.
- `testing` (`src/testing.rs`) — visual testing harness. `BuiltinScene`
  enum (8 scenes: launchpad, builder-empty, builder-grid, builder-tablet,
  builder-mobile, inspector, code-editor, explorer). `apply_scene(scene)`
  builds an `AppState` for a scene. `TestHarness` wraps a `Shell` with
  scene loading, input simulation, and state assertions. `TestInput`
  provides `send_command`, `send_key`, `click_grid_cell`, `click_node`,
  `select_panel`, `set_viewport`. `ShellScreenshot` captures screenshots
  via platform tools. 11 unit tests.

## Testing
When implementing UI features, use the testing harness to verify your
changes visually:
```bash
# Quick visual check — opens builder directly (no click-through)
cargo run -p prism-shell -- --app lattice --panel builder

# Check at different viewports
cargo run -p prism-shell -- --scene builder-tablet
cargo run -p prism-shell -- --scene builder-mobile

# Capture screenshots for review
prism visual --scene builder-grid

# Run the full visual suite
prism visual
```
New scenes should be added to `BuiltinScene` in `src/testing.rs` when
adding major visual features. Each scene should be self-contained and
deterministic.

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
- **Layered input manager**: `InputManager` with composable
  `InputScheme` stack. Builder pattern for scheme construction,
  registration/DI for apps to push/pop their own hotkey maps.
  All key events route through a single `dispatch-key` callback
  from Slint to Rust's `InputManager::dispatch()`, replacing the
  hardcoded FocusScope handler. Built-in schemes for shell, edit
  panel, and search. Tab navigation (Ctrl+Tab, Ctrl+1-9), inspector
  arrow-key navigation, and panel switching via command system.
- **Notification toasts**: bottom-right toast overlay with
  kind-colored indicators (info/success/warning/error) and dismiss.
- **Selection model**: `SelectionModel` replaces `selected_node`,
  supports multi-select, focus index, and depth levels.
- **Drag-and-drop**: node move-up/move-down reordering in the
  inspector panel via `move_node_in_siblings`.
- **Undo/redo UI**: StatusBar at bottom with undo/redo buttons,
  description labels, and document snapshot stack (100-entry limit).
- **Search**: TF-IDF full-text search in sidebar with live results.
- **Help tooltips**: context-sensitive hover tooltips on all interactive
  elements (activity bar, toolbar, component palette, tabs). Port of the
  React help system per ADR-005. `HelpRegistry` in `prism-core` +
  `register_help_entries` in `prism-shell/src/help.rs` + Slint tooltip
  overlay with 380ms show delay and 8s auto-hide.
- **Grid canvas editor**: Interactive page grid with `GridCanvas` Slint
  component. Empty cells show "+" icons with hover highlights; occupied
  cells show component type labels. Click empty cell to add component,
  click occupied to select. "+ Col" / "+ Row" buttons for adding tracks.
  Grid callbacks: `grid-cell-clicked`, `grid-add-column/row`,
  `grid-remove-column/row`, `grid-track-resize`, `grid-cell-add-component`.
- **Page shells**: New pages come with a basic grid layout shell
  (`BuilderDocument::page_shell()`) — single-section responsive grid
  (1 col × 1 row), margins, and a root container. The grid canvas
  always renders when the page has tracks defined (even a 1×1 grid).
  Users add components via the picker popup (clicking "+" in empty
  cells) or by selecting a palette item and clicking a cell.
- **Layout-aware components**: Container, Columns, Form, and List
  components carry correct default `FlowProps` (display, direction,
  gap) via `default_layout_for_component()`, so the Taffy layout
  engine handles them consistently within the grid system.
- **Palette → grid placement**: When a page has grid cells, clicking
  a component in the left palette enters "place mode" — the item
  highlights and grid cells show "Place [type]" on hover. Clicking
  a cell places the component there. Clicking the same palette item
  again cancels. When no grid exists, palette click adds directly
  (flat list behavior preserved).
- **Style cascade UI**: Right sidebar "Style (cascading)" section showing
  resolved style properties with app → page → node inheritance. When
  nothing is selected, shows page-level styles. Edits dispatch through
  `apply_style_edit` to node or page `StyleProperties`.
- **Viewport presets**: Desktop (1280px) / Tablet (768px) / Mobile (375px)
  toggle buttons in the builder toolbar. `viewport_width` on `AppState`
  drives page layout computation. `on_viewport_preset_changed` callback
  syncs the preset to state and Slint properties.
- **Canvas zoom**: `canvas-zoom` in-out property on AppWindow (0.25–3.0).
  Toolbar zoom controls (−, %, +) and keyboard shortcuts (Ctrl+=/−/0).
  Page surface, margins, grid overlays, and gutters all scale by zoom.
  Flickable-based pan with explicit viewport dimensions.
- **Visual testing harness**: `src/testing.rs` — 8 predefined scenes,
  `TestHarness` for state assertions + input simulation,
  `ShellScreenshot` for platform screenshot capture. CLI flags
  `--app`, `--panel`, `--scene`, `--viewport`, `--zoom`, `--screenshot`
  on the native binary. `prism visual` runs automated screenshot suite.

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
