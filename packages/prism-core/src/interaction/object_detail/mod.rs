//! `interaction::object_detail` — tabbed detail view for any object.
//!
//! Defines the data model for the Object Detail View: a tabbed
//! surface showing overview, relations, comments, time logs, and
//! activity for any `GraphObject`.

use serde::{Deserialize, Serialize};

// ── Detail Tab ───────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DetailTab {
    Overview,
    Relations,
    Comments,
    TimeLogs,
    Activity,
    Notes,
    Attachments,
}

impl DetailTab {
    pub fn label(self) -> &'static str {
        match self {
            Self::Overview => "Overview",
            Self::Relations => "Relations",
            Self::Comments => "Comments",
            Self::TimeLogs => "Time Logs",
            Self::Activity => "Activity",
            Self::Notes => "Notes",
            Self::Attachments => "Attachments",
        }
    }

    pub fn icon(self) -> &'static str {
        match self {
            Self::Overview => "info",
            Self::Relations => "link",
            Self::Comments => "message-circle",
            Self::TimeLogs => "clock",
            Self::Activity => "activity",
            Self::Notes => "file-text",
            Self::Attachments => "paperclip",
        }
    }
}

// ── Detail View Config ───────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetailViewConfig {
    pub tabs: Vec<DetailTab>,
    pub default_tab: DetailTab,
}

impl Default for DetailViewConfig {
    fn default() -> Self {
        Self {
            tabs: vec![
                DetailTab::Overview,
                DetailTab::Relations,
                DetailTab::Comments,
                DetailTab::TimeLogs,
                DetailTab::Activity,
            ],
            default_tab: DetailTab::Overview,
        }
    }
}

// ── Detail View State ────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetailViewState {
    pub object_id: String,
    pub object_type: String,
    pub active_tab: DetailTab,
    pub config: DetailViewConfig,
}

impl DetailViewState {
    pub fn new(object_id: String, object_type: String) -> Self {
        let config = config_for_type(&object_type);
        let active_tab = config.default_tab;
        Self {
            object_id,
            object_type,
            active_tab,
            config,
        }
    }

    pub fn set_tab(&mut self, tab: DetailTab) {
        if self.config.tabs.contains(&tab) {
            self.active_tab = tab;
        }
    }

    pub fn available_tabs(&self) -> &[DetailTab] {
        &self.config.tabs
    }
}

// ── Per-type tab configuration ───────────────────────────────────

pub fn config_for_type(object_type: &str) -> DetailViewConfig {
    match object_type {
        "flux:task" | "flux:project" | "flux:goal" => DetailViewConfig {
            tabs: vec![
                DetailTab::Overview,
                DetailTab::Relations,
                DetailTab::Comments,
                DetailTab::TimeLogs,
                DetailTab::Activity,
                DetailTab::Notes,
            ],
            default_tab: DetailTab::Overview,
        },
        "flux:contact" | "flux:organization" => DetailViewConfig {
            tabs: vec![
                DetailTab::Overview,
                DetailTab::Relations,
                DetailTab::Activity,
                DetailTab::Notes,
            ],
            default_tab: DetailTab::Overview,
        },
        "flux:invoice" | "flux:transaction" => DetailViewConfig {
            tabs: vec![
                DetailTab::Overview,
                DetailTab::Relations,
                DetailTab::Activity,
                DetailTab::Attachments,
            ],
            default_tab: DetailTab::Overview,
        },
        _ => DetailViewConfig::default(),
    }
}

// ── Overview section ─────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OverviewSection {
    pub label: String,
    pub fields: Vec<OverviewField>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OverviewField {
    pub key: String,
    pub label: String,
    pub value: serde_json::Value,
    pub editable: bool,
}

pub fn build_overview(
    object_type: &str,
    data: &serde_json::Map<String, serde_json::Value>,
) -> Vec<OverviewSection> {
    let mut fields: Vec<OverviewField> = data
        .iter()
        .map(|(k, v)| OverviewField {
            key: k.clone(),
            label: field_label(k),
            value: v.clone(),
            editable: true,
        })
        .collect();

    fields.sort_by(|a, b| a.key.cmp(&b.key));

    vec![OverviewSection {
        label: format!("{} Details", type_label(object_type)),
        fields,
    }]
}

fn field_label(key: &str) -> String {
    key.replace('_', " ")
        .split(' ')
        .map(|w| {
            let mut c = w.chars();
            match c.next() {
                None => String::new(),
                Some(f) => f.to_uppercase().to_string() + c.as_str(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn type_label(object_type: &str) -> &str {
    object_type.rsplit(':').next().unwrap_or(object_type)
}

// ── Tests ─────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_has_five_tabs() {
        let cfg = DetailViewConfig::default();
        assert_eq!(cfg.tabs.len(), 5);
        assert_eq!(cfg.default_tab, DetailTab::Overview);
    }

    #[test]
    fn task_config_includes_time_logs() {
        let cfg = config_for_type("flux:task");
        assert!(cfg.tabs.contains(&DetailTab::TimeLogs));
        assert!(cfg.tabs.contains(&DetailTab::Comments));
        assert!(cfg.tabs.contains(&DetailTab::Notes));
    }

    #[test]
    fn contact_config_excludes_time_logs() {
        let cfg = config_for_type("flux:contact");
        assert!(!cfg.tabs.contains(&DetailTab::TimeLogs));
        assert!(!cfg.tabs.contains(&DetailTab::Comments));
    }

    #[test]
    fn invoice_config_has_attachments() {
        let cfg = config_for_type("flux:invoice");
        assert!(cfg.tabs.contains(&DetailTab::Attachments));
    }

    #[test]
    fn unknown_type_gets_default() {
        let cfg = config_for_type("custom:widget");
        assert_eq!(cfg.tabs.len(), 5);
    }

    #[test]
    fn detail_view_state_new() {
        let state = DetailViewState::new("obj-1".into(), "flux:task".into());
        assert_eq!(state.active_tab, DetailTab::Overview);
        assert_eq!(state.object_id, "obj-1");
    }

    #[test]
    fn set_tab_valid() {
        let mut state = DetailViewState::new("obj-1".into(), "flux:task".into());
        state.set_tab(DetailTab::Comments);
        assert_eq!(state.active_tab, DetailTab::Comments);
    }

    #[test]
    fn set_tab_invalid_noop() {
        let mut state = DetailViewState::new("obj-1".into(), "flux:contact".into());
        state.set_tab(DetailTab::TimeLogs);
        assert_eq!(state.active_tab, DetailTab::Overview);
    }

    #[test]
    fn tab_labels_and_icons() {
        assert_eq!(DetailTab::Overview.label(), "Overview");
        assert_eq!(DetailTab::Overview.icon(), "info");
        assert_eq!(DetailTab::Comments.label(), "Comments");
        assert_eq!(DetailTab::TimeLogs.label(), "Time Logs");
    }

    #[test]
    fn build_overview_from_data() {
        let mut data = serde_json::Map::new();
        data.insert("status".into(), serde_json::json!("active"));
        data.insert("priority".into(), serde_json::json!(3));
        let sections = build_overview("flux:task", &data);
        assert_eq!(sections.len(), 1);
        assert_eq!(sections[0].fields.len(), 2);
        assert_eq!(sections[0].label, "task Details");
    }

    #[test]
    fn field_label_formatting() {
        assert_eq!(field_label("first_name"), "First Name");
        assert_eq!(field_label("status"), "Status");
    }

    #[test]
    fn detail_tab_serde_roundtrip() {
        let tab = DetailTab::TimeLogs;
        let json = serde_json::to_string(&tab).unwrap();
        assert_eq!(json, "\"time-logs\"");
        let back: DetailTab = serde_json::from_str(&json).unwrap();
        assert_eq!(back, tab);
    }
}
