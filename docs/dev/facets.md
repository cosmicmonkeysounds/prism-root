# Facets — Data-Driven Content Generation for Prism Builder

## Problem

Content in Prism Builder is authored one component at a time. Prefabs reduce repetition for
structurally-similar blocks, but each instance must be placed and configured by hand. There is no
way to say "generate one card per file in this folder" or "create a nav item for each page in my
site" — the relationship between data and layout is entirely manual.

The Phase 1–3 facet system solved the simple case (repeat a prefab per data item) but hardcoded
a single behavior into `FacetDef`. Every facet was a list with a max-items cap. There was no way
to derive content from the object graph, compute aggregates, or script custom data pipelines.
The facet designer panel was a flat list of property rows split across two workflow pages — the
Schema Designer and the Properties panel — with no cohesive editing experience.

## Solution: Facet Kinds

A **Facet** is a data-driven content generator. Each facet has a **kind** that determines how
it produces output. The kind is selected from a dropdown in the facet designer panel; switching
kinds swaps the property editor to show that kind's specific fields.

### Facet kinds

| Kind | Description |
|------|-------------|
| `List` | Repeat a prefab template once per data item. The original facet behavior. |
| `ObjectQuery` | Query the object graph (Loro `CollectionStore`) by entity type with filter/sort. Returns `GraphObject`s bound to a prefab. |
| `Script` | A Luau script that returns data items. Full access to the object graph and document context. |
| `Aggregate` | Reduce a data source to a single computed value (count, sum, min, max, avg, join). Renders as a single prefab instance with the result bound. |
| `Lookup` | Follow object references from a source entity to pull related data. Like FileMaker relationships. |

### Use cases

- **Portfolio grid** (List) — `items.json` array → image-card prefab
- **Navigation** (List) — page titles/slugs → nav-link prefab
- **Recent activity feed** (ObjectQuery) — query `Activity` entities, sort by `-created_at`, limit 10
- **Dashboard KPI** (Aggregate) — count of `Order` entities where `status == "active"`
- **Custom report** (Script) — Luau script that queries multiple entity types, joins and transforms data
- **Related items** (Lookup) — from a `Project` entity, follow `has_member` edges to `User` entities
- **AI-generated content** (Script) — Luau script that calls external API and returns structured data

## Data model

### `FacetKind`

```rust
#[serde(tag = "type", rename_all = "kebab-case")]
enum FacetKind {
    List,
    ObjectQuery {
        entity_type: String,
        filter: Option<String>,
        sort_by: Option<String>,
        limit: Option<usize>,
    },
    Script {
        source: String,            // Luau source code
        language: ScriptLanguage,  // Luau (future: visual graph)
    },
    Aggregate {
        operation: AggregateOp,    // Count, Sum, Min, Max, Avg, Join
        field: Option<String>,     // field to aggregate (not needed for Count)
    },
    Lookup {
        source_entity: String,     // entity type to start from
        edge_type: String,         // relationship to follow
        target_entity: String,     // entity type at the other end
    },
}
```

### `AggregateOp`

```rust
enum AggregateOp {
    Count,
    Sum,
    Min,
    Max,
    Avg,
    Join { separator: String },
}
```

### `ScriptLanguage`

```rust
enum ScriptLanguage {
    Luau,
}
```

Future: `VisualGraph` for node-graph-authored facets (Prism Syntax / codegen).

### Updated `FacetDef`

```rust
FacetDef {
    id: String,
    label: String,
    description: String,
    kind: FacetKind,                      // NEW — determines behavior + panel UI
    schema_id: Option<FacetSchemaId>,
    prefab_id: ComponentId,
    data: FacetDataSource,                // used by List + Aggregate; ignored by ObjectQuery/Script
    bindings: Vec<FacetBinding>,
    layout: FacetLayout,
    #[serde(skip)]
    resolved_data: Option<Vec<Value>>,    // pre-populated by shell before render
}
```

### Data resolution per kind

| Kind | Data source | Resolution |
|------|-------------|------------|
| `List` | `FacetDataSource` (Static/Resource/Query) | Resolves to `Vec<Value>`, one prefab per item |
| `ObjectQuery` | Object graph (`CollectionStore`) | Queries by entity_type + filter + sort, converts `GraphObject`s to `Vec<Value>` |
| `Script` | Luau script output | Executes script via `luau_module::exec`, expects JSON array return |
| `Aggregate` | `FacetDataSource` | Resolves source to `Vec<Value>`, applies operation, renders single prefab |
| `Lookup` | Object graph edges | Follows edges from source entity, collects targets as `Vec<Value>` |

### `FacetDataSource` (unchanged)

```
Static  { items: Vec<Value>, records: Vec<FacetRecord> }
Resource { id: ResourceId }
Query { source: ResourceId, filter: Option<String>, sort_by: Option<String> }
```

These remain the data sources for `List` and `Aggregate` kinds. `ObjectQuery`, `Script`,
and `Lookup` carry their own data resolution logic inside `FacetKind`.

### Pre-resolution pattern

`prism-builder` has no dependency on `prism-daemon`, so it cannot call Luau or query the
object graph. The shell layer resolves external data before the render walker runs:

1. `resolve_facet_data(&mut BuilderDocument, &CollectionStore)` in `prism-shell/src/app.rs`
   iterates all facets and populates `resolved_data` for kinds that need external execution.
2. **Script** → `luau_module::exec(source, None)`. The script's return value is stored as
   `resolved_data` (array returns become items; scalar returns wrap in a single-element vec).
3. **ObjectQuery** → `CollectionStore::list_objects` filtered by `entity_type`, then the
   same filter/sort expression syntax as `FacetDataSource::Query` (via `evaluate_filter` /
   `value_sort_key`), truncated by `limit`. `GraphObject`s are serialized to `Value` via
   `serde_json::to_value`.
4. **Lookup** → for each instance of `source_entity`, queries `list_edges` by `edge_type`,
   resolves targets via `get_object`, filters to `target_entity` type, deduplicates by
   target ID.
5. `FacetDef::resolve_items()` reads `resolved_data` for ObjectQuery/Script/Lookup kinds;
   List resolves from `FacetDataSource`; Aggregate reduces List data via `apply_aggregate`.
6. The resolution runs in `sync_ui_impl` right before `push_wysiwyg_preview`, only when
   the document has facets that need it (no clone penalty for the common case).
7. `resolved_data` is `#[serde(skip)]` — never persisted, always recomputed.

## Facet designer panel

The facet properties panel is the primary editing surface. When a `facet` component is
selected, the Properties panel renders a kind-aware editor:

### Common header (all kinds)

1. **Kind** dropdown — `List | ObjectQuery | Script | Aggregate | Lookup`
2. **Label** — display name
3. **Prefab** dropdown — which prefab template to render with
4. **Schema** dropdown — optional schema for structured data

### Kind-specific sections

**List:**
- Data source selector (Static / Resource / Query)
- Static: record editor (schema-validated forms per item)
- Resource: resource ID picker
- Query: source ID + filter expression + sort field
- Layout: direction (row/column), gap, wrap, columns
- Bindings: schema field → prefab slot paired dropdowns

**ObjectQuery:**
- Entity type selector (populated from object graph entity definitions)
- Filter expression (field-based, same syntax as Query data source)
- Sort field
- Limit
- Bindings: entity field → prefab slot

**Script:**
- Embedded code editor (Luau)
- Language selector (Luau; future: Visual Graph)
- Bindings: output field → prefab slot
- Test/preview button: runs script and shows output

**Aggregate:**
- Data source selector (same as List)
- Operation dropdown (Count / Sum / Min / Max / Avg / Join)
- Field selector (which field to aggregate; disabled for Count)
- Join separator (only shown for Join operation)
- Result binding: which prefab slot receives the computed value

**Lookup:**
- Source entity type
- Edge/relationship type
- Target entity type
- Bindings: target entity field → prefab slot

## Luau script facets

Script facets execute Luau code via `prism_daemon::luau_module::exec`. The script
receives a context table and must return a JSON-serializable array:

```lua
-- Context: `prism` table with document and graph access
local items = {}
for _, obj in prism.query("BlogPost", { status = "published" }) do
    table.insert(items, {
        title = obj.name,
        date = obj.data.published_at,
        excerpt = string.sub(obj.data.body, 1, 200),
    })
end
table.sort(items, function(a, b) return a.date > b.date end)
return items
```

The returned array is treated identically to a `List` facet's resolved data —
each item is bound to a prefab instance via the bindings map.

Script facets are authored in the facet designer panel's embedded code editor,
which provides the same Luau editing experience as the CodeEditor panel (syntax
highlighting, completions via Prism Syntax). The script can also be authored
externally and referenced by path (future).

## Object graph integration

`ObjectQuery` and `Lookup` facets resolve data from the Loro-backed
`CollectionStore` rather than from `ResourceDef` data sources. At render time:

1. The facet component requests data from the shell's graph store
2. `GraphObject`s are converted to `serde_json::Value` via their `data` field
3. Standard fields (`id`, `name`, `object_type`, `created_at`, `updated_at`) are
   merged into the value so bindings can reference them
4. The result `Vec<Value>` feeds into the normal prefab expansion pipeline

This means ObjectQuery facets work with the same binding system as List facets —
the only difference is where the data comes from.

## FacetSchema (unchanged from Phase 3)

`FacetSchema` defines the shape of a facet's data — typed fields with names,
defaults, validation, and value lists. Schemas are shared across facets; a
schema can be used by any facet kind.

```rust
FacetSchema {
    id: FacetSchemaId,
    label: String,
    description: String,
    fields: Vec<SchemaField>,
}
```

`SchemaField` kinds: Text, Number, Integer, Boolean, Date, Color, Image, Url,
Select, Calculation.

### Calculation fields

`SchemaFieldKind::Calculation { formula }` fields are evaluated at resolve time
via `prism_core::language::expression::evaluate_expression`. The formula can
reference sibling fields by name — bare identifiers are automatically wrapped
by `wrap_bare_identifiers` so they resolve through the expression evaluator's
`ContextStore`.

```rust
// Schema field: { key: "total", kind: Calculation { formula: "price * qty" } }
// Item: { "price": 10.0, "qty": 3 }
// → After evaluate_calculations: { "price": 10.0, "qty": 3, "total": 30.0 }
```

The evaluation pipeline:
1. `FacetDef::resolve_items()` resolves data items as before (List/ObjectQuery/etc.)
2. If the facet has a `schema_id` pointing to a schema with Calculation fields,
   `evaluate_calculations(&mut items, &schema)` runs over each item
3. For each item, a `HashMap<String, ExprValue>` context is built from the item's fields
4. Each Calculation field's formula is evaluated and the result stored back into the item
5. All 26+ built-in expression functions are available (concat, sum, avg, abs, round, etc.)

Calculation fields participate in Aggregate reduction — e.g. you can sum a
calculated "total" field across all items.

## Render pipeline

The render pipeline is kind-agnostic. Every facet kind ultimately produces a
`Vec<Value>` (data items) that flows through the same expansion:

```
FacetKind → resolve to Vec<Value>
  → apply max_items truncation
  → for each item:
      clone prefab root
      apply bindings (slot_key → item value)
      render child
  → wrap in layout container (VerticalLayout / HorizontalLayout)
```

The exception is `Aggregate`, which produces a single value and renders one
prefab instance with the aggregate result bound.

## Live data refresh

The `facet.refresh` command triggers a full data re-resolution cycle:

1. `sync_builder_document()` is called on the shell
2. For Script/ObjectQuery/Lookup kinds, `resolve_facet_data` re-executes
   scripts, re-queries the object graph, and re-follows lookup edges
3. For List/Aggregate kinds with Resource data sources, the resource data
   is re-read from `doc.resources`
4. Calculation fields are re-evaluated
5. The render walker re-expands prefabs with the fresh data

The command is accessible via:
- Command palette: "Refresh Facet Data"
- Context menu: right-click a facet component → "Refresh Data"

## File layout

```
packages/prism-builder/src/
  facet.rs           ← FacetDef, FacetKind, FacetDataSource, FacetLayout,
                       AggregateOp, ScriptLanguage,
                       FacetComponent, FacetHtmlBlock,
                       FacetSchema, SchemaField, FacetRecord
  document.rs        ← +facet_schemas, +facets on BuilderDocument
  component.rs       ← +prefabs, +facets on RenderSlintContext
  html_block.rs      ← +prefabs, +facets on HtmlRenderContext
  render.rs          ← pass &doc.prefabs / &doc.facets to context constructors
  schemas.rs         ← +facet() schema fn
  starter.rs         ← register FacetComponent
  html_starter.rs    ← register FacetHtmlBlock
  lib.rs             ← pub mod facet + re-exports

packages/prism-shell/src/
  panels/properties.rs  ← kind-aware facet_rows() with per-kind sections
  panels/schema.rs      ← SchemaDesignerPanel (unchanged)
  app.rs                ← apply_facet_edit handles kind-specific keys
```

## Phase plan

| Phase | Scope |
|-------|-------|
| 1 ✅ | `Static` + `Resource` data sources, `Row`/`Column` layout, full Slint + HTML render |
| 2 ✅ | `Query` data source with filter/sort expressions |
| 3 ✅ | `FacetSchema` + `FacetRecord` + Schema Designer workflow page + validated bindings |
| 4 ✅ | `FacetKind` enum: List, ObjectQuery, Script, Aggregate, Lookup. Kind-aware properties panel. Serde backward compat. |
| 5 ✅ | Luau script execution for Script facets — `resolve_facet_data()` in shell calls `luau_module::exec`, populates `FacetDef.resolved_data` before the render walker. |
| 6 ✅ | Object graph data resolution for ObjectQuery + Lookup — `resolve_facet_data` queries `CollectionStore` via `ObjectFilter`/`EdgeFilter`, reuses `evaluate_filter`/`value_sort_key` for expression-based filter+sort. |
| 7 ✅ | Calculation fields with expression evaluation — `evaluate_calculations()` in `facet.rs` evaluates `SchemaFieldKind::Calculation { formula }` fields via `prism_core::language::expression::evaluate_expression`. Wired into `resolve_items()` when a schema is set. |
| 8 ✅ | Live data refresh in Studio — `facet.refresh` command re-triggers `sync_builder_document()`, which re-runs `resolve_facet_data` for Script/ObjectQuery/Lookup kinds and re-renders. Available in command palette and facet context menu. |
| 9 ✅ | Facet-level variant overrides — `FacetVariantRule` maps data field conditions to prefab variant axis selections. `evaluate_variant_rules()` sets axis key props on cloned prefab roots per item; existing `apply_variant_defaults` picks them up. Properties panel exposes rule CRUD. |
| 10 ✅ | Visual graph authoring for Script facets — `ScriptLanguage::VisualGraph` variant stores a `ScriptGraph` on the facet. `sync_script_language()` bidirectionally decompiles/compiles via `LuauVisualLanguage`. Properties panel shows language selector, graph info, and compiled source preview. |
