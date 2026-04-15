//! Field schema primitives — the atomic unit of a form-driven
//! document.
//!
//! Port of `language/forms/field-schema.ts` from the pre-Rust
//! reference commit (8426588). Pure data; every runtime concern
//! (rendering, validation, conditionals) lives in sibling modules.

use serde::{Deserialize, Serialize};

/// The set of input widgets a [`FieldSchema`] can request. Matches the
/// TS union exactly so Puck/Flux docs round-trip without translation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FieldType {
    Text,
    Textarea,
    RichText,
    Number,
    Currency,
    Duration,
    Rating,
    Slider,
    Boolean,
    Date,
    Datetime,
    Url,
    Email,
    Phone,
    Color,
    Select,
    MultiSelect,
    Tags,
    Formula,
}

/// One option in a `select` / `multi-select` / `tags` field.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SelectOption {
    pub value: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
}

/// One field on a form / document schema. Every optional TS property
/// becomes an `Option<…>` here, which preserves round-trip identity
/// with the legacy JSON when fields are absent.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FieldSchema {
    pub id: String,
    pub label: String,
    #[serde(rename = "type")]
    pub field_type: FieldType,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placeholder: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub required: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub options: Option<Vec<SelectOption>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub step: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
    #[serde(rename = "maxLength", default, skip_serializing_if = "Option::is_none")]
    pub max_length: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pattern: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hidden: Option<bool>,
    #[serde(rename = "readOnly", default, skip_serializing_if = "Option::is_none")]
    pub read_only: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub section: Option<String>,

    /// For `type == "formula"`, the expression evaluated at render
    /// time. Parsed and evaluated through
    /// [`crate::language::expression`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expression: Option<String>,
}

impl FieldSchema {
    /// Minimal constructor — callers fluent-set the rest. Matches the
    /// pattern used by the other `prism-core` builders.
    pub fn new(id: impl Into<String>, label: impl Into<String>, field_type: FieldType) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            field_type,
            description: None,
            placeholder: None,
            required: None,
            options: None,
            min: None,
            max: None,
            step: None,
            unit: None,
            max_length: None,
            pattern: None,
            hidden: None,
            read_only: None,
            section: None,
            expression: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn field_type_serializes_kebab_case() {
        let t = serde_json::to_string(&FieldType::MultiSelect).unwrap();
        assert_eq!(t, "\"multi-select\"");
        let back: FieldType = serde_json::from_str("\"multi-select\"").unwrap();
        assert_eq!(back, FieldType::MultiSelect);
    }

    #[test]
    fn field_schema_omits_absent_optionals() {
        let f = FieldSchema::new("title", "Title", FieldType::Text);
        let json = serde_json::to_string(&f).unwrap();
        assert_eq!(json, r#"{"id":"title","label":"Title","type":"text"}"#);
    }

    #[test]
    fn field_schema_round_trips_select_options() {
        let f = FieldSchema {
            options: Some(vec![
                SelectOption {
                    value: "a".into(),
                    label: "Alpha".into(),
                    color: Some("teal".into()),
                },
                SelectOption {
                    value: "b".into(),
                    label: "Beta".into(),
                    color: None,
                },
            ]),
            required: Some(true),
            ..FieldSchema::new("pick", "Pick", FieldType::Select)
        };
        let json = serde_json::to_string(&f).unwrap();
        let back: FieldSchema = serde_json::from_str(&json).unwrap();
        assert_eq!(back, f);
    }

    #[test]
    fn formula_field_carries_expression() {
        let f = FieldSchema {
            expression: Some("a + b".into()),
            ..FieldSchema::new("total", "Total", FieldType::Formula)
        };
        assert_eq!(f.expression.as_deref(), Some("a + b"));
    }
}
