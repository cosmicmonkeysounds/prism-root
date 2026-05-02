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
- `cargo test -p prism-shell` — unit + insta snapshot tests (329+).
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
- `prism e2e` — end-to-end test suite. Runs built-in test scripts
  that drive the shell through the same Slint callback paths a human
  uses. Each script is a sequence of `TestAction` steps (key presses,
  command dispatches, mouse clicks, viewport changes) interlaced with
  `StateCheck` assertions.
  - `prism e2e --test viewport-switching` — run one test.
  - `prism e2e --list` — list available tests (14 built-in).
  - `prism e2e --record` — capture baseline screenshots.
  - Build with `--features e2e` for screenshot diffing (`image`)
    and OS-level input injection (`enigo`).
  - Direct: `cargo run -p prism-shell -- --e2e [--e2e-test <name>]`.
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
  Slint theming. Activity bar (40px, Home button only) uses
  `NavButton` with `@image-url("icons/*.svg")`.
  Dock-driven layout (ADR-005): the main content area uses
  `prism_dock::compute_layout()` to flatten the dock tree into
  absolutely-positioned `DockPanelRect` and `DockDividerRect`
  models. Each panel is routed by `panel.panel-id` to its content
  (builder, component-palette, inspector, explorer, properties,
  navigation, code-editor, console, etc.). Dividers are draggable with ratio
  updates flowing through `dock-divider-dragged`.
  **Chrome layout** (DaVinci Resolve / UE5 style): unified top bar
  (28px `MenuBarRow` with menu labels + page tabs + app name),
  slim activity bar (40px, Home only), DaVinci-style bottom
  workflow page bar (32px, centered Edit/Design/Code/Fusion mode
  buttons), and status bar (26px). The builder toolbar (28px) is
  inside the builder panel with breadcrumbs merged inline. No
  separate app header bar — tabs live in the menu row.
  Builder panel conditionally renders `GridCanvas` (when page has
  multiple grid cells) or falls back to the flat `BuilderBlock`
  list. The Edit workflow page includes CodeEditor tabbed alongside
  Properties for bidirectional visual+source editing.

## Features
- `native` (default) — on-desktop stack. Pulls in `slint` (with the
  default winit + femtovg backend) plus `prism-daemon` with `crdt`
  / `luau` / `vfs` / `crypto`, and enables `prism-core/crdt` so
  `CollectionStore` is available for facet ObjectQuery/Lookup
  resolution. ObjectQuery and Query data sources use
  `prism_core::widget::DataQuery::apply()` for structured
  filter/sort/limit. The daemon is kept as a *native* dependency only;
  the browser build talks to a remote relay.
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
- `e2e` — enables screenshot comparison (`image` crate) and
  OS-level input injection (`enigo` crate) for the e2e test
  framework in `src/e2e.rs`. Without this feature, the e2e driver
  still works in `Callback` mode (Slint callbacks) — only the
  `OsInput` mode and `ScreenDiff::compare` require the feature.

## Public surface
From `src/lib.rs`:
- `Shell` — owning wrapper around `Rc<RefCell<ShellInner>>` +
  the root Slint `AppWindow`. `ShellInner` holds the `Store<AppState>`,
  `ComponentRegistry`, `LiveDocument` (source-first builder state),
  `InputManager`, `CommandRegistry`, and source-text undo stacks.
  Callbacks capture `Rc::clone` of the inner state so Slint closures
  can mutate the store directly. Builder mutations go through
  `LiveDocument` source-editing methods; `sync_builder_document()`
  derives `BuilderDocument` from the live source for panel display.
  Build one with `Shell::new()`, call `Shell::run()` to hand
  control to Slint's event loop. `sync_ui_from_shared` saves source
  to the active page and pushes current state into Slint properties.
  `Shell::add_notification` pushes toast notifications.
  `Shell::with_collection(f)` (native only) borrows the inner
  `CollectionStore` mutably — hosts use this to populate objects
  and edges that ObjectQuery/Lookup facets resolve against. When a
  project is open, delegates to the project's collection.
  `Shell::open_project(path)` (native only) opens a project folder,
  scans files into the object graph, and syncs into the shell's
  collection. `Shell::close_project()`, `Shell::save_project()`,
  `Shell::has_project()` manage the project lifecycle.
- `FirstPaint` — boot telemetry slot (`src/telemetry.rs`).
- `AppState` — the reloadable root state. Panel/page state lives in
  `workspace: prism_dock::DockWorkspace` on `AppState`.
  `panel_id_for_slint(&DockWorkspace) -> i32` derives the legacy
  Slint panel ID from the active workspace page.
  `page_id_for_panel(&str) -> &str` maps panel names to page IDs.
  `push_dock_layout(&AppWindow, &DockWorkspace)` computes pixel-level
  layout from the dock tree and pushes `DockPanelRect`, `DockDividerRect`,
  and `WorkflowPageItem` models to Slint. `serialize_addr`/`deserialize_addr`
  convert `NodeAddress` to/from dot-separated "0"/"1" strings for
  Slint callback round-tripping.
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
  properties. Seven panels: Identity, Builder, Inspector, Properties,
  Signals, Navigation, CodeEditor. The Properties panel is kind-aware
  for facet components: `facet_id` renders as a select dropdown
  populated from the document's facet definitions, and kind-specific
  fields (e.g. `max_items`) are conditionally shown based on the
  selected facet's `FacetKind`. Schema changes in the Data workflow
  page auto-sync to the Properties panel via `sync_builder_document`.
- `signals` (`src/signals.rs`) — `SignalRuntime` bridges builder signal
  dispatch to the shell. `fire`, `fire_simple`, `fire_pointer`,
  `fire_changed` create `SignalEvent`s and evaluate connections.
  `apply_result` mutates `BuilderDocument` for SetProperty,
  ToggleVisibility, and PlayAnimation actions. PlayAnimation sets
  `animating` + `animation` props on the target node for the Slint
  render walker to pick up. Shell-side `fire_signal` handles
  cascading `EmitSignal` dispatch (max depth 8), `NavigateTo`
  page navigation, and `Custom` handler Luau execution via
  `prism_daemon::modules::luau_module::exec`. Luau return values
  with `set_properties` or `navigate` keys are applied back to the
  document. Signals fired from shell callbacks: clicked, hovered,
  hover-ended, focused, blurred, drag-started, drag-ended, changed,
  deleted, mounted. Focused/blurred fire on selection change in
  `sync_ui_from_shared` — "focused" = node selected, "blurred" =
  previously selected node deselected.
  Visual language bridge: `connections_to_event_listeners` converts
  `Connection` list to `ScriptGraph` `EventListener` nodes;
  `event_listeners_to_connections` converts back. 16 unit tests.
- `panels::signals` (`src/panels/signals.rs`) — `SignalsPanel` for
  authoring signal connections. `connection_rows`, `connections_for_node`,
  `available_signals`, `available_targets`, `create_connection`,
  `remove_connection`, `signal_contexts_for_node`,
  `build_schema_context`, `generate_handler_stub`,
  `generate_all_handler_stubs`, `connection_as_luau`. The panel and
  Luau code editor are two lenses on the same `Connection` data —
  `connection_as_luau` renders any connection as executable Luau, and
  `generate_handler_stub` creates editable Custom handler stubs.
  `build_schema_context` produces a `SchemaContext` with both field
  definitions and signal contexts populated, ready for the Luau/Slint
  syntax provider completion pipeline. The Slint signals panel UI
  includes a richer create-connection flow with target node picker
  and action kind ComboBox (all 6 action kinds supported).
  `ACTION_KIND_LABELS` const and `action_kind_from_index` map
  ComboBox integer indices to typed `ActionKind` variants — the
  callback signature is `(string, int, int)` (signal name, target
  index, action index). Target labels are pushed as a
  `signal-target-labels` `[string]` model from Rust.
  27 unit tests.
- `panels::navigation` (`src/panels/navigation.rs`) — `NavigationPanel`
  for page management and visual link authoring. `page_rows` lists
  all pages as `PageRow` items (index, id, title, route, is_active,
  node_count, link_count). `graph_nodes` / `graph_edges` provide
  positioned `GraphNode` / `GraphEdge` data for the Slint nav-graph
  canvas — nodes carry pixel position in a 3-column grid layout,
  edges carry source/target page indices and a kind (`"href"` or
  `"signal"`); line endpoints are computed in `push_navigation_panel_data`
  from node centers. CRUD: `create_page` (collision-safe IDs, activates
  new page), `delete_page`, `rename_page`, `set_page_route`,
  `move_page_up` / `move_page_down`, `set_navigation_style`. Link
  scanning: `graph_edges` walks all pages for `href` prop links
  (via `collect_href_targets`) and `NavigateTo` signal connections.
  `intra_app_links` returns href links on the active page only.
  All mutations push undo snapshots. 14 unit tests.
- `project` (`src/project.rs`) — `ProjectManager` (native only)
  opens a project folder on disk, reads/creates `.prism.json` manifest,
  initialises a `VaultManager<FileSystemAdapter>` and `VfsManager` with
  `FileSystemVfsAdapter`. `open()` scans all files into the object graph
  as `GraphObject` entities with type `"file"` and deterministic IDs
  (`sha256("file:" + relative_path)`). `ingest_file()` / `remove_file()`
  for incremental updates. `save()` persists vault collections to disk.
  `ProjectError` enum. Skip patterns: dotfiles, `data/`, `node_modules/`,
  `target/`, `.git`. 10 unit tests.
- `persistence` (`src/persistence.rs`) — `ProjectPersistence` tracks
  the current `.prism` file path for Save/Save As/Open/New. Uses
  `rfd::FileDialog` (native feature) for OS file pickers. File format
  is `prism_builder::ProjectFile` (JSON with version, apps, sidecar
  data). Four commands wired in `execute_command`: `file.save` (falls
  through to `file.save_as` when no path), `file.save_as` (rfd dialog),
  `file.open` (replaces apps, returns to launchpad), `file.new` (resets
  to sample apps). Keyboard shortcuts: Ctrl+N/O/S/Shift+S. 4 unit tests.
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
- `e2e` (`src/e2e.rs`) — end-to-end testing framework. Two execution
  modes: `Callback` (Slint callbacks, fast/deterministic) and `OsInput`
  (real OS events via `enigo`, requires `e2e` feature). Core types:
  `E2eDriver` (drives shell through test scripts), `TestAction` (12
  action types: keyboard, mouse, navigation, assertions), `StateCheck`
  (12 assertion types: app state, selection, viewport, document),
  `TestScript` (fluent builder API), `TestResult`/`SuiteResult`
  (diagnostics). `LayoutOracle` computes pixel positions for activity
  bar buttons, grid cells, and content area from `AppState`.
  `ScreenDiff` compares screenshots pixel-by-pixel and generates
  visual diff images (requires `e2e` feature + `image` crate).
  14 built-in test scripts exercising launchpad, scenes, viewport,
  keyboard dispatch, command palette, panels, selection, undo/redo,
  grid, sidebars, zoom, document structure, bidirectional editor,
  and workflow page switching. 14 unit tests.

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

### E2E tests
End-to-end tests drive the shell through complete workflows using the
same input paths a human uses:
```bash
# Run all e2e tests (callback mode, no display needed)
cargo run -p prism-cli -- e2e

# Run a single test
cargo run -p prism-cli -- e2e --test viewport-switching

# List tests
cargo run -p prism-cli -- e2e --list

# Direct via shell binary
cargo run -p prism-shell -- --e2e
cargo run -p prism-shell -- --e2e --e2e-test keyboard-dispatch

# Record screenshot baselines
cargo run -p prism-shell -- --e2e --e2e-record
```
New tests should be added as functions returning `TestScript` in
`src/e2e.rs` and registered in `builtin_scripts()`. Use the fluent
builder API:
```rust
fn test_my_feature() -> TestScript {
    TestScript::new("my-feature", "Verify my feature works")
        .scene("builder-grid")
        .send_command("my.command")
        .assert(StateCheck::HasSelection)
        .send_key("ctrl+z")
        .assert(StateCheck::NoSelection)
        .screenshot("after-undo")
}
```

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
- **Absolute/Relative positioning**: Components can be placed off-grid
  via `LayoutMode::Absolute` (anchored to parent rect via
  `Transform2D.anchor` + `Transform2D.position`) or
  `LayoutMode::Relative` (flow-positioned + offset). The properties
  panel exposes display mode switching (block/flex/grid/none/absolute/
  relative/free). Positioned nodes show a mode badge on hover and
  have dashed-outline overlays on the builder canvas.
- **Transform properties panel**: Godot-style unified Transform section
  in the right sidebar properties panel. Shows Position X/Y, Rotation
  (degrees), Scale X/Y, and Anchor (9-point + Stretch select) for the
  selected node. Edits are wired through `apply_transform_to_node`
  with full undo support. The `transform_rows()` generator in
  `panels/properties.rs` produces `FieldRowData` items displayed via
  the persistent `transform_rows` VecModel.
- **Canvas drag gizmo**: Selected positioned nodes (Absolute/Free/
  Relative) can be dragged on the canvas. Delta-based drag protocol:
  Slint sends `(mouse - pressed) / zoom` deltas via three callbacks
  (`node-drag-started`, `node-drag-moved`, `node-drag-finished`);
  Rust snapshots the initial transform into a `DragSnapshot` on press
  and applies deltas relative to that snapshot. Undo is pushed on
  release. Tool indicator badge on the selection shows the current
  tool mode icon. Godot-style gizmos drawn at the center of selected
  positioned nodes: Move (red/green axis arms with arrow tips + white
  center), Rotate (orange circle with handle dot), Scale (red/green
  arms with square tips + white center). Right-click on the gizmo
  cycles tool mode.
- **Transform tool mode**: Three toolbar buttons (Move / Rotate /
  Scale) toggle the `transform-tool` property. Keyboard shortcuts:
  W (move), E (rotate), R (scale) — Godot-standard bindings.
  All three modes are fully functional: Move applies dx/dy to
  position, Rotate applies 0.5 deg/px horizontal drag to rotation,
  Scale applies dx/dy as proportional scale factors (min 0.01).
  Shift-drag constraints: Move constrains to major axis, Rotate
  snaps to 15-degree increments, Scale applies uniform scaling.
  The WYSIWYG preview renders `transform-rotation`, `transform-scale-x`,
  `transform-scale-y` via the Slint render walker; the ComponentContainer
  is rendered at unscaled page size with `transform-scale` for zoom
  so internal layout is correct at all zoom levels.
- **Resize handles**: 8-point selection handles (tl/t/tr/r/br/b/bl/l)
  on selected positioned nodes. Handles are 8×8px white squares with
  accent border, positioned at corners and edge midpoints. Each handle
  has an appropriate cursor (nwse-resize, ns-resize, etc.). Delta-based
  protocol via `ResizeSnapshot` (position + computed dimensions);
  handle direction maps delta to position/size changes (e.g. "tl"
  moves origin and shrinks, "br" only grows). Shift-drag constrains
  aspect ratio on corner handles. Free nodes promote to Absolute on
  resize. Minimum size 4px. Undo pushed on release.
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
- **Navigation panel**: Dedicated workflow page for multi-page app
  management. Split layout: top half is a visual node graph showing
  pages as positioned cards and cross-page links as directed edges
  (both `href` prop links and `NavigateTo` signal connections).
  Click-to-link interaction: click a page's link handle to start,
  click a target page to create a `NavigateTo` connection. Click an
  edge to remove it. Bottom half is the page list with inline
  `LineEdit` editing for titles and routes, reorder buttons, delete,
  navigation style cycling (Tabs/Sidebar/BottomBar/None), and add
  page. All mutations push undo snapshots. `panel.navigation` and
  `add_page` commands registered in the command palette.
- **Project persistence**: Save/Save As/Open/New project via `.prism`
  JSON files. `ProjectPersistence` on `ShellInner` tracks the current
  file path. Save flushes the active page then serializes all apps
  through `ProjectFile` (preserves source + sidecar data). Open
  replaces apps and returns to launchpad. File dialogs via `rfd`
  (native only). Keyboard shortcuts Ctrl+N/O/S/Shift+S. File menu
  with New/Open/Open Folder/Close Folder separator Save/Save As.
- **Project vault**: File → Open Folder (Ctrl+Shift+O) opens a
  project directory via `ProjectManager`, scans files into the object
  graph as `GraphObject` entities with type `"file"`, and persists
  Loro snapshots via `FileSystemAdapter`. File → Close Folder clears
  the project. `file.save` also saves the vault when a project is
  open. Project folder name and dirty state display in the title bar
  and status bar. Explorer panel shows a "Project Files" section
  (collapsible) listing files from the object graph. 10 ProjectManager
  tests + 3 explorer file-node tests.
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
