# Record List

A **Record List** queries kernel records by type and renders them as a simple list of rows. It is the fastest way to embed a live, filter-able view of your data on a page.

## Fields

- **recordType** — the kebab-case entity type to query (e.g. `task`, `contact`, `bookmark`).
- **titleField** — which field to show as the row title. Defaults to `name`.
- **subtitleField** — optional secondary line per row.
- **metaFields** — comma-separated field ids shown as chips after the subtitle.
- **filterExpression** — compact filter grammar (see below).
- **sortField** — field id to sort by.
- **sortDir** — `asc` or `desc`.
- **limit** — maximum rows to show.
- **emptyMessage** — text shown when no records match.

## Filter expression grammar

Record List parses a compact filter string into the same `FilterConfig` that `@prism/core/view` uses throughout Prism. Format: `field op value; field op value; ...`

Examples:

- `status eq open`
- `priority in high,urgent`
- `due_date lt 2026-05-01`
- `status eq open; priority in high,urgent`

Supported operators: `eq`, `neq`, `gt`, `gte`, `lt`, `lte`, `in`, `nin`, `contains`, `starts-with`, `ends-with`, `exists`.

## Record List vs Facet View

| | Record List | Facet View |
|-|-|-|
| Setup | None | Requires a FacetDefinition |
| Power | Basic filter + sort + limit | Full FacetDefinition schema |
| Use for | Ad-hoc views | Production data UI |
| Editability | Read-only | Optional inline editing |

## Tips

- Leave `titleField` blank to fall back to `name`, then `title`, then `id`.
- `metaFields` chips pull from both shell fields and per-entity data — so `priority, tags` works for tasks, and `phone, email` works for contacts.
- Sort is stable — adding a secondary sort key happens via the Saved View panel.
