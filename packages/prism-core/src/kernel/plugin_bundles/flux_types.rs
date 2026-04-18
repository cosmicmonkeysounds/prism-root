//! `plugin_bundles::flux_types` — minimal Flux domain surface.
//!
//! Extracted out of `domain::flux` so the plugin bundles can compile
//! without pulling in the (Phase-2b pending) full Flux port. Everything
//! here is the subset the built-in bundles reference: the `FLUX_TYPES`
//! and `FLUX_EDGES` string constants and the `FluxAutomationPreset` /
//! `FluxAutomationAction` data model. The real `domain::flux` module
//! will re-export from here once it lands.

use serde::{Deserialize, Serialize};

// ── Entity type strings ─────────────────────────────────────────────────────

#[allow(clippy::module_inception)]
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

// ── Edge relation strings ───────────────────────────────────────────────────

pub mod flux_edges {
    pub const ASSIGNED_TO: &str = "flux:assigned-to";
    pub const DEPENDS_ON: &str = "flux:depends-on";
    pub const BLOCKS: &str = "flux:blocks";
    pub const BELONGS_TO: &str = "flux:belongs-to";
    pub const RELATED_TO: &str = "flux:related-to";
    pub const INVOICED_TO: &str = "flux:invoiced-to";
    pub const STORED_AT: &str = "flux:stored-at";
}

// ── Automation preset ───────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FluxTriggerKind {
    OnCreate,
    OnUpdate,
    OnStatusChange,
    OnDueDate,
    OnSchedule,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FluxActionKind {
    SetField,
    CreateEdge,
    SendNotification,
    MoveToStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FluxAutomationAction {
    pub kind: FluxActionKind,
    pub target: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FluxAutomationPreset {
    pub id: String,
    pub name: String,
    #[serde(rename = "entityType")]
    pub entity_type: String,
    pub trigger: FluxTriggerKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub condition: Option<String>,
    pub actions: Vec<FluxAutomationAction>,
}
