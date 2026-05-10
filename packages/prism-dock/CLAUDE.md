# prism-dock

Renderer-agnostic dockable panel layout engine (ADR-005). Pure Rust
data model for Unity/Resolve-style resizable, tabbable, dockable
panel layouts. No Slint, no UI dependency — `prism-shell` renders
the dock tree; this crate only manages the layout state.

## Build & Test
- `cargo build -p prism-dock`
- `cargo test -p prism-dock` — 82+ unit tests.
- `cargo clippy -p prism-dock -- -D warnings`

## Public surface

### Data model
- `DockState` — root wrapper. Holds a single `DockNode` tree.
  Methods: `panel_ids`, `contains_panel`, `find_panel`,
  `activate_tab`, `set_ratio`, `add_tab`, `close_tab`, `split`,
  `remove_panel`, `move_panel`, `tab_count`, `leaf_count`.
- `DockNode` — binary tree enum (`PartialEq`):
  - `Split { axis, ratio, first, second }` — internal node.
  - `TabGroup { tabs, active }` — leaf node with one or more panels.
  - Constructors: `tab(id)`, `tabs(vec)`, `hsplit(ratio, a, b)`,
    `vsplit(ratio, a, b)`.
  - Tree operations: `split_at`, `collapse_at`, `remove_panel`,
    `move_panel`, `node_at`, `node_at_mut`.
- `MoveTarget` — enum for panel move destinations:
  - `TabGroup(NodeAddress)` — insert as tab into existing group.
  - `SplitEdge { addr, axis, position }` — split at edge, creating
    a new split node.
- `NodeAddress` — path through the binary tree (root = empty,
  `false` = first child, `true` = second child).
- `Axis` — `Horizontal | Vertical`.
- `SplitPosition` — `Before | After`.

### Panel registry (declarative, §34)
- `PanelId` — `String` alias.
- `PanelKind` — `Copy` data struct holding everything one panel needs:
  `id`, `label`, `icon_hint`, `min_width`, `min_height`,
  `allow_multiple`, and `tag` (`Option<&'static str>` — the
  `shell.*` content tag the renderer embeds inside the dock leaf,
  formerly the standalone `prism_shell::components::panel_routing`
  table). Each panel is a `pub const PanelKind` (`PanelKind::BUILDER`,
  `PanelKind::INSPECTOR`, …); `PanelKind::ALL` is the
  `&'static [&'static PanelKind]` table that drives every consumer.
  Adding a new panel = one `pub const` + one `&Self::FOO` row in
  `ALL`. No enum, no `meta()` match, no parallel routing table.
- `PanelKind::from_id(&str) -> Option<&'static PanelKind>` — reverse
  lookup from kebab-case id.
- `PanelKind::tag_for(&str) -> Option<&'static str>` — convenience
  alias for `from_id(id).and_then(|p| p.tag)`. Subsumes the former
  `panel_routing::tag_for_panel`.
- `PanelKind::panel_id(&self) -> PanelId` — owned `String`, used at
  `WorkflowPage` construction sites that need an owned tab id.

### Layout computation (`layout.rs`)
Pure geometry — no UI dependency. Computes pixel rectangles from
the dock tree given a bounding rectangle.
- `Rect` — `{ x, y, width, height }` with `contains(px, py)`,
  `center()`.
- `LayoutRect` — resolved rectangle for a tree node, with `addr`,
  `rect`, and `kind` (`TabGroup` or `SplitDivider`).
- `compute_layout(node, bounds)` → `Vec<LayoutRect>` — resolve
  the full tree into pixel rects.
- `find_tab_group_at(layout, px, py)` — hit-test for tab groups.
- `find_divider_at(layout, px, py)` — hit-test for split dividers.
- `constrain_ratio(node, addr, width, height, min_sizes)` — compute
  min/max ratio range for a split given panel minimum sizes.
- `DIVIDER_THICKNESS` — 4px constant for split handle size.

### Drop zones (`drop_zone.rs`)
Drag-and-drop target computation for DaVinci/Unity-style panel
rearrangement.
- `DropZone` — enum of five zone types per tab group: `TabInsert`
  (center), `Left`, `Right`, `Top`, `Bottom` (edges). Each carries
  the target `NodeAddress` and highlight `Rect`.
- `DropZone::to_move_target()` — convert to `MoveTarget` for
  executing the move.
- `compute_drop_zones(layout)` → `Vec<DropZone>` — five zones per
  tab group (25% edge fraction).
- `hit_test_drop_zone(zones, px, py)` — find the zone under a point.

### Workspace (`workspace.rs`)
Page-level management for workflow switching (like Resolve's page bar).
- `DockWorkspace` — holds `Vec<WorkflowPage>` + active index +
  per-page customization overrides.
- `with_builtins()` — six built-in pages (Edit, Design, Code, Fusion, Navigation, Preview).
- `switch_page(index)`, `switch_page_by_id(id)`, `cycle_page(forward)`.
- `active_page()`, `active_dock()`, `active_dock_mut()`.
- `toggle_panel(id)`, `ensure_panel_visible(id)`,
  `navigate_to_panel(id)` — cross-page panel navigation.
- `reset_active_page()` — discard customizations, restore default.
- `add_page(page)`, `remove_page(index)` — dynamic page management.
- `save_layout_for_page`, `restore_layout_for_page` — persist/restore.
- `find_page_for_panel(id)` — locate which page contains a panel.
- `Default` impl returns `with_builtins()`.

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
- `WorkflowPage::navigation()` — 2-column: Navigation (40%) | Builder (60%).
- `WorkflowPage::preview()` — full-page Builder preview.
- `WorkflowPage::builtins()` — returns all six presets.

## Architecture

Seven modules:
- `node.rs` — `DockNode`, `NodeAddress`, `Axis`, `SplitPosition`,
  `MoveTarget`. All tree manipulation lives here. 25 unit tests.
- `state.rs` — `DockState` wrapper. Delegates to `DockNode`. 3 tests.
- `panel.rs` — `PanelKind`, `PanelMeta`, `PanelId`. 6 tests.
- `page.rs` — `WorkflowPage` presets. 6 tests.
- `layout.rs` — `Rect`, `LayoutRect`, `compute_layout`,
  `constrain_ratio`. Pure geometry. 13 tests.
- `drop_zone.rs` — `DropZone`, `compute_drop_zones`,
  `hit_test_drop_zone`. 10 tests.
- `workspace.rs` — `DockWorkspace` page management. 17 tests.

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
5. Wire drop zones into the builder's drag-and-drop system for panel
   rearrangement.
6. Use `DockWorkspace` on `AppState` for workflow page switching.
7. Use `constrain_ratio` during split handle drags to enforce panel
   minimum sizes.
