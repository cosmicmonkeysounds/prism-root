# Page Shell

The **Page Shell** is the outermost frame of a single page. It defines the grid of named regions — `header`, `sidebar`, `main`, `footer` — that every block on the page lives inside.

## When to use it

Use a Page Shell when you want a single page with a persistent chrome around its content. Each region is resizable from its shared edge. Drag the divider between two regions and the shell commits the new size on release.

## Regions

- **header** — top strip. Good for a page title, breadcrumbs, or a page-level toolbar.
- **sidebar** — left column. Good for a table of contents or filter panel.
- **main** — the primary content area. This is where most blocks land.
- **footer** — bottom strip. Good for status chips, tags, or a save button.

## Differences from App Shell

Page Shell defines the frame for **one page**. App Shell defines the frame for **the whole app** — it wraps multiple pages and handles global navigation. If you want every page in your site to share the same outer chrome, use App Shell and put Page Shell inside its `main` slot.

## Resize behaviour

Drag the edge of any region to resize it. The drag preview snaps to the nearest pixel while you hold the pointer, and commits the final value on release. Resize values are stored as percentages so the shell stays responsive across window sizes.
