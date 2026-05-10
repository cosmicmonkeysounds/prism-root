# prism-builder

The page builder that replaced Puck. Owns the component-type
registry, the document tree schema, the layout engine (Taffy-backed
CSS Grid / Flexbox / Block + free-form positioning), the unified
render pipeline (`lower_ui` → `prism_ui_runtime` for both shell
rendering and relay SSR), and the property-panel field factories.

> **Migration note:** Both the Slint stack and the parallel
> `HtmlBlock` / `HtmlRegistry` SSR walker have been deleted. SSR now
> flows through `ui_runtime::lower_semantic_html_with_registry`,
> which dispatches per-block `Component::lower_ui` impls — the same
> path the unified Taffy renderer consumes. The Slint source emitter
> (`render_slint`, `SlintEmitter`, `render_document_slint_source*`,
> `LiveDocument`, `BuilderSyntaxProvider`, `source_parse`,
> `source_map`) was deleted in the Phase 5 cutover follow-up; see
> `docs/dev/clay-migration-plan.md`.

## Build & Test
- `cargo build -p prism-builder`
- `cargo test -p prism-builder` — 313+ unit tests (post-cutover).
- `cargo build -p prism-builder --features luau` — pulls in the
  Luau-derive + `LuauComponent` glue when Luau-authored blocks need
  to live next to the Rust ones.

## Public surface
From `src/lib.rs`:

### Component contract
- `Component`, `ComponentId`, `RenderContext`, `RenderError` — the
  render trait. One method that matters: `lower_ui(ctx, node, style)
  -> prism_ui_runtime::layout::Node`. `schema()` returns
  `Vec<FieldSpec>` for the property panel; `signals()`,
  `variants()`, `toolbar_actions()`, `help_entry()` are the rest of
  the surface.
- `Block` — single-trait sugar layer over `Component`. Implementing
  `Block` gives you a `Component` for free via the blanket impl in
  `src/block.rs`. New built-ins implement `Block`.
- `register_block(&mut registry, Arc::new(YourBlock))` — one-line
  registration.
- `ComponentRegistry`, `RegistryError` — DI entry point. Implements
  `HelpProvider` to collect entries from registered components.
- `starter::register_builtins(&mut ComponentRegistry)` — seeds the
  17-block default catalog (`text`, `image`, `container`, `form`,
  `input`, `button`, `card`, `code`, `divider`, `spacer`, `columns`,
  `list`, `table`, `tabs`, `accordion`, `facet`, `graph-view`).

### SSR
The relay calls
`ui_runtime::lower_semantic_html_with_registry(doc, registry)`,
which walks every block's `Component::lower_ui` impl and emits
semantic HTML via `prism_ui_runtime::backends::semantic_html::lower`.
One declaration per block, two consumers (shell renderer + relay
SSR). `html.rs` (`Html` buffer + `escape_text` / `escape_attr`)
remains as a tiny chrome-composition helper for `prism-relay` page
wrappers and the `prism-luau-derive` macro.

### Layout engine (ADR-003)
- `PageLayout`, `PageSize`, `Orientation`, `TrackSize` — structural
  page properties (size, margins, bleed, CSS Grid template).
- `LayoutMode` (`Flow` | `Free` | `Absolute` | `Relative`),
  `FlowProps`, `AbsoluteProps`, `Dimension`, `FlowDisplay`,
  `FlexDirection`, `AlignOption`, `JustifyOption`,
  `GridPlacement` — per-node layout participation.
  - `Flow(FlowProps)` — positioned by parent's flex/grid/block flow.
  - `Free` — `position: absolute` in Taffy, Transform2D only.
  - `Absolute(AbsoluteProps)` — removed from flow, positioned by
    `Transform2D.position` + `Transform2D.anchor` relative to the
    parent's rect.
  - `Relative(FlowProps)` — participates in flow, then Transform2D
    position is applied as a post-flow offset (CSS `position: relative`).
- `compute_layout(doc, viewport_size) -> ComputedLayout` — Taffy
  layout pass + transform propagation, returns per-node
  `NodeLayout { rect, transform }`.
- `Node` carries `layout_mode: LayoutMode` and
  `transform: Transform2D` (from `prism-core::foundation::spatial`).
- `BuilderDocument` carries `page_layout: PageLayout`.
- `GridCell` — recursive grid tree (`Leaf` with optional `node_id`,
  or `Split` with direction/tracks/gap/children).
- `SplitDirection` (`Horizontal` | `Vertical`), `CellEdge`
  (`Top`/`Bottom`/`Left`/`Right`).
- `FlatCell`, `EdgeHandle`, `path_to_string`/`path_from_string`,
  `compute_track_sizes`, `GridEditError` — grid query/layout helpers.
- `PageLayout::has_grid()`, `leaf_count()`, `flatten_cells()`,
  `flatten_edge_handles()`, `insert_at_edge`, `remove_cell`,
  `place_node_at`, `clear_cell` — interactive grid manipulation.
- `BuilderDocument::place_in_grid(node_id, path)`,
  `BuilderDocument::page_shell()` — page factory: 1×1 grid, root
  container, 24/32px margins.

### Style cascade
- `StyleProperties` — 10-field all-`Option` struct (font_family,
  font_size, font_weight, line_height, letter_spacing, color,
  background, accent, base_spacing, border_radius). Serde-friendly
  with `skip_serializing_if = "Option::is_none"`.
- `resolve_cascade(app, page, node)` — three-level cascade,
  most-specific non-None wins.
- `PrismApp`, `Page`, `Node` all carry `style: StyleProperties`.
- `PrismApp` — multi-page application container. `Vec<Page>`,
  `active_page`, `add_page`, `remove_page`, `find_page_by_route`,
  `find_page_by_id`, `active_document()`, `active_document_mut()`.
  `NavigationConfig { style: NavigationStyle }`
  (Tabs/Sidebar/BottomBar/None).

### Composition (ADR-004)
- `Modifier`, `ModifierKind`, `modifier_schema(kind)` — attachable
  behaviors (ScrollOverflow, HoverEffect, EnterAnimation,
  ResponsiveVisibility, Tooltip, AccessibilityOverride).
- `PrefabDef`, `PrefabComponent`, `ExposedSlot` — user-authored
  compound components. `PrefabComponent` implements `Component`.
- `ResourceDef`, `ResourceId`, `ResourceKind`,
  `resolve_resource_refs(props, resources)` — typed shareable data
  via `{ "$ref": "resource:<id>" }`.
- `SignalDef`, `Connection`, `ConnectionId`, `ActionKind`,
  `SignalEvent`, `DispatchResult`, `dispatch_signal`,
  `common_signals` (12 universal), `with_common_signals`,
  `signal_symbols`, `generate_signal_type_stubs`,
  `signal_contexts` — runtime signal dispatch + codegen.
- `VariantAxis`, `VariantOption`, `apply_variant_overrides`,
  `apply_variant_defaults` — named bundles of prop overrides.

### Asset resolution
- `AssetSource` — VFS (content-addressed `BinaryRef`) or URL.
  `from_prop`, `to_html_src`, `to_prop`.
- `collect_vfs_hashes(node)` — returns all VFS hashes referenced.
- `FileFieldConfig` — MIME filter for `FieldKind::File`.

### Shared
- `BuilderDocument`, `Node`, `NodeId` — serializable document tree.
  Carries `resources`, `connections`, `prefabs`, `facets`.
- `FieldSpec`, `FieldKind`, `NumericBounds`, `SelectOption`,
  `FieldValue` — property-panel field factories.
- `Html`, `escape_text`, `escape_attr` — HTML buffer helpers.

## Architecture
Modules in `src/` (excluding `lib.rs`):

- `app.rs` — `PrismApp`, `Page`, `AppIcon`, `NavigationConfig`,
  `NavigationStyle`. `Page::ensure_source` is now a no-op (legacy
  hook from the Slint era).
- `asset.rs` — `AssetSource`, `collect_vfs_hashes`, `FileFieldConfig`.
- `block.rs` — `Block` trait + blanket `impl<T: Block> Component`.
  One declaration; the registry sees a `Component`.
- `component.rs` — `Component` trait + `ComponentId` + `RenderError`
  + `RenderContext` (ad-hoc host-side carrier).
- `core_widget.rs` — `CoreWidgetBlock` wraps a `WidgetContribution`
  from a core engine into a `Block`. `collect_all_contributions`
  fans out across every domain/interaction module that exposes
  `widget_contributions()`. `register_core_widgets` plugs them in.
- `document.rs` — `BuilderDocument` + `Node` + `NodeId`.
- `facet/` — `FacetDef`, `FacetKind`, `FacetDataSource`,
  `FacetTemplate`, `FacetOutput`, `FacetBinding`, `FacetLayout`,
  `AggregateOp`, `ScriptLanguage`, `FacetVariantRule`,
  `FacetComponent`, `ResolvedFacetData`, `FacetSchema`,
  `SchemaField`, `SchemaFieldKind`, `FacetRecord`,
  `ValidationError`, `FACET_KIND_TAGS`, `AGGREGATE_OP_TAGS`.
  `apply_scalar_bindings`, `evaluate_calculations`,
  `promote_inline_to_component`, `parse_filter_expr`,
  `resolve_template_expressions`, `apply_aggregate`. Full template
  + binding resolution; the only `Component` impl in here is
  `FacetComponent`, which lowers via `lower_ui` like every other
  block.
- `html.rs` — `Html` buffer + escape helpers (chrome composition).
- `layout.rs` — Taffy layout (ADR-003). `PageLayout`, `LayoutMode`
  variants, `compute_layout`.
- `modifier.rs` — `ModifierKind` enum, `Modifier` struct,
  `modifier_schema(kind)`. Render walker applies modifiers as
  wrapper layers via the `lower_ui` pipeline.
- `prefab.rs` — `PrefabDef`, `ExposedSlot`, `PrefabComponent`.
- `project.rs` — `ProjectFile`, `SavedApp`, `SavedPage`,
  `FILE_EXTENSION`, `FORMAT_VERSION`. `SavedPage::source` is a
  pass-through string field; the auto-emit-from-document path was
  retired with Slint.
- `registry.rs` — `ComponentRegistry` + field-factory primitives.
- `resource.rs` — `ResourceDef`, `ResourceKind`,
  `resolve_resource_refs`.
- `schemas.rs` — shared component field definitions.
- `signal.rs` — `SignalDef`, `Connection`, `ActionKind`,
  `SignalEvent`, `DispatchResult`, `dispatch_signal`,
  `common_signals`, `with_common_signals`, `signal_symbols`,
  `generate_signal_type_stubs`, `signal_contexts`.
- `starter.rs` — 17 built-in blocks + `register_builtins`.
- `style.rs` — `StyleProperties` + `resolve_cascade`.
- `ui_lower.rs` — shared `LowerCtx` + helpers (`container_with`,
  `synthetic_container`, `bare_container`, `text_node`,
  `spacer_node`, `image_node`, `with_semantic`, `uniform_radius`,
  `parse_color`, `prop_str`, `prop_bool`).
- `ui_resolver.rs` — `<shell.*>` tag resolution helpers consumed by
  the runtime's interpret pipeline.
- `ui_runtime.rs` — `BuilderDocument` →
  `prism_ui_runtime::layout::Node` translator +
  `lower_semantic_html_with_registry` (relay SSR entry).
- `variant.rs` — `VariantAxis`, `VariantOption`, override
  application.
- `luau_component.rs` (feature `luau`) — `LuauComponent`,
  `ActiveRegistry`, `LuauRenderRegistry`, `VirtualNode`.
- `script_loader.rs` (feature `luau`) — `load_widgets`,
  `LoadReport`, `ScriptLoadError`.

## Adding a new block

Use the unified `Block` trait (`src/block.rs`) — one impl, one
render method (`lower_ui`), one registration call.

1. Add a struct implementing `Block` in `src/starter.rs` (or a new
   module). Override only the methods that need bespoke behaviour;
   `lower_ui` defaults to a generic container, `signals` to the 12
   common signals.
2. Implement `schema()` using `FieldSpec` builders.
3. Optionally implement `signals()` / `variants()` /
   `toolbar_actions()` / `help_entry()`.
4. Add a row to `register_builtins` (`reg!("id", BlockType)`) — one
   line per builtin.
5. Add unit tests covering `lower_ui`.

Core-engine widgets go through `CoreWidgetBlock` — a single `Block`
impl wrapping a `WidgetContribution`, registered en masse by
`register_core_widgets`.

## Dependencies
- `prism-core` — `design_tokens`, `language::codegen::SourceBuilder`,
  `foundation::geometry`, `foundation::spatial`, `widget`.
- `glam` — SIMD-accelerated 2D math (Vec2, Affine2).
- `taffy` — CSS Grid + Flexbox + Block layout engine.
- No `slint`, no `slint-interpreter`, no `spin_on`. No hard dep on
  `prism-shell` or `prism-relay`.
