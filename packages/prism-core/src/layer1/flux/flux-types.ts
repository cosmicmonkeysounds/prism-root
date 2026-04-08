/**
 * @prism/core — Flux Domain Types (Layer 1)
 *
 * Flux is Prism's operational hub — productivity, finance, CRM, goals, inventory.
 * These types define the domain schemas for 6 entity families, their edge relations,
 * automation presets, and import/export formats.
 *
 * All types are registered into the ObjectRegistry via createFluxRegistry().
 */

import type { EntityDef, EdgeTypeDef } from "../object-model/types.js";

// ── Domain Constants ──────────────────────────────────────────────────────

export const FLUX_CATEGORIES = {
  PRODUCTIVITY: "flux:productivity",
  PEOPLE: "flux:people",
  FINANCE: "flux:finance",
  INVENTORY: "flux:inventory",
} as const;

export type FluxCategory = typeof FLUX_CATEGORIES[keyof typeof FLUX_CATEGORIES];

// ── Entity Type Strings ───────────────────────────────────────────────────

export const FLUX_TYPES = {
  // Productivity
  TASK: "flux:task",
  PROJECT: "flux:project",
  GOAL: "flux:goal",
  MILESTONE: "flux:milestone",
  // People
  CONTACT: "flux:contact",
  ORGANIZATION: "flux:organization",
  // Finance
  TRANSACTION: "flux:transaction",
  ACCOUNT: "flux:account",
  INVOICE: "flux:invoice",
  // Inventory
  ITEM: "flux:item",
  LOCATION: "flux:location",
} as const;

export type FluxEntityType = typeof FLUX_TYPES[keyof typeof FLUX_TYPES];

// ── Edge Relation Strings ─────────────────────────────────────────────────

export const FLUX_EDGES = {
  ASSIGNED_TO: "flux:assigned-to",
  DEPENDS_ON: "flux:depends-on",
  BLOCKS: "flux:blocks",
  BELONGS_TO: "flux:belongs-to",
  RELATED_TO: "flux:related-to",
  INVOICED_TO: "flux:invoiced-to",
  STORED_AT: "flux:stored-at",
} as const;

export type FluxEdgeType = typeof FLUX_EDGES[keyof typeof FLUX_EDGES];

// ── Status Values ─────────────────────────────────────────────────────────

export const TASK_STATUSES = [
  { value: "backlog", label: "Backlog" },
  { value: "todo", label: "To Do" },
  { value: "in_progress", label: "In Progress" },
  { value: "review", label: "In Review" },
  { value: "done", label: "Done" },
  { value: "cancelled", label: "Cancelled" },
] as const;

export const PROJECT_STATUSES = [
  { value: "planning", label: "Planning" },
  { value: "active", label: "Active" },
  { value: "on_hold", label: "On Hold" },
  { value: "completed", label: "Completed" },
  { value: "archived", label: "Archived" },
] as const;

export const GOAL_STATUSES = [
  { value: "draft", label: "Draft" },
  { value: "active", label: "Active" },
  { value: "achieved", label: "Achieved" },
  { value: "abandoned", label: "Abandoned" },
] as const;

export const TRANSACTION_TYPES = [
  { value: "income", label: "Income" },
  { value: "expense", label: "Expense" },
  { value: "transfer", label: "Transfer" },
  { value: "refund", label: "Refund" },
] as const;

export const CONTACT_TYPES = [
  { value: "person", label: "Person" },
  { value: "company", label: "Company" },
  { value: "lead", label: "Lead" },
  { value: "vendor", label: "Vendor" },
  { value: "partner", label: "Partner" },
] as const;

export const INVOICE_STATUSES = [
  { value: "draft", label: "Draft" },
  { value: "sent", label: "Sent" },
  { value: "paid", label: "Paid" },
  { value: "overdue", label: "Overdue" },
  { value: "cancelled", label: "Cancelled" },
] as const;

export const ITEM_STATUSES = [
  { value: "in_stock", label: "In Stock" },
  { value: "low_stock", label: "Low Stock" },
  { value: "out_of_stock", label: "Out of Stock" },
  { value: "discontinued", label: "Discontinued" },
] as const;

// ── Automation Preset ─────────────────────────────────────────────────────

export interface FluxAutomationPreset {
  /** Unique preset ID. */
  id: string;
  /** Human-readable name. */
  name: string;
  /** Which entity type this preset applies to (FluxEntityType or plugin-extended). */
  entityType: FluxEntityType | (string & {});
  /** Trigger event. */
  trigger: FluxTriggerKind;
  /** Condition expression (expression engine syntax). */
  condition?: string;
  /** Actions to execute. */
  actions: FluxAutomationAction[];
}

export type FluxTriggerKind =
  | "on_create"
  | "on_update"
  | "on_status_change"
  | "on_due_date"
  | "on_schedule";

export interface FluxAutomationAction {
  /** Action type. */
  kind: "set_field" | "create_edge" | "send_notification" | "move_to_status";
  /** Target field or relation. */
  target: string;
  /** Value to set (or template string). */
  value: string;
}

// ── Import/Export ─────────────────────────────────────────────────────────

export type FluxExportFormat = "csv" | "json";

export interface FluxExportOptions {
  /** Entity type to export. */
  entityType: FluxEntityType;
  /** Output format. */
  format: FluxExportFormat;
  /** Which fields to include (undefined = all). */
  fields?: string[];
  /** Include edge relations. */
  includeEdges?: boolean;
}

export interface FluxImportResult {
  /** Number of records imported. */
  imported: number;
  /** Number of records skipped (duplicates or errors). */
  skipped: number;
  /** Error messages for skipped records. */
  errors: string[];
}

// ── Registry ──────────────────────────────────────────────────────────────

export interface FluxRegistry {
  /** All Flux entity definitions. */
  getEntityDefs(): EntityDef[];
  /** All Flux edge type definitions. */
  getEdgeDefs(): EdgeTypeDef[];
  /** Get entity def by type string. */
  getEntityDef(type: FluxEntityType): EntityDef | undefined;
  /** Get edge def by relation string. */
  getEdgeDef(relation: FluxEdgeType): EdgeTypeDef | undefined;
  /** Get all built-in automation presets. */
  getAutomationPresets(): FluxAutomationPreset[];
  /** Get presets for a specific entity type. */
  getPresetsForEntity(type: FluxEntityType): FluxAutomationPreset[];
  /** Export objects to CSV/JSON string. */
  exportData(objects: Record<string, unknown>[], options: FluxExportOptions): string;
  /** Parse imported data. */
  parseImport(data: string, format: FluxExportFormat): Record<string, unknown>[];
}
