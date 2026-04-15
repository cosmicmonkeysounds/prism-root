//! `plugin::types` — data model for plugin contributions.
//!
//! Port of `kernel/plugin/plugin-types.ts` at 8426588. Every plugin
//! contribution is plain data — strings, ids, zone enums — so each
//! struct derives `Serialize`/`Deserialize`. Icons are `Option<String>`
//! (svg id or asset path); hosts that want richer icon types keep a
//! parallel table keyed by contribution id.

use serde::{Deserialize, Serialize};

pub type PluginId = String;

pub fn plugin_id(id: impl Into<String>) -> PluginId {
    id.into()
}

// ── Views ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ViewZone {
    Left,
    Right,
    Top,
    Bottom,
    Content,
    Floating,
    Toolbar,
    ActivityBar,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViewContributionDef {
    pub id: String,
    pub label: String,
    pub zone: ViewZone,
    #[serde(rename = "componentId")]
    pub component_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(
        default,
        rename = "defaultVisible",
        skip_serializing_if = "Option::is_none"
    )]
    pub default_visible: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
}

// ── Commands ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandContributionDef {
    pub id: String,
    pub label: String,
    pub category: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shortcut: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub action: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payload: Option<serde_json::Map<String, serde_json::Value>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub when: Option<String>,
}

// ── Context menus ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextMenuContributionDef {
    pub id: String,
    pub label: String,
    pub context: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub when: Option<String>,
    pub action: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shortcut: Option<String>,
    #[serde(
        default,
        rename = "separatorBefore",
        skip_serializing_if = "Option::is_none"
    )]
    pub separator_before: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub danger: Option<bool>,
}

// ── Keybindings ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeybindingContributionDef {
    pub command: String,
    pub key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub when: Option<String>,
}

// ── Activity bar ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ActivityBarPosition {
    Top,
    Bottom,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActivityBarContributionDef {
    pub id: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub position: Option<ActivityBarPosition>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub priority: Option<i32>,
}

// ── Settings ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SettingsContributionDef {
    pub id: String,
    pub label: String,
    #[serde(rename = "componentId")]
    pub component_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub order: Option<i32>,
}

// ── Toolbar / status bar ────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ToolbarPosition {
    Left,
    Center,
    Right,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolbarContributionDef {
    pub id: String,
    pub position: ToolbarPosition,
    #[serde(rename = "componentId")]
    pub component_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub order: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub when: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StatusBarPosition {
    Left,
    Right,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StatusBarContributionDef {
    pub id: String,
    pub position: StatusBarPosition,
    #[serde(rename = "componentId")]
    pub component_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub order: Option<i32>,
}

// ── Weak-ref providers ──────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WeakRefProviderContributionDef {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(rename = "sourceTypes")]
    pub source_types: Vec<String>,
}

// ── Unified contributions ───────────────────────────────────────────────────

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginContributions {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub views: Option<Vec<ViewContributionDef>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commands: Option<Vec<CommandContributionDef>>,
    #[serde(
        default,
        rename = "contextMenus",
        skip_serializing_if = "Option::is_none"
    )]
    pub context_menus: Option<Vec<ContextMenuContributionDef>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keybindings: Option<Vec<KeybindingContributionDef>>,
    #[serde(
        default,
        rename = "activityBar",
        skip_serializing_if = "Option::is_none"
    )]
    pub activity_bar: Option<Vec<ActivityBarContributionDef>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub settings: Option<Vec<SettingsContributionDef>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub toolbar: Option<Vec<ToolbarContributionDef>>,
    #[serde(default, rename = "statusBar", skip_serializing_if = "Option::is_none")]
    pub status_bar: Option<Vec<StatusBarContributionDef>>,
    #[serde(
        default,
        rename = "weakRefProviders",
        skip_serializing_if = "Option::is_none"
    )]
    pub weak_ref_providers: Option<Vec<WeakRefProviderContributionDef>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub immersive: Option<bool>,
}

// ── Plugin ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrismPlugin {
    pub id: PluginId,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contributes: Option<PluginContributions>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requires: Option<Vec<PluginId>>,
}

impl PrismPlugin {
    pub fn new(id: impl Into<PluginId>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            icon: None,
            contributes: None,
            requires: None,
        }
    }

    pub fn with_contributes(mut self, c: PluginContributions) -> Self {
        self.contributes = Some(c);
        self
    }
}
