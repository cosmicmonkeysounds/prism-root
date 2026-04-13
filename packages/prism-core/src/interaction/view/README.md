# view

Derived views over a `CollectionStore`. A `ViewRegistry` declares 7 built-in view modes (`list`, `kanban`, `grid`, `table`, `timeline`, `calendar`, `graph`) with capability flags (`supportsSort`, `supportsFilter`, `supportsGrouping`, `supportsColumns`, `supportsInlineEdit`, `supportsBulkSelect`, `supportsHierarchy`, `requiresDate`, `requiresStatus`) that UIs query to decide which controls to show. `applyFilters`/`applySorts`/`applyGroups`/`applyViewConfig` are pure transforms over `GraphObject[]`. `LiveView` materializes a filtered/sorted/grouped snapshot that updates reactively. `SavedView` + `SavedViewRegistry` persist named configurations (FileMaker "Found Sets").

Note: these `ViewMode`s are the structural/capability layer only. Higher-level visual "views" (kanban boards, charts, maps, etc.) are composed as Puck widgets at the app layer — do not add new `ViewMode` entries for them.

## Import

```ts
import {
  createViewRegistry,
  applyViewConfig,
  createLiveView,
  createSavedView,
  createSavedViewRegistry,
} from "@prism/core/view";
```

## Key exports

- `createViewRegistry()` — `get`/`all`/`register`/`supports`/`modesWithCapability`.
- `applyFilters` / `applySorts` / `applyGroups` / `applyViewConfig` — pure GraphObject[] transforms.
- `getFieldValue(obj, field)` — resolves shell and `data.*` fields uniformly.
- `createLiveView(store, options?)` — materialized, auto-updating `LiveView` with `snapshot`, `setConfig`/`setFilters`/`setSorts`/`setGroups`/`subscribe`.
- `createSavedView(id, objectType, config, name?)` — build a persistable `SavedView`.
- `createSavedViewRegistry()` — registry with add/remove/pin/share/forObjectType/subscribe.
- Types: `ViewMode`, `ViewDef`, `ViewRegistry`, `FilterOp` (12 operators), `FilterConfig`, `SortConfig`, `GroupConfig`, `GroupedResult`, `ViewConfig`, `LiveViewSnapshot`, `LiveView`, `LiveViewOptions`, `SavedView`, `SavedViewRegistry`.

## Usage

```ts
import { createLiveView } from "@prism/core/view";

const live = createLiveView(tasksStore, {
  mode: "list",
  config: {
    filters: [{ field: "status", op: "eq", value: "open" }],
    sorts: [{ field: "name", dir: "asc" }],
    groups: [{ field: "type" }],
  },
});

const unsub = live.subscribe((snap) => {
  console.log(snap.total, snap.groups, snap.typeFacets);
});

live.setFilters([{ field: "tags", op: "in", value: ["urgent"] }]);
```
