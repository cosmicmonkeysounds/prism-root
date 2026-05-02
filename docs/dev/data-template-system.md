# Data Template System — Inline Templates, Component Promotion, and Prefab Unification

## Problem

Prism's data-binding layer (facets) and composition layer (prefabs) overlap
in confusing ways. A facet defines where data comes from and how to map it
to UI. A prefab defines a reusable node subtree with exposed slots. The
connection between them — `FacetDef.prefab_id` + `Vec<FacetBinding>` +
`Vec<ExposedSlot>` — is a three-layer indirection that creates friction:

1. **Throwaway prefabs.** Most facets need a unique item template, so the
   user creates a prefab that's only ever used by one facet. The reuse
   primitive is being used for single-use purposes.

2. **Invisible rendering.** The facet component is a black box on the
   canvas. You can't see what it renders without running data through it.
   The prefab is edited in a separate panel, disconnected from the data
   context.

3. **Three-layer binding.** Schema field → `FacetBinding.item_field` →
   `ExposedSlot.target_prop` → inner node prop. This chain is hard to
   understand, hard to debug, and hard to explain in the properties panel.

4. **Scalar facets misuse templates.** Aggregate, Lookup (single result),
   and Script (single return) facets produce one value, not a list. Wrapping
   that value in a prefab is like putting one item in a list just to
   display it.

## Design

### Principle: Separate the Data Concern from the Reuse Concern

Facets handle data: where it comes from, how to filter/sort/aggregate it.
Components handle rendering: what something looks like, how it behaves.
The current pain point isn't the separation — it's the **prefab as
mandatory intermediary**. The fix is to make prefabs optional by adding
inline templates and direct prop binding.

### Three incremental changes

#### 1. Inline templates on FacetDef

A `FacetDef` can own its item template directly as a `Node` subtree.
No prefab indirection, no `ExposedSlot` layer. The bindings reference
the template's node props directly.

```rust
pub enum FacetTemplate {
    /// Reference a registered component or prefab by ID.
    /// This is the existing behavior — keeps backward compat.
    ComponentRef { component_id: String },

    /// Inline node subtree owned by this facet.
    /// Editable in context on the builder canvas.
    Inline { root: Node },
}

pub struct FacetDef {
    pub id: String,
    pub label: String,
    pub description: String,
    pub kind: FacetKind,
    pub schema_id: Option<String>,
    pub template: FacetTemplate,         // replaces prefab_id
    pub data: FacetDataSource,
    pub bindings: Vec<FacetBinding>,     // kept for ComponentRef
    pub variant_rules: Vec<FacetVariantRule>,
    pub layout: FacetLayout,
    pub resolved_data: Option<Vec<Value>>,
}
```

When the template is `Inline`, bindings are expressed as `{{field}}`
expressions in the template's node props:

```json
{
  "component": "card",
  "props": {
    "title": "{{record.title}}",
    "body": "{{record.description}}",
    "image_src": "{{record.thumbnail}}"
  }
}
```

The render walker resolves expressions against each data item before
rendering the template instance. No binding list needed — the binding
IS the prop value.

When the template is `ComponentRef`, the existing `FacetBinding` list
and `ExposedSlot` mechanism still work. This preserves backward
compatibility and supports the case where multiple facets legitimately
share the same template component.

#### 2. User-defined components replace prefabs for the reuse case

When an inline template is worth reusing, it gets **promoted to a
Component** — not saved as a separate prefab. The `ComponentRegistry`
already supports runtime registration via `PrefabComponent`. This
change makes the promotion explicit and first-class:

- "Save as Component" in the builder extracts the inline template
  subtree into a registered component with a generated schema
  derived from the `{{field}}` expressions in its props.
- The facet's `FacetTemplate` switches from `Inline` to
  `ComponentRef` pointing at the newly registered component.
- The new component appears in the palette like any built-in.

The `PrefabDef` struct and `ExposedSlot` system remain as the
implementation mechanism — they're sound internally — but the user
never thinks in terms of "prefabs" or "exposed slots." They think
"save this template as a reusable component."

#### 3. Scalar binding for non-list facets

Aggregate, Lookup (single), and Script (scalar return) facets don't
need a template at all. They produce a single value that should bind
directly to a prop on a sibling or parent widget.

```rust
pub enum FacetOutput {
    /// Repeat template per item (List, ObjectQuery, multi-result Lookup/Script).
    Repeated { template: FacetTemplate },

    /// Bind a single computed value to a target node's prop.
    Scalar {
        target_node: NodeId,
        target_prop: String,
    },
}
```

A `text` component showing a revenue total doesn't need a facet
component wrapper. The aggregate facet's output binds directly:

```
FacetDef {
    kind: Aggregate { operation: Sum, field: Some("amount") },
    output: Scalar { target_node: "revenue-label", target_prop: "body" },
}
```

The facet no longer needs to be a node in the document tree for the
scalar case. It's a data computation that feeds a result into an
existing widget.

## Render pipeline (updated)

```
FacetDef
  → resolve data per FacetKind (unchanged)
  → match output:
      Repeated:
        → match template:
            Inline:
              → for each item:
                  clone template root
                  resolve {{field}} expressions against item
                  apply variant rules
                  render child
            ComponentRef:
              → for each item:
                  clone component subtree
                  apply bindings (slot_key → item value)
                  apply variant rules
                  render child
        → wrap in layout container
      Scalar:
        → resolve to single value
        → set target_node.target_prop = value
        → (no render output from the facet itself)
```

## Builder canvas integration

### Inline template editing

When a facet with an `Inline` template is selected on the canvas:

1. The builder renders the template subtree visually, expanded with
   sample data from the first record (or placeholder text for
   `{{field}}` expressions when no data exists).
2. The template nodes are editable in-place — same drag/drop, property
   editing, and styling as any other node in the document.
3. Changes to the template are saved back to the `FacetDef.template`
   immediately (no separate "edit prefab" workflow).
4. The "Facet Data" section in the Properties panel shows the data
   source, schema, and bindings — but the bindings are inline in the
   template props, not a separate list.

### Component promotion

Right-click a facet's inline template → "Save as Component":

1. The template subtree is extracted into a new `PrefabDef`.
2. `{{field}}` expressions are converted to `ExposedSlot` entries.
3. The prefab is registered in `ComponentRegistry` as a component.
4. The facet switches to `ComponentRef { component_id }`.
5. The new component appears in the palette under "Custom."

### Scalar facets

Scalar facets don't render on the canvas. They appear in the
document's facet list (visible in the Data workflow page) with a
"→ target_node.target_prop" indicator showing where their value
feeds. The target widget shows a data-binding badge.

## Migration path

### Phase 1: Add `FacetTemplate::Inline`
- Add the `FacetTemplate` enum alongside the existing `prefab_id`.
- New facets default to `Inline` with a card-like template.
- Existing facets with `prefab_id` map to `ComponentRef`.
- The render walker checks `template` first; falls back to
  `prefab_id` for backward compatibility.
- No breaking changes to `FacetDef` serialization (serde default).

### Phase 2: Inline template editing on canvas
- The builder canvas renders inline templates with sample data.
- Template nodes are editable in the normal builder workflow.
- `{{field}}` expression resolution in the render walker.

### Phase 3: Scalar output for non-list facets
- Add `FacetOutput::Scalar` for Aggregate/Lookup/Script facets.
- Scalar facets bind directly to target node props.
- The facet component wrapper becomes optional.

### Phase 4: Component promotion
- "Save as Component" workflow for inline templates.
- Auto-generate schema from `{{field}}` expressions.
- Palette integration for user-defined components.

### Phase 5: Deprecate raw prefab_id
- Remove `prefab_id` from `FacetDef`.
- All existing prefab references migrate to `ComponentRef`.
- `PrefabDef` remains as the internal storage for user-defined
  components but is no longer directly referenced by facets.

## Relationship to the Widget System

The widget system (`docs/dev/widget-system.md`) describes how core
engines declare widgets via `WidgetContribution` with `WidgetTemplate`.
The `TemplateNode::Repeater` and `TemplateNode::DataBinding` in that
system are the widget-side equivalent of what facets do for
user-authored content.

The two systems converge:
- **Engine widgets** use `WidgetTemplate` (declarative, code-authored).
- **User facets** use `FacetTemplate` (inline or component-ref,
  GUI-authored).
- Both resolve to the same render pipeline: data → template → expand →
  render child.

The `DataQuery` in `WidgetContribution` is the engine-side equivalent
of `FacetDataSource` + `FacetKind`. A future unification could make
`DataQuery` the single data-resolution primitive, with facet kinds
mapping onto it.

## File layout

```
packages/prism-builder/src/
  facet.rs           ← +FacetTemplate, +FacetOutput enums
  document.rs        ← FacetDef uses FacetTemplate instead of prefab_id
  component.rs       ← render walker handles Inline + expression resolution
  prefab.rs          ← unchanged (internal storage for promoted components)
  render.rs          ← Scalar output binding in document-level walker

packages/prism-shell/src/
  panels/properties.rs  ← inline template editing, scalar target picker
  app.rs                ← expression resolution for {{field}} in templates
```
