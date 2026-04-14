# Facet View

A **Facet View** renders a `FacetDefinition` — Prism's schema-driven UI primitive — inside the Puck builder. One Facet Definition can show up as a **form**, a **list**, a **table**, a **report**, or a **card grid** without duplicating its schema.

## When to use it

Use Facet View when you've already defined a `FacetDefinition` elsewhere (via the Facet Designer, `Shift+X`) and want to embed its view on a page. Typical examples:

- An "Add Task" form embedded in a landing page.
- A table of recent orders inside an admin dashboard.
- A card grid of products driven by an inventory schema.

## Fields

- **facetId** — the id of the FacetDefinition to render.
- **viewMode** — `form`, `list`, `table`, `report`, or `card`.
- **filter** — optional filter expression over the facet's collection (e.g. `status eq open`).
- **limit** — maximum rows/cards to show.

## Differences from Record List

Record List is a **lightweight** primitive for quick data views over any record type. Facet View is heavier — it honours the FacetDefinition's full schema, including spell-check, value lists, computed fields, and conditional formatting.

Rule of thumb:

- **Ad-hoc listing** → Record List.
- **Production data UI** → Facet View with a FacetDefinition.

## Tips

- Create the FacetDefinition first in Facet Designer (`Shift+X`), then drop a Facet View onto the page.
- Switching `viewMode` is cheap — the same schema drives all five layouts, so you can A/B two view modes on the same page without duplicating work.
