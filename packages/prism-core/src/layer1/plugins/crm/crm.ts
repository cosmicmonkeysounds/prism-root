/**
 * @prism/plugin-crm — CRM Plugin Registry (Layer 1)
 *
 * No new entity types — wraps existing Flux Contact/Organization with
 * CRM-specific views, commands, and pipeline lenses.
 */

import type { PrismPlugin } from "../../plugin/plugin-types.js";
import { pluginId } from "../../plugin/plugin-types.js";
import type { CrmRegistry } from "./crm-types.js";

// ── Plugin ───────────────────────────────────────────────────────────────

function buildPlugin(): PrismPlugin {
  return {
    id: pluginId("prism.plugin.crm"),
    name: "CRM",
    contributes: {
      views: [
        { id: "crm:contacts", label: "Contacts", zone: "content", componentId: "ContactListView", description: "Contact directory" },
        { id: "crm:organizations", label: "Organizations", zone: "content", componentId: "OrgListView", description: "Organization directory" },
        { id: "crm:pipeline", label: "Deal Pipeline", zone: "content", componentId: "PipelineView", description: "Sales pipeline kanban" },
        { id: "crm:relationships", label: "Relationships", zone: "content", componentId: "RelationshipGraphView", description: "Contact relationship graph" },
      ],
      commands: [
        { id: "crm:new-contact", label: "New Contact", category: "CRM", action: "crm.newContact" },
        { id: "crm:new-organization", label: "New Organization", category: "CRM", action: "crm.newOrganization" },
        { id: "crm:log-activity", label: "Log Activity", category: "CRM", action: "crm.logActivity" },
      ],
      keybindings: [
        { command: "crm:new-contact", key: "ctrl+shift+c" },
      ],
      activityBar: [
        { id: "crm:activity", label: "CRM", position: "top", priority: 15 },
      ],
    },
  };
}

// ── Factory ──────────────────────────────────────────────────────────────

export function createCrmRegistry(): CrmRegistry {
  const plugin = buildPlugin();

  return {
    getPlugin: () => plugin,
  };
}
