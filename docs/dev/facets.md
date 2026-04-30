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

Future Phase 2: `Query { source: ResourceId, filter: String, sort_by: String }` for filtering and
sorting without changing the underlying resource.

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

## Phase plan

| Phase | Scope |
|-------|-------|
| 1 (now) | `Static` + `Resource` data sources, `Row`/`Column` layout, full Slint + HTML render |
| 2 | `Query` data source with filter/sort expressions |
| 3 | Live data refresh in Studio (reload resource, re-render facet in place) |
| 4 | Facet-level variant overrides (e.g. alternate card style for "featured" items) |
