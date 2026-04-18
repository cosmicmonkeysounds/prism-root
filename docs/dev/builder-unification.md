# Builder Unification Plan

> Collapsing the Shell + Builder into one Slint-native system.
> Slint's `std-widgets` and property bindings replace hand-rolled
> primitives and the `sync_ui_from_shared` rebuild loop.
>
> **Created:** 2026-04-18
> **Last updated:** 2026-04-18

---

## Problem

Three layers of indirection sit between the data and the pixels:

1. **Shell** hand-rolls every widget from `Rectangle` + `Text` +
   `TouchArea`, ignoring Slint's standard widget library entirely.
   The only std-widgets import is `ScrollView`.

2. **Builder** generates `.slint` DSL as strings via `SlintEmitter`,
   then compiles them at runtime through `slint-interpreter`. This is
   a separate UI tree from the shell.

3. **`sync_ui_from_shared`** tears down every Slint model on every
   mutation and rebuilds from scratch — React's `setState` pattern
   reimplemented in Rust, fighting Slint's declarative model.

Meanwhile Slint 1.8 ships `Button`, `Switch`, `LineEdit`, `TextEdit`,
`ComboBox`, `Slider`, `SpinBox`, `TabWidget`, `ListView`,
`StandardListView`, `StandardTableView`, `GroupBox`,
`ProgressIndicator` — all with focus handling, keyboard nav,
accessibility, and platform theming. None are used.

## Sovereign Portals / SSR

Slint renders pixels (femtovg/skia/software), not HTML. It has no
SSR path. For L1–L3 Sovereign Portals (crawler-friendly, no-JS HTML),
a lean HTML renderer stays in `prism-relay`. For L4 interactive
portals, the relay serves Slint WASM — the portal viewer is a Slint
app. The HTML rendering concern gets decoupled from the builder's
`Component` trait and lives solely in the relay.

---

## Phases

### B1 — Shell Modernization (std-widgets)

Replace hand-rolled primitives in `ui/app.slint` with Slint's
standard widget library.

| Current | Replacement |
|---|---|
| `SidebarButton` (Rectangle + Text + TouchArea) | `Button` |
| `FieldRowView` TextInput for text/number/color | `LineEdit` |
| `FieldRowView` boolean toggle (Rectangle animation) | `Switch` |
| `FieldRowView` select fields | `ComboBox` |
| `TabButton` (Rectangle + Text + TouchArea) | `TabWidget` or styled `Button` |
| `ActivityBarButton` | Styled `Button` with selection indicator |
| Manual `ScrollView` wrappers | `ListView` / `StandardListView` |
| Inspector `InspectorRow` | `StandardListView` rows |
| Command palette manual list | `ListView` |
| Toast manual layout | Styled `GroupBox` or overlay |

Deliverables:
- [ ] Rewrite `ui/app.slint` using `std-widgets.slint` imports
- [ ] Update `sync_ui_from_shared` for any changed property names
- [ ] Verify native + WASM builds
- [ ] All existing tests pass

### B2 — Reactivity Cleanup

Replace the full-rebuild `sync_ui_from_shared` with targeted Slint
model updates and two-way property bindings.

- Use `<=>` two-way bindings for editable fields (search query,
  command palette input, property values)
- Push granular `VecModel` mutations (insert/remove/set_row_data)
  instead of replacing entire models on every state change
- Move panel-switching logic into `.slint` conditional visibility
  (already partially done) and reduce Rust-side orchestration
- Let Slint's `Timer` handle toast auto-dismiss instead of Rust-side
  polling

Deliverables:
- [ ] Two-way bindings for text inputs
- [ ] Granular model updates instead of full rebuilds
- [ ] Measure frame budget impact

### B3 — Builder/Shell Merge

Stop generating DSL strings for built-in components. The document
tree drives Slint models directly.

- Built-in components (heading, text, link, image, container, form,
  input, button) become `.slint` components in a shared library file
  (e.g. `ui/components.slint`) that the shell imports
- Document nodes map to Slint model items that select which component
  to render via conditional `if` blocks or a `ComponentContainer`
- `SlintEmitter` + `render_document_slint_source` retained only for
  user-authored custom components (loaded via `slint-interpreter`)
- `ComponentRegistry` stays as the DI surface but its `render_slint`
  path becomes optional — built-ins render natively

Deliverables:
- [ ] `.slint` component library for the 8 starter blocks
- [ ] Document-to-model mapper that drives native Slint rendering
- [ ] `slint-interpreter` path retained for custom user components
- [ ] Builder panel shows live interactive preview (not monospace DSL dump)

### B4 — HTML SSR Separation

Decouple HTML rendering from the `Component` trait.

- New `HtmlBlock` trait in `prism-builder` (or directly in
  `prism-relay`) with just `render_html`
- `Component` trait loses `render_html` — it becomes Slint-only
- Relay registers `HtmlBlock` impls for the starter catalog
- Relay stays Slint-free (no `interpreter` feature needed)

Deliverables:
- [ ] `HtmlBlock` trait with the 8 starter impls
- [ ] `Component` trait simplified to Slint-only surface
- [ ] Relay tests pass with separated traits
- [ ] Zero Slint dependencies in relay dep graph

### B5 — Interactive Builder Surface

With the shell on std-widgets and the builder rendering natively,
add the interactive editing features.

- Component palette: browsable/searchable picker using `ListView` +
  `LineEdit` filter
- Drag-drop: Slint `TouchArea` pointer-move tracking with visual
  drop indicators
- Resize handles on selected components
- Inline text editing (click-to-edit heading/text body)
- Puck-Loro bridge: `CollectionStore` ↔ `BuilderDocument` CRDT sync

Deliverables:
- [ ] Component palette panel
- [ ] Drag-drop reordering in the builder surface
- [ ] Inline editing of text props
- [ ] CRDT sync of builder state

---

## Ordering

B1 → B2 → B3 are sequential — each builds on the previous.
B4 can run in parallel with B2/B3 (relay-side, no shell dependency).
B5 depends on B3 (needs native rendering before interactive editing).

```
B1 (std-widgets) → B2 (reactivity) → B3 (merge) → B5 (interactive)
                                  ↗
              B4 (HTML separation)
```

## What stays

- `BuilderDocument` / `Node` / `NodeId` — the serializable tree
- `ComponentRegistry` — the DI surface
- `FieldSpec` / `FieldKind` — property panel field factories
- `Html` / `escape_text` / `escape_attr` — HTML builder (moves to relay)
- `SlintEmitter` — retained for user-authored custom components
- `slint-interpreter` — retained for custom components only
- `Store<AppState>` — state management stays, binding layer changes
