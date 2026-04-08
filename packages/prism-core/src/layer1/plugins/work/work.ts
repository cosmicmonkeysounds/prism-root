/**
 * @prism/plugin-work — Work Domain Registry (Layer 1)
 *
 * Registers freelance gigs, time entries, and focus blocks.
 */

import type { EntityDef, EntityFieldDef, EdgeTypeDef } from "../../object-model/types.js";
import type { FluxAutomationPreset } from "../../flux/flux-types.js";
import { FLUX_TYPES } from "../../flux/flux-types.js";
import type { PrismPlugin } from "../../plugin/plugin-types.js";
import { pluginId } from "../../plugin/plugin-types.js";
import type { WorkRegistry, WorkEntityType, WorkEdgeType } from "./work-types.js";
import { WORK_CATEGORIES, WORK_TYPES, WORK_EDGES } from "./work-types.js";

// ── Field Definitions ────────────────────────────────────────────────────

const GIG_FIELDS: EntityFieldDef[] = [
  { id: "client", type: "object_ref", label: "Client", refTypes: [FLUX_TYPES.CONTACT, FLUX_TYPES.ORGANIZATION] },
  { id: "rate", type: "float", label: "Rate" },
  { id: "rateUnit", type: "enum", label: "Rate Unit", enumOptions: [
    { value: "hourly", label: "Hourly" },
    { value: "daily", label: "Daily" },
    { value: "weekly", label: "Weekly" },
    { value: "fixed", label: "Fixed Price" },
    { value: "retainer", label: "Retainer" },
  ], default: "hourly" },
  { id: "currency", type: "enum", label: "Currency", enumOptions: [
    { value: "USD", label: "USD" },
    { value: "EUR", label: "EUR" },
    { value: "GBP", label: "GBP" },
    { value: "CAD", label: "CAD" },
    { value: "AUD", label: "AUD" },
  ], default: "USD" },
  { id: "estimatedHours", type: "float", label: "Estimated Hours" },
  { id: "actualHours", type: "float", label: "Actual Hours", default: 0, ui: { readonly: true } },
  { id: "startDate", type: "date", label: "Start Date" },
  { id: "endDate", type: "date", label: "End Date" },
  { id: "contractUrl", type: "url", label: "Contract URL" },
  { id: "totalBilled", type: "float", label: "Total Billed", expression: "actualHours * rate", ui: { readonly: true } },
  { id: "scope", type: "text", label: "Scope of Work", ui: { multiline: true, group: "Details" } },
];

const TIME_ENTRY_FIELDS: EntityFieldDef[] = [
  { id: "startTime", type: "datetime", label: "Start Time", required: true },
  { id: "endTime", type: "datetime", label: "End Time" },
  { id: "durationMinutes", type: "int", label: "Duration (min)", ui: { readonly: true } },
  { id: "billable", type: "bool", label: "Billable", default: true },
  { id: "rate", type: "float", label: "Rate Override" },
  { id: "description", type: "text", label: "Description", ui: { multiline: true } },
  { id: "tags", type: "string", label: "Tags", ui: { placeholder: "comma-separated" } },
];

const FOCUS_BLOCK_FIELDS: EntityFieldDef[] = [
  { id: "scheduledStart", type: "datetime", label: "Scheduled Start", required: true },
  { id: "scheduledEnd", type: "datetime", label: "Scheduled End", required: true },
  { id: "durationMinutes", type: "int", label: "Duration (min)" },
  { id: "focusType", type: "enum", label: "Focus Type", enumOptions: [
    { value: "deep_work", label: "Deep Work" },
    { value: "shallow_work", label: "Shallow Work" },
    { value: "creative", label: "Creative" },
    { value: "admin", label: "Admin" },
    { value: "learning", label: "Learning" },
    { value: "break", label: "Break" },
  ], default: "deep_work" },
  { id: "energyLevel", type: "enum", label: "Energy Level", enumOptions: [
    { value: "high", label: "High" },
    { value: "medium", label: "Medium" },
    { value: "low", label: "Low" },
  ] },
  { id: "cognitiveLoad", type: "enum", label: "Cognitive Load", enumOptions: [
    { value: "heavy", label: "Heavy" },
    { value: "moderate", label: "Moderate" },
    { value: "light", label: "Light" },
  ] },
  { id: "completionNote", type: "text", label: "Completion Note", ui: { multiline: true } },
];

// ── Entity Definitions ───────────────────────────────────────────────────

function buildEntityDefs(): EntityDef[] {
  return [
    {
      type: WORK_TYPES.GIG,
      nsid: "io.prismapp.work.gig",
      category: WORK_CATEGORIES.FREELANCE,
      label: "Gig",
      pluralLabel: "Gigs",
      defaultChildView: "kanban",
      fields: GIG_FIELDS,
      extraChildTypes: [FLUX_TYPES.TASK, WORK_TYPES.TIME_ENTRY],
    },
    {
      type: WORK_TYPES.TIME_ENTRY,
      nsid: "io.prismapp.work.time-entry",
      category: WORK_CATEGORIES.TIME,
      label: "Time Entry",
      pluralLabel: "Time Entries",
      defaultChildView: "list",
      fields: TIME_ENTRY_FIELDS,
      childOnly: true,
    },
    {
      type: WORK_TYPES.FOCUS_BLOCK,
      nsid: "io.prismapp.work.focus-block",
      category: WORK_CATEGORIES.FOCUS,
      label: "Focus Block",
      pluralLabel: "Focus Blocks",
      defaultChildView: "timeline",
      fields: FOCUS_BLOCK_FIELDS,
    },
  ];
}

// ── Edge Definitions ─────────────────────────────────────────────────────

function buildEdgeDefs(): EdgeTypeDef[] {
  return [
    {
      relation: WORK_EDGES.TRACKED_FOR,
      nsid: "io.prismapp.work.tracked-for",
      label: "Tracked For",
      behavior: "membership",
      sourceTypes: [WORK_TYPES.TIME_ENTRY],
      targetTypes: [FLUX_TYPES.TASK, FLUX_TYPES.PROJECT, WORK_TYPES.GIG],
    },
    {
      relation: WORK_EDGES.BILLED_TO,
      nsid: "io.prismapp.work.billed-to",
      label: "Billed To",
      behavior: "assignment",
      sourceTypes: [WORK_TYPES.TIME_ENTRY],
      targetTypes: [FLUX_TYPES.INVOICE],
    },
    {
      relation: WORK_EDGES.FOCUS_ON,
      nsid: "io.prismapp.work.focus-on",
      label: "Focus On",
      behavior: "weak",
      sourceTypes: [WORK_TYPES.FOCUS_BLOCK],
      targetTypes: [FLUX_TYPES.TASK, FLUX_TYPES.PROJECT, WORK_TYPES.GIG],
    },
  ];
}

// ── Automation Presets ────────────────────────────────────────────────────

function buildAutomationPresets(): FluxAutomationPreset[] {
  return [
    {
      id: "work:auto:gig-hours-rollup",
      name: "Roll up tracked hours to gig",
      entityType: WORK_TYPES.TIME_ENTRY,
      trigger: "on_update",
      condition: "durationMinutes > 0",
      actions: [
        { kind: "set_field", target: "actualHours", value: "{{sum(children.durationMinutes) / 60}}" },
      ],
    },
    {
      id: "work:auto:time-entry-stop",
      name: "Calculate duration on stop",
      entityType: WORK_TYPES.TIME_ENTRY,
      trigger: "on_status_change",
      condition: "status == 'stopped'",
      actions: [
        { kind: "set_field", target: "durationMinutes", value: "{{diff(endTime, startTime, 'minutes')}}" },
      ],
    },
    {
      id: "work:auto:focus-complete",
      name: "Mark focus block completed",
      entityType: WORK_TYPES.FOCUS_BLOCK,
      trigger: "on_update",
      condition: "now() >= scheduledEnd",
      actions: [
        { kind: "move_to_status", target: "status", value: "completed" },
      ],
    },
  ];
}

// ── Plugin ───────────────────────────────────────────────────────────────

function buildPlugin(): PrismPlugin {
  return {
    id: pluginId("prism.plugin.work"),
    name: "Work",
    contributes: {
      views: [
        { id: "work:gigs", label: "Gigs", zone: "content", componentId: "GigBoardView", description: "Freelance gig board" },
        { id: "work:timesheet", label: "Timesheet", zone: "content", componentId: "TimesheetView", description: "Time tracking table" },
        { id: "work:focus", label: "Focus Planner", zone: "content", componentId: "FocusPlannerView", description: "Daily focus block scheduler" },
      ],
      commands: [
        { id: "work:start-timer", label: "Start Timer", category: "Work", action: "work.startTimer" },
        { id: "work:stop-timer", label: "Stop Timer", category: "Work", action: "work.stopTimer" },
        { id: "work:new-gig", label: "New Gig", category: "Work", action: "work.newGig" },
        { id: "work:new-focus-block", label: "New Focus Block", category: "Work", action: "work.newFocusBlock" },
      ],
      keybindings: [
        { command: "work:start-timer", key: "ctrl+shift+t" },
        { command: "work:stop-timer", key: "ctrl+shift+s" },
      ],
      activityBar: [
        { id: "work:activity", label: "Work", position: "top", priority: 20 },
      ],
    },
  };
}

// ── Factory ──────────────────────────────────────────────────────────────

export function createWorkRegistry(): WorkRegistry {
  const entityDefs = buildEntityDefs();
  const edgeDefs = buildEdgeDefs();
  const presets = buildAutomationPresets();
  const plugin = buildPlugin();

  return {
    getEntityDefs: () => entityDefs,
    getEdgeDefs: () => edgeDefs,
    getEntityDef: (type: WorkEntityType) => entityDefs.find(d => d.type === type),
    getEdgeDef: (relation: WorkEdgeType) => edgeDefs.find(d => d.relation === relation),
    getAutomationPresets: () => presets,
    getPlugin: () => plugin,
  };
}

// ── Self-Registering Bundle ─��────────────────────────────────────────────

import type { PluginBundle, PluginInstallContext } from "../plugin-install.js";

export function createWorkBundle(): PluginBundle {
  return {
    id: "prism.plugin.work",
    name: "Work",
    install(ctx: PluginInstallContext) {
      const reg = createWorkRegistry();
      ctx.objectRegistry.registerAll(reg.getEntityDefs());
      ctx.objectRegistry.registerEdges(reg.getEdgeDefs());
      const unsub = ctx.pluginRegistry.register(reg.getPlugin());
      return unsub;
    },
  };
}
