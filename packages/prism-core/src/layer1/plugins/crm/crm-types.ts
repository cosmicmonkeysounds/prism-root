/**
 * @prism/plugin-crm — CRM Domain Types (Layer 1)
 *
 * Plugin wrapper around existing Flux people types (Contact, Organization).
 * Adds no new entity types — contributes CRM-specific views, commands,
 * and relationship lenses for the existing Flux domain.
 */

import type { PrismPlugin } from "../../plugin/plugin-types.js";

// ── Deal Stage Constants ─────────────────────────────────────────────────

export const CRM_DEAL_STAGES = [
  { value: "prospect", label: "Prospect" },
  { value: "qualified", label: "Qualified" },
  { value: "proposal", label: "Proposal" },
  { value: "negotiation", label: "Negotiation" },
  { value: "closed_won", label: "Closed Won" },
  { value: "closed_lost", label: "Closed Lost" },
] as const;

export const CRM_ACTIVITY_TYPES = [
  { value: "call", label: "Call" },
  { value: "email", label: "Email" },
  { value: "meeting", label: "Meeting" },
  { value: "note", label: "Note" },
  { value: "task", label: "Task" },
] as const;

// ── Registry ─────────────────────────────────────────────────────────────

export interface CrmRegistry {
  getPlugin(): PrismPlugin;
}
