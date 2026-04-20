# prism-dock

Renderer-agnostic dockable panel layout engine (ADR-005). Pure Rust
data model for Unity/Resolve-style resizable, tabbable, dockable
panel layouts. No Slint, no UI dependency — `prism-shell` renders
the dock tree; this crate only manages the layout state.

## Build & Test
- `cargo build -p prism-dock`
- `cargo test -p prism-dock` — 31 unit tests.
- `cargo clippy -p prism-dock -- -D warnings`

## Public surface

### Data model
- `DockState` — root wrapper. Holds a single `DockNode` tree.
  Methods: `panel_ids`, `contains_panel`, `find_panel`,
  `activate_tab`, `set_ratio`, `add_tab`, `close_tab`, `split`,
  `remove_panel`, `tab_count`, `leaf_count`.
- `DockNode` — binary tree enum:
  - `Split { axis, ratio, first, second }` — internal node.
  - `TabGroup { tabs, active }` — leaf node with one or more panels.
  - Constructors: `tab(id)`, `tabs(vec)`, `hsplit(ratio, a, b)`,
    `vsplit(ratio, a, b)`.
  - Tree operations: `split_at`, `collapse_at`, `remove_panel`,
    `move_panel_to_tab_group`, `node_at`, `node_at_mut`.
- `NodeAddress` — path through the binary tree (root = empty,
  `false` = first child, `true` = second child).
- `Axis` — `Horizontal | Vertical`.
- `SplitPosition` — `Before | After`.

### Panel registry
- `PanelId` — string alias.
- `PanelKind` — enum of known panel types (Builder, Inspector,
  Properties, Explorer, CodeEditor, Identity, Timeline, NodeGraph,
  AssetBrowser, ComponentPalette, Console).
- `PanelMeta` — metadata per kind: label, icon hint, min size,
  allow_multiple.
- `PanelKind::id()` — kebab-case string from the enum variant.
- `PanelKind::meta()` — returns `PanelMeta` for the kind.

### Workflow pages
- `WorkflowPage` — bundles an id, label, icon hint, and a
  `DockState` preset.
- `WorkflowPage::edit()` — 3-column: Explorer | Builder |
  Inspector+Properties.
- `WorkflowPage::design()` — 3-column + bottom: Palette | Builder |
  Inspector over Properties.
- `WorkflowPage::code()` — 2-column: Explorer | CodeEditor over
  Console.
- `WorkflowPage::fusion()` — quad layout with NodeGraph + Timeline.
- `WorkflowPage::builtins()` — returns all four presets.

## Architecture

Four modules:
- `node.rs` — `DockNode`, `NodeAddress`, `Axis`, `SplitPosition`.
  All tree manipulation lives here. 20 unit tests.
- `state.rs` — `DockState` wrapper. Delegates to `DockNode`. 3 tests.
- `panel.rs` — `PanelKind`, `PanelMeta`, `PanelId`. 3 tests.
- `page.rs` — `WorkflowPage` presets. 6 tests.

## Dependencies
- `serde` + `serde_json` — serialization only.
- No `prism-core`, no `prism-builder`, no Slint.

## Integration plan
`prism-shell` will depend on `prism-dock` and:
1. Replace `ActivePanel` enum with `PanelId` lookups into `DockState`.
2. Replace fixed `left-sidebar-width` / `right-sidebar-width` with
   `Split.ratio` values driven from the dock tree.
3. Add a workflow page bar to `app.slint` (top strip of mode buttons).
4. Render the dock tree recursively using `SplitHandle` + model-driven
   tab bars.
