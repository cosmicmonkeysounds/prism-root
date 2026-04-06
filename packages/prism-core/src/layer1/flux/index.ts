// ── Flux Domain ───────────────────────────────────────────────────────────
export type {
  FluxCategory,
  FluxEntityType,
  FluxEdgeType,
  FluxAutomationPreset,
  FluxTriggerKind,
  FluxAutomationAction,
  FluxExportFormat,
  FluxExportOptions,
  FluxImportResult,
  FluxRegistry,
} from "./flux-types.js";

export {
  FLUX_CATEGORIES,
  FLUX_TYPES,
  FLUX_EDGES,
  TASK_STATUSES,
  PROJECT_STATUSES,
  GOAL_STATUSES,
  TRANSACTION_TYPES,
  CONTACT_TYPES,
  INVOICE_STATUSES,
  ITEM_STATUSES,
} from "./flux-types.js";

export { createFluxRegistry } from "./flux.js";
