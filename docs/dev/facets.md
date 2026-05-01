# Facets — Programmatic List Generation for Prism Builder

## Problem

Content in Prism Builder is authored one component at a time. Prefabs reduce repetition for
structurally-similar blocks, but each instance must be placed and configured by hand. There is no
way to say "generate one card per file in this folder" or "create a nav item for each page in my
site" — the relationship between data and layout is entirely manual.

## Solution: Facets

A **Facet** is a programmatic layout generator inspired by FileMaker Pro's Facets feature. It
combines a **prefab template** with a **data source** to produce an arbitrary number of prefab
instances, one per data item, with field values injected automatically via **bindings**.

### Use cases

- **Portfolio grid** — `items.json` array of `{ title, image, href }` → image-card prefab
- **File gallery** — list of filenames → file-item prefab
- **Navigation** — array of page titles + slugs → nav-link prefab
- **Playlist** — array of tracks → track-row prefab
- **Team section** — team members JSON → bio-card prefab

## Data model

### `FacetDef`

```
FacetDef {
    id: String                    // stable identifier, e.g. "facet:portfolio"
    label: String                 // display name
    description: String
    prefab_id: ComponentId        // which PrefabDef to use as the item template
    data: FacetDataSource         // where items come from
    bindings: Vec<FacetBinding>   // slot_key → item field path
    layout: FacetLayout           // how instances are arranged
}
```

### `FacetDataSource`

```
Static  { items: Vec<Value> }    // inline JSON array — hand-authored or imported
Resource { id: ResourceId }      // reference to a DataSource resource whose `data` is an array
```

**Phase 2 (landed):** `Query { source: ResourceId, filter: Option<String>, sort_by: Option<String> }`
— filter and sort a resource array without mutating it. `filter` accepts
`"field == value"`, `"field != value"`, or `"field"` (truthy check). `sort_by` is a
dot-notation path; prefix with `-` for descending (e.g. `"-date"`). Full serde round-trip,
13 unit tests, properties panel exposes filter + sort fields when source kind is `query`.

### `FacetBinding`

```
FacetBinding {
    slot_key: String    // ExposedSlot key on the prefab
    item_field: String  // dot-notation path in each data item, e.g. "meta.title"
}
```

At render time each binding is resolved by:
1. Finding the `ExposedSlot` whose `key == slot_key` on the referenced `PrefabDef`
2. Reading the value from the item using the dot-path in `item_field`
3. Calling `apply_prop_to_node` on a cloned prefab tree to inject the value into the right node

### `FacetLayout`

```
FacetLayout {
    direction: FacetDirection   // Row | Column (default Column)
    gap: f32                    // spacing between instances
    wrap: bool                  // allow row wrapping (reserved, Phase 2)
    columns: Option<u32>        // fixed column count (reserved, Phase 2)
}
```

Phase 1 maps `Column` → `VerticalLayout`, `Row` → `HorizontalLayout`.

## Integration

### `BuilderDocument`

`BuilderDocument` gains a new `facets: IndexMap<String, FacetDef>` field (serde-defaulted to
empty, so existing documents deserialize cleanly).

### Render contexts

`RenderSlintContext` and `HtmlRenderContext` each gain two new reference fields:

```rust
pub prefabs: &'a IndexMap<String, PrefabDef>,
pub facets:  &'a IndexMap<String, FacetDef>,
```

These are filled from the document at the start of every render walk.

### `facet` component

A new built-in component with `ComponentId = "facet"` is registered in both the Slint and HTML
catalogs. Its schema exposes two props:

- `facet_id` (text, required) — which `FacetDef` to expand
- `max_items` (integer, optional) — cap the number of items rendered

At render time the component:
1. Looks up the `FacetDef` by `facet_id`
2. Looks up the `PrefabDef` by `facet.prefab_id`
3. Resolves the data source to `Vec<Value>`
4. Applies `max_items` truncation
5. Emits a `VerticalLayout` / `HorizontalLayout` wrapper
6. For each item: clones the prefab root, applies all bindings, calls `ctx.render_child`

Missing facet_id → renders a labelled placeholder rectangle (graceful degradation in Studio).
Missing prefab_id → `RenderError::Failed`.

## File layout

```
packages/prism-builder/src/
  facet.rs           ← new: FacetDef, FacetDataSource, FacetLayout, FacetComponent, FacetHtmlBlock
  document.rs        ← +facets field on BuilderDocument
  component.rs       ← +prefabs, +facets on RenderSlintContext
  html_block.rs      ← +prefabs, +facets on HtmlRenderContext
  render.rs          ← pass &doc.prefabs / &doc.facets to context constructors
  schemas.rs         ← +facet() schema fn
  starter.rs         ← register FacetComponent
  html_starter.rs    ← register FacetHtmlBlock
  lib.rs             ← pub mod facet + re-exports
```

## Phase 3: Facet Schemas (FileMaker-inspired structured data)

### Problem

Facet IDs are freeform strings. Bindings map `slot_key → item_field` as arbitrary
strings — nothing validates that a field exists, that its type matches the prefab slot,
or that records contain the expected keys. The properties panel can't offer structured
editing because it doesn't know the shape of the data. Adding items pushes empty `{}`
JSON objects. The result: Facets exist in the type system but can't produce anything
useful without hand-authoring raw JSON.

### Solution: FacetSchema

Inspired by FileMaker Pro's Manage Database dialog. A **FacetSchema** defines the shape
of a facet's data — typed fields with names, defaults, validation, and value lists. Each
`FacetDef` references a schema by ID, and bindings become validated mappings between
schema fields and prefab slots. Records are structured objects validated against their
schema, and the properties panel renders proper field editors per type.

### `FacetSchema`

```rust
FacetSchema {
    id: FacetSchemaId,              // e.g. "schema:portfolio"
    label: String,                  // "Portfolio Item"
    description: String,
    fields: Vec<SchemaField>,       // ordered field definitions
}
```

### `SchemaField`

```rust
SchemaField {
    key: String,                    // "title", "due_date", "status"
    label: String,                  // "Title", "Due Date", "Status"
    kind: SchemaFieldKind,          // typed discriminator
    required: bool,
    default_value: Option<Value>,   // serde_json::Value
}
```

### `SchemaFieldKind`

```
Text                              // single-line string
Number { min, max }               // f64 with optional bounds
Integer { min, max }              // i64 with optional bounds
Boolean                           // true/false toggle
Date                              // ISO 8601 date string
Color                             // hex color "#RRGGBB"
Image                             // URL or VFS asset reference
Url                               // validated URL string
Select { options }                // value list (like FileMaker)
Calculation { formula }           // derived value (formula over sibling fields)
```

### `FacetRecord`

Replaces `Vec<Value>` in `FacetDataSource::Static`:

```rust
FacetRecord {
    id: String,                     // unique record ID (e.g. "rec:1")
    fields: IndexMap<String, Value>, // key → value, validated against schema
}
```

### Updated `FacetDef`

```rust
FacetDef {
    id: String,
    label: String,
    description: String,
    schema_id: Option<FacetSchemaId>,  // references a FacetSchema (None = legacy untyped)
    prefab_id: ComponentId,
    data: FacetDataSource,
    bindings: Vec<FacetBinding>,
    layout: FacetLayout,
}
```

### Updated `BuilderDocument`

```rust
BuilderDocument {
    ...
    facet_schemas: IndexMap<FacetSchemaId, FacetSchema>,  // new
    facets: IndexMap<String, FacetDef>,
}
```

### Schema Designer workflow page

A new DaVinci Resolve-style bottom-bar mode ("Data" page) with a dedicated panel layout:

- **Left panel**: Schema list — all `FacetSchema`s in the document. Click to select,
  buttons to create/delete.
- **Center panel**: Field definition table — add, edit, reorder, delete fields. Each row
  shows key, label, kind (dropdown), required (toggle), default value.
- **Right panel**: Record editor — when a schema is selected, shows all records as a
  scrollable form. Each record renders proper field editors per `SchemaFieldKind`
  (text inputs, number spinners, color pickers, select dropdowns, etc.).

The Schema Designer panel (`PanelKind::SchemaDesigner`) renders all three zones in one
panel via the dock system.

### Updated properties panel

When a `facet` component is selected:

1. **Schema** dropdown — select from defined schemas (not freeform text)
2. **Bindings** — schema fields → prefab slots as paired dropdowns (both sides enumerated)
3. **Record count** — with Add/Clear actions that create schema-validated records
4. **Validation** — indicators when records have missing required fields

### Validation

`FacetSchema::validate_record(record) -> Vec<ValidationError>` checks:
- Required fields present and non-null
- Number/Integer within bounds
- Select value is one of the defined options

Validation runs at edit time (properties panel highlights errors) and at render time
(invalid records are skipped with a warning, not a hard error).

### Calculation fields

`SchemaFieldKind::Calculation { formula }` defines derived fields. The formula is a
simple expression language supporting:
- Field references: `{field_key}` (e.g. `{price}`, `{quantity}`)
- Arithmetic: `+`, `-`, `*`, `/`
- String concatenation: `{first_name} + " " + {last_name}`
- Numeric formatting: `format({price}, "$0.00")`

Calculation fields are read-only in the record editor and computed at render time.

## Phase plan

| Phase | Scope |
|-------|-------|
| 1 ✅ | `Static` + `Resource` data sources, `Row`/`Column` layout, full Slint + HTML render |
| 2 ✅ | `Query` data source with filter/sort expressions |
| 3 ✅ | `FacetSchema` + `FacetRecord` + Schema Designer workflow page + validated bindings |
| 4 | Live data refresh in Studio (reload resource, re-render facet in place) |
| 5 | Facet-level variant overrides (e.g. alternate card style for "featured" items) |
| 6 | Calculation fields with expression evaluation |
