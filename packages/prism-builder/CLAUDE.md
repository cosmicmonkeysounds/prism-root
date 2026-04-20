# prism-builder

The Slint-native page builder that replaces Puck. Owns the
component-type registry, the document tree schema, the layout engine
(Taffy-backed CSS Grid / Flexbox / Block + free-form positioning),
two independent render paths (Slint DSL + HTML SSR), and the
property-panel field factories.

## Build & Test
- `cargo build -p prism-builder`
- `cargo build -p prism-builder --features interpreter` — pulls in
  `slint` + `slint-interpreter` + `spin_on` so the runtime compile
  path (`compile_slint_source` / `instantiate_document`) is
  available. `prism-shell` and `prism-studio` flip this on; the
  relay leaves it off so its dep graph stays Slint-free.
- `cargo test -p prism-builder` — 129 unit tests (baseline).
- `cargo test -p prism-builder --features interpreter` — adds one
  interpreter round-trip (130 tests total).

## Public surface
From `src/lib.rs`:

### Slint side
- `Component`, `ComponentId`, `RenderContext`, `RenderSlintContext`,
  `RenderError` — the Slint render trait. `Component::schema`
  returns `Vec<FieldSpec>`; `render_slint` emits `.slint` DSL into
  a shared `SlintEmitter`; `help_entry` returns an optional
  `HelpEntry` for context-sensitive tooltips (default `None`).
  Default render impl emits a transparent `Rectangle` wrapper.
- `ComponentRegistry`, `RegistryError` — the DI entry point.
  Implements `HelpProvider` — collects help entries from all
  registered components via `Component::help_entry()`.
- `starter::register_builtins(&mut ComponentRegistry)` — seeds the
  17-component Slint catalog.
- `render_document_slint_source(doc, registry, tokens)` — walks a
  document and returns a self-contained `.slint` source string.
- `compile_slint_source` / `instantiate_document` — gated behind
  `interpreter` feature.

### HTML SSR side
- `HtmlBlock`, `HtmlRegistry`, `HtmlRenderContext` — separate trait
  + registry for HTML rendering. Decoupled from `Component` so the
  relay's dep graph stays Slint-free.
- `register_html_builtins(&mut HtmlRegistry)` — seeds the 17-block
  HTML catalog (same component IDs as the Slint side).
- `render_document_html(doc, html_registry, tokens)` — walks a
  document against an `HtmlRegistry` and returns an HTML fragment.

### Layout engine (ADR-003)
- `PageLayout`, `PageSize`, `Orientation`, `TrackSize` — structural
  page properties (size, margins, bleed, CSS Grid template).
- `LayoutMode` (`Flow` | `Free`), `FlowProps`, `Dimension`,
  `FlowDisplay`, `FlexDirection`, `AlignOption`, `JustifyOption`,
  `GridPlacement` — per-node layout participation.
- `compute_layout(doc, viewport_size) -> ComputedLayout` — runs
  the Taffy layout pass + transform propagation, returns per-node
  `NodeLayout { rect, transform }`.
- `Node` now carries `layout_mode: LayoutMode` and
  `transform: Transform2D` (from `prism-core::foundation::spatial`).
- `BuilderDocument` now carries `page_layout: PageLayout`.
- `GridEditError` — error enum for grid track operations
  (IndexOutOfBounds, CannotRemoveLastTrack).
- `PageLayout::insert_column/row`, `remove_column/row`,
  `resize_column/row` — interactive grid manipulation methods.
- `PageLayout::cell_positions()` — all (col, row) pairs, row-major.
- `PageLayout::empty_cells(occupied)` — unoccupied cells.
- `GridPlacement::resolved_index()` — converts 1-based line index
  to 0-based cell index.
- `BuilderDocument::place_in_cell(node_id, col, row)` and
  `move_to_cell` — set a node's grid_column/grid_row placement.
- `BuilderDocument::page_shell()` — factory for new pages: returns
  a document with a single-section responsive grid (1 col × 1 row),
  root container, and 24/32px margins. Every new page starts from
  this shell rather than an empty document.

### Style cascade
- `StyleProperties` — 10-field all-`Option` struct (font_family,
  font_size, font_weight, line_height, letter_spacing, color,
  background, accent, base_spacing, border_radius). Serde-friendly
  with `skip_serializing_if = "Option::is_none"` on every field.
- `resolve_cascade(app, page, node) -> StyleProperties` — merges
  three levels; most-specific non-None wins.
- `PrismApp`, `Page`, and `Node` all carry
  `#[serde(default)] style: StyleProperties`.

### Composition patterns (ADR-004)
- `Modifier`, `ModifierKind`, `modifier_schema(kind)` — attachable
  behaviors (ScrollOverflow, HoverEffect, EnterAnimation,
  ResponsiveVisibility, Tooltip, AccessibilityOverride). Nodes carry
  `modifiers: Vec<Modifier>`; render walkers chain them as wrappers.
- `PrefabDef`, `PrefabComponent`, `ExposedSlot` — user-authored
  compound components. `PrefabComponent` implements `Component`;
  exposed slots pin inner-node props as instance-editable fields.
- `ResourceDef`, `ResourceId`, `ResourceKind`,
  `resolve_resource_refs(props, resources)` — typed shareable data
  objects referenced via `{ "$ref": "resource:<id>" }`. Render walker
  resolves refs transparently before component render.
- `SignalDef`, `Connection`, `ConnectionId`, `ActionKind` — event
  wiring. Components declare signals; documents store connections
  (source signal → target action: SetProperty, ToggleVisibility,
  NavigateTo, PlayAnimation, EmitSignal, Custom).
- `VariantAxis`, `VariantOption`, `apply_variant_overrides`,
  `apply_variant_defaults` — named bundles of prop overrides per axis.
  Render walker applies variant defaults before component render.
- Both `Component` and `HtmlBlock` traits gained `signals()` and
  `variants()` default methods (backward-compatible, return `vec![]`).
- `RenderSlintContext` and `HtmlRenderContext` `render_child()` now
  pipeline: resolve resource refs → apply variant defaults → chain
  modifier wrappers → call component render.

### Shared
- `BuilderDocument`, `Node`, `NodeId` — the serializable document
  tree (extended with layout + transform fields per ADR-003, modifiers
  per ADR-004). `BuilderDocument` also carries `resources`, `connections`,
  and `prefabs`.
- `FieldSpec`, `FieldKind`, `NumericBounds`, `SelectOption`,
  `FieldValue` — the property-panel field factories.
- `Html`, `escape_text`, `escape_attr` — HTML builder.
- `SlintEmitter`, `SlintIdent` — `.slint` DSL emitter.

## Architecture
Fifteen modules in `src/`:

- `component.rs` — the `Component` trait (Slint-only) + `RenderError`
  + `RenderSlintContext` / `RenderContext`.
- `document.rs` — `BuilderDocument` + `Node` + `NodeId`. Nodes now
  carry `layout_mode` and `transform`; documents carry `page_layout`.
- `layout.rs` — the Taffy-backed layout engine (ADR-003). `PageLayout`,
  `LayoutMode`, `FlowProps`, `compute_layout`. 15 unit tests.
- `html_block.rs` — the `HtmlBlock` trait + `HtmlRegistry` +
  `HtmlRenderContext`. Independent from `component.rs`.
- `html_starter.rs` — 17 built-in HTML blocks + `register_html_builtins`.
- `registry.rs` — `ComponentRegistry` + field-factory primitives.
- `html.rs` — `Html` buffer + escape helpers.
- `slint_source.rs` — `SlintEmitter` wrapping `SourceBuilder`.
- `render.rs` — document-level walkers: `render_document_html`
  (uses `HtmlRegistry`), `render_document_slint_source` (uses
  `ComponentRegistry`), plus `compile_slint_source` /
  `instantiate_document` behind `interpreter`.
- `modifier.rs` — `ModifierKind` enum, `Modifier` struct,
  `modifier_schema(kind)`. 6 unit tests.
- `prefab.rs` — `PrefabDef`, `ExposedSlot`, `PrefabComponent`
  (implements `Component`), `apply_prop_to_node`. 5 unit tests.
- `resource.rs` — `ResourceDef`, `ResourceKind`, `resolve_resource_refs`.
  7 unit tests.
- `signal.rs` — `SignalDef`, `Connection`, `ActionKind`. 5 unit tests.
- `variant.rs` — `VariantAxis`, `VariantOption`, `apply_variant_overrides`,
  `apply_variant_defaults`. 6 unit tests.
- `starter.rs` — 17 built-in Slint components + `register_builtins`.
  Button, Form, Input, Link, Tabs, Accordion declare signals; Button
  declares a variant axis.

## Adding a new block type

1. Add a struct implementing `Component` in `src/starter.rs` (Slint
   rendering).
2. Add a struct implementing `HtmlBlock` in `src/html_starter.rs`
   (HTML SSR). Use the same `ComponentId`.
3. Implement `schema` using `FieldSpec` builders.
4. Implement `render_slint` and `render_html` on their respective
   traits.
5. Optionally implement `signals()` and `variants()` if the component
   emits events or has named style/size axes.
6. Register in both `register_builtins` and `register_html_builtins`.
7. Add unit tests — Slint tests in `starter.rs`, HTML tests in
   `html_starter.rs`.

## Dependencies
- `prism-core` — `design_tokens`, `language::codegen::SourceBuilder`,
  `foundation::geometry`, `foundation::spatial`.
- `glam` — SIMD-accelerated 2D math (Vec2, Affine2).
- `taffy` — CSS Grid + Flexbox + Block layout engine.
- `slint` / `slint-interpreter` / `spin_on` — optional, behind the
  `interpreter` feature. The relay keeps it off.
- No hard dep on `prism-shell` or `prism-relay`.
