/**
 * @prism/plugin-finance — Finance Domain Types (Layer 1)
 *
 * Extends Flux with loans, grants, and budgets.
 * Builds on existing Flux finance types (Transaction, Account, Invoice).
 */

import type { EntityDef, EdgeTypeDef } from "@prism/core/object-model";
import type { FluxAutomationPreset } from "@prism/core/flux";
import type { PrismPlugin } from "@prism/core/plugin";

// ── Categories ─────────────────��─────────────────────────────────────────

export const FINANCE_CATEGORIES = {
  LENDING: "finance:lending",
  BUDGETING: "finance:budgeting",
} as const;

export type FinanceCategory = typeof FINANCE_CATEGORIES[keyof typeof FINANCE_CATEGORIES];

// ── Entity Type Strings ──────────────────────────────────────────────────

export const FINANCE_TYPES = {
  LOAN: "finance:loan",
  GRANT: "finance:grant",
  BUDGET: "finance:budget",
} as const;

export type FinanceEntityType = typeof FINANCE_TYPES[keyof typeof FINANCE_TYPES];

// ── Edge Relation Strings ────────────────────────────────────────────────

export const FINANCE_EDGES = {
  FUNDED_BY: "finance:funded-by",
  BUDGET_FOR: "finance:budget-for",
  PAYMENT_OF: "finance:payment-of",
} as const;

export type FinanceEdgeType = typeof FINANCE_EDGES[keyof typeof FINANCE_EDGES];

// ── Status Values ────────────���───────────────────────────────────────────

export const LOAN_STATUSES = [
  { value: "application", label: "Application" },
  { value: "approved", label: "Approved" },
  { value: "active", label: "Active" },
  { value: "deferred", label: "Deferred" },
  { value: "paid_off", label: "Paid Off" },
  { value: "defaulted", label: "Defaulted" },
] as const;

export const GRANT_STATUSES = [
  { value: "researching", label: "Researching" },
  { value: "drafting", label: "Drafting" },
  { value: "submitted", label: "Submitted" },
  { value: "awarded", label: "Awarded" },
  { value: "active", label: "Active" },
  { value: "reporting", label: "Reporting" },
  { value: "closed", label: "Closed" },
  { value: "rejected", label: "Rejected" },
] as const;

export const BUDGET_STATUSES = [
  { value: "draft", label: "Draft" },
  { value: "active", label: "Active" },
  { value: "closed", label: "Closed" },
] as const;

// ── Registry ───────────────────────────────────���─────────────────────────

export interface FinanceRegistry {
  getEntityDefs(): EntityDef[];
  getEdgeDefs(): EdgeTypeDef[];
  getEntityDef(type: FinanceEntityType): EntityDef | undefined;
  getEdgeDef(relation: FinanceEdgeType): EdgeTypeDef | undefined;
  getAutomationPresets(): FluxAutomationPreset[];
  getPlugin(): PrismPlugin;
}
