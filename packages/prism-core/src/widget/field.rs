//! Unified field descriptor for property panels, widget config, and
//! facet schemas.
//!
//! Moved from `prism-builder::registry` so that `prism-core` modules
//! can declare widget contributions without depending on the builder.

use serde::{Deserialize, Serialize};
use serde_json::Value;

// ── FieldSpec ────────────────────────────────────────────────────

/// Typed field descriptor. Components return `Vec<FieldSpec>` from
/// their schema method; the property panel walks the list and paints
/// one editor per entry. Nodes store their values under
/// [`FieldSpec::key`] in the node's `props` map.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldSpec {
    pub key: String,
    pub label: String,
    pub kind: FieldKind,
    #[serde(default)]
    pub default: Value,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub help: Option<String>,
    #[serde(default)]
    pub group: Option<String>,
}

impl FieldSpec {
    pub fn new(key: impl Into<String>, label: impl Into<String>, kind: FieldKind) -> Self {
        Self {
            key: key.into(),
            label: label.into(),
            kind,
            default: Value::Null,
            required: false,
            help: None,
            group: None,
        }
    }

    pub fn with_default(mut self, default: Value) -> Self {
        self.default = default;
        self
    }

    pub fn required(mut self) -> Self {
        self.required = true;
        self
    }

    pub fn with_help(mut self, help: impl Into<String>) -> Self {
        self.help = Some(help.into());
        self
    }

    pub fn group(mut self, group: impl Into<String>) -> Self {
        self.group = Some(group.into());
        self
    }

    pub fn text(key: impl Into<String>, label: impl Into<String>) -> Self {
        Self::new(key, label, FieldKind::Text).with_default(Value::String(String::new()))
    }

    pub fn textarea(key: impl Into<String>, label: impl Into<String>) -> Self {
        Self::new(key, label, FieldKind::TextArea).with_default(Value::String(String::new()))
    }

    pub fn number(key: impl Into<String>, label: impl Into<String>, bounds: NumericBounds) -> Self {
        Self::new(key, label, FieldKind::Number(bounds)).with_default(Value::from(0.0))
    }

    pub fn integer(
        key: impl Into<String>,
        label: impl Into<String>,
        bounds: NumericBounds,
    ) -> Self {
        Self::new(key, label, FieldKind::Integer(bounds)).with_default(Value::from(0))
    }

    pub fn boolean(key: impl Into<String>, label: impl Into<String>) -> Self {
        Self::new(key, label, FieldKind::Boolean).with_default(Value::Bool(false))
    }

    pub fn select(
        key: impl Into<String>,
        label: impl Into<String>,
        options: Vec<SelectOption>,
    ) -> Self {
        let default = options
            .first()
            .map(|o| Value::String(o.value.clone()))
            .unwrap_or(Value::Null);
        Self::new(key, label, FieldKind::Select(options)).with_default(default)
    }

    pub fn color(key: impl Into<String>, label: impl Into<String>) -> Self {
        Self::new(key, label, FieldKind::Color).with_default(Value::String("#000000".into()))
    }

    pub fn file(key: impl Into<String>, label: impl Into<String>, accept: Vec<String>) -> Self {
        Self::new(key, label, FieldKind::File(FileFieldConfig { accept }))
    }

    pub fn date(key: impl Into<String>, label: impl Into<String>) -> Self {
        Self::new(key, label, FieldKind::Date)
    }

    pub fn date_time(key: impl Into<String>, label: impl Into<String>) -> Self {
        Self::new(key, label, FieldKind::DateTime)
    }

    pub fn duration(key: impl Into<String>, label: impl Into<String>) -> Self {
        Self::new(key, label, FieldKind::Duration).with_default(Value::from(0))
    }

    pub fn currency(
        key: impl Into<String>,
        label: impl Into<String>,
        currency_code: Option<String>,
    ) -> Self {
        Self::new(key, label, FieldKind::Currency { currency_code }).with_default(Value::from(0.0))
    }

    pub fn calculation(
        key: impl Into<String>,
        label: impl Into<String>,
        formula: impl Into<String>,
    ) -> Self {
        Self::new(
            key,
            label,
            FieldKind::Calculation {
                formula: formula.into(),
            },
        )
    }
}

// ── FieldKind ────────────────────────────────────────────────────

/// Discriminator telling the property panel which editor to render.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", content = "data", rename_all = "kebab-case")]
pub enum FieldKind {
    Text,
    TextArea,
    Number(NumericBounds),
    Integer(NumericBounds),
    Boolean,
    Select(Vec<SelectOption>),
    Color,
    File(FileFieldConfig),
    Date,
    DateTime,
    Duration,
    Currency {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        currency_code: Option<String>,
    },
    Calculation {
        formula: String,
    },
}

// ── Supporting types ─────────────────────────────────────────────

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FileFieldConfig {
    #[serde(default)]
    pub accept: Vec<String>,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct NumericBounds {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,
}

impl NumericBounds {
    pub const fn unbounded() -> Self {
        Self {
            min: None,
            max: None,
        }
    }

    pub const fn min_max(min: f64, max: f64) -> Self {
        Self {
            min: Some(min),
            max: Some(max),
        }
    }

    pub const fn min(min: f64) -> Self {
        Self {
            min: Some(min),
            max: None,
        }
    }

    pub const fn max(max: f64) -> Self {
        Self {
            min: None,
            max: Some(max),
        }
    }

    pub fn clamp(&self, value: f64) -> f64 {
        let mut v = value;
        if let Some(m) = self.min {
            if v < m {
                v = m;
            }
        }
        if let Some(m) = self.max {
            if v > m {
                v = m;
            }
        }
        v
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelectOption {
    pub value: String,
    pub label: String,
}

impl SelectOption {
    pub fn new(value: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            value: value.into(),
            label: label.into(),
        }
    }
}

// ── FieldValue reader ────────────────────────────────────────────

/// Typed read of a [`FieldSpec`]-shaped slot out of a node's `props`.
pub struct FieldValue;

impl FieldValue {
    pub fn read_string<'a>(props: &'a Value, spec: &'a FieldSpec) -> &'a str {
        props
            .get(&spec.key)
            .and_then(|v| v.as_str())
            .or_else(|| spec.default.as_str())
            .unwrap_or("")
    }

    pub fn read_number(props: &Value, spec: &FieldSpec) -> f64 {
        let raw = props
            .get(&spec.key)
            .and_then(|v| v.as_f64())
            .or_else(|| spec.default.as_f64())
            .unwrap_or(0.0);
        match &spec.kind {
            FieldKind::Number(b) | FieldKind::Integer(b) => b.clamp(raw),
            _ => raw,
        }
    }

    pub fn read_integer(props: &Value, spec: &FieldSpec) -> i64 {
        let raw = props
            .get(&spec.key)
            .and_then(|v| v.as_i64())
            .or_else(|| spec.default.as_i64())
            .unwrap_or(0);
        match &spec.kind {
            FieldKind::Integer(b) => b.clamp(raw as f64) as i64,
            _ => raw,
        }
    }

    pub fn read_boolean(props: &Value, spec: &FieldSpec) -> bool {
        props
            .get(&spec.key)
            .and_then(|v| v.as_bool())
            .or_else(|| spec.default.as_bool())
            .unwrap_or(false)
    }
}

// ── Prop helpers ─────────────────────────────────────────────────

pub fn prop_str<'a>(props: &'a Value, key: &str, default: &'a str) -> &'a str {
    props.get(key).and_then(|v| v.as_str()).unwrap_or(default)
}

pub fn prop_u64(props: &Value, key: &str, default: u64) -> u64 {
    props.get(key).and_then(|v| v.as_u64()).unwrap_or(default)
}

pub fn prop_f64(props: &Value, key: &str, default: f64) -> f64 {
    props.get(key).and_then(|v| v.as_f64()).unwrap_or(default)
}

pub fn prop_bool(props: &Value, key: &str) -> bool {
    props.get(key).and_then(|v| v.as_bool()).unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn text_builder_defaults_to_empty_string() {
        let spec = FieldSpec::text("title", "Title");
        assert_eq!(spec.default, Value::String(String::new()));
        assert!(matches!(spec.kind, FieldKind::Text));
        assert!(!spec.required);
    }

    #[test]
    fn number_bounds_clamp_on_read() {
        let spec = FieldSpec::number("volume", "Volume", NumericBounds::min_max(0.0, 1.0));
        let props = json!({ "volume": 2.5 });
        assert!((FieldValue::read_number(&props, &spec) - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn integer_clamps_to_bounds() {
        let spec = FieldSpec::integer("level", "Level", NumericBounds::min_max(1.0, 6.0));
        let props = json!({ "level": 99 });
        assert_eq!(FieldValue::read_integer(&props, &spec), 6);
    }

    #[test]
    fn select_default_is_first_option_value() {
        let spec = FieldSpec::select(
            "style",
            "Style",
            vec![
                SelectOption::new("solid", "Solid"),
                SelectOption::new("dashed", "Dashed"),
            ],
        );
        assert_eq!(spec.default, Value::String("solid".into()));
    }

    #[test]
    fn required_flag_is_chainable() {
        let spec = FieldSpec::text("title", "Title").required();
        assert!(spec.required);
    }

    #[test]
    fn read_string_falls_back_to_default() {
        let spec = FieldSpec::text("text", "Text").with_default(Value::String("hi".into()));
        let props = json!({});
        assert_eq!(FieldValue::read_string(&props, &spec), "hi");
    }

    #[test]
    fn read_boolean_reads_actual_value() {
        let spec = FieldSpec::boolean("enabled", "Enabled");
        let props = json!({ "enabled": true });
        assert!(FieldValue::read_boolean(&props, &spec));
    }

    #[test]
    fn date_builder_defaults_to_null() {
        let spec = FieldSpec::date("due", "Due Date");
        assert_eq!(spec.default, Value::Null);
        assert!(matches!(spec.kind, FieldKind::Date));
    }

    #[test]
    fn currency_builder_defaults_to_zero() {
        let spec = FieldSpec::currency("amount", "Amount", Some("USD".into()));
        assert_eq!(spec.default, Value::from(0.0));
        assert!(matches!(spec.kind, FieldKind::Currency { .. }));
    }

    #[test]
    fn calculation_field_stores_formula() {
        let spec = FieldSpec::calculation("total", "Total", "SUM(A1:A10)");
        match &spec.kind {
            FieldKind::Calculation { formula } => assert_eq!(formula, "SUM(A1:A10)"),
            _ => panic!("expected Calculation"),
        }
    }
}
