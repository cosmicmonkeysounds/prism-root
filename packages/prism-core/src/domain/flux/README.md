# flux/

Flux Domain registry: 11 entity types across 4 categories (productivity, people, finance, inventory), 7 edge types, 8 automation presets, and CSV/JSON import/export. Produces `EntityDef[]` and `EdgeTypeDef[]` ready to feed into an `ObjectRegistry`.

```ts
import { createFluxRegistry } from "@prism/core/flux";
```

## Key exports

- `createFluxRegistry()` — construct a `FluxRegistry` with all entity/edge defs and automation presets baked in.
- `FLUX_CATEGORIES`, `FLUX_TYPES`, `FLUX_EDGES` — string enums for category IDs, entity type IDs, edge relation names.
- `TASK_STATUSES`, `PROJECT_STATUSES`, `GOAL_STATUSES`, `TRANSACTION_TYPES`, `CONTACT_TYPES`, `INVOICE_STATUSES`, `ITEM_STATUSES` — enum option sets reused across entity fields.
- `FluxRegistry` — interface with `getEntityDefs`, `getEdgeDefs`, `getEntityDef`, `getEdgeDef`, `getAutomationPresets`, `getPresetsForEntity`, `exportData`, `parseImport`.
- `FluxCategory`, `FluxEntityType`, `FluxEdgeType`, `FluxAutomationPreset`, `FluxTriggerKind`, `FluxAutomationAction`, `FluxExportFormat`, `FluxExportOptions`, `FluxImportResult` — supporting types.

## Usage

```ts
import { createFluxRegistry, FLUX_TYPES } from "@prism/core/flux";

const flux = createFluxRegistry();

const taskDef = flux.getEntityDef(FLUX_TYPES.TASK);
const presets = flux.getPresetsForEntity(FLUX_TYPES.TASK);

const csv = flux.exportData(tasks, {
  format: "csv",
  fields: ["id", "name", "status", "dueDate"],
});

const imported = flux.parseImport(csv, "csv");
```
