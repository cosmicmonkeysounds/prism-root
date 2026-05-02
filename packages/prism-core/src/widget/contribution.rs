//! Widget contribution types — the vocabulary core engines use to
//! declare droppable widgets without depending on prism-builder.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::field::FieldSpec;

// ── WidgetContribution ───────────────────────────────────────────

/// A widget declaration from a core engine. Pure data — no rendering
/// code, no builder dependency. The builder wraps each contribution
/// in a `CoreWidgetComponent` that implements `Component`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WidgetContribution {
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    pub category: WidgetCategory,

    #[serde(default)]
    pub config_fields: Vec<FieldSpec>,
    #[serde(default)]
    pub default_config: Value,

    #[serde(default)]
    pub data_fields: Vec<FieldSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data_query: Option<DataQuery>,

    #[serde(default)]
    pub toolbar_actions: Vec<ToolbarAction>,

    #[serde(default)]
    pub signals: Vec<SignalSpec>,

    #[serde(default)]
    pub variants: Vec<VariantSpec>,

    pub default_size: WidgetSize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_size: Option<WidgetSize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_size: Option<WidgetSize>,

    pub template: WidgetTemplate,
}

impl Default for WidgetContribution {
    fn default() -> Self {
        Self {
            id: String::new(),
            label: String::new(),
            description: String::new(),
            icon: None,
            category: WidgetCategory::Display,
            config_fields: Vec::new(),
            default_config: Value::Null,
            data_fields: Vec::new(),
            data_query: None,
            toolbar_actions: Vec::new(),
            signals: Vec::new(),
            variants: Vec::new(),
            default_size: WidgetSize::default(),
            min_size: None,
            max_size: None,
            template: WidgetTemplate::default(),
        }
    }
}

// ── WidgetCategory ───────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum WidgetCategory {
    Display,
    Input,
    Navigation,
    DataTable,
    Temporal,
    Communication,
    Finance,
    Layout,
    Custom,
}

// ── WidgetSize ───────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct WidgetSize {
    pub col_span: u8,
    pub row_span: u8,
}

impl Default for WidgetSize {
    fn default() -> Self {
        Self {
            col_span: 1,
            row_span: 1,
        }
    }
}

impl WidgetSize {
    pub const fn new(col_span: u8, row_span: u8) -> Self {
        Self { col_span, row_span }
    }
}

// ── DataQuery ────────────────────────────────────────────────────

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DataQuery {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub object_type: Option<String>,
    #[serde(default)]
    pub filters: Vec<QueryFilter>,
    #[serde(default)]
    pub sort: Vec<QuerySort>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryFilter {
    pub field: String,
    pub op: FilterOp,
    pub value: Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FilterOp {
    Eq,
    Neq,
    Gt,
    Gte,
    Lt,
    Lte,
    Contains,
    In,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuerySort {
    pub field: String,
    #[serde(default)]
    pub descending: bool,
}

// ── ToolbarAction ────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolbarAction {
    pub id: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shortcut: Option<String>,
    pub kind: ToolbarActionKind,
}

impl ToolbarAction {
    pub fn signal(
        id: impl Into<String>,
        label: impl Into<String>,
        icon: impl Into<String>,
    ) -> Self {
        let id_str = id.into();
        Self {
            id: id_str.clone(),
            label: label.into(),
            icon: Some(icon.into()),
            group: None,
            shortcut: None,
            kind: ToolbarActionKind::Signal { signal: id_str },
        }
    }

    pub fn toggle(
        id: impl Into<String>,
        label: impl Into<String>,
        config_key: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            icon: None,
            group: None,
            shortcut: None,
            kind: ToolbarActionKind::ToggleConfig {
                key: config_key.into(),
            },
        }
    }

    pub fn custom(
        id: impl Into<String>,
        label: impl Into<String>,
        action_type: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            icon: None,
            group: None,
            shortcut: None,
            kind: ToolbarActionKind::Custom {
                action_type: action_type.into(),
            },
        }
    }

    pub fn with_icon(mut self, icon: impl Into<String>) -> Self {
        self.icon = Some(icon.into());
        self
    }

    pub fn with_group(mut self, group: impl Into<String>) -> Self {
        self.group = Some(group.into());
        self
    }

    pub fn with_shortcut(mut self, shortcut: impl Into<String>) -> Self {
        self.shortcut = Some(shortcut.into());
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum ToolbarActionKind {
    Signal { signal: String },
    SetConfig { key: String, value: Value },
    ToggleConfig { key: String },
    Custom { action_type: String },
}

// ── SignalSpec ────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignalSpec {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub payload_fields: Vec<FieldSpec>,
}

impl SignalSpec {
    pub fn new(name: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            payload_fields: Vec::new(),
        }
    }

    pub fn with_payload(mut self, fields: Vec<FieldSpec>) -> Self {
        self.payload_fields = fields;
        self
    }
}

// ── VariantSpec ──────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VariantSpec {
    pub key: String,
    pub label: String,
    pub options: Vec<VariantOptionSpec>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VariantOptionSpec {
    pub value: String,
    pub label: String,
    #[serde(default)]
    pub overrides: Value,
}

// ── WidgetTemplate ───────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WidgetTemplate {
    pub root: TemplateNode,
}

impl Default for WidgetTemplate {
    fn default() -> Self {
        Self {
            root: TemplateNode::Container {
                direction: LayoutDirection::Vertical,
                gap: None,
                padding: None,
                children: Vec::new(),
            },
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum TemplateNode {
    Container {
        direction: LayoutDirection,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        gap: Option<u32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        padding: Option<u32>,
        children: Vec<TemplateNode>,
    },
    Component {
        component_id: String,
        #[serde(default)]
        props: Value,
    },
    DataBinding {
        field: String,
        component_id: String,
        prop_key: String,
    },
    Repeater {
        source: String,
        item_template: Box<TemplateNode>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        empty_label: Option<String>,
    },
    Conditional {
        field: String,
        child: Box<TemplateNode>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        fallback: Option<Box<TemplateNode>>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LayoutDirection {
    Horizontal,
    Vertical,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_contribution_has_empty_template() {
        let c = WidgetContribution::default();
        assert!(c.id.is_empty());
        assert!(matches!(c.category, WidgetCategory::Display));
        assert!(matches!(c.template.root, TemplateNode::Container { .. }));
    }

    #[test]
    fn toolbar_action_signal_builder() {
        let a = ToolbarAction::signal("play", "Play", "play-icon").with_shortcut("Space");
        assert_eq!(a.id, "play");
        assert_eq!(a.shortcut.as_deref(), Some("Space"));
        assert!(matches!(a.kind, ToolbarActionKind::Signal { .. }));
    }

    #[test]
    fn signal_spec_with_payload() {
        let s = SignalSpec::new("item-selected", "An item was selected")
            .with_payload(vec![FieldSpec::text("item_id", "Item ID")]);
        assert_eq!(s.payload_fields.len(), 1);
    }

    #[test]
    fn widget_size_default_is_1x1() {
        let s = WidgetSize::default();
        assert_eq!(s.col_span, 1);
        assert_eq!(s.row_span, 1);
    }

    #[test]
    fn data_query_default_is_empty() {
        let q = DataQuery::default();
        assert!(q.object_type.is_none());
        assert!(q.filters.is_empty());
        assert!(q.sort.is_empty());
        assert!(q.limit.is_none());
    }

    #[test]
    fn contribution_roundtrips_through_json() {
        let c = WidgetContribution {
            id: "test-widget".into(),
            label: "Test Widget".into(),
            category: WidgetCategory::Temporal,
            toolbar_actions: vec![ToolbarAction::signal("start", "Start", "play")],
            signals: vec![SignalSpec::new("started", "Timer started")],
            template: WidgetTemplate {
                root: TemplateNode::Container {
                    direction: LayoutDirection::Vertical,
                    gap: Some(8),
                    padding: Some(12),
                    children: vec![TemplateNode::DataBinding {
                        field: "label".into(),
                        component_id: "text".into(),
                        prop_key: "body".into(),
                    }],
                },
            },
            ..Default::default()
        };
        let json = serde_json::to_string(&c).unwrap();
        let back: WidgetContribution = serde_json::from_str(&json).unwrap();
        assert_eq!(back.id, "test-widget");
        assert_eq!(back.toolbar_actions.len(), 1);
        assert_eq!(back.signals.len(), 1);
    }

    #[test]
    fn template_repeater_roundtrips() {
        let node = TemplateNode::Repeater {
            source: "items".into(),
            item_template: Box::new(TemplateNode::DataBinding {
                field: "name".into(),
                component_id: "text".into(),
                prop_key: "body".into(),
            }),
            empty_label: Some("No items".into()),
        };
        let json = serde_json::to_string(&node).unwrap();
        let back: TemplateNode = serde_json::from_str(&json).unwrap();
        assert!(matches!(back, TemplateNode::Repeater { .. }));
    }

    #[test]
    fn template_conditional_roundtrips() {
        let node = TemplateNode::Conditional {
            field: "is_active".into(),
            child: Box::new(TemplateNode::Component {
                component_id: "text".into(),
                props: serde_json::json!({"body": "Active"}),
            }),
            fallback: Some(Box::new(TemplateNode::Component {
                component_id: "text".into(),
                props: serde_json::json!({"body": "Inactive"}),
            })),
        };
        let json = serde_json::to_string(&node).unwrap();
        let back: TemplateNode = serde_json::from_str(&json).unwrap();
        assert!(matches!(back, TemplateNode::Conditional { .. }));
    }
}
