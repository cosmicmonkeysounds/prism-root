/**
 * @prism/plugin-work — Work Domain Types (Layer 1)
 *
 * Extends Flux with freelance, time-tracking, and focus-planning entities.
 * Builds on existing Flux productivity types (Task, Project, Goal, Milestone).
 */

import type { EdgeTypeDef, EntityDef } from "@prism/core/object-model";
import type { FluxAutomationPreset } from "@prism/core/flux";
import type { PrismPlugin } from "@prism/core/plugin";

// ── Categories ─────���─────────────────────────────────────────────────────

export const WORK_CATEGORIES = {
  FREELANCE: "work:freelance",
  TIME: "work:time",
  FOCUS: "work:focus",
} as const;

export type WorkCategory = typeof WORK_CATEGORIES[keyof typeof WORK_CATEGORIES];

// ── Entity Type Strings ─────────���────────────────────────────────────────

export const WORK_TYPES = {
  GIG: "work:gig",
  TIME_ENTRY: "work:time-entry",
  FOCUS_BLOCK: "work:focus-block",
} as const;

export type WorkEntityType = typeof WORK_TYPES[keyof typeof WORK_TYPES];

// ── Edge Relation Strings ────────────────────────────────────────────────

export const WORK_EDGES = {
  TRACKED_FOR: "work:tracked-for",
  BILLED_TO: "work:billed-to",
  FOCUS_ON: "work:focus-on",
} as const;

export type WorkEdgeType = typeof WORK_EDGES[keyof typeof WORK_EDGES];

// ── Status Values ────────────────────────────────────────────────────────

export const GIG_STATUSES = [
  { value: "lead", label: "Lead" },
  { value: "proposal", label: "Proposal Sent" },
  { value: "negotiation", label: "Negotiation" },
  { value: "active", label: "Active" },
  { value: "paused", label: "Paused" },
  { value: "completed", label: "Completed" },
  { value: "cancelled", label: "Cancelled" },
] as const;

export const TIME_ENTRY_STATUSES = [
  { value: "running", label: "Running" },
  { value: "stopped", label: "Stopped" },
  { value: "submitted", label: "Submitted" },
  { value: "approved", label: "Approved" },
  { value: "invoiced", label: "Invoiced" },
] as const;

export const FOCUS_BLOCK_STATUSES = [
  { value: "planned", label: "Planned" },
  { value: "active", label: "Active" },
  { value: "completed", label: "Completed" },
  { value: "skipped", label: "Skipped" },
] as const;

// ── Registry ──────────────────────────────���──────────────────────────────

export interface WorkRegistry {
  getEntityDefs(): EntityDef[];
  getEdgeDefs(): EdgeTypeDef[];
  getEntityDef(type: WorkEntityType): EntityDef | undefined;
  getEdgeDef(relation: WorkEdgeType): EdgeTypeDef | undefined;
  getAutomationPresets(): FluxAutomationPreset[];
  getPlugin(): PrismPlugin;
}
