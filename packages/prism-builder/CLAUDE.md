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
- `cargo test -p prism-builder` — 277+ unit tests.
- `cargo test -p prism-builder --features interpreter` — adds
  live document, syntax provider, and compile round-trip tests.

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
  16-component Slint catalog.
- `render_document_slint_source(doc, registry, tokens)` — walks a
  document and returns a self-contained `.slint` source string.
- `render_document_slint_source_mapped(doc, registry, tokens)` —
  same walk but injects marker comments and returns `(source, SourceMap)`.
- `build_source_map_from_markers(source)` — parses `// @node-start`
  / `// @node-end` marker comments to build a `SourceMap`.
- `SourceMap`, `SourceSpan`, `PropSpan`, `MappedEmitter` — bidirectional
  source mapping types (ADR-006).
- `compile_slint_source` / `instantiate_document` — gated behind
  `interpreter` feature.
- `LiveDocument`, `LiveDiagnostic`, `SourceSelection`,
  `SourceEditError` — gated behind `interpreter`. Source-first
  compile loop: `.slint` source is canonical, `BuilderDocument`
  derived on demand. Source mutations via `edit_prop_in_source`,
  `insert_node_in_source`, `remove_node_from_source`,
  `move_node_in_source`.
- `derive_document_from_source`, `parse_slint_value`,
  `format_slint_value` — parse marker-annotated `.slint` source
  back into structured types.
- `BuilderSyntaxProvider` — gated behind `interpreter`. Compiler-backed
  `SyntaxProvider` with context-aware completions and hover.

### HTML SSR side
- `HtmlBlock`, `HtmlRegistry`, `HtmlRenderContext` — separate trait
  + registry for HTML rendering. Decoupled from `Component` so the
  relay's dep graph stays Slint-free.
- `register_html_builtins(&mut HtmlRegistry)` — seeds the 16-block
  HTML catalog (same component IDs as the Slint side).
- `render_document_html(doc, html_registry, tokens)` — walks a
  document against an `HtmlRegistry` and returns an HTML fragment.

### Layout engine (ADR-003)
- `PageLayout`, `PageSize`, `Orientation`, `TrackSize` — structural
  page properties (size, margins, bleed, CSS Grid template).
- `LayoutMode` (`Flow` | `Free` | `Absolute` | `Relative`),
  `FlowProps`, `AbsoluteProps`, `Dimension`, `FlowDisplay`,
  `FlexDirection`, `AlignOption`, `JustifyOption`,
  `GridPlacement` — per-node layout participation.
  - `Flow(FlowProps)` — positioned by parent's flex/grid/block flow.
  - `Free` — legacy mode: `position: absolute` in Taffy, Transform2D only.
  - `Absolute(AbsoluteProps)` — removed from flow, positioned by
    `Transform2D.position` + `Transform2D.anchor` relative to the
    parent's rect. Parent is the anchor (grid cell, container, or page).
  - `Relative(FlowProps)` — participates in flow, then Transform2D
    position is applied as a post-flow offset (like CSS `position: relative`).
  - `LayoutMode::is_in_flow()`, `is_positioned()`, `flow_props()` — helpers.
- `compute_layout(doc, viewport_size) -> ComputedLayout` — runs
  the Taffy layout pass + transform propagation, returns per-node
  `NodeLayout { rect, transform }`.
- `Node` now carries `layout_mode: LayoutMode` and
  `transform: Transform2D` (from `prism-core::foundation::spatial`).
- `BuilderDocument` now carries `page_layout: PageLayout`.
- `GridCell` — recursive grid tree (`Leaf` with optional `node_id`,
  or `Split` with direction/tracks/gap/children). Each cell can be
  independently subdivided horizontally or vertically.
- `SplitDirection` (`Horizontal` | `Vertical`), `CellEdge`
  (`Top`/`Bottom`/`Left`/`Right`) — subdivision types.
- `FlatCell`, `EdgeHandle` — output of `flatten_cells` /
  `flatten_edge_handles`: pixel-positioned leaf cells and interactive
  edge handles with cell paths.
- `path_to_string(path)` / `path_from_string(s)` — dot-separated
  cell path encoding (e.g. "1.0.2").
- `compute_track_sizes(tracks, gap, available) -> Vec<f32>` —
  resolves `TrackSize` values to pixel widths.
- `GridEditError` — error enum for grid operations
  (IndexOutOfBounds, CannotRemoveLastCell, NoGrid, NotALeaf).
- `PageLayout::has_grid()`, `leaf_count()`, `flatten_cells()`,
  `flatten_edge_handles()` — grid query/layout methods.
- `PageLayout::insert_at_edge(path, edge)`, `remove_cell(path)`,
  `place_node_at(path, node_id)`, `clear_cell(path)` — interactive
  grid manipulation via cell paths.
- `GridPlacement::resolved_index()` — converts 1-based line index
  to 0-based cell index.
- `BuilderDocument::place_in_grid(node_id, path)` — place a node
  at a specific grid cell path.
- `BuilderDocument::page_shell()` — factory for new pages: returns
  a document with a single leaf grid cell, root container, and
  24/32px margins. Every new page starts from this shell rather
  than an empty document.

### Style cascade
- `StyleProperties` — 10-field all-`Option` struct (font_family,
  font_size, font_weight, line_height, letter_spacing, color,
  background, accent, base_spacing, border_radius). Serde-friendly
  with `skip_serializing_if = "Option::is_none"` on every field.
- `resolve_cascade(app, page, node) -> StyleProperties` — merges
  three levels; most-specific non-None wins.
- `PrismApp`, `Page`, and `Node` all carry
  `#[serde(default)] style: StyleProperties`.
- `PrismApp` — multi-page application container. Pages are
  `Vec<Page>` with `active_page: usize`. Page management:
  `add_page`, `remove_page` (adjusts active_page, prevents
  removing last page), `find_page_by_route`, `find_page_by_id`.
  `active_document()` / `active_document_mut()` access the current
  page's `BuilderDocument`. `NavigationConfig` holds the
  `NavigationStyle` (Tabs/Sidebar/BottomBar/None).

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
- `common_signals()` — 12 universal interaction signals every
  component gets automatically (clicked, double-clicked, hovered,
  hover-ended, drag-started/moved/ended, changed, focused, blurred,
  deleted, mounted). `with_common_signals(component_signals)` merges
  component-specific + common (component wins on name collision).
- `SignalEvent`, `DispatchResult`, `dispatch_signal(event, connections)`
  — runtime signal dispatch. Evaluates a fired signal against the
  document's connection list and returns actions for the shell to
  execute.
- `signal_symbols(component_id, signals)` — codegen bridge that
  produces a `SymbolDef` class for LuaLS `.d.luau` type stubs.
  `generate_signal_type_stubs(registry, project_name)` iterates the
  full registry and emits a complete `signals.d.luau` file via
  `SymbolEmmyDocEmitter`.
- `signal_contexts(signals)` — bridge from builder `SignalDef`s to
  syntax engine `SignalContext`s for the Luau provider's signal-aware
  completions and hover.
- `VariantAxis`, `VariantOption`, `apply_variant_overrides`,
  `apply_variant_defaults` — named bundles of prop overrides per axis.
  Render walker applies variant defaults before component render.
- Both `Component` and `HtmlBlock` traits' `signals()` default impl
  returns `common_signals()` (12 universal signals). Components
  override with `with_common_signals(extras)` to add component-specific
  signals alongside the common set.
- `RenderSlintContext` and `HtmlRenderContext` `render_child()` now
  pipeline: resolve resource refs → apply variant defaults → chain
  modifier wrappers → call component render.

### Asset resolution
- `AssetSource` — unified enum for component asset references: `Url`
  (external string) or `Vfs` (content-addressed `BinaryRef`-shaped
  object with hash/filename/mimeType/size). `from_prop(Value)` parses
  either form; `to_html_src()` resolves VFS to `/asset/{hash}` for
  relay SSR; `to_prop()` serializes back to `Value`.
- `collect_vfs_hashes(node)` — walks a document tree and returns all
  VFS hashes referenced in node props.
- `FileFieldConfig` — MIME type filter for `FieldKind::File` fields.

### Shared
- `BuilderDocument`, `Node`, `NodeId` — the serializable document
  tree (extended with layout + transform fields per ADR-003, modifiers
  per ADR-004). `BuilderDocument` also carries `resources`, `connections`,
  `prefabs`, and `facets`.
- `FieldSpec`, `FieldKind`, `NumericBounds`, `SelectOption`,
  `FieldValue` — the property-panel field factories. `FieldKind::File`
  enables file/asset picker UI with MIME filtering.
- `Html`, `escape_text`, `escape_attr` — HTML builder.
- `SlintEmitter`, `SlintIdent` — `.slint` DSL emitter.

## Architecture
Twenty-five modules in `src/` (excluding `lib.rs`):

- `app.rs` — `PrismApp`, `Page`, `AppIcon`, `NavigationConfig`,
  `NavigationStyle`. Multi-page app container with page CRUD and
  active-document accessors. 7 unit tests.
- `asset.rs` — `AssetSource` enum (URL vs VFS), prop parsing,
  `collect_vfs_hashes`, `FileFieldConfig`. 10 unit tests.
- `component.rs` — the `Component` trait (Slint-only) + `RenderError`
  + `RenderSlintContext` / `RenderContext`. `emit_layout_props` emits
  x/y for Absolute, Free, and Relative (non-zero offset) nodes;
  Absolute also emits width/height. `emit_transform_props` emits
  `transform-rotation` (deg), `transform-scale-x`, `transform-scale-y`
  when non-default. `emit_flow_props` is the shared helper for Flow
  and Relative flow properties. Positioned children (those that emit
  x/y) are automatically separated from flow children during rendering:
  `render_component_inner` wraps the component in a Rectangle and
  renders positioned children as siblings outside the component's
  layout element, preventing Slint's "cannot set x/y in layout"
  compile error.
- `document.rs` — `BuilderDocument` + `Node` + `NodeId`. Nodes now
  carry `layout_mode` and `transform`; documents carry `page_layout`.
- `layout.rs` — the Taffy-backed layout engine (ADR-003). `PageLayout`,
  `LayoutMode` (Flow/Free/Absolute/Relative), `FlowProps`,
  `AbsoluteProps`, `compute_layout`. 34 unit tests.
- `html_block.rs` — the `HtmlBlock` trait + `HtmlRegistry` +
  `HtmlRenderContext`. Independent from `component.rs`.
- `html_starter.rs` — 16 built-in HTML blocks + `register_html_builtins`.
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
- `facet.rs` — `FacetDef`, `FacetKind` (List/ObjectQuery/Script/
  Aggregate/Lookup), `FacetDataSource` (Static/Resource/Query),
  `FacetTemplate` (Inline/ComponentRef), `FacetOutput` (Repeated/Scalar),
  `FacetBinding`, `FacetLayout`, `AggregateOp` (Count/Sum/Min/Max/
  Avg/Join), `ScriptLanguage` (Luau/VisualGraph),
  `FacetVariantRule` (field condition → axis value mapping),
  `FacetComponent` (Slint), `FacetHtmlBlock` (HTML),
  `ResolvedFacetData` (Items/Single).
  Each kind resolves data differently: List uses FacetDataSource,
  Aggregate reduces to a single value via `apply_aggregate`,
  ObjectQuery/Script/Lookup use pre-resolved `resolved_data` from
  the shell layer. Script kind supports both Luau source and
  `VisualGraph` (stores a `ScriptGraph`, compiled to Luau at
  resolution time via `LuauVisualLanguage`). `Query` variant
  filters (`"field == val"`, `"field != val"`, `"field"` truthy)
  and sorts (ascending `"field"` or descending `"-field"`) a
  resource array without mutating it.
  `FacetTemplate` replaces the mandatory `prefab_id` indirection:
  `Inline { root: Node }` owns the template subtree directly with
  `{{field}}` expression bindings in node props; `ComponentRef`
  points to a registered component (backward-compatible with the
  existing prefab pipeline). `FacetOutput` separates repeated
  (template-per-item) from scalar (bind single value to a target
  widget prop) output — see `docs/dev/data-template-system.md`.
  `evaluate_calculations(&mut items, &schema)` evaluates
  `SchemaFieldKind::Calculation { formula }` fields via
  `prism_core::language::expression::evaluate_expression`, wired
  into `resolve_items()` when a schema with calc fields is set.
  `evaluate_variant_rules(root, rules, item)` conditionally sets
  axis key props on cloned prefab roots per data item, triggering
  the variant system's `apply_variant_defaults`.
  `FacetSchema`, `SchemaField`, `SchemaFieldKind`, `FacetRecord`,
  `ValidationError` — typed schema system with validation and
  default record generation. `FACET_KIND_TAGS`, `AGGREGATE_OP_TAGS`
  — string constants for UI dropdowns. 68 unit tests.
- `project.rs` — `ProjectFile`, `SavedApp`, `SavedPage`,
  `FILE_EXTENSION`, `FORMAT_VERSION`. Portable `.prism` file format
  with `from_apps`/`into_apps` conversion. `SavedPage` serializes
  source + all sidecar data (page_layout, resources, connections,
  prefabs, facets) that `Page.document` would lose via `skip_serializing`.
  5 unit tests.
- `resource.rs` — `ResourceDef`, `ResourceKind`, `resolve_resource_refs`.
  7 unit tests.
- `schemas.rs` — shared component field definitions (field specs for
  common props like body, href, src, level) used by both Slint and
  HTML render paths.
- `signal.rs` — `SignalDef`, `Connection`, `ActionKind`, `SignalEvent`,
  `DispatchResult`, `dispatch_signal`, `common_signals` (12 universal),
  `with_common_signals` (merge/dedup), `signal_symbols` (codegen),
  `generate_signal_type_stubs` (full registry → `.d.luau`),
  `signal_contexts` (builder→syntax bridge). 20 unit tests.
- `style.rs` — `StyleProperties` (10-field all-`Option` struct),
  `resolve_cascade(app, page, node)`. Three-level CSS-like cascade:
  component > page > app; most-specific non-None field wins. 6 unit tests.
- `variant.rs` — `VariantAxis`, `VariantOption`, `apply_variant_overrides`,
  `apply_variant_defaults`. 6 unit tests.
- `source_map.rs` — `SourceMap`, `SourceSpan`, `PropSpan`,
  `MappedEmitter`. Bidirectional node↔source byte-range mapping
  (ADR-006). Forward: `span_for_node(id)`, reverse:
  `node_at_offset(byte)`. `MappedEmitter` is an alternative emitter
  that tracks property-level spans. 6 unit tests.
- `source_parse.rs` — `derive_document_from_source`, `parse_slint_value`,
  `format_slint_value`. Reconstructs a `BuilderDocument` from
  marker-annotated `.slint` source. Inverse of the render walker.
  15 unit tests.
- `live.rs` (behind `interpreter`) — `LiveDocument`, `LiveDiagnostic`,
  `SourceSelection`, `SourceEditError`. **Source-first**: `.slint`
  source is canonical; `BuilderDocument` is derived on demand.
  Constructors: `from_source` (primary), `from_document` (import).
  Source mutations: `edit_prop_in_source`, `insert_node_in_source`,
  `remove_node_from_source`, `move_node_in_source`. Editor sync via
  `apply_editor_changes`. Selection bridge: `select_node(id)` →
  editor line/col range, `node_at_cursor()` → node ID. 19 unit tests.
- `syntax_provider.rs` (behind `interpreter`) —
  `BuilderSyntaxProvider`. Compiler-backed `SyntaxProvider` impl that
  extends the lightweight `SlintSyntaxProvider` (prism-core) with real
  `slint-interpreter::Compiler` diagnostics, `SourceMap`-aware
  `ComponentRegistry` schema completions, and component `help_entry()`
  hover. 9 unit tests.
- `starter.rs` — 16 built-in Slint components + `register_builtins`.
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
