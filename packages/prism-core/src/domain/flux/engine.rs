//! Flux registry factory + import/export.
//!
//! Port of `packages/prism-core/src/domain/flux/flux.ts` at commit
//! 8426588. The TS file returned a `FluxRegistry` interface object
//! built from closures over local arrays. The Rust port collapses
//! that into a concrete [`FluxRegistry`] struct with the same
//! method surface (`get_entity_defs`, `get_entity_def`,
//! `get_edge_defs`, `get_edge_def`, `get_automation_presets`,
//! `get_presets_for_entity`, `export_data`, `parse_import`).

use indexmap::IndexMap;
use serde_json::{Number, Value};

/// Insertion-order-preserving JSON object map. The TS port relied on
/// `Object.keys()` returning insertion order; `serde_json::Map`
/// defaults to `BTreeMap` so we use `IndexMap` instead for the
/// import / export surface.
pub type ObjectMap = IndexMap<String, Value>;

use super::types::{
    flux_categories, flux_edges, flux_types, FluxAutomationAction, FluxAutomationActionKind,
    FluxAutomationPreset, FluxExportFormat, FluxExportOptions, FluxTriggerKind, StatusOption,
    CONTACT_TYPES, TRANSACTION_TYPES,
};
use crate::foundation::object_model::types::{DefaultChildView, EnumOption};
use crate::foundation::object_model::{
    EdgeBehavior, EdgeTypeDef, EntityDef, EntityFieldDef, EntityFieldType, UiHints,
};

// ── Field-builder helpers ──────────────────────────────────────────

fn enum_options_from(values: &[StatusOption]) -> Vec<EnumOption> {
    values
        .iter()
        .map(|v| EnumOption {
            value: v.value.into(),
            label: v.label.into(),
        })
        .collect()
}

fn enum_options_lit(pairs: &[(&str, &str)]) -> Vec<EnumOption> {
    pairs
        .iter()
        .map(|(v, l)| EnumOption {
            value: (*v).into(),
            label: (*l).into(),
        })
        .collect()
}

fn ui_multiline() -> UiHints {
    UiHints {
        multiline: Some(true),
        ..Default::default()
    }
}

fn ui_multiline_group(group: &str) -> UiHints {
    UiHints {
        multiline: Some(true),
        group: Some(group.into()),
        ..Default::default()
    }
}

fn ui_group(group: &str) -> UiHints {
    UiHints {
        group: Some(group.into()),
        ..Default::default()
    }
}

fn ui_placeholder(placeholder: &str) -> UiHints {
    UiHints {
        placeholder: Some(placeholder.into()),
        ..Default::default()
    }
}

fn ui_hidden() -> UiHints {
    UiHints {
        hidden: Some(true),
        ..Default::default()
    }
}

fn ui_readonly() -> UiHints {
    UiHints {
        readonly: Some(true),
        ..Default::default()
    }
}

#[derive(Debug, Clone, Default)]
struct FieldB {
    id: &'static str,
    field_type: Option<EntityFieldType>,
    label: Option<&'static str>,
    required: Option<bool>,
    default: Option<Value>,
    expression: Option<&'static str>,
    enum_options: Option<Vec<EnumOption>>,
    ref_types: Option<Vec<String>>,
    ui: Option<UiHints>,
}

impl FieldB {
    fn new(id: &'static str, field_type: EntityFieldType) -> Self {
        Self {
            id,
            field_type: Some(field_type),
            ..Self::default()
        }
    }
    fn label(mut self, l: &'static str) -> Self {
        self.label = Some(l);
        self
    }
    fn required(mut self) -> Self {
        self.required = Some(true);
        self
    }
    fn default_val(mut self, v: Value) -> Self {
        self.default = Some(v);
        self
    }
    fn expr(mut self, e: &'static str) -> Self {
        self.expression = Some(e);
        self
    }
    fn enum_vals(mut self, opts: Vec<EnumOption>) -> Self {
        self.enum_options = Some(opts);
        self
    }
    fn ref_types(mut self, refs: &[&str]) -> Self {
        self.ref_types = Some(refs.iter().map(|s| (*s).to_string()).collect());
        self
    }
    fn ui(mut self, h: UiHints) -> Self {
        self.ui = Some(h);
        self
    }
    fn build(self) -> EntityFieldDef {
        EntityFieldDef {
            id: self.id.into(),
            field_type: self.field_type.expect("field type required"),
            label: self.label.map(|s| s.into()),
            description: None,
            required: self.required,
            default: self.default,
            expression: self.expression.map(|s| s.into()),
            enum_options: self.enum_options,
            ref_types: self.ref_types,
            lookup_relation: None,
            lookup_field: None,
            rollup_relation: None,
            rollup_field: None,
            rollup_function: None,
            ui: self.ui,
        }
    }
}

// ── Field Definitions ──────────────────────────────────────────────

fn task_fields() -> Vec<EntityFieldDef> {
    vec![
        FieldB::new("priority", EntityFieldType::Enum)
            .label("Priority")
            .enum_vals(enum_options_lit(&[
                ("urgent", "Urgent"),
                ("high", "High"),
                ("medium", "Medium"),
                ("low", "Low"),
                ("none", "None"),
            ]))
            .default_val(Value::String("medium".into()))
            .build(),
        FieldB::new("effort", EntityFieldType::Enum)
            .label("Effort")
            .enum_vals(enum_options_lit(&[
                ("xs", "XS"),
                ("s", "S"),
                ("m", "M"),
                ("l", "L"),
                ("xl", "XL"),
            ]))
            .build(),
        FieldB::new("dueDate", EntityFieldType::Date)
            .label("Due Date")
            .build(),
        FieldB::new("completedAt", EntityFieldType::Datetime)
            .label("Completed At")
            .ui(ui_readonly())
            .build(),
        FieldB::new("estimateHours", EntityFieldType::Float)
            .label("Estimate (hours)")
            .build(),
        FieldB::new("actualHours", EntityFieldType::Float)
            .label("Actual (hours)")
            .build(),
        FieldB::new("recurring", EntityFieldType::Enum)
            .label("Recurring")
            .enum_vals(enum_options_lit(&[
                ("none", "None"),
                ("daily", "Daily"),
                ("weekly", "Weekly"),
                ("biweekly", "Bi-weekly"),
                ("monthly", "Monthly"),
                ("quarterly", "Quarterly"),
                ("yearly", "Yearly"),
            ]))
            .default_val(Value::String("none".into()))
            .build(),
    ]
}

fn project_fields() -> Vec<EntityFieldDef> {
    vec![
        FieldB::new("startDate", EntityFieldType::Date)
            .label("Start Date")
            .build(),
        FieldB::new("targetDate", EntityFieldType::Date)
            .label("Target Date")
            .build(),
        FieldB::new("budget", EntityFieldType::Float)
            .label("Budget")
            .build(),
        FieldB::new("progress", EntityFieldType::Float)
            .label("Progress (%)")
            .default_val(Value::Number(Number::from(0)))
            .build(),
        FieldB::new("lead", EntityFieldType::ObjectRef)
            .label("Project Lead")
            .ref_types(&[flux_types::CONTACT])
            .build(),
    ]
}

fn goal_fields() -> Vec<EntityFieldDef> {
    vec![
        FieldB::new("targetDate", EntityFieldType::Date)
            .label("Target Date")
            .build(),
        FieldB::new("progress", EntityFieldType::Float)
            .label("Progress (%)")
            .default_val(Value::Number(Number::from(0)))
            .build(),
        FieldB::new("metric", EntityFieldType::String)
            .label("Key Metric")
            .build(),
        FieldB::new("targetValue", EntityFieldType::Float)
            .label("Target Value")
            .build(),
        FieldB::new("currentValue", EntityFieldType::Float)
            .label("Current Value")
            .default_val(Value::Number(Number::from(0)))
            .build(),
        FieldB::new("progressFormula", EntityFieldType::String)
            .label("Progress Formula")
            .expr("currentValue / targetValue * 100")
            .build(),
    ]
}

fn milestone_fields() -> Vec<EntityFieldDef> {
    vec![
        FieldB::new("dueDate", EntityFieldType::Date)
            .label("Due Date")
            .build(),
        FieldB::new("completed", EntityFieldType::Bool)
            .label("Completed")
            .default_val(Value::Bool(false))
            .build(),
    ]
}

fn contact_fields() -> Vec<EntityFieldDef> {
    vec![
        FieldB::new("contactType", EntityFieldType::Enum)
            .label("Contact Type")
            .enum_vals(enum_options_from(CONTACT_TYPES))
            .default_val(Value::String("person".into()))
            .build(),
        FieldB::new("email", EntityFieldType::String)
            .label("Email")
            .ui(ui_placeholder("name@example.com"))
            .build(),
        FieldB::new("phone", EntityFieldType::String)
            .label("Phone")
            .build(),
        FieldB::new("company", EntityFieldType::ObjectRef)
            .label("Organization")
            .ref_types(&[flux_types::ORGANIZATION])
            .build(),
        FieldB::new("role", EntityFieldType::String)
            .label("Role / Title")
            .build(),
        FieldB::new("address", EntityFieldType::Text)
            .label("Address")
            .ui(ui_multiline())
            .build(),
        FieldB::new("website", EntityFieldType::Url)
            .label("Website")
            .build(),
        FieldB::new("notes", EntityFieldType::Text)
            .label("Notes")
            .ui(ui_multiline_group("Details"))
            .build(),
        FieldB::new("lastContactDate", EntityFieldType::Date)
            .label("Last Contact")
            .build(),
        FieldB::new("dealValue", EntityFieldType::Float)
            .label("Deal Value")
            .ui(ui_group("CRM"))
            .build(),
        FieldB::new("dealStage", EntityFieldType::Enum)
            .label("Deal Stage")
            .enum_vals(enum_options_lit(&[
                ("prospect", "Prospect"),
                ("qualified", "Qualified"),
                ("proposal", "Proposal"),
                ("negotiation", "Negotiation"),
                ("closed_won", "Closed Won"),
                ("closed_lost", "Closed Lost"),
            ]))
            .ui(ui_group("CRM"))
            .build(),
    ]
}

fn organization_fields() -> Vec<EntityFieldDef> {
    vec![
        FieldB::new("industry", EntityFieldType::String)
            .label("Industry")
            .build(),
        FieldB::new("website", EntityFieldType::Url)
            .label("Website")
            .build(),
        FieldB::new("address", EntityFieldType::Text)
            .label("Address")
            .ui(ui_multiline())
            .build(),
        FieldB::new("employeeCount", EntityFieldType::Int)
            .label("Employees")
            .build(),
        FieldB::new("annualRevenue", EntityFieldType::Float)
            .label("Annual Revenue")
            .build(),
    ]
}

fn transaction_fields() -> Vec<EntityFieldDef> {
    vec![
        FieldB::new("txnType", EntityFieldType::Enum)
            .label("Type")
            .enum_vals(enum_options_from(TRANSACTION_TYPES))
            .required()
            .build(),
        FieldB::new("amount", EntityFieldType::Float)
            .label("Amount")
            .required()
            .build(),
        FieldB::new("currency", EntityFieldType::Enum)
            .label("Currency")
            .enum_vals(enum_options_lit(&[
                ("USD", "USD"),
                ("EUR", "EUR"),
                ("GBP", "GBP"),
                ("JPY", "JPY"),
                ("CAD", "CAD"),
                ("AUD", "AUD"),
            ]))
            .default_val(Value::String("USD".into()))
            .build(),
        FieldB::new("txnDate", EntityFieldType::Date)
            .label("Date")
            .required()
            .build(),
        FieldB::new("account", EntityFieldType::ObjectRef)
            .label("Account")
            .ref_types(&[flux_types::ACCOUNT])
            .build(),
        FieldB::new("category", EntityFieldType::String)
            .label("Category")
            .build(),
        FieldB::new("payee", EntityFieldType::String)
            .label("Payee")
            .build(),
        FieldB::new("reference", EntityFieldType::String)
            .label("Reference #")
            .build(),
        FieldB::new("notes", EntityFieldType::Text)
            .label("Notes")
            .ui(ui_multiline())
            .build(),
        FieldB::new("reconciled", EntityFieldType::Bool)
            .label("Reconciled")
            .default_val(Value::Bool(false))
            .build(),
    ]
}

fn account_fields() -> Vec<EntityFieldDef> {
    vec![
        FieldB::new("accountType", EntityFieldType::Enum)
            .label("Account Type")
            .enum_vals(enum_options_lit(&[
                ("checking", "Checking"),
                ("savings", "Savings"),
                ("credit", "Credit Card"),
                ("cash", "Cash"),
                ("investment", "Investment"),
                ("loan", "Loan"),
            ]))
            .build(),
        FieldB::new("balance", EntityFieldType::Float)
            .label("Balance")
            .default_val(Value::Number(Number::from(0)))
            .build(),
        FieldB::new("currency", EntityFieldType::Enum)
            .label("Currency")
            .enum_vals(enum_options_lit(&[
                ("USD", "USD"),
                ("EUR", "EUR"),
                ("GBP", "GBP"),
            ]))
            .default_val(Value::String("USD".into()))
            .build(),
        FieldB::new("institution", EntityFieldType::String)
            .label("Institution")
            .build(),
        FieldB::new("accountNumber", EntityFieldType::String)
            .label("Account Number")
            .ui(ui_hidden())
            .build(),
    ]
}

fn invoice_fields() -> Vec<EntityFieldDef> {
    vec![
        FieldB::new("invoiceNumber", EntityFieldType::String)
            .label("Invoice #")
            .required()
            .build(),
        FieldB::new("issueDate", EntityFieldType::Date)
            .label("Issue Date")
            .required()
            .build(),
        FieldB::new("dueDate", EntityFieldType::Date)
            .label("Due Date")
            .required()
            .build(),
        FieldB::new("subtotal", EntityFieldType::Float)
            .label("Subtotal")
            .build(),
        FieldB::new("taxRate", EntityFieldType::Float)
            .label("Tax Rate (%)")
            .default_val(Value::Number(Number::from(0)))
            .build(),
        FieldB::new("taxAmount", EntityFieldType::Float)
            .label("Tax Amount")
            .expr("subtotal * taxRate / 100")
            .build(),
        FieldB::new("total", EntityFieldType::Float)
            .label("Total")
            .expr("subtotal + subtotal * taxRate / 100")
            .build(),
        FieldB::new("currency", EntityFieldType::Enum)
            .label("Currency")
            .enum_vals(enum_options_lit(&[
                ("USD", "USD"),
                ("EUR", "EUR"),
                ("GBP", "GBP"),
            ]))
            .default_val(Value::String("USD".into()))
            .build(),
        FieldB::new("notes", EntityFieldType::Text)
            .label("Notes")
            .ui(ui_multiline())
            .build(),
    ]
}

fn item_fields() -> Vec<EntityFieldDef> {
    vec![
        FieldB::new("sku", EntityFieldType::String)
            .label("SKU")
            .build(),
        FieldB::new("quantity", EntityFieldType::Int)
            .label("Quantity")
            .default_val(Value::Number(Number::from(0)))
            .build(),
        FieldB::new("unit", EntityFieldType::Enum)
            .label("Unit")
            .enum_vals(enum_options_lit(&[
                ("each", "Each"),
                ("kg", "Kilogram"),
                ("lb", "Pound"),
                ("l", "Liter"),
                ("m", "Meter"),
                ("box", "Box"),
            ]))
            .default_val(Value::String("each".into()))
            .build(),
        FieldB::new("costPrice", EntityFieldType::Float)
            .label("Cost Price")
            .build(),
        FieldB::new("sellPrice", EntityFieldType::Float)
            .label("Sell Price")
            .build(),
        FieldB::new("reorderLevel", EntityFieldType::Int)
            .label("Reorder Level")
            .build(),
        FieldB::new("supplier", EntityFieldType::ObjectRef)
            .label("Supplier")
            .ref_types(&[flux_types::CONTACT, flux_types::ORGANIZATION])
            .build(),
        FieldB::new("barcode", EntityFieldType::String)
            .label("Barcode")
            .build(),
        FieldB::new("stockValue", EntityFieldType::Float)
            .label("Stock Value")
            .expr("quantity * costPrice")
            .ui(ui_readonly())
            .build(),
    ]
}

fn location_fields() -> Vec<EntityFieldDef> {
    vec![
        FieldB::new("address", EntityFieldType::Text)
            .label("Address")
            .ui(ui_multiline())
            .build(),
        FieldB::new("locationType", EntityFieldType::Enum)
            .label("Location Type")
            .enum_vals(enum_options_lit(&[
                ("warehouse", "Warehouse"),
                ("store", "Store"),
                ("office", "Office"),
                ("virtual", "Virtual"),
            ]))
            .build(),
        FieldB::new("capacity", EntityFieldType::Int)
            .label("Capacity")
            .build(),
    ]
}

// ── Entity Definitions ─────────────────────────────────────────────

fn entity_def(
    type_name: &str,
    nsid: &str,
    category: &str,
    label: &str,
    plural_label: &str,
    default_child_view: Option<DefaultChildView>,
    fields: Vec<EntityFieldDef>,
) -> EntityDef {
    EntityDef {
        type_name: type_name.into(),
        nsid: Some(nsid.into()),
        category: category.into(),
        label: label.into(),
        plural_label: Some(plural_label.into()),
        description: None,
        color: None,
        default_child_view,
        tabs: None,
        child_only: None,
        extra_child_types: None,
        extra_parent_types: None,
        fields: Some(fields),
        api: None,
    }
}

fn build_entity_defs() -> Vec<EntityDef> {
    let mut defs = Vec::new();

    // ── Productivity ──
    defs.push(entity_def(
        flux_types::TASK,
        "io.prismapp.flux.task",
        flux_categories::PRODUCTIVITY,
        "Task",
        "Tasks",
        Some(DefaultChildView::List),
        task_fields(),
    ));

    let mut project = entity_def(
        flux_types::PROJECT,
        "io.prismapp.flux.project",
        flux_categories::PRODUCTIVITY,
        "Project",
        "Projects",
        Some(DefaultChildView::Kanban),
        project_fields(),
    );
    project.extra_child_types = Some(vec![flux_types::TASK.into(), flux_types::MILESTONE.into()]);
    defs.push(project);

    let mut goal = entity_def(
        flux_types::GOAL,
        "io.prismapp.flux.goal",
        flux_categories::PRODUCTIVITY,
        "Goal",
        "Goals",
        Some(DefaultChildView::Timeline),
        goal_fields(),
    );
    goal.extra_child_types = Some(vec![flux_types::MILESTONE.into()]);
    defs.push(goal);

    let mut milestone = entity_def(
        flux_types::MILESTONE,
        "io.prismapp.flux.milestone",
        flux_categories::PRODUCTIVITY,
        "Milestone",
        "Milestones",
        None,
        milestone_fields(),
    );
    milestone.child_only = Some(true);
    defs.push(milestone);

    // ── People / CRM ──
    defs.push(entity_def(
        flux_types::CONTACT,
        "io.prismapp.flux.contact",
        flux_categories::PEOPLE,
        "Contact",
        "Contacts",
        Some(DefaultChildView::List),
        contact_fields(),
    ));

    let mut org = entity_def(
        flux_types::ORGANIZATION,
        "io.prismapp.flux.organization",
        flux_categories::PEOPLE,
        "Organization",
        "Organizations",
        Some(DefaultChildView::List),
        organization_fields(),
    );
    org.extra_child_types = Some(vec![flux_types::CONTACT.into()]);
    defs.push(org);

    // ── Finance ──
    let mut txn = entity_def(
        flux_types::TRANSACTION,
        "io.prismapp.flux.transaction",
        flux_categories::FINANCE,
        "Transaction",
        "Transactions",
        Some(DefaultChildView::List),
        transaction_fields(),
    );
    txn.child_only = Some(true);
    defs.push(txn);

    let mut account = entity_def(
        flux_types::ACCOUNT,
        "io.prismapp.flux.account",
        flux_categories::FINANCE,
        "Account",
        "Accounts",
        Some(DefaultChildView::List),
        account_fields(),
    );
    account.extra_child_types = Some(vec![flux_types::TRANSACTION.into()]);
    defs.push(account);

    defs.push(entity_def(
        flux_types::INVOICE,
        "io.prismapp.flux.invoice",
        flux_categories::FINANCE,
        "Invoice",
        "Invoices",
        Some(DefaultChildView::List),
        invoice_fields(),
    ));

    // ── Inventory ──
    defs.push(entity_def(
        flux_types::ITEM,
        "io.prismapp.flux.item",
        flux_categories::INVENTORY,
        "Item",
        "Items",
        Some(DefaultChildView::Grid),
        item_fields(),
    ));

    let mut location = entity_def(
        flux_types::LOCATION,
        "io.prismapp.flux.location",
        flux_categories::INVENTORY,
        "Location",
        "Locations",
        Some(DefaultChildView::List),
        location_fields(),
    );
    location.extra_child_types = Some(vec![flux_types::ITEM.into()]);
    defs.push(location);

    defs
}

// ── Edge Definitions ───────────────────────────────────────────────

fn edge_def(relation: &str, nsid: &str, label: &str, behavior: EdgeBehavior) -> EdgeTypeDef {
    EdgeTypeDef {
        relation: relation.into(),
        nsid: Some(nsid.into()),
        label: label.into(),
        description: None,
        behavior: Some(behavior),
        undirected: None,
        allow_multiple: None,
        cascade: None,
        suggest_inline: None,
        color: None,
        source_types: None,
        source_categories: None,
        target_types: None,
        target_categories: None,
        scope: None,
    }
}

fn build_edge_defs() -> Vec<EdgeTypeDef> {
    let mut defs = Vec::new();

    let mut assigned = edge_def(
        flux_edges::ASSIGNED_TO,
        "io.prismapp.flux.assigned-to",
        "Assigned To",
        EdgeBehavior::Assignment,
    );
    assigned.source_types = Some(vec![
        flux_types::TASK.into(),
        flux_types::PROJECT.into(),
        flux_types::INVOICE.into(),
    ]);
    assigned.target_types = Some(vec![flux_types::CONTACT.into()]);
    assigned.suggest_inline = Some(true);
    defs.push(assigned);

    let mut depends = edge_def(
        flux_edges::DEPENDS_ON,
        "io.prismapp.flux.depends-on",
        "Depends On",
        EdgeBehavior::Dependency,
    );
    depends.source_categories = Some(vec![flux_categories::PRODUCTIVITY.into()]);
    depends.target_categories = Some(vec![flux_categories::PRODUCTIVITY.into()]);
    defs.push(depends);

    let mut blocks = edge_def(
        flux_edges::BLOCKS,
        "io.prismapp.flux.blocks",
        "Blocks",
        EdgeBehavior::Dependency,
    );
    blocks.source_categories = Some(vec![flux_categories::PRODUCTIVITY.into()]);
    blocks.target_categories = Some(vec![flux_categories::PRODUCTIVITY.into()]);
    defs.push(blocks);

    let mut belongs = edge_def(
        flux_edges::BELONGS_TO,
        "io.prismapp.flux.belongs-to",
        "Belongs To",
        EdgeBehavior::Membership,
    );
    belongs.source_types = Some(vec![flux_types::TASK.into()]);
    belongs.target_types = Some(vec![flux_types::PROJECT.into(), flux_types::GOAL.into()]);
    defs.push(belongs);

    let mut related = edge_def(
        flux_edges::RELATED_TO,
        "io.prismapp.flux.related-to",
        "Related To",
        EdgeBehavior::Weak,
    );
    related.undirected = Some(true);
    related.suggest_inline = Some(true);
    defs.push(related);

    let mut invoiced = edge_def(
        flux_edges::INVOICED_TO,
        "io.prismapp.flux.invoiced-to",
        "Invoiced To",
        EdgeBehavior::Assignment,
    );
    invoiced.source_types = Some(vec![flux_types::INVOICE.into()]);
    invoiced.target_types = Some(vec![
        flux_types::CONTACT.into(),
        flux_types::ORGANIZATION.into(),
    ]);
    defs.push(invoiced);

    let mut stored = edge_def(
        flux_edges::STORED_AT,
        "io.prismapp.flux.stored-at",
        "Stored At",
        EdgeBehavior::Membership,
    );
    stored.source_types = Some(vec![flux_types::ITEM.into()]);
    stored.target_types = Some(vec![flux_types::LOCATION.into()]);
    defs.push(stored);

    defs
}

// ── Automation Presets ─────────────────────────────────────────────

fn preset(
    id: &str,
    name: &str,
    entity_type: &str,
    trigger: FluxTriggerKind,
    condition: Option<&str>,
    actions: Vec<FluxAutomationAction>,
) -> FluxAutomationPreset {
    FluxAutomationPreset {
        id: id.into(),
        name: name.into(),
        entity_type: entity_type.into(),
        trigger,
        condition: condition.map(|s| s.into()),
        actions,
    }
}

fn action(kind: FluxAutomationActionKind, target: &str, value: &str) -> FluxAutomationAction {
    FluxAutomationAction {
        kind,
        target: target.into(),
        value: value.into(),
    }
}

fn build_automation_presets() -> Vec<FluxAutomationPreset> {
    vec![
        preset(
            "flux:auto:task-complete-timestamp",
            "Auto-fill completion timestamp",
            flux_types::TASK,
            FluxTriggerKind::OnStatusChange,
            Some("status == 'done'"),
            vec![action(
                FluxAutomationActionKind::SetField,
                "completedAt",
                "{{now}}",
            )],
        ),
        preset(
            "flux:auto:task-recurring-reset",
            "Reset recurring task",
            flux_types::TASK,
            FluxTriggerKind::OnStatusChange,
            Some("status == 'done' and recurring != 'none'"),
            vec![
                action(FluxAutomationActionKind::MoveToStatus, "status", "todo"),
                action(FluxAutomationActionKind::SetField, "completedAt", ""),
            ],
        ),
        preset(
            "flux:auto:task-overdue-notify",
            "Notify on overdue task",
            flux_types::TASK,
            FluxTriggerKind::OnDueDate,
            Some("status != 'done' and status != 'cancelled'"),
            vec![action(
                FluxAutomationActionKind::SendNotification,
                "assignee",
                "Task '{{name}}' is overdue",
            )],
        ),
        preset(
            "flux:auto:invoice-overdue",
            "Mark invoice overdue",
            flux_types::INVOICE,
            FluxTriggerKind::OnDueDate,
            Some("status == 'sent'"),
            vec![
                action(FluxAutomationActionKind::MoveToStatus, "status", "overdue"),
                action(
                    FluxAutomationActionKind::SendNotification,
                    "owner",
                    "Invoice {{invoiceNumber}} is overdue",
                ),
            ],
        ),
        preset(
            "flux:auto:item-low-stock",
            "Low stock alert",
            flux_types::ITEM,
            FluxTriggerKind::OnUpdate,
            Some("quantity <= reorderLevel and quantity > 0"),
            vec![
                action(
                    FluxAutomationActionKind::MoveToStatus,
                    "status",
                    "low_stock",
                ),
                action(
                    FluxAutomationActionKind::SendNotification,
                    "owner",
                    "Item '{{name}}' is low on stock ({{quantity}} remaining)",
                ),
            ],
        ),
        preset(
            "flux:auto:item-out-of-stock",
            "Out of stock alert",
            flux_types::ITEM,
            FluxTriggerKind::OnUpdate,
            Some("quantity == 0"),
            vec![action(
                FluxAutomationActionKind::MoveToStatus,
                "status",
                "out_of_stock",
            )],
        ),
        preset(
            "flux:auto:goal-progress",
            "Auto-update goal progress",
            flux_types::GOAL,
            FluxTriggerKind::OnUpdate,
            Some("targetValue > 0"),
            vec![action(
                FluxAutomationActionKind::SetField,
                "progress",
                "{{currentValue / targetValue * 100}}",
            )],
        ),
        preset(
            "flux:auto:project-complete",
            "Complete project when all tasks done",
            flux_types::PROJECT,
            FluxTriggerKind::OnUpdate,
            Some("progress >= 100"),
            vec![action(
                FluxAutomationActionKind::MoveToStatus,
                "status",
                "completed",
            )],
        ),
    ]
}

// ── Import/Export ──────────────────────────────────────────────────

#[derive(Debug, thiserror::Error)]
pub enum FluxImportError {
    #[error("invalid JSON: {0}")]
    InvalidJson(String),
    #[error("expected JSON array")]
    ExpectedArray,
}

fn value_to_csv_cell(value: Option<&Value>) -> String {
    match value {
        None | Some(Value::Null) => String::new(),
        Some(Value::String(s)) => s.clone(),
        Some(Value::Bool(b)) => b.to_string(),
        Some(Value::Number(n)) => n.to_string(),
        Some(other) => other.to_string(),
    }
}

fn escape_csv(value: &str) -> String {
    if value.contains(',') || value.contains('"') || value.contains('\n') {
        let mut out = String::with_capacity(value.len() + 2);
        out.push('"');
        for ch in value.chars() {
            if ch == '"' {
                out.push('"');
                out.push('"');
            } else {
                out.push(ch);
            }
        }
        out.push('"');
        out
    } else {
        value.to_string()
    }
}

fn export_to_csv(objects: &[ObjectMap], fields: Option<&[String]>) -> String {
    if objects.is_empty() {
        return String::new();
    }
    let first = &objects[0];
    let keys: Vec<String> = match fields {
        Some(fs) => fs.to_vec(),
        None => first.keys().cloned().collect(),
    };

    let header = keys
        .iter()
        .map(|k| escape_csv(k))
        .collect::<Vec<_>>()
        .join(",");

    let rows = objects.iter().map(|obj| {
        keys.iter()
            .map(|k| {
                let raw = value_to_csv_cell(obj.get(k));
                escape_csv(&raw)
            })
            .collect::<Vec<_>>()
            .join(",")
    });

    let mut lines = vec![header];
    lines.extend(rows);
    lines.join("\n")
}

fn export_to_json(objects: &[ObjectMap]) -> String {
    serde_json::to_string_pretty(objects).unwrap_or_else(|_| "[]".to_string())
}

fn strip_quotes(s: &str) -> &str {
    let bytes = s.as_bytes();
    if bytes.len() >= 2 && bytes[0] == b'"' && bytes[bytes.len() - 1] == b'"' {
        &s[1..s.len() - 1]
    } else if bytes.first() == Some(&b'"') {
        &s[1..]
    } else if bytes.last() == Some(&b'"') {
        &s[..s.len() - 1]
    } else {
        s
    }
}

fn parse_csv(data: &str) -> Vec<ObjectMap> {
    let lines: Vec<&str> = data.lines().filter(|l| !l.trim().is_empty()).collect();
    if lines.len() < 2 {
        return Vec::new();
    }

    let headers: Vec<String> = lines[0]
        .split(',')
        .map(|h| strip_quotes(h.trim()).to_string())
        .collect();

    let mut result = Vec::new();
    for line in &lines[1..] {
        let values: Vec<String> = line
            .split(',')
            .map(|v| strip_quotes(v.trim()).to_string())
            .collect();
        let mut obj = ObjectMap::new();
        for (j, key) in headers.iter().enumerate() {
            if key.is_empty() {
                continue;
            }
            let val = values.get(j).cloned().unwrap_or_default();
            // Try to parse numbers — TS used `Number(val)` + isNaN.
            let typed: Value = if val.is_empty() {
                Value::String(String::new())
            } else if let Ok(n) = val.parse::<i64>() {
                Value::Number(Number::from(n))
            } else if let Ok(f) = val.parse::<f64>() {
                Number::from_f64(f)
                    .map(Value::Number)
                    .unwrap_or_else(|| Value::String(val.clone()))
            } else {
                Value::String(val.clone())
            };
            obj.insert(key.clone(), typed);
        }
        result.push(obj);
    }
    result
}

fn parse_json(data: &str) -> Result<Vec<ObjectMap>, FluxImportError> {
    let parsed: Value =
        serde_json::from_str(data).map_err(|e| FluxImportError::InvalidJson(e.to_string()))?;
    let arr = match parsed {
        Value::Array(a) => a,
        _ => return Err(FluxImportError::ExpectedArray),
    };
    let mut out = Vec::with_capacity(arr.len());
    for v in arr {
        match v {
            Value::Object(m) => {
                let mut obj = ObjectMap::new();
                for (k, val) in m {
                    obj.insert(k, val);
                }
                out.push(obj);
            }
            _ => {
                // Preserve TS shape: non-object array members get
                // wrapped into a one-key map under "value".
                let mut obj = ObjectMap::new();
                obj.insert("value".into(), v);
                out.push(obj);
            }
        }
    }
    Ok(out)
}

// ── Registry ───────────────────────────────────────────────────────

/// Registry of Flux entity defs, edge defs, automation presets, and
/// import/export helpers. Concrete struct counterpart of the TS
/// `FluxRegistry` interface.
#[derive(Debug, Clone)]
pub struct FluxRegistry {
    entity_defs: Vec<EntityDef>,
    edge_defs: Vec<EdgeTypeDef>,
    presets: Vec<FluxAutomationPreset>,
}

impl FluxRegistry {
    pub fn get_entity_defs(&self) -> &[EntityDef] {
        &self.entity_defs
    }

    pub fn get_edge_defs(&self) -> &[EdgeTypeDef] {
        &self.edge_defs
    }

    pub fn get_entity_def(&self, type_name: &str) -> Option<&EntityDef> {
        self.entity_defs.iter().find(|d| d.type_name == type_name)
    }

    pub fn get_edge_def(&self, relation: &str) -> Option<&EdgeTypeDef> {
        self.edge_defs.iter().find(|d| d.relation == relation)
    }

    pub fn get_automation_presets(&self) -> &[FluxAutomationPreset] {
        &self.presets
    }

    pub fn get_presets_for_entity(&self, type_name: &str) -> Vec<&FluxAutomationPreset> {
        self.presets
            .iter()
            .filter(|p| p.entity_type == type_name)
            .collect()
    }

    pub fn export_data(&self, objects: &[ObjectMap], options: &FluxExportOptions) -> String {
        match options.format {
            FluxExportFormat::Csv => export_to_csv(objects, options.fields.as_deref()),
            FluxExportFormat::Json => export_to_json(objects),
        }
    }

    pub fn parse_import(
        &self,
        data: &str,
        format: FluxExportFormat,
    ) -> Result<Vec<ObjectMap>, FluxImportError> {
        match format {
            FluxExportFormat::Csv => Ok(parse_csv(data)),
            FluxExportFormat::Json => parse_json(data),
        }
    }
}

/// Build a fresh [`FluxRegistry`] populated with the eleven built-in
/// entity types, seven edge types, and eight automation presets.
pub fn create_flux_registry() -> FluxRegistry {
    FluxRegistry {
        entity_defs: build_entity_defs(),
        edge_defs: build_edge_defs(),
        presets: build_automation_presets(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn registry() -> FluxRegistry {
        create_flux_registry()
    }

    // ── Entity Definitions ──

    #[test]
    fn registers_eleven_entity_types() {
        assert_eq!(registry().get_entity_defs().len(), 11);
    }

    #[test]
    fn covers_all_flux_types() {
        let r = registry();
        let types: Vec<&str> = r
            .get_entity_defs()
            .iter()
            .map(|d| d.type_name.as_str())
            .collect();
        for t in [
            flux_types::TASK,
            flux_types::PROJECT,
            flux_types::GOAL,
            flux_types::MILESTONE,
            flux_types::CONTACT,
            flux_types::ORGANIZATION,
            flux_types::TRANSACTION,
            flux_types::ACCOUNT,
            flux_types::INVOICE,
            flux_types::ITEM,
            flux_types::LOCATION,
        ] {
            assert!(types.contains(&t), "missing type: {t}");
        }
    }

    #[test]
    fn assigns_nsids_to_all_entities() {
        for d in registry().get_entity_defs() {
            let nsid = d.nsid.as_deref().expect("nsid");
            assert!(nsid.starts_with("io.prismapp.flux."));
        }
    }

    #[test]
    fn assigns_categories_to_all_entities() {
        let r = registry();
        let cats: std::collections::HashSet<&str> = r
            .get_entity_defs()
            .iter()
            .map(|d| d.category.as_str())
            .collect();
        assert_eq!(cats.len(), 4);
        for c in [
            flux_categories::PRODUCTIVITY,
            flux_categories::PEOPLE,
            flux_categories::FINANCE,
            flux_categories::INVENTORY,
        ] {
            assert!(cats.contains(c));
        }
    }

    #[test]
    fn retrieves_task_entity_by_type() {
        let r = registry();
        let task = r.get_entity_def(flux_types::TASK).unwrap();
        assert_eq!(task.label, "Task");
        assert_eq!(task.category, flux_categories::PRODUCTIVITY);
    }

    #[test]
    fn task_has_required_fields() {
        let r = registry();
        let task = r.get_entity_def(flux_types::TASK).unwrap();
        let ids: Vec<&str> = task
            .fields
            .as_ref()
            .unwrap()
            .iter()
            .map(|f| f.id.as_str())
            .collect();
        for f in [
            "priority",
            "dueDate",
            "effort",
            "recurring",
            "estimateHours",
        ] {
            assert!(ids.contains(&f), "missing field {f}");
        }
    }

    #[test]
    fn contact_has_crm_fields() {
        let r = registry();
        let c = r.get_entity_def(flux_types::CONTACT).unwrap();
        let ids: Vec<&str> = c
            .fields
            .as_ref()
            .unwrap()
            .iter()
            .map(|f| f.id.as_str())
            .collect();
        for f in ["email", "phone", "dealValue", "dealStage"] {
            assert!(ids.contains(&f));
        }
    }

    #[test]
    fn transaction_amount_is_required_float() {
        let r = registry();
        let t = r.get_entity_def(flux_types::TRANSACTION).unwrap();
        let amount = t
            .fields
            .as_ref()
            .unwrap()
            .iter()
            .find(|f| f.id == "amount")
            .unwrap();
        assert_eq!(amount.required, Some(true));
        assert_eq!(amount.field_type, EntityFieldType::Float);
    }

    #[test]
    fn invoice_has_computed_fields() {
        let r = registry();
        let inv = r.get_entity_def(flux_types::INVOICE).unwrap();
        let fields = inv.fields.as_ref().unwrap();
        let tax = fields.iter().find(|f| f.id == "taxAmount").unwrap();
        let total = fields.iter().find(|f| f.id == "total").unwrap();
        assert!(tax.expression.is_some());
        assert!(total.expression.is_some());
    }

    #[test]
    fn item_stock_value_formula() {
        let r = registry();
        let item = r.get_entity_def(flux_types::ITEM).unwrap();
        let stock = item
            .fields
            .as_ref()
            .unwrap()
            .iter()
            .find(|f| f.id == "stockValue")
            .unwrap();
        assert_eq!(stock.expression.as_deref(), Some("quantity * costPrice"));
    }

    #[test]
    fn milestone_is_child_only() {
        let r = registry();
        let ms = r.get_entity_def(flux_types::MILESTONE).unwrap();
        assert_eq!(ms.child_only, Some(true));
    }

    #[test]
    fn project_has_extra_child_types() {
        let r = registry();
        let p = r.get_entity_def(flux_types::PROJECT).unwrap();
        let ec = p.extra_child_types.as_ref().unwrap();
        assert!(ec.iter().any(|s| s == flux_types::TASK));
        assert!(ec.iter().any(|s| s == flux_types::MILESTONE));
    }

    #[test]
    fn returns_none_for_unknown_type() {
        let r = registry();
        assert!(r.get_entity_def("nonsense").is_none());
    }

    // ── Edge Definitions ──

    #[test]
    fn registers_seven_edge_types() {
        assert_eq!(registry().get_edge_defs().len(), 7);
    }

    #[test]
    fn covers_all_flux_edges() {
        let r = registry();
        let rels: Vec<&str> = r
            .get_edge_defs()
            .iter()
            .map(|e| e.relation.as_str())
            .collect();
        for e in [
            flux_edges::ASSIGNED_TO,
            flux_edges::DEPENDS_ON,
            flux_edges::BLOCKS,
            flux_edges::BELONGS_TO,
            flux_edges::RELATED_TO,
            flux_edges::INVOICED_TO,
            flux_edges::STORED_AT,
        ] {
            assert!(rels.contains(&e));
        }
    }

    #[test]
    fn assigned_to_edge_has_correct_constraints() {
        let r = registry();
        let e = r.get_edge_def(flux_edges::ASSIGNED_TO).unwrap();
        assert_eq!(e.behavior, Some(EdgeBehavior::Assignment));
        assert!(e
            .target_types
            .as_ref()
            .unwrap()
            .iter()
            .any(|t| t == flux_types::CONTACT));
        assert!(e
            .source_types
            .as_ref()
            .unwrap()
            .iter()
            .any(|t| t == flux_types::TASK));
    }

    #[test]
    fn depends_on_is_dependency_edge() {
        let r = registry();
        let e = r.get_edge_def(flux_edges::DEPENDS_ON).unwrap();
        assert_eq!(e.behavior, Some(EdgeBehavior::Dependency));
        assert!(e
            .source_categories
            .as_ref()
            .unwrap()
            .iter()
            .any(|c| c == flux_categories::PRODUCTIVITY));
    }

    #[test]
    fn related_to_is_undirected() {
        let r = registry();
        let e = r.get_edge_def(flux_edges::RELATED_TO).unwrap();
        assert_eq!(e.undirected, Some(true));
        assert_eq!(e.behavior, Some(EdgeBehavior::Weak));
    }

    #[test]
    fn stored_at_links_items_to_locations() {
        let r = registry();
        let e = r.get_edge_def(flux_edges::STORED_AT).unwrap();
        assert!(e
            .source_types
            .as_ref()
            .unwrap()
            .iter()
            .any(|t| t == flux_types::ITEM));
        assert!(e
            .target_types
            .as_ref()
            .unwrap()
            .iter()
            .any(|t| t == flux_types::LOCATION));
    }

    #[test]
    fn unknown_relation_returns_none() {
        assert!(registry().get_edge_def("nope").is_none());
    }

    // ── Automation Presets ──

    #[test]
    fn has_eight_built_in_presets() {
        assert_eq!(registry().get_automation_presets().len(), 8);
    }

    #[test]
    fn filters_presets_by_entity_type() {
        let r = registry();
        let task = r.get_presets_for_entity(flux_types::TASK);
        assert!(task.len() >= 2);
        for p in &task {
            assert_eq!(p.entity_type, flux_types::TASK);
        }
    }

    #[test]
    fn task_completion_preset_sets_timestamp() {
        let r = registry();
        let task = r.get_presets_for_entity(flux_types::TASK);
        let complete = task
            .iter()
            .find(|p| p.id == "flux:auto:task-complete-timestamp")
            .unwrap();
        assert_eq!(complete.trigger, FluxTriggerKind::OnStatusChange);
        assert!(complete.condition.as_deref().unwrap().contains("done"));
        assert_eq!(complete.actions[0].kind, FluxAutomationActionKind::SetField);
    }

    #[test]
    fn invoice_overdue_preset_changes_status() {
        let r = registry();
        let inv = r.get_presets_for_entity(flux_types::INVOICE);
        let overdue = inv
            .iter()
            .find(|p| p.id == "flux:auto:invoice-overdue")
            .unwrap();
        assert_eq!(overdue.trigger, FluxTriggerKind::OnDueDate);
        assert_eq!(
            overdue.actions[0].kind,
            FluxAutomationActionKind::MoveToStatus
        );
    }

    #[test]
    fn item_low_stock_preset_sends_notification() {
        let r = registry();
        let item = r.get_presets_for_entity(flux_types::ITEM);
        let low = item
            .iter()
            .find(|p| p.id == "flux:auto:item-low-stock")
            .unwrap();
        assert_eq!(low.actions.len(), 2);
    }

    #[test]
    fn returns_empty_for_entity_without_presets() {
        let r = registry();
        let presets = r.get_presets_for_entity(flux_types::LOCATION);
        assert!(presets.is_empty());
    }

    // ── Import/Export ──

    fn obj_map(pairs: &[(&str, Value)]) -> ObjectMap {
        let mut m = ObjectMap::new();
        for (k, v) in pairs {
            m.insert((*k).to_string(), v.clone());
        }
        m
    }

    #[test]
    fn exports_objects_to_csv() {
        let r = registry();
        let data = vec![
            obj_map(&[
                ("name", Value::String("Task 1".into())),
                ("priority", Value::String("high".into())),
                ("amount", Value::Number(Number::from(100))),
            ]),
            obj_map(&[
                ("name", Value::String("Task 2".into())),
                ("priority", Value::String("low".into())),
                ("amount", Value::Number(Number::from(200))),
            ]),
        ];
        let csv = r.export_data(
            &data,
            &FluxExportOptions {
                entity_type: flux_types::TASK.into(),
                format: FluxExportFormat::Csv,
                fields: None,
                include_edges: None,
            },
        );
        // Map iteration order in TS used insertion; serde_json::Map
        // preserves insertion order — the BTreeMap-like alphabetical
        // ordering only happens with the `preserve_order` feature
        // disabled; we rely on the default (insertion) order.
        assert!(csv.contains("name,priority,amount"));
        assert!(csv.contains("Task 1,high,100"));
        assert!(csv.contains("Task 2,low,200"));
    }

    #[test]
    fn exports_with_specific_fields() {
        let r = registry();
        let data = vec![obj_map(&[
            ("name", Value::String("Task 1".into())),
            ("priority", Value::String("high".into())),
            ("amount", Value::Number(Number::from(100))),
        ])];
        let csv = r.export_data(
            &data,
            &FluxExportOptions {
                entity_type: flux_types::TASK.into(),
                format: FluxExportFormat::Csv,
                fields: Some(vec!["name".into(), "priority".into()]),
                include_edges: None,
            },
        );
        assert!(csv.contains("name,priority"));
        assert!(!csv.contains("amount"));
    }

    #[test]
    fn escapes_csv_values_with_commas() {
        let r = registry();
        let data = vec![obj_map(&[
            ("name", Value::String("Task, with comma".into())),
            ("value", Value::Number(Number::from(42))),
        ])];
        let csv = r.export_data(
            &data,
            &FluxExportOptions {
                entity_type: flux_types::TASK.into(),
                format: FluxExportFormat::Csv,
                fields: None,
                include_edges: None,
            },
        );
        assert!(csv.contains("\"Task, with comma\""));
    }

    #[test]
    fn parses_csv_back_to_objects() {
        let r = registry();
        let csv = "name,priority,amount\nTask 1,high,100\nTask 2,low,200";
        let parsed = r.parse_import(csv, FluxExportFormat::Csv).unwrap();
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].get("name"), Some(&Value::String("Task 1".into())));
        assert_eq!(
            parsed[0].get("priority"),
            Some(&Value::String("high".into()))
        );
        assert_eq!(
            parsed[0].get("amount"),
            Some(&Value::Number(Number::from(100)))
        );
        assert_eq!(
            parsed[1].get("amount"),
            Some(&Value::Number(Number::from(200)))
        );
    }

    #[test]
    fn empty_csv_returns_empty_array() {
        let r = registry();
        assert!(r
            .parse_import("", FluxExportFormat::Csv)
            .unwrap()
            .is_empty());
    }

    #[test]
    fn header_only_csv_returns_empty_array() {
        let r = registry();
        assert!(r
            .parse_import("name,priority", FluxExportFormat::Csv)
            .unwrap()
            .is_empty());
    }

    #[test]
    fn exports_objects_to_json() {
        let r = registry();
        let data = vec![obj_map(&[
            ("name", Value::String("Test".into())),
            ("value", Value::Number(Number::from(42))),
        ])];
        let json = r.export_data(
            &data,
            &FluxExportOptions {
                entity_type: flux_types::TASK.into(),
                format: FluxExportFormat::Json,
                fields: None,
                include_edges: None,
            },
        );
        let parsed: Value = serde_json::from_str(&json).unwrap();
        let expected: Value = serde_json::to_value(&data).unwrap();
        assert_eq!(parsed, expected);
    }

    #[test]
    fn parses_json_back_to_objects() {
        let r = registry();
        let json = r#"[{"name":"Task 1","priority":"high"}]"#;
        let parsed = r.parse_import(json, FluxExportFormat::Json).unwrap();
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].get("name"), Some(&Value::String("Task 1".into())));
    }

    #[test]
    fn invalid_json_errors() {
        let r = registry();
        assert!(r.parse_import("not json", FluxExportFormat::Json).is_err());
    }

    #[test]
    fn non_array_json_errors() {
        let r = registry();
        let err = r
            .parse_import(r#"{"key":"val"}"#, FluxExportFormat::Json)
            .unwrap_err();
        assert!(matches!(err, FluxImportError::ExpectedArray));
    }

    #[test]
    fn exports_empty_array_to_csv() {
        let r = registry();
        let csv = r.export_data(
            &[],
            &FluxExportOptions {
                entity_type: flux_types::TASK.into(),
                format: FluxExportFormat::Csv,
                fields: None,
                include_edges: None,
            },
        );
        assert_eq!(csv, "");
    }

    #[test]
    fn exports_empty_array_to_json() {
        let r = registry();
        let json = r.export_data(
            &[],
            &FluxExportOptions {
                entity_type: flux_types::TASK.into(),
                format: FluxExportFormat::Json,
                fields: None,
                include_edges: None,
            },
        );
        assert_eq!(json, "[]");
    }
}
