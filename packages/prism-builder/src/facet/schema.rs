//! Facet schema, records, and validation.

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::registry::{FieldKind, FieldSpec};

// ── Schema types ─────────────────────────────────────────────────────────────

pub type FacetSchemaId = String;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FacetSchema {
    pub id: FacetSchemaId,
    pub label: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub fields: Vec<FieldSpec>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FacetRecord {
    pub id: String,
    #[serde(default)]
    pub fields: IndexMap<String, Value>,
}

impl FacetRecord {
    pub fn to_value(&self) -> Value {
        Value::Object(
            self.fields
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ValidationError {
    pub field: String,
    pub message: String,
}

impl FacetSchema {
    pub fn validate_record(&self, record: &FacetRecord) -> Vec<ValidationError> {
        let mut errors = Vec::new();
        for field in &self.fields {
            let val = record.fields.get(&field.key);
            if field.required {
                let missing = match val {
                    None | Some(Value::Null) => true,
                    Some(Value::String(s)) => s.is_empty(),
                    _ => false,
                };
                if missing {
                    errors.push(ValidationError {
                        field: field.key.clone(),
                        message: format!("{} is required", field.label),
                    });
                }
            }
            if let Some(val) = val {
                match &field.kind {
                    FieldKind::Number(bounds) => {
                        if let Some(n) = val.as_f64() {
                            if let Some(lo) = bounds.min {
                                if n < lo {
                                    errors.push(ValidationError {
                                        field: field.key.clone(),
                                        message: format!("must be >= {lo}"),
                                    });
                                }
                            }
                            if let Some(hi) = bounds.max {
                                if n > hi {
                                    errors.push(ValidationError {
                                        field: field.key.clone(),
                                        message: format!("must be <= {hi}"),
                                    });
                                }
                            }
                        }
                    }
                    FieldKind::Integer(bounds) => {
                        if let Some(n) = val.as_i64() {
                            if let Some(lo) = bounds.min {
                                if (n as f64) < lo {
                                    errors.push(ValidationError {
                                        field: field.key.clone(),
                                        message: format!("must be >= {lo}"),
                                    });
                                }
                            }
                            if let Some(hi) = bounds.max {
                                if (n as f64) > hi {
                                    errors.push(ValidationError {
                                        field: field.key.clone(),
                                        message: format!("must be <= {hi}"),
                                    });
                                }
                            }
                        }
                    }
                    FieldKind::Select(options) => {
                        if let Some(s) = val.as_str() {
                            if !s.is_empty() && !options.iter().any(|o| o.value == s) {
                                errors.push(ValidationError {
                                    field: field.key.clone(),
                                    message: format!("'{s}' is not a valid option"),
                                });
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        errors
    }

    pub fn default_record(&self, id: impl Into<String>) -> FacetRecord {
        let mut fields = IndexMap::new();
        for field in &self.fields {
            if matches!(field.kind, FieldKind::Calculation { .. }) {
                continue;
            }
            let val = if field.default != Value::Null {
                field.default.clone()
            } else {
                default_for_kind(&field.kind)
            };
            fields.insert(field.key.clone(), val);
        }
        FacetRecord {
            id: id.into(),
            fields,
        }
    }
}

fn default_for_kind(kind: &FieldKind) -> Value {
    match kind {
        FieldKind::Text | FieldKind::TextArea | FieldKind::Date | FieldKind::DateTime => {
            Value::String(String::new())
        }
        FieldKind::Number(_) | FieldKind::Currency { .. } => Value::from(0.0),
        FieldKind::Integer(_) | FieldKind::Duration => Value::from(0),
        FieldKind::Boolean => Value::Bool(false),
        FieldKind::Color => Value::String("#000000".into()),
        FieldKind::File(_) => Value::Null,
        FieldKind::Select(options) => options
            .first()
            .map(|o| Value::String(o.value.clone()))
            .unwrap_or(Value::String(String::new())),
        FieldKind::Calculation { .. } => Value::Null,
        FieldKind::Custom { .. } => Value::Null,
    }
}

// ── Facet kinds ──────────────────────────────────────────────────────────────
