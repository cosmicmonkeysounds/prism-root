/**
 * @prism/plugin-finance — Finance Domain Registry (Layer 1)
 *
 * Registers loans, grants, and budgets. Extends existing Flux finance types.
 */

import type { EntityDef, EntityFieldDef, EdgeTypeDef } from "../../object-model/types.js";
import type { FluxAutomationPreset } from "../../flux/flux-types.js";
import { FLUX_TYPES } from "../../flux/flux-types.js";
import type { PrismPlugin } from "../../plugin/plugin-types.js";
import { pluginId } from "../../plugin/plugin-types.js";
import type { FinanceRegistry, FinanceEntityType, FinanceEdgeType } from "./finance-types.js";
import { FINANCE_CATEGORIES, FINANCE_TYPES, FINANCE_EDGES } from "./finance-types.js";

// ── Field Definitions ────────────────────────────────────────────────────

const LOAN_FIELDS: EntityFieldDef[] = [
  { id: "lender", type: "object_ref", label: "Lender", refTypes: [FLUX_TYPES.CONTACT, FLUX_TYPES.ORGANIZATION] },
  { id: "principal", type: "float", label: "Principal Amount", required: true },
  { id: "interestRate", type: "float", label: "Interest Rate (%)" },
  { id: "termMonths", type: "int", label: "Term (months)" },
  { id: "monthlyPayment", type: "float", label: "Monthly Payment", expression: "principal * (interestRate / 100 / 12) / (1 - (1 + interestRate / 100 / 12) ^ -termMonths)" },
  { id: "remainingBalance", type: "float", label: "Remaining Balance" },
  { id: "currency", type: "enum", label: "Currency", enumOptions: [
    { value: "USD", label: "USD" },
    { value: "EUR", label: "EUR" },
    { value: "GBP", label: "GBP" },
  ], default: "USD" },
  { id: "startDate", type: "date", label: "Start Date" },
  { id: "endDate", type: "date", label: "End Date" },
  { id: "nextPaymentDate", type: "date", label: "Next Payment" },
  { id: "account", type: "object_ref", label: "Linked Account", refTypes: [FLUX_TYPES.ACCOUNT] },
  { id: "notes", type: "text", label: "Notes", ui: { multiline: true } },
];

const GRANT_FIELDS: EntityFieldDef[] = [
  { id: "grantor", type: "object_ref", label: "Grantor", refTypes: [FLUX_TYPES.ORGANIZATION] },
  { id: "amount", type: "float", label: "Award Amount" },
  { id: "currency", type: "enum", label: "Currency", enumOptions: [
    { value: "USD", label: "USD" },
    { value: "EUR", label: "EUR" },
    { value: "GBP", label: "GBP" },
  ], default: "USD" },
  { id: "applicationDeadline", type: "date", label: "Application Deadline" },
  { id: "awardDate", type: "date", label: "Award Date" },
  { id: "reportingDeadline", type: "date", label: "Reporting Deadline" },
  { id: "disbursedAmount", type: "float", label: "Disbursed", default: 0 },
  { id: "matchRequired", type: "bool", label: "Match Required", default: false },
  { id: "matchPercentage", type: "float", label: "Match (%)" },
  { id: "purpose", type: "text", label: "Purpose", ui: { multiline: true } },
  { id: "restrictions", type: "text", label: "Restrictions", ui: { multiline: true, group: "Compliance" } },
];

const BUDGET_FIELDS: EntityFieldDef[] = [
  { id: "period", type: "enum", label: "Period", enumOptions: [
    { value: "weekly", label: "Weekly" },
    { value: "monthly", label: "Monthly" },
    { value: "quarterly", label: "Quarterly" },
    { value: "yearly", label: "Yearly" },
    { value: "custom", label: "Custom" },
  ], default: "monthly" },
  { id: "startDate", type: "date", label: "Start Date", required: true },
  { id: "endDate", type: "date", label: "End Date" },
  { id: "plannedAmount", type: "float", label: "Planned Amount", required: true },
  { id: "actualAmount", type: "float", label: "Actual Spent", default: 0, ui: { readonly: true } },
  { id: "remainingAmount", type: "float", label: "Remaining", expression: "plannedAmount - actualAmount", ui: { readonly: true } },
  { id: "currency", type: "enum", label: "Currency", enumOptions: [
    { value: "USD", label: "USD" },
    { value: "EUR", label: "EUR" },
    { value: "GBP", label: "GBP" },
  ], default: "USD" },
  { id: "category", type: "string", label: "Category" },
  { id: "notes", type: "text", label: "Notes", ui: { multiline: true } },
];

// ── Entity Definitions ───────────────────────────────────────────────────

function buildEntityDefs(): EntityDef[] {
  return [
    {
      type: FINANCE_TYPES.LOAN,
      nsid: "io.prismapp.finance.loan",
      category: FINANCE_CATEGORIES.LENDING,
      label: "Loan",
      pluralLabel: "Loans",
      defaultChildView: "list",
      fields: LOAN_FIELDS,
      extraChildTypes: [FLUX_TYPES.TRANSACTION],
    },
    {
      type: FINANCE_TYPES.GRANT,
      nsid: "io.prismapp.finance.grant",
      category: FINANCE_CATEGORIES.LENDING,
      label: "Grant",
      pluralLabel: "Grants",
      defaultChildView: "list",
      fields: GRANT_FIELDS,
    },
    {
      type: FINANCE_TYPES.BUDGET,
      nsid: "io.prismapp.finance.budget",
      category: FINANCE_CATEGORIES.BUDGETING,
      label: "Budget",
      pluralLabel: "Budgets",
      defaultChildView: "list",
      fields: BUDGET_FIELDS,
      extraChildTypes: [FLUX_TYPES.TRANSACTION],
    },
  ];
}

// ── Edge Definitions ─────────────────────────────────────────────────────

function buildEdgeDefs(): EdgeTypeDef[] {
  return [
    {
      relation: FINANCE_EDGES.FUNDED_BY,
      nsid: "io.prismapp.finance.funded-by",
      label: "Funded By",
      behavior: "membership",
      sourceTypes: [FLUX_TYPES.TRANSACTION],
      targetTypes: [FINANCE_TYPES.GRANT, FINANCE_TYPES.LOAN],
    },
    {
      relation: FINANCE_EDGES.BUDGET_FOR,
      nsid: "io.prismapp.finance.budget-for",
      label: "Budget For",
      behavior: "weak",
      sourceTypes: [FINANCE_TYPES.BUDGET],
      targetTypes: [FLUX_TYPES.PROJECT, FLUX_TYPES.ACCOUNT],
    },
    {
      relation: FINANCE_EDGES.PAYMENT_OF,
      nsid: "io.prismapp.finance.payment-of",
      label: "Payment Of",
      behavior: "weak",
      sourceTypes: [FLUX_TYPES.TRANSACTION],
      targetTypes: [FINANCE_TYPES.LOAN, FLUX_TYPES.INVOICE],
    },
  ];
}

// ── Automation Presets ────────────────────────────────────────────────────

function buildAutomationPresets(): FluxAutomationPreset[] {
  return [
    {
      id: "finance:auto:loan-payment-reminder",
      name: "Loan payment reminder",
      entityType: FINANCE_TYPES.LOAN,
      trigger: "on_due_date",
      condition: "status == 'active'",
      actions: [
        { kind: "send_notification", target: "owner", value: "Loan payment of {{monthlyPayment}} {{currency}} due for '{{name}}'" },
      ],
    },
    {
      id: "finance:auto:grant-deadline-alert",
      name: "Grant deadline alert",
      entityType: FINANCE_TYPES.GRANT,
      trigger: "on_due_date",
      condition: "status == 'drafting' or status == 'researching'",
      actions: [
        { kind: "send_notification", target: "owner", value: "Grant application deadline approaching for '{{name}}'" },
      ],
    },
    {
      id: "finance:auto:budget-overspend",
      name: "Budget overspend alert",
      entityType: FINANCE_TYPES.BUDGET,
      trigger: "on_update",
      condition: "actualAmount > plannedAmount",
      actions: [
        { kind: "send_notification", target: "owner", value: "Budget '{{name}}' exceeded: {{actualAmount}} / {{plannedAmount}} {{currency}}" },
      ],
    },
  ];
}

// ── Plugin ───────────────────────────────────────────────────────────────

function buildPlugin(): PrismPlugin {
  return {
    id: pluginId("prism.plugin.finance"),
    name: "Finance",
    contributes: {
      views: [
        { id: "finance:loans", label: "Loans", zone: "content", componentId: "LoanListView", description: "Loan tracker" },
        { id: "finance:grants", label: "Grants", zone: "content", componentId: "GrantListView", description: "Grant applications" },
        { id: "finance:budgets", label: "Budgets", zone: "content", componentId: "BudgetView", description: "Budget planner" },
      ],
      commands: [
        { id: "finance:new-loan", label: "New Loan", category: "Finance", action: "finance.newLoan" },
        { id: "finance:new-grant", label: "New Grant", category: "Finance", action: "finance.newGrant" },
        { id: "finance:new-budget", label: "New Budget", category: "Finance", action: "finance.newBudget" },
      ],
      activityBar: [
        { id: "finance:activity", label: "Finance", position: "top", priority: 30 },
      ],
    },
  };
}

// ── Factory ────────────────────���─────────────────────────────────────────

export function createFinanceRegistry(): FinanceRegistry {
  const entityDefs = buildEntityDefs();
  const edgeDefs = buildEdgeDefs();
  const presets = buildAutomationPresets();
  const plugin = buildPlugin();

  return {
    getEntityDefs: () => entityDefs,
    getEdgeDefs: () => edgeDefs,
    getEntityDef: (type: FinanceEntityType) => entityDefs.find(d => d.type === type),
    getEdgeDef: (relation: FinanceEdgeType) => edgeDefs.find(d => d.relation === relation),
    getAutomationPresets: () => presets,
    getPlugin: () => plugin,
  };
}
