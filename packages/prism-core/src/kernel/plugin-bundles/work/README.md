# plugin-bundles/work

Work bundle. Extends Flux with freelance, time-tracking, and focus-planning entities — built on top of the existing Flux productivity types (`Task`, `Project`, `Goal`, `Milestone`).

```ts
import { createWorkBundle } from "@prism/core/plugin-bundles";
```

## What it registers

- **Categories** (`WORK_CATEGORIES`): `work:freelance`, `work:time`, `work:focus`.
- **Entity types** (`WORK_TYPES`): `Gig` (with client ref, rate, currency, billed-hours derivation), `TimeEntry` (start/end, billable, duration), `FocusBlock` (scheduled start/end, focus type).
- **Edges** (`WORK_EDGES`): `tracked-for`, `billed-to`, `focus-on`.
- **Status enums**: `GIG_STATUSES` (lead → completed/cancelled), `TIME_ENTRY_STATUSES` (running/stopped/submitted/approved/invoiced), `FOCUS_BLOCK_STATUSES` (planned/active/completed/skipped).
- **Plugin contributions**: work views, commands, and activity-bar entries.

## Key exports

- `createWorkBundle()` — self-registering `PluginBundle`.
- `createWorkRegistry()` — lower-level `WorkRegistry` exposing entity/edge defs, automation presets, and the `PrismPlugin`.
- Constants: `WORK_CATEGORIES`, `WORK_TYPES`, `WORK_EDGES`, `GIG_STATUSES`, `TIME_ENTRY_STATUSES`, `FOCUS_BLOCK_STATUSES`.
- Types: `WorkCategory`, `WorkEntityType`, `WorkEdgeType`, `WorkRegistry`.

## Usage

```ts
import {
  createWorkBundle,
  installPluginBundles,
} from "@prism/core/plugin-bundles";

installPluginBundles([createWorkBundle()], {
  objectRegistry,
  pluginRegistry,
});
```
