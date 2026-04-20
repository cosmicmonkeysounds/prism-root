# ADR-005: Dockable Panel System

**Status:** Accepted  
**Date:** 2026-04-20

## Context

Prism's shell uses a fixed 3-pane chrome: a left activity bar selects
one of four hardcoded `ActivePanel` variants (Identity, Edit,
CodeEditor, Explorer), a left sidebar shows panel-specific content,
and a right sidebar shows properties/inspector. `SplitHandle`
components allow resizing the left and right sidebars, but the layout
topology is static — panels cannot be rearranged, tabbed together,
torn out, or reconfigured per workflow.

Professional creative tools (DaVinci Resolve, Unity, Godot, Blender)
solve this with two layered concepts:

1. **Workflow pages** — top-level mode switches (Resolve's Media/Cut/
   Edit/Fusion/Color/Fairlight/Deliver; Unity's Scene/Game/Inspector
   arrangement presets) that load a preconfigured panel layout tuned
   for a specific task.

2. **Dockable panels** — within each workflow page, panels live in
   resizable dock zones organized as a binary split tree. Each leaf
   is a tab group that can hold one or more panels. Users can drag
   tabs between groups, split zones horizontally or vertically, and
   resize splits by dragging dividers.

Prism needs both as it grows beyond a simple page builder into a
multi-modal creative environment (builder, timeline, node graph,
code editor, asset browser, inspector).

### Prior art surveyed

- **egui_dock** — mature Rust docking library. Data model:
  `DockState<Tab>` containing `Surface`s, each with a `Tree` (binary
  tree of `Node`s: `Split { axis, ratio }` and `TabGroup`). Tightly
  coupled to egui rendering.
- **KDDockWidgets 2.0** (KDAB, C++) — gold standard for
  renderer-agnostic docking. Separates `Core` (pure C++, no Qt) from
  `View` frontends (QtWidgets, QtQuick, Flutter). Architecture to
  emulate.
- **Slint** — has no splitter widget, no dynamic `TabWidget`, and
  no docking primitives. Issues #6949 (Splitter) and #1723 (Docking)
  are open with no timeline. Community workaround from Discussion
  #343: `TouchArea`-based split handles with proportion properties.
  Prism already uses this pattern (`SplitHandle` in `app.slint`).

No renderer-agnostic Rust crate for dock layout state management
exists. Every implementation is coupled to its rendering framework.

## Decision

Introduce a new workspace crate `prism-dock` that provides a
renderer-agnostic dock layout data model, following the
KDDockWidgets Core/View separation. Prism's existing pattern —
Rust owns state, Slint renders it — maps directly to this split.

### Data model (`prism-dock`)

```
DockState {
    surfaces: Vec<DockSurface>,  // main + floating windows (Phase 5)
}

DockSurface {
    root: DockNode,
}

DockNode = Split {
    axis: Axis,        // Horizontal | Vertical
    ratio: f32,        // 0.0..1.0, position of the divider
    first: Box<DockNode>,
    second: Box<DockNode>,
} | TabGroup {
    tabs: Vec<PanelId>,
    active: usize,
}

PanelId — string identifier for a panel kind (e.g. "builder",
"inspector", "timeline", "node-graph", "properties", "explorer",
"code-editor", "identity", "asset-browser")
```

The tree is a strict binary tree: every internal node is a `Split`
with exactly two children; every leaf is a `TabGroup` with one or
more tabs. This matches egui_dock's proven model.

### Panel registry

`PanelKind` is an extensible enum of known panel types. Each kind
carries metadata: display name, icon, default minimum size, and
whether multiple instances are allowed. Panel content rendering
remains in `prism-shell` — `prism-dock` only manages layout.

### Workflow pages

`WorkflowPage` bundles a name, icon, and a `DockState` preset.
Built-in presets:

| Page | Layout | Panels |
|------|--------|--------|
| **Edit** | 3-column: tree \| canvas \| inspector | Explorer, Builder, Inspector+Properties (tabbed) |
| **Design** | 3-column with bottom strip | Builder, Inspector, Properties, Component Palette |
| **Code** | 2-column | Explorer, Code Editor |
| **Fusion** | center canvas + surrounding docks | Node Graph, Inspector, Builder, Timeline |

Users can customize any page's layout; customized layouts persist
via serde into the Loro CRDT workspace document.

### Tree manipulation API

Pure functions on `DockState`:

- `split(zone, axis, panel, position)` — split a tab group, placing
  a new panel before or after.
- `move_panel(from_zone, tab_index, to_zone, position)` — drag a
  tab from one group to another (or to a split edge).
- `close_panel(zone, tab_index)` — remove a tab; if the group
  empties, collapse the parent split.
- `set_ratio(zone, ratio)` — resize a split divider.
- `activate_tab(zone, index)` — switch active tab in a group.
- `find_panel(panel_id)` — locate a panel in the tree.

All mutations return a new `DockState` (or mutate `&mut self`) and
are deterministic, making them testable without a UI.

### Slint rendering strategy (future: prism-shell integration)

The Slint side will render the dock tree using:

1. **Recursive split layout** — each `Split` node becomes a pair of
   `Rectangle`s separated by the existing `SplitHandle` component,
   sized by `ratio * available_space`. Horizontal splits stack
   left-right; vertical splits stack top-bottom.

2. **Dynamic tab bars** — each `TabGroup` renders a model-driven
   tab strip using `VecModel` + `for` repeater (since Slint's
   `TabWidget` doesn't support dynamic tabs). Clicking a tab
   activates it; dragging initiates a panel move.

3. **Dock zone indicators** — during drag, overlay rectangles
   highlight valid drop targets (center = tab into group, edges =
   split). Hit-testing runs against the resolved layout rectangles
   from step 1.

4. **Workflow page bar** — a horizontal strip of mode buttons at
   the top (like Resolve's page tabs), each loading its preset
   `DockState`.

This is a data-model-first ADR. The Slint rendering integration
is a follow-up to the `prism-dock` crate implementation.

### Relationship to existing shell code

- `ActivePanel` enum becomes a thin adapter: its four variants map
  to `PanelId` strings in the dock tree.
- The fixed `left-sidebar-width` / `right-sidebar-width` properties
  in `app.slint` are replaced by `Split.ratio` values in the dock
  tree.
- `show-left-sidebar` / `show-right-sidebar` booleans become
  implicit: a panel is visible iff it exists in the active
  `DockState`.
- The activity bar's hardcoded `NavButton` instances are replaced
  by the workflow page bar + a panel menu for showing/hiding panels.

Migration is incremental: `prism-dock` ships as a standalone crate
first, then `prism-shell` adopts it in a subsequent phase.

## Rationale

- **Renderer-agnostic core** — follows KDDockWidgets' proven
  architecture. `prism-dock` has zero Slint/UI dependency, making
  it testable in CI without a display server and reusable if the
  rendering backend ever changes.
- **Binary split tree** — the simplest model that supports arbitrary
  nested layouts. egui_dock, KDDockWidgets, and every major IDE use
  this. More complex models (egui_tiles' flat hashmap, ImGui's
  dockspace graph) add flexibility Prism doesn't need yet.
- **Workflow pages** — professional tools prove that per-task layout
  presets dramatically improve UX over a single configurable layout.
  Users can override presets without losing the curated defaults.
- **Incremental migration** — the existing shell keeps working.
  `prism-dock` can be developed, tested, and reviewed before any
  `app.slint` changes.

## Consequences

- New workspace crate: `packages/prism-dock` with `Cargo.toml`,
  `src/lib.rs`, and modules for state, node, panel, page.
- `prism-dock` depends only on `serde`, `serde_json` — no Slint,
  no `prism-core`, no `prism-builder`.
- `prism-shell` will gain a `prism-dock` dependency in the
  integration phase.
- `ActivePanel` will eventually be superseded by `PanelId` but
  is not removed in this phase.
- `app.slint`'s fixed 3-pane `HorizontalLayout` will eventually
  be replaced by a recursive dock renderer, but is not changed in
  this phase.
- The dock tree is serializable via serde, enabling workspace
  layout persistence through Loro CRDT.
