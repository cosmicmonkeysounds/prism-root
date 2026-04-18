//! `plugin_bundles::finance` — Finance domain bundle.
//!
//! Port of `kernel/plugin-bundles/finance/finance.ts`: loans, grants,
//! budgets on top of Flux transactions / accounts / invoices.

use serde_json::json;

use super::builders::{
    edge_def, entity_def, enum_options, owned_strings, ui_multiline, ui_multiline_group,
    ui_readonly, EdgeSpec, EntitySpec, Field,
};
use super::flux_types::{
    flux_types, FluxActionKind, FluxAutomationAction, FluxAutomationPreset, FluxTriggerKind,
};
use super::install::{PluginBundle, PluginInstallContext};
use crate::foundation::object_model::types::DefaultChildView;
use crate::foundation::object_model::{
    EdgeBehavior, EdgeTypeDef, EntityDef, EntityFieldDef, EntityFieldType,
};
use crate::kernel::plugin::{
    plugin_id, ActivityBarContributionDef, ActivityBarPosition, CommandContributionDef,
    PluginContributions, PrismPlugin, ViewContributionDef, ViewZone,
};

// ── Domain constants ────────────────────────────────────────────────────────

pub mod finance_categories {
    pub const LENDING: &str = "finance:lending";
    pub const BUDGETING: &str = "finance:budgeting";
}

pub mod finance_types {
    pub const LOAN: &str = "finance:loan";
    pub const GRANT: &str = "finance:grant";
    pub const BUDGET: &str = "finance:budget";
}

pub mod finance_edges {
    pub const FUNDED_BY: &str = "finance:funded-by";
    pub const BUDGET_FOR: &str = "finance:budget-for";
    pub const PAYMENT_OF: &str = "finance:payment-of";
}

// ── Fields ──────────────────────────────────────────────────────────────────

fn currency_field() -> EntityFieldDef {
    Field::new("currency", EntityFieldType::Enum)
        .label("Currency")
        .enum_values(enum_options(&[
            ("USD", "USD"),
            ("EUR", "EUR"),
            ("GBP", "GBP"),
        ]))
        .default(json!("USD"))
        .build()
}

fn loan_fields() -> Vec<EntityFieldDef> {
    vec![
        Field::new("lender", EntityFieldType::ObjectRef)
            .label("Lender")
            .ref_types([flux_types::CONTACT, flux_types::ORGANIZATION])
            .build(),
        Field::new("principal", EntityFieldType::Float)
            .label("Principal Amount")
            .required()
            .build(),
        Field::new("interestRate", EntityFieldType::Float)
            .label("Interest Rate (%)")
            .build(),
        Field::new("termMonths", EntityFieldType::Int)
            .label("Term (months)")
            .build(),
        Field::new("monthlyPayment", EntityFieldType::Float)
            .label("Monthly Payment")
            .expression(
                "principal * (interestRate / 100 / 12) / (1 - (1 + interestRate / 100 / 12) ^ -termMonths)",
            )
            .build(),
        Field::new("remainingBalance", EntityFieldType::Float)
            .label("Remaining Balance")
            .build(),
        currency_field(),
        Field::new("startDate", EntityFieldType::Date)
            .label("Start Date")
            .build(),
        Field::new("endDate", EntityFieldType::Date)
            .label("End Date")
            .build(),
        Field::new("nextPaymentDate", EntityFieldType::Date)
            .label("Next Payment")
            .build(),
        Field::new("account", EntityFieldType::ObjectRef)
            .label("Linked Account")
            .ref_types([flux_types::ACCOUNT])
            .build(),
        Field::new("notes", EntityFieldType::Text)
            .label("Notes")
            .ui(ui_multiline())
            .build(),
    ]
}

fn grant_fields() -> Vec<EntityFieldDef> {
    vec![
        Field::new("grantor", EntityFieldType::ObjectRef)
            .label("Grantor")
            .ref_types([flux_types::ORGANIZATION])
            .build(),
        Field::new("amount", EntityFieldType::Float)
            .label("Award Amount")
            .build(),
        currency_field(),
        Field::new("applicationDeadline", EntityFieldType::Date)
            .label("Application Deadline")
            .build(),
        Field::new("awardDate", EntityFieldType::Date)
            .label("Award Date")
            .build(),
        Field::new("reportingDeadline", EntityFieldType::Date)
            .label("Reporting Deadline")
            .build(),
        Field::new("disbursedAmount", EntityFieldType::Float)
            .label("Disbursed")
            .default(json!(0))
            .build(),
        Field::new("matchRequired", EntityFieldType::Bool)
            .label("Match Required")
            .default(json!(false))
            .build(),
        Field::new("matchPercentage", EntityFieldType::Float)
            .label("Match (%)")
            .build(),
        Field::new("purpose", EntityFieldType::Text)
            .label("Purpose")
            .ui(ui_multiline())
            .build(),
        Field::new("restrictions", EntityFieldType::Text)
            .label("Restrictions")
            .ui(ui_multiline_group("Compliance"))
            .build(),
    ]
}

fn budget_fields() -> Vec<EntityFieldDef> {
    vec![
        Field::new("period", EntityFieldType::Enum)
            .label("Period")
            .enum_values(enum_options(&[
                ("weekly", "Weekly"),
                ("monthly", "Monthly"),
                ("quarterly", "Quarterly"),
                ("yearly", "Yearly"),
                ("custom", "Custom"),
            ]))
            .default(json!("monthly"))
            .build(),
        Field::new("startDate", EntityFieldType::Date)
            .label("Start Date")
            .required()
            .build(),
        Field::new("endDate", EntityFieldType::Date)
            .label("End Date")
            .build(),
        Field::new("plannedAmount", EntityFieldType::Float)
            .label("Planned Amount")
            .required()
            .build(),
        Field::new("actualAmount", EntityFieldType::Float)
            .label("Actual Spent")
            .default(json!(0))
            .ui(ui_readonly())
            .build(),
        Field::new("remainingAmount", EntityFieldType::Float)
            .label("Remaining")
            .expression("plannedAmount - actualAmount")
            .ui(ui_readonly())
            .build(),
        currency_field(),
        Field::new("category", EntityFieldType::String)
            .label("Category")
            .build(),
        Field::new("notes", EntityFieldType::Text)
            .label("Notes")
            .ui(ui_multiline())
            .build(),
    ]
}

// ── Entity + edge defs ──────────────────────────────────────────────────────

pub fn build_entity_defs() -> Vec<EntityDef> {
    vec![
        entity_def(EntitySpec {
            type_name: finance_types::LOAN,
            nsid: "io.prismapp.finance.loan",
            category: finance_categories::LENDING,
            label: "Loan",
            plural_label: "Loans",
            default_child_view: Some(DefaultChildView::List),
            child_only: false,
            extra_child_types: Some(owned_strings([flux_types::TRANSACTION])),
            fields: loan_fields(),
        }),
        entity_def(EntitySpec {
            type_name: finance_types::GRANT,
            nsid: "io.prismapp.finance.grant",
            category: finance_categories::LENDING,
            label: "Grant",
            plural_label: "Grants",
            default_child_view: Some(DefaultChildView::List),
            child_only: false,
            extra_child_types: None,
            fields: grant_fields(),
        }),
        entity_def(EntitySpec {
            type_name: finance_types::BUDGET,
            nsid: "io.prismapp.finance.budget",
            category: finance_categories::BUDGETING,
            label: "Budget",
            plural_label: "Budgets",
            default_child_view: Some(DefaultChildView::List),
            child_only: false,
            extra_child_types: Some(owned_strings([flux_types::TRANSACTION])),
            fields: budget_fields(),
        }),
    ]
}

pub fn build_edge_defs() -> Vec<EdgeTypeDef> {
    vec![
        edge_def(EdgeSpec {
            relation: finance_edges::FUNDED_BY,
            nsid: "io.prismapp.finance.funded-by",
            label: "Funded By",
            behavior: EdgeBehavior::Membership,
            source_types: owned_strings([flux_types::TRANSACTION]),
            target_types: Some(owned_strings([finance_types::GRANT, finance_types::LOAN])),
            description: None,
            suggest_inline: false,
            undirected: false,
        }),
        edge_def(EdgeSpec {
            relation: finance_edges::BUDGET_FOR,
            nsid: "io.prismapp.finance.budget-for",
            label: "Budget For",
            behavior: EdgeBehavior::Weak,
            source_types: owned_strings([finance_types::BUDGET]),
            target_types: Some(owned_strings([flux_types::PROJECT, flux_types::ACCOUNT])),
            description: None,
            suggest_inline: false,
            undirected: false,
        }),
        edge_def(EdgeSpec {
            relation: finance_edges::PAYMENT_OF,
            nsid: "io.prismapp.finance.payment-of",
            label: "Payment Of",
            behavior: EdgeBehavior::Weak,
            source_types: owned_strings([flux_types::TRANSACTION]),
            target_types: Some(owned_strings([finance_types::LOAN, flux_types::INVOICE])),
            description: None,
            suggest_inline: false,
            undirected: false,
        }),
    ]
}

// ── Automation presets ──────────────────────────────────────────────────────

pub fn build_automation_presets() -> Vec<FluxAutomationPreset> {
    vec![
        FluxAutomationPreset {
            id: "finance:auto:loan-payment-reminder".into(),
            name: "Loan payment reminder".into(),
            entity_type: finance_types::LOAN.into(),
            trigger: FluxTriggerKind::OnDueDate,
            condition: Some("status == 'active'".into()),
            actions: vec![FluxAutomationAction {
                kind: FluxActionKind::SendNotification,
                target: "owner".into(),
                value: "Loan payment of {{monthlyPayment}} {{currency}} due for '{{name}}'".into(),
            }],
        },
        FluxAutomationPreset {
            id: "finance:auto:grant-deadline-alert".into(),
            name: "Grant deadline alert".into(),
            entity_type: finance_types::GRANT.into(),
            trigger: FluxTriggerKind::OnDueDate,
            condition: Some("status == 'drafting' or status == 'researching'".into()),
            actions: vec![FluxAutomationAction {
                kind: FluxActionKind::SendNotification,
                target: "owner".into(),
                value: "Grant application deadline approaching for '{{name}}'".into(),
            }],
        },
        FluxAutomationPreset {
            id: "finance:auto:budget-overspend".into(),
            name: "Budget overspend alert".into(),
            entity_type: finance_types::BUDGET.into(),
            trigger: FluxTriggerKind::OnUpdate,
            condition: Some("actualAmount > plannedAmount".into()),
            actions: vec![FluxAutomationAction {
                kind: FluxActionKind::SendNotification,
                target: "owner".into(),
                value:
                    "Budget '{{name}}' exceeded: {{actualAmount}} / {{plannedAmount}} {{currency}}"
                        .into(),
            }],
        },
    ]
}

// ── Plugin ──────────────────────────────────────────────────────────────────

pub fn build_plugin() -> PrismPlugin {
    PrismPlugin::new(plugin_id("prism.plugin.finance"), "Finance").with_contributes(
        PluginContributions {
            views: Some(vec![
                ViewContributionDef {
                    id: "finance:loans".into(),
                    label: "Loans".into(),
                    zone: ViewZone::Content,
                    component_id: "LoanListView".into(),
                    icon: None,
                    default_visible: None,
                    description: Some("Loan tracker".into()),
                    tags: None,
                },
                ViewContributionDef {
                    id: "finance:grants".into(),
                    label: "Grants".into(),
                    zone: ViewZone::Content,
                    component_id: "GrantListView".into(),
                    icon: None,
                    default_visible: None,
                    description: Some("Grant applications".into()),
                    tags: None,
                },
                ViewContributionDef {
                    id: "finance:budgets".into(),
                    label: "Budgets".into(),
                    zone: ViewZone::Content,
                    component_id: "BudgetView".into(),
                    icon: None,
                    default_visible: None,
                    description: Some("Budget planner".into()),
                    tags: None,
                },
            ]),
            commands: Some(vec![
                CommandContributionDef {
                    id: "finance:new-loan".into(),
                    label: "New Loan".into(),
                    category: "Finance".into(),
                    shortcut: None,
                    description: None,
                    action: "finance.newLoan".into(),
                    payload: None,
                    when: None,
                },
                CommandContributionDef {
                    id: "finance:new-grant".into(),
                    label: "New Grant".into(),
                    category: "Finance".into(),
                    shortcut: None,
                    description: None,
                    action: "finance.newGrant".into(),
                    payload: None,
                    when: None,
                },
                CommandContributionDef {
                    id: "finance:new-budget".into(),
                    label: "New Budget".into(),
                    category: "Finance".into(),
                    shortcut: None,
                    description: None,
                    action: "finance.newBudget".into(),
                    payload: None,
                    when: None,
                },
            ]),
            activity_bar: Some(vec![ActivityBarContributionDef {
                id: "finance:activity".into(),
                label: "Finance".into(),
                icon: None,
                position: Some(ActivityBarPosition::Top),
                priority: Some(30),
            }]),
            keybindings: None,
            context_menus: None,
            settings: None,
            toolbar: None,
            status_bar: None,
            weak_ref_providers: None,
            immersive: None,
        },
    )
}

pub struct FinanceBundle;

impl PluginBundle for FinanceBundle {
    fn id(&self) -> &str {
        "prism.plugin.finance"
    }

    fn name(&self) -> &str {
        "Finance"
    }

    fn install(&self, ctx: &mut PluginInstallContext<'_>) {
        ctx.object_registry.register_all(build_entity_defs());
        ctx.object_registry.register_edges(build_edge_defs());
        ctx.plugin_registry.register(build_plugin());
    }
}

pub fn create_finance_bundle() -> Box<dyn PluginBundle> {
    Box::new(FinanceBundle)
}
