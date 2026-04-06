/**
 * @prism/core — Flux Domain Registry (Layer 1)
 *
 * Registers all Flux entity schemas, edge types, and automation presets.
 * Provides import/export for CSV and JSON data migration.
 */

import type { EntityDef, EntityFieldDef, EdgeTypeDef } from "../object-model/types.js";
import type {
  FluxRegistry,
  FluxEntityType,
  FluxEdgeType,
  FluxExportOptions,
  FluxExportFormat,
  FluxAutomationPreset,
} from "./flux-types.js";
import {
  FLUX_CATEGORIES,
  FLUX_TYPES,
  FLUX_EDGES,
  TRANSACTION_TYPES,
  CONTACT_TYPES,
} from "./flux-types.js";

// ── Field Definitions ─────────────────────────────────────────────────────

function enumOptions(values: ReadonlyArray<{ value: string; label: string }>): Array<{ value: string; label: string }> {
  return values.map(v => ({ value: v.value, label: v.label }));
}

const TASK_FIELDS: EntityFieldDef[] = [
  { id: "priority", type: "enum", label: "Priority", enumOptions: [
    { value: "urgent", label: "Urgent" },
    { value: "high", label: "High" },
    { value: "medium", label: "Medium" },
    { value: "low", label: "Low" },
    { value: "none", label: "None" },
  ], default: "medium" },
  { id: "effort", type: "enum", label: "Effort", enumOptions: [
    { value: "xs", label: "XS" },
    { value: "s", label: "S" },
    { value: "m", label: "M" },
    { value: "l", label: "L" },
    { value: "xl", label: "XL" },
  ] },
  { id: "dueDate", type: "date", label: "Due Date" },
  { id: "completedAt", type: "datetime", label: "Completed At", ui: { readonly: true } },
  { id: "estimateHours", type: "float", label: "Estimate (hours)" },
  { id: "actualHours", type: "float", label: "Actual (hours)" },
  { id: "recurring", type: "enum", label: "Recurring", enumOptions: [
    { value: "none", label: "None" },
    { value: "daily", label: "Daily" },
    { value: "weekly", label: "Weekly" },
    { value: "biweekly", label: "Bi-weekly" },
    { value: "monthly", label: "Monthly" },
    { value: "quarterly", label: "Quarterly" },
    { value: "yearly", label: "Yearly" },
  ], default: "none" },
];

const PROJECT_FIELDS: EntityFieldDef[] = [
  { id: "startDate", type: "date", label: "Start Date" },
  { id: "targetDate", type: "date", label: "Target Date" },
  { id: "budget", type: "float", label: "Budget" },
  { id: "progress", type: "float", label: "Progress (%)", default: 0 },
  { id: "lead", type: "object_ref", label: "Project Lead", refTypes: [FLUX_TYPES.CONTACT] },
];

const GOAL_FIELDS: EntityFieldDef[] = [
  { id: "targetDate", type: "date", label: "Target Date" },
  { id: "progress", type: "float", label: "Progress (%)", default: 0 },
  { id: "metric", type: "string", label: "Key Metric" },
  { id: "targetValue", type: "float", label: "Target Value" },
  { id: "currentValue", type: "float", label: "Current Value", default: 0 },
  { id: "progressFormula", type: "string", label: "Progress Formula", expression: "currentValue / targetValue * 100" },
];

const MILESTONE_FIELDS: EntityFieldDef[] = [
  { id: "dueDate", type: "date", label: "Due Date" },
  { id: "completed", type: "bool", label: "Completed", default: false },
];

const CONTACT_FIELDS: EntityFieldDef[] = [
  { id: "contactType", type: "enum", label: "Contact Type", enumOptions: enumOptions(CONTACT_TYPES), default: "person" },
  { id: "email", type: "string", label: "Email", ui: { placeholder: "name@example.com" } },
  { id: "phone", type: "string", label: "Phone" },
  { id: "company", type: "object_ref", label: "Organization", refTypes: [FLUX_TYPES.ORGANIZATION] },
  { id: "role", type: "string", label: "Role / Title" },
  { id: "address", type: "text", label: "Address", ui: { multiline: true } },
  { id: "website", type: "url", label: "Website" },
  { id: "notes", type: "text", label: "Notes", ui: { multiline: true, group: "Details" } },
  { id: "lastContactDate", type: "date", label: "Last Contact" },
  { id: "dealValue", type: "float", label: "Deal Value", ui: { group: "CRM" } },
  { id: "dealStage", type: "enum", label: "Deal Stage", enumOptions: [
    { value: "prospect", label: "Prospect" },
    { value: "qualified", label: "Qualified" },
    { value: "proposal", label: "Proposal" },
    { value: "negotiation", label: "Negotiation" },
    { value: "closed_won", label: "Closed Won" },
    { value: "closed_lost", label: "Closed Lost" },
  ], ui: { group: "CRM" } },
];

const ORGANIZATION_FIELDS: EntityFieldDef[] = [
  { id: "industry", type: "string", label: "Industry" },
  { id: "website", type: "url", label: "Website" },
  { id: "address", type: "text", label: "Address", ui: { multiline: true } },
  { id: "employeeCount", type: "int", label: "Employees" },
  { id: "annualRevenue", type: "float", label: "Annual Revenue" },
];

const TRANSACTION_FIELDS: EntityFieldDef[] = [
  { id: "txnType", type: "enum", label: "Type", enumOptions: enumOptions(TRANSACTION_TYPES), required: true },
  { id: "amount", type: "float", label: "Amount", required: true },
  { id: "currency", type: "enum", label: "Currency", enumOptions: [
    { value: "USD", label: "USD" },
    { value: "EUR", label: "EUR" },
    { value: "GBP", label: "GBP" },
    { value: "JPY", label: "JPY" },
    { value: "CAD", label: "CAD" },
    { value: "AUD", label: "AUD" },
  ], default: "USD" },
  { id: "txnDate", type: "date", label: "Date", required: true },
  { id: "account", type: "object_ref", label: "Account", refTypes: [FLUX_TYPES.ACCOUNT] },
  { id: "category", type: "string", label: "Category" },
  { id: "payee", type: "string", label: "Payee" },
  { id: "reference", type: "string", label: "Reference #" },
  { id: "notes", type: "text", label: "Notes", ui: { multiline: true } },
  { id: "reconciled", type: "bool", label: "Reconciled", default: false },
];

const ACCOUNT_FIELDS: EntityFieldDef[] = [
  { id: "accountType", type: "enum", label: "Account Type", enumOptions: [
    { value: "checking", label: "Checking" },
    { value: "savings", label: "Savings" },
    { value: "credit", label: "Credit Card" },
    { value: "cash", label: "Cash" },
    { value: "investment", label: "Investment" },
    { value: "loan", label: "Loan" },
  ] },
  { id: "balance", type: "float", label: "Balance", default: 0 },
  { id: "currency", type: "enum", label: "Currency", enumOptions: [
    { value: "USD", label: "USD" },
    { value: "EUR", label: "EUR" },
    { value: "GBP", label: "GBP" },
  ], default: "USD" },
  { id: "institution", type: "string", label: "Institution" },
  { id: "accountNumber", type: "string", label: "Account Number", ui: { hidden: true } },
];

const INVOICE_FIELDS: EntityFieldDef[] = [
  { id: "invoiceNumber", type: "string", label: "Invoice #", required: true },
  { id: "issueDate", type: "date", label: "Issue Date", required: true },
  { id: "dueDate", type: "date", label: "Due Date", required: true },
  { id: "subtotal", type: "float", label: "Subtotal" },
  { id: "taxRate", type: "float", label: "Tax Rate (%)", default: 0 },
  { id: "taxAmount", type: "float", label: "Tax Amount", expression: "subtotal * taxRate / 100" },
  { id: "total", type: "float", label: "Total", expression: "subtotal + subtotal * taxRate / 100" },
  { id: "currency", type: "enum", label: "Currency", enumOptions: [
    { value: "USD", label: "USD" },
    { value: "EUR", label: "EUR" },
    { value: "GBP", label: "GBP" },
  ], default: "USD" },
  { id: "notes", type: "text", label: "Notes", ui: { multiline: true } },
];

const ITEM_FIELDS: EntityFieldDef[] = [
  { id: "sku", type: "string", label: "SKU" },
  { id: "quantity", type: "int", label: "Quantity", default: 0 },
  { id: "unit", type: "enum", label: "Unit", enumOptions: [
    { value: "each", label: "Each" },
    { value: "kg", label: "Kilogram" },
    { value: "lb", label: "Pound" },
    { value: "l", label: "Liter" },
    { value: "m", label: "Meter" },
    { value: "box", label: "Box" },
  ], default: "each" },
  { id: "costPrice", type: "float", label: "Cost Price" },
  { id: "sellPrice", type: "float", label: "Sell Price" },
  { id: "reorderLevel", type: "int", label: "Reorder Level" },
  { id: "supplier", type: "object_ref", label: "Supplier", refTypes: [FLUX_TYPES.CONTACT, FLUX_TYPES.ORGANIZATION] },
  { id: "barcode", type: "string", label: "Barcode" },
  { id: "stockValue", type: "float", label: "Stock Value", expression: "quantity * costPrice", ui: { readonly: true } },
];

const LOCATION_FIELDS: EntityFieldDef[] = [
  { id: "address", type: "text", label: "Address", ui: { multiline: true } },
  { id: "locationType", type: "enum", label: "Location Type", enumOptions: [
    { value: "warehouse", label: "Warehouse" },
    { value: "store", label: "Store" },
    { value: "office", label: "Office" },
    { value: "virtual", label: "Virtual" },
  ] },
  { id: "capacity", type: "int", label: "Capacity" },
];

// ── Entity Definitions ────────────────────────────────────────────────────

function buildEntityDefs(): EntityDef[] {
  return [
    // ── Productivity ────────────────────────────
    {
      type: FLUX_TYPES.TASK,
      nsid: "io.prismapp.flux.task",
      category: FLUX_CATEGORIES.PRODUCTIVITY,
      label: "Task",
      pluralLabel: "Tasks",
      defaultChildView: "list",
      fields: TASK_FIELDS,
    },
    {
      type: FLUX_TYPES.PROJECT,
      nsid: "io.prismapp.flux.project",
      category: FLUX_CATEGORIES.PRODUCTIVITY,
      label: "Project",
      pluralLabel: "Projects",
      defaultChildView: "kanban",
      fields: PROJECT_FIELDS,
      extraChildTypes: [FLUX_TYPES.TASK, FLUX_TYPES.MILESTONE],
    },
    {
      type: FLUX_TYPES.GOAL,
      nsid: "io.prismapp.flux.goal",
      category: FLUX_CATEGORIES.PRODUCTIVITY,
      label: "Goal",
      pluralLabel: "Goals",
      defaultChildView: "timeline",
      fields: GOAL_FIELDS,
      extraChildTypes: [FLUX_TYPES.MILESTONE],
    },
    {
      type: FLUX_TYPES.MILESTONE,
      nsid: "io.prismapp.flux.milestone",
      category: FLUX_CATEGORIES.PRODUCTIVITY,
      label: "Milestone",
      pluralLabel: "Milestones",
      childOnly: true,
      fields: MILESTONE_FIELDS,
    },

    // ── People / CRM ───────────────────────────
    {
      type: FLUX_TYPES.CONTACT,
      nsid: "io.prismapp.flux.contact",
      category: FLUX_CATEGORIES.PEOPLE,
      label: "Contact",
      pluralLabel: "Contacts",
      defaultChildView: "list",
      fields: CONTACT_FIELDS,
    },
    {
      type: FLUX_TYPES.ORGANIZATION,
      nsid: "io.prismapp.flux.organization",
      category: FLUX_CATEGORIES.PEOPLE,
      label: "Organization",
      pluralLabel: "Organizations",
      defaultChildView: "list",
      fields: ORGANIZATION_FIELDS,
      extraChildTypes: [FLUX_TYPES.CONTACT],
    },

    // ── Finance ────────────────────────────────
    {
      type: FLUX_TYPES.TRANSACTION,
      nsid: "io.prismapp.flux.transaction",
      category: FLUX_CATEGORIES.FINANCE,
      label: "Transaction",
      pluralLabel: "Transactions",
      defaultChildView: "list",
      fields: TRANSACTION_FIELDS,
      childOnly: true,
    },
    {
      type: FLUX_TYPES.ACCOUNT,
      nsid: "io.prismapp.flux.account",
      category: FLUX_CATEGORIES.FINANCE,
      label: "Account",
      pluralLabel: "Accounts",
      defaultChildView: "list",
      fields: ACCOUNT_FIELDS,
      extraChildTypes: [FLUX_TYPES.TRANSACTION],
    },
    {
      type: FLUX_TYPES.INVOICE,
      nsid: "io.prismapp.flux.invoice",
      category: FLUX_CATEGORIES.FINANCE,
      label: "Invoice",
      pluralLabel: "Invoices",
      defaultChildView: "list",
      fields: INVOICE_FIELDS,
    },

    // ── Inventory ──────────────────────────────
    {
      type: FLUX_TYPES.ITEM,
      nsid: "io.prismapp.flux.item",
      category: FLUX_CATEGORIES.INVENTORY,
      label: "Item",
      pluralLabel: "Items",
      defaultChildView: "grid",
      fields: ITEM_FIELDS,
    },
    {
      type: FLUX_TYPES.LOCATION,
      nsid: "io.prismapp.flux.location",
      category: FLUX_CATEGORIES.INVENTORY,
      label: "Location",
      pluralLabel: "Locations",
      defaultChildView: "list",
      fields: LOCATION_FIELDS,
      extraChildTypes: [FLUX_TYPES.ITEM],
    },
  ];
}

// ── Edge Definitions ──────────────────────────────────────────────────────

function buildEdgeDefs(): EdgeTypeDef[] {
  return [
    {
      relation: FLUX_EDGES.ASSIGNED_TO,
      nsid: "io.prismapp.flux.assigned-to",
      label: "Assigned To",
      behavior: "assignment",
      sourceTypes: [FLUX_TYPES.TASK, FLUX_TYPES.PROJECT, FLUX_TYPES.INVOICE],
      targetTypes: [FLUX_TYPES.CONTACT],
      suggestInline: true,
    },
    {
      relation: FLUX_EDGES.DEPENDS_ON,
      nsid: "io.prismapp.flux.depends-on",
      label: "Depends On",
      behavior: "dependency",
      sourceCategories: [FLUX_CATEGORIES.PRODUCTIVITY],
      targetCategories: [FLUX_CATEGORIES.PRODUCTIVITY],
    },
    {
      relation: FLUX_EDGES.BLOCKS,
      nsid: "io.prismapp.flux.blocks",
      label: "Blocks",
      behavior: "dependency",
      sourceCategories: [FLUX_CATEGORIES.PRODUCTIVITY],
      targetCategories: [FLUX_CATEGORIES.PRODUCTIVITY],
    },
    {
      relation: FLUX_EDGES.BELONGS_TO,
      nsid: "io.prismapp.flux.belongs-to",
      label: "Belongs To",
      behavior: "membership",
      sourceTypes: [FLUX_TYPES.TASK],
      targetTypes: [FLUX_TYPES.PROJECT, FLUX_TYPES.GOAL],
    },
    {
      relation: FLUX_EDGES.RELATED_TO,
      nsid: "io.prismapp.flux.related-to",
      label: "Related To",
      behavior: "weak",
      undirected: true,
      suggestInline: true,
    },
    {
      relation: FLUX_EDGES.INVOICED_TO,
      nsid: "io.prismapp.flux.invoiced-to",
      label: "Invoiced To",
      behavior: "assignment",
      sourceTypes: [FLUX_TYPES.INVOICE],
      targetTypes: [FLUX_TYPES.CONTACT, FLUX_TYPES.ORGANIZATION],
    },
    {
      relation: FLUX_EDGES.STORED_AT,
      nsid: "io.prismapp.flux.stored-at",
      label: "Stored At",
      behavior: "membership",
      sourceTypes: [FLUX_TYPES.ITEM],
      targetTypes: [FLUX_TYPES.LOCATION],
    },
  ];
}

// ── Automation Presets ─────────────────────────────────────────────────────

function buildAutomationPresets(): FluxAutomationPreset[] {
  return [
    {
      id: "flux:auto:task-complete-timestamp",
      name: "Auto-fill completion timestamp",
      entityType: FLUX_TYPES.TASK,
      trigger: "on_status_change",
      condition: "status == 'done'",
      actions: [
        { kind: "set_field", target: "completedAt", value: "{{now}}" },
      ],
    },
    {
      id: "flux:auto:task-recurring-reset",
      name: "Reset recurring task",
      entityType: FLUX_TYPES.TASK,
      trigger: "on_status_change",
      condition: "status == 'done' and recurring != 'none'",
      actions: [
        { kind: "move_to_status", target: "status", value: "todo" },
        { kind: "set_field", target: "completedAt", value: "" },
      ],
    },
    {
      id: "flux:auto:task-overdue-notify",
      name: "Notify on overdue task",
      entityType: FLUX_TYPES.TASK,
      trigger: "on_due_date",
      condition: "status != 'done' and status != 'cancelled'",
      actions: [
        { kind: "send_notification", target: "assignee", value: "Task '{{name}}' is overdue" },
      ],
    },
    {
      id: "flux:auto:invoice-overdue",
      name: "Mark invoice overdue",
      entityType: FLUX_TYPES.INVOICE,
      trigger: "on_due_date",
      condition: "status == 'sent'",
      actions: [
        { kind: "move_to_status", target: "status", value: "overdue" },
        { kind: "send_notification", target: "owner", value: "Invoice {{invoiceNumber}} is overdue" },
      ],
    },
    {
      id: "flux:auto:item-low-stock",
      name: "Low stock alert",
      entityType: FLUX_TYPES.ITEM,
      trigger: "on_update",
      condition: "quantity <= reorderLevel and quantity > 0",
      actions: [
        { kind: "move_to_status", target: "status", value: "low_stock" },
        { kind: "send_notification", target: "owner", value: "Item '{{name}}' is low on stock ({{quantity}} remaining)" },
      ],
    },
    {
      id: "flux:auto:item-out-of-stock",
      name: "Out of stock alert",
      entityType: FLUX_TYPES.ITEM,
      trigger: "on_update",
      condition: "quantity == 0",
      actions: [
        { kind: "move_to_status", target: "status", value: "out_of_stock" },
      ],
    },
    {
      id: "flux:auto:goal-progress",
      name: "Auto-update goal progress",
      entityType: FLUX_TYPES.GOAL,
      trigger: "on_update",
      condition: "targetValue > 0",
      actions: [
        { kind: "set_field", target: "progress", value: "{{currentValue / targetValue * 100}}" },
      ],
    },
    {
      id: "flux:auto:project-complete",
      name: "Complete project when all tasks done",
      entityType: FLUX_TYPES.PROJECT,
      trigger: "on_update",
      condition: "progress >= 100",
      actions: [
        { kind: "move_to_status", target: "status", value: "completed" },
      ],
    },
  ];
}

// ── Import/Export ──────────────────────────────────────────────────────────

function escapeCSV(value: unknown): string {
  const str = String(value ?? "");
  if (str.includes(",") || str.includes('"') || str.includes("\n")) {
    return '"' + str.replace(/"/g, '""') + '"';
  }
  return str;
}

function exportToCSV(objects: Record<string, unknown>[], fields?: string[]): string {
  if (objects.length === 0) return "";

  const firstObj = objects[0];
  if (!firstObj) return "";
  const keys = fields ?? Object.keys(firstObj);
  const header = keys.map(escapeCSV).join(",");

  const rows = objects.map(obj =>
    keys.map(k => escapeCSV(obj[k])).join(",")
  );

  return [header, ...rows].join("\n");
}

function exportToJSON(objects: Record<string, unknown>[]): string {
  return JSON.stringify(objects, null, 2);
}

function parseCSV(data: string): Record<string, unknown>[] {
  const lines = data.split("\n").filter(l => l.trim().length > 0);
  if (lines.length < 2) return [];

  const headerLine = lines[0];
  if (!headerLine) return [];
  const headers = headerLine.split(",").map(h => h.trim().replace(/^"|"$/g, ""));
  const result: Record<string, unknown>[] = [];

  for (let i = 1; i < lines.length; i++) {
    const line = lines[i];
    if (!line) continue;
    const values = line.split(",").map(v => v.trim().replace(/^"|"$/g, ""));
    const obj: Record<string, unknown> = {};
    for (let j = 0; j < headers.length; j++) {
      const key = headers[j];
      if (key) {
        const val = values[j] ?? "";
        // Try to parse numbers
        const num = Number(val);
        obj[key] = val === "" ? "" : !isNaN(num) && val !== "" ? num : val;
      }
    }
    result.push(obj);
  }

  return result;
}

function parseJSON(data: string): Record<string, unknown>[] {
  const parsed: unknown = JSON.parse(data);
  if (!Array.isArray(parsed)) throw new Error("Expected JSON array");
  return parsed as Record<string, unknown>[];
}

// ── Factory ───────────────────────────────────────────────────────────────

export function createFluxRegistry(): FluxRegistry {
  const entityDefs = buildEntityDefs();
  const edgeDefs = buildEdgeDefs();
  const presets = buildAutomationPresets();

  return {
    getEntityDefs(): EntityDef[] {
      return entityDefs;
    },

    getEdgeDefs(): EdgeTypeDef[] {
      return edgeDefs;
    },

    getEntityDef(type: FluxEntityType): EntityDef | undefined {
      return entityDefs.find(d => d.type === type);
    },

    getEdgeDef(relation: FluxEdgeType): EdgeTypeDef | undefined {
      return edgeDefs.find(d => d.relation === relation);
    },

    getAutomationPresets(): FluxAutomationPreset[] {
      return presets;
    },

    getPresetsForEntity(type: FluxEntityType): FluxAutomationPreset[] {
      return presets.filter(p => p.entityType === type);
    },

    exportData(objects: Record<string, unknown>[], options: FluxExportOptions): string {
      switch (options.format) {
        case "csv":
          return exportToCSV(objects, options.fields);
        case "json":
          return exportToJSON(objects);
      }
    },

    parseImport(data: string, format: FluxExportFormat): Record<string, unknown>[] {
      switch (format) {
        case "csv":
          return parseCSV(data);
        case "json":
          return parseJSON(data);
      }
    },
  };
}
