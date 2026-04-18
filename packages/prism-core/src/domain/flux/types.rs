//! Pure data types for the Flux registry.
//!
//! Port of `packages/prism-core/src/domain/flux/flux-types.ts` at
//! commit 8426588. The TS file used `as const` string-union enums for
//! `FLUX_CATEGORIES` / `FLUX_TYPES` / `FLUX_EDGES`; in Rust we keep
//! them as `&'static str` constants inside namespaced modules so
//! `flux_types::TASK` reads the same as the TS `FLUX_TYPES.TASK`.

use serde::{Deserialize, Serialize};

// ── Domain Constants ───────────────────────────────────────────────

/// Category strings used by Flux entity defs.
pub mod flux_categories {
    pub const PRODUCTIVITY: &str = "flux:productivity";
    pub const PEOPLE: &str = "flux:people";
    pub const FINANCE: &str = "flux:finance";
    pub const INVENTORY: &str = "flux:inventory";
}

/// Entity type strings used by Flux entity defs.
pub mod flux_types {
    // Productivity
    pub const TASK: &str = "flux:task";
    pub const PROJECT: &str = "flux:project";
    pub const GOAL: &str = "flux:goal";
    pub const MILESTONE: &str = "flux:milestone";
    // People
    pub const CONTACT: &str = "flux:contact";
    pub const ORGANIZATION: &str = "flux:organization";
    // Finance
    pub const TRANSACTION: &str = "flux:transaction";
    pub const ACCOUNT: &str = "flux:account";
    pub const INVOICE: &str = "flux:invoice";
    // Inventory
    pub const ITEM: &str = "flux:item";
    pub const LOCATION: &str = "flux:location";
}

/// Edge relation strings used by Flux edge defs.
pub mod flux_edges {
    pub const ASSIGNED_TO: &str = "flux:assigned-to";
    pub const DEPENDS_ON: &str = "flux:depends-on";
    pub const BLOCKS: &str = "flux:blocks";
    pub const BELONGS_TO: &str = "flux:belongs-to";
    pub const RELATED_TO: &str = "flux:related-to";
    pub const INVOICED_TO: &str = "flux:invoiced-to";
    pub const STORED_AT: &str = "flux:stored-at";
}

// ── Status Tables ──────────────────────────────────────────────────

/// One option inside a status / type lookup table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StatusOption {
    pub value: &'static str,
    pub label: &'static str,
}

pub const TASK_STATUSES: &[StatusOption] = &[
    StatusOption {
        value: "backlog",
        label: "Backlog",
    },
    StatusOption {
        value: "todo",
        label: "To Do",
    },
    StatusOption {
        value: "in_progress",
        label: "In Progress",
    },
    StatusOption {
        value: "review",
        label: "In Review",
    },
    StatusOption {
        value: "done",
        label: "Done",
    },
    StatusOption {
        value: "cancelled",
        label: "Cancelled",
    },
];

pub const PROJECT_STATUSES: &[StatusOption] = &[
    StatusOption {
        value: "planning",
        label: "Planning",
    },
    StatusOption {
        value: "active",
        label: "Active",
    },
    StatusOption {
        value: "on_hold",
        label: "On Hold",
    },
    StatusOption {
        value: "completed",
        label: "Completed",
    },
    StatusOption {
        value: "archived",
        label: "Archived",
    },
];

pub const GOAL_STATUSES: &[StatusOption] = &[
    StatusOption {
        value: "draft",
        label: "Draft",
    },
    StatusOption {
        value: "active",
        label: "Active",
    },
    StatusOption {
        value: "achieved",
        label: "Achieved",
    },
    StatusOption {
        value: "abandoned",
        label: "Abandoned",
    },
];

pub const TRANSACTION_TYPES: &[StatusOption] = &[
    StatusOption {
        value: "income",
        label: "Income",
    },
    StatusOption {
        value: "expense",
        label: "Expense",
    },
    StatusOption {
        value: "transfer",
        label: "Transfer",
    },
    StatusOption {
        value: "refund",
        label: "Refund",
    },
];

pub const CONTACT_TYPES: &[StatusOption] = &[
    StatusOption {
        value: "person",
        label: "Person",
    },
    StatusOption {
        value: "company",
        label: "Company",
    },
    StatusOption {
        value: "lead",
        label: "Lead",
    },
    StatusOption {
        value: "vendor",
        label: "Vendor",
    },
    StatusOption {
        value: "partner",
        label: "Partner",
    },
];

pub const INVOICE_STATUSES: &[StatusOption] = &[
    StatusOption {
        value: "draft",
        label: "Draft",
    },
    StatusOption {
        value: "sent",
        label: "Sent",
    },
    StatusOption {
        value: "paid",
        label: "Paid",
    },
    StatusOption {
        value: "overdue",
        label: "Overdue",
    },
    StatusOption {
        value: "cancelled",
        label: "Cancelled",
    },
];

pub const ITEM_STATUSES: &[StatusOption] = &[
    StatusOption {
        value: "in_stock",
        label: "In Stock",
    },
    StatusOption {
        value: "low_stock",
        label: "Low Stock",
    },
    StatusOption {
        value: "out_of_stock",
        label: "Out of Stock",
    },
    StatusOption {
        value: "discontinued",
        label: "Discontinued",
    },
];

// ── Automation Preset ──────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FluxTriggerKind {
    OnCreate,
    OnUpdate,
    OnStatusChange,
    OnDueDate,
    OnSchedule,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FluxAutomationActionKind {
    SetField,
    CreateEdge,
    SendNotification,
    MoveToStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FluxAutomationAction {
    pub kind: FluxAutomationActionKind,
    pub target: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FluxAutomationPreset {
    pub id: String,
    pub name: String,
    /// Which entity type this preset applies to (FluxEntityType or
    /// plugin-extended). Stored as a plain string to mirror the TS
    /// `FluxEntityType | (string & {})` escape hatch.
    #[serde(rename = "entityType")]
    pub entity_type: String,
    pub trigger: FluxTriggerKind,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub condition: Option<String>,
    pub actions: Vec<FluxAutomationAction>,
}

// ── Import/Export ──────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FluxExportFormat {
    Csv,
    Json,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FluxExportOptions {
    #[serde(rename = "entityType")]
    pub entity_type: String,
    pub format: FluxExportFormat,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub fields: Option<Vec<String>>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        rename = "includeEdges",
        default
    )]
    pub include_edges: Option<bool>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FluxImportResult {
    pub imported: usize,
    pub skipped: usize,
    pub errors: Vec<String>,
}
