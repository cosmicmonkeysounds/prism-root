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

    /// Prop key where resolved `data_query` results are stored.
    /// When set alongside `data_query`, the shell resolves the query
    /// and injects the result array at `props[data_key]` before
    /// template rendering.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data_key: Option<String>,
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
            data_key: None,
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

// ── Data resolution helpers ─────────────────────────────────────

/// Dot-notation field access into a JSON value (e.g. `"meta.title"`).
pub fn get_json_field(item: &Value, path: &str) -> Option<Value> {
    let mut current = item;
    for segment in path.split('.') {
        current = current.get(segment)?;
    }
    Some(current.clone())
}

/// Stringify a JSON value for stable string-based sorting.
pub fn json_sort_key(v: Value) -> String {
    match v {
        Value::String(s) => s,
        Value::Number(n) => {
            if let Some(f) = n.as_f64() {
                format!("{f:020.6}")
            } else {
                n.to_string()
            }
        }
        Value::Bool(b) => (if b { "1" } else { "0" }).into(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

impl QueryFilter {
    pub fn new(field: impl Into<String>, op: FilterOp, value: Value) -> Self {
        Self {
            field: field.into(),
            op,
            value,
        }
    }

    /// Test whether a JSON item matches this filter.
    pub fn matches(&self, item: &Value) -> bool {
        let field_val = match get_json_field(item, &self.field) {
            Some(v) => v,
            None => Value::Null,
        };
        match self.op {
            FilterOp::Eq => json_values_eq(&field_val, &self.value),
            FilterOp::Neq => !json_values_eq(&field_val, &self.value),
            FilterOp::Gt => json_cmp(&field_val, &self.value).is_some_and(|o| o.is_gt()),
            FilterOp::Gte => json_cmp(&field_val, &self.value).is_some_and(|o| o.is_ge()),
            FilterOp::Lt => json_cmp(&field_val, &self.value).is_some_and(|o| o.is_lt()),
            FilterOp::Lte => json_cmp(&field_val, &self.value).is_some_and(|o| o.is_le()),
            FilterOp::Contains => {
                if let (Some(haystack), Some(needle)) = (field_val.as_str(), self.value.as_str()) {
                    haystack.contains(needle)
                } else {
                    false
                }
            }
            FilterOp::In => {
                if let Some(arr) = self.value.as_array() {
                    arr.iter().any(|v| json_values_eq(&field_val, v))
                } else {
                    false
                }
            }
        }
    }
}

fn json_values_eq(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(na), Value::Number(nb)) => na.as_f64() == nb.as_f64(),
        (Value::String(sa), Value::Number(nb)) => sa.parse::<f64>().ok() == nb.as_f64(),
        (Value::Number(na), Value::String(sb)) => na.as_f64() == sb.parse::<f64>().ok(),
        _ => a == b,
    }
}

fn json_cmp(a: &Value, b: &Value) -> Option<std::cmp::Ordering> {
    let fa = value_as_f64(a)?;
    let fb = value_as_f64(b)?;
    fa.partial_cmp(&fb)
}

fn value_as_f64(v: &Value) -> Option<f64> {
    match v {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.parse().ok(),
        _ => None,
    }
}

impl DataQuery {
    /// Apply filters, sort, and limit to a mutable item list.
    pub fn apply(&self, items: &mut Vec<Value>) {
        self.apply_filters(items);
        self.apply_sort(items);
        self.apply_limit(items);
    }

    pub fn apply_filters(&self, items: &mut Vec<Value>) {
        if self.filters.is_empty() {
            return;
        }
        items.retain(|item| self.filters.iter().all(|f| f.matches(item)));
    }

    pub fn apply_sort(&self, items: &mut [Value]) {
        if self.sort.is_empty() {
            return;
        }
        items.sort_by(|a, b| {
            for qs in &self.sort {
                let va = get_json_field(a, &qs.field)
                    .map(json_sort_key)
                    .unwrap_or_default();
                let vb = get_json_field(b, &qs.field)
                    .map(json_sort_key)
                    .unwrap_or_default();
                let ord = if qs.descending {
                    vb.cmp(&va)
                } else {
                    va.cmp(&vb)
                };
                if ord != std::cmp::Ordering::Equal {
                    return ord;
                }
            }
            std::cmp::Ordering::Equal
        });
    }

    pub fn apply_limit(&self, items: &mut Vec<Value>) {
        if let Some(lim) = self.limit {
            items.truncate(lim);
        }
    }
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

    // ── DataQuery resolution tests ──────────────────────────────

    #[test]
    fn get_json_field_flat() {
        let item = serde_json::json!({"name": "Alpha"});
        assert_eq!(
            get_json_field(&item, "name"),
            Some(serde_json::json!("Alpha"))
        );
    }

    #[test]
    fn get_json_field_nested() {
        let item = serde_json::json!({"meta": {"title": "Deep"}});
        assert_eq!(
            get_json_field(&item, "meta.title"),
            Some(serde_json::json!("Deep"))
        );
    }

    #[test]
    fn get_json_field_missing() {
        let item = serde_json::json!({"a": 1});
        assert!(get_json_field(&item, "b").is_none());
    }

    #[test]
    fn query_filter_eq() {
        let f = QueryFilter::new("status", FilterOp::Eq, serde_json::json!("active"));
        assert!(f.matches(&serde_json::json!({"status": "active"})));
        assert!(!f.matches(&serde_json::json!({"status": "draft"})));
    }

    #[test]
    fn query_filter_neq() {
        let f = QueryFilter::new("status", FilterOp::Neq, serde_json::json!("deleted"));
        assert!(f.matches(&serde_json::json!({"status": "active"})));
        assert!(!f.matches(&serde_json::json!({"status": "deleted"})));
    }

    #[test]
    fn query_filter_gt_lt() {
        let gt = QueryFilter::new("price", FilterOp::Gt, serde_json::json!(10));
        assert!(gt.matches(&serde_json::json!({"price": 15})));
        assert!(!gt.matches(&serde_json::json!({"price": 5})));

        let lt = QueryFilter::new("price", FilterOp::Lt, serde_json::json!(10));
        assert!(lt.matches(&serde_json::json!({"price": 5})));
        assert!(!lt.matches(&serde_json::json!({"price": 15})));
    }

    #[test]
    fn query_filter_contains() {
        let f = QueryFilter::new("name", FilterOp::Contains, serde_json::json!("pha"));
        assert!(f.matches(&serde_json::json!({"name": "Alpha"})));
        assert!(!f.matches(&serde_json::json!({"name": "Beta"})));
    }

    #[test]
    fn query_filter_in() {
        let f = QueryFilter::new(
            "status",
            FilterOp::In,
            serde_json::json!(["active", "pending"]),
        );
        assert!(f.matches(&serde_json::json!({"status": "active"})));
        assert!(f.matches(&serde_json::json!({"status": "pending"})));
        assert!(!f.matches(&serde_json::json!({"status": "deleted"})));
    }

    #[test]
    fn query_filter_numeric_eq() {
        let f = QueryFilter::new("count", FilterOp::Eq, serde_json::json!(3));
        assert!(f.matches(&serde_json::json!({"count": 3})));
        assert!(!f.matches(&serde_json::json!({"count": 4})));
    }

    #[test]
    fn data_query_apply_filters_and_sort() {
        let q = DataQuery {
            filters: vec![QueryFilter::new(
                "status",
                FilterOp::Eq,
                serde_json::json!("active"),
            )],
            sort: vec![QuerySort {
                field: "name".into(),
                descending: false,
            }],
            ..Default::default()
        };
        let mut items = vec![
            serde_json::json!({"name": "Charlie", "status": "active"}),
            serde_json::json!({"name": "Alpha", "status": "active"}),
            serde_json::json!({"name": "Bravo", "status": "deleted"}),
        ];
        q.apply(&mut items);
        assert_eq!(items.len(), 2);
        assert_eq!(items[0]["name"], "Alpha");
        assert_eq!(items[1]["name"], "Charlie");
    }

    #[test]
    fn data_query_apply_limit() {
        let q = DataQuery {
            limit: Some(2),
            ..Default::default()
        };
        let mut items = vec![
            serde_json::json!("a"),
            serde_json::json!("b"),
            serde_json::json!("c"),
        ];
        q.apply(&mut items);
        assert_eq!(items.len(), 2);
    }

    #[test]
    fn data_query_apply_sort_descending() {
        let q = DataQuery {
            sort: vec![QuerySort {
                field: "score".into(),
                descending: true,
            }],
            ..Default::default()
        };
        let mut items = vec![
            serde_json::json!({"score": 10}),
            serde_json::json!({"score": 30}),
            serde_json::json!({"score": 20}),
        ];
        q.apply(&mut items);
        assert_eq!(items[0]["score"], 30);
        assert_eq!(items[1]["score"], 20);
        assert_eq!(items[2]["score"], 10);
    }

    #[test]
    fn data_query_empty_is_noop() {
        let q = DataQuery::default();
        let mut items = vec![serde_json::json!("a"), serde_json::json!("b")];
        q.apply(&mut items);
        assert_eq!(items.len(), 2);
    }
}
