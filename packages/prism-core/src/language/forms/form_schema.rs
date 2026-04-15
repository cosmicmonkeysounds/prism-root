//! `FormSchema` — a [`DocumentSchema`] extended with validation and
//! conditional-visibility rules.
//!
//! Port of `language/forms/form-schema.ts`. Validation rule callbacks
//! are intentionally *not* carried on the schema in the Rust port: the
//! TS version tucked an optional `validator` function onto each
//! `ValidationRule`, which broke serialization and forced every
//! consumer to branch on "schema from disk" vs "schema with a live
//! closure". Custom validators live next to the host that registers
//! them instead.

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

use super::document_schema::DocumentSchema;

/// Built-in validator kinds the core supports. `Custom` means the
/// host has registered a named validator out-of-band; the schema just
/// references it by the `message` field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ValidatorType {
    Required,
    Min,
    Max,
    MinLength,
    MaxLength,
    Pattern,
    Custom,
}

/// One validation rule on a field. `value` is stored as an opaque
/// JSON value so min/max can carry numbers and pattern can carry a
/// regex string with one type.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ValidationRule {
    #[serde(rename = "type")]
    pub rule_type: ValidatorType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<JsonValue>,
    pub message: String,
}

/// Validation rules for one field. Matches the TS shape exactly.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FieldValidation {
    #[serde(rename = "fieldId")]
    pub field_id: String,
    pub rules: Vec<ValidationRule>,
}

/// The comparison operator used by a [`FieldCondition`]. Matches the
/// TS union verbatim.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ConditionalOperator {
    Eq,
    Neq,
    Gt,
    Lt,
    Gte,
    Lte,
    Includes,
    NotEmpty,
    Empty,
}

/// One term in a conditional-visibility rule. `value` is `None` for
/// unary operators (`empty`, `notEmpty`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FieldCondition {
    #[serde(rename = "fieldId")]
    pub field_id: String,
    pub operator: ConditionalOperator,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<JsonValue>,
}

/// "Show this field only when the following conditions match." All
/// conditions are ANDed in the TS tree; we keep the same semantics.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConditionalRule {
    #[serde(rename = "targetFieldId")]
    pub target_field_id: String,
    #[serde(rename = "showWhen")]
    pub show_when: Vec<FieldCondition>,
}

/// A document schema with validation + conditional rules + submit
/// button labels. The TS type extended `DocumentSchema`; we flatten
/// the inner schema via `#[serde(flatten)]` so the on-disk JSON
/// remains a single object.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FormSchema {
    #[serde(flatten)]
    pub document: DocumentSchema,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub validation: Option<Vec<FieldValidation>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conditional: Option<Vec<ConditionalRule>>,
    #[serde(
        rename = "submitLabel",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub submit_label: Option<String>,
    #[serde(
        rename = "resetLabel",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub reset_label: Option<String>,
}

impl FormSchema {
    pub fn from_document(document: DocumentSchema) -> Self {
        Self {
            document,
            validation: None,
            conditional: None,
            submit_label: None,
            reset_label: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::language::forms::document_schema::{DocumentSchema, SectionDef};
    use crate::language::forms::field_schema::{FieldSchema, FieldType};

    #[test]
    fn validator_type_serializes_camel_case() {
        let t = serde_json::to_string(&ValidatorType::MinLength).unwrap();
        assert_eq!(t, "\"minLength\"");
    }

    #[test]
    fn conditional_operator_serializes_camel_case() {
        let t = serde_json::to_string(&ConditionalOperator::NotEmpty).unwrap();
        assert_eq!(t, "\"notEmpty\"");
    }

    #[test]
    fn form_schema_flattens_document_fields() {
        let document = DocumentSchema {
            id: "contact".into(),
            name: "Contact".into(),
            fields: vec![FieldSchema::new("name", "Name", FieldType::Text)],
            sections: Vec::<SectionDef>::new(),
        };
        let form = FormSchema {
            validation: Some(vec![FieldValidation {
                field_id: "name".into(),
                rules: vec![ValidationRule {
                    rule_type: ValidatorType::Required,
                    value: None,
                    message: "Name is required".into(),
                }],
            }]),
            submit_label: Some("Save".into()),
            ..FormSchema::from_document(document.clone())
        };

        let json = serde_json::to_string(&form).unwrap();
        assert!(json.contains("\"id\":\"contact\""));
        assert!(json.contains("\"fields\""));
        assert!(json.contains("\"validation\""));
        assert!(json.contains("\"submitLabel\":\"Save\""));

        let back: FormSchema = serde_json::from_str(&json).unwrap();
        assert_eq!(back.document.id, document.id);
        assert_eq!(back.submit_label.as_deref(), Some("Save"));
    }

    #[test]
    fn round_trip_conditional_rules() {
        let rule = ConditionalRule {
            target_field_id: "phone".into(),
            show_when: vec![FieldCondition {
                field_id: "country".into(),
                operator: ConditionalOperator::Eq,
                value: Some(JsonValue::String("US".into())),
            }],
        };
        let json = serde_json::to_string(&rule).unwrap();
        let back: ConditionalRule = serde_json::from_str(&json).unwrap();
        assert_eq!(back, rule);
    }
}
