//! Lightweight JSON Schema subset for config value validation. Port of
//! `@prism/core/kernel/config/config-schema.ts`. Dependency-free,
//! recursive for `array.items` and `object.properties`.

use std::collections::BTreeMap;

use serde_json::{Map as JsonMap, Value as JsonValue};

#[derive(Debug, Clone, PartialEq)]
pub enum ConfigSchema {
    String(StringSchema),
    Number(NumberSchema),
    Boolean,
    Array(ArraySchema),
    Object(ObjectSchema),
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct StringSchema {
    pub min_length: Option<usize>,
    pub max_length: Option<usize>,
    pub enum_values: Option<Vec<String>>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct NumberSchema {
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub integer: bool,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ArraySchema {
    pub items: Option<Box<ConfigSchema>>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ObjectSchema {
    pub properties: BTreeMap<String, ConfigSchema>,
    pub required: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ValidationError {
    pub path: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ValidationResult {
    pub valid: bool,
    pub errors: Vec<ValidationError>,
}

impl ValidationResult {
    fn from_errors(errors: Vec<ValidationError>) -> Self {
        Self {
            valid: errors.is_empty(),
            errors,
        }
    }
}

pub fn validate_config(value: &JsonValue, schema: &ConfigSchema) -> ValidationResult {
    let mut errors = Vec::new();
    validate_inner(value, schema, "", &mut errors);
    ValidationResult::from_errors(errors)
}

fn validate_inner(
    value: &JsonValue,
    schema: &ConfigSchema,
    path: &str,
    errors: &mut Vec<ValidationError>,
) {
    match schema {
        ConfigSchema::String(s) => validate_string(value, s, path, errors),
        ConfigSchema::Number(s) => validate_number(value, s, path, errors),
        ConfigSchema::Boolean => validate_boolean(value, path, errors),
        ConfigSchema::Array(s) => validate_array(value, s, path, errors),
        ConfigSchema::Object(s) => validate_object(value, s, path, errors),
    }
}

fn validate_string(
    value: &JsonValue,
    schema: &StringSchema,
    path: &str,
    errors: &mut Vec<ValidationError>,
) {
    let Some(s) = value.as_str() else {
        errors.push(ValidationError {
            path: path.to_string(),
            message: format!("Expected string, got {}", type_name(value)),
        });
        return;
    };

    if let Some(min) = schema.min_length {
        if s.chars().count() < min {
            errors.push(ValidationError {
                path: path.to_string(),
                message: format!("String too short (min {}, got {})", min, s.chars().count()),
            });
        }
    }
    if let Some(max) = schema.max_length {
        if s.chars().count() > max {
            errors.push(ValidationError {
                path: path.to_string(),
                message: format!("String too long (max {}, got {})", max, s.chars().count()),
            });
        }
    }
    if let Some(allowed) = &schema.enum_values {
        if !allowed.iter().any(|v| v == s) {
            let joined = allowed
                .iter()
                .map(|v| format!("\"{}\"", v))
                .collect::<Vec<_>>()
                .join(", ");
            errors.push(ValidationError {
                path: path.to_string(),
                message: format!("Must be one of: {}", joined),
            });
        }
    }
}

fn validate_number(
    value: &JsonValue,
    schema: &NumberSchema,
    path: &str,
    errors: &mut Vec<ValidationError>,
) {
    let Some(n) = value.as_f64() else {
        errors.push(ValidationError {
            path: path.to_string(),
            message: format!("Expected number, got {}", type_name(value)),
        });
        return;
    };
    if n.is_nan() {
        errors.push(ValidationError {
            path: path.to_string(),
            message: "Expected number, got NaN".to_string(),
        });
        return;
    }
    if schema.integer && n.fract() != 0.0 {
        errors.push(ValidationError {
            path: path.to_string(),
            message: format!("Expected integer, got {}", n),
        });
    }
    if let Some(min) = schema.min {
        if n < min {
            errors.push(ValidationError {
                path: path.to_string(),
                message: format!("Too small (min {}, got {})", min, n),
            });
        }
    }
    if let Some(max) = schema.max {
        if n > max {
            errors.push(ValidationError {
                path: path.to_string(),
                message: format!("Too large (max {}, got {})", max, n),
            });
        }
    }
}

fn validate_boolean(value: &JsonValue, path: &str, errors: &mut Vec<ValidationError>) {
    if !value.is_boolean() {
        errors.push(ValidationError {
            path: path.to_string(),
            message: format!("Expected boolean, got {}", type_name(value)),
        });
    }
}

fn validate_array(
    value: &JsonValue,
    schema: &ArraySchema,
    path: &str,
    errors: &mut Vec<ValidationError>,
) {
    let Some(arr) = value.as_array() else {
        errors.push(ValidationError {
            path: path.to_string(),
            message: format!("Expected array, got {}", type_name(value)),
        });
        return;
    };
    if let Some(items) = &schema.items {
        for (i, v) in arr.iter().enumerate() {
            let child_path = if path.is_empty() {
                format!("[{}]", i)
            } else {
                format!("{}[{}]", path, i)
            };
            validate_inner(v, items, &child_path, errors);
        }
    }
}

fn validate_object(
    value: &JsonValue,
    schema: &ObjectSchema,
    path: &str,
    errors: &mut Vec<ValidationError>,
) {
    let Some(obj) = value.as_object() else {
        errors.push(ValidationError {
            path: path.to_string(),
            message: format!("Expected object, got {}", type_name(value)),
        });
        return;
    };
    for key in &schema.required {
        if !obj.contains_key(key) {
            let child_path = if path.is_empty() {
                key.clone()
            } else {
                format!("{}.{}", path, key)
            };
            errors.push(ValidationError {
                path: child_path,
                message: "Required property missing".to_string(),
            });
        }
    }
    for (key, prop_schema) in &schema.properties {
        let Some(child) = obj.get(key) else { continue };
        let child_path = if path.is_empty() {
            key.clone()
        } else {
            format!("{}.{}", path, key)
        };
        validate_inner(child, prop_schema, &child_path, errors);
    }
}

fn type_name(value: &JsonValue) -> &'static str {
    match value {
        JsonValue::Null => "null",
        JsonValue::Bool(_) => "boolean",
        JsonValue::Number(_) => "number",
        JsonValue::String(_) => "string",
        JsonValue::Array(_) => "array",
        JsonValue::Object(_) => "object",
    }
}

/// Coerce a raw string (e.g. from an env var) into the type described
/// by `schema`.
pub fn coerce_config_value(value: &str, schema: &ConfigSchema) -> Result<JsonValue, String> {
    match schema {
        ConfigSchema::String(_) => Ok(JsonValue::String(value.to_string())),
        ConfigSchema::Number(_) => value
            .parse::<f64>()
            .map(JsonValue::from)
            .map_err(|_| format!("Cannot coerce '{}' to number", value)),
        ConfigSchema::Boolean => Ok(JsonValue::Bool(value == "true" || value == "1")),
        ConfigSchema::Array(_) | ConfigSchema::Object(_) => {
            serde_json::from_str(value).map_err(|_| {
                format!(
                    "Cannot coerce '{}' to {}: invalid JSON",
                    value,
                    match schema {
                        ConfigSchema::Array(_) => "array",
                        ConfigSchema::Object(_) => "object",
                        _ => unreachable!(),
                    }
                )
            })
        }
    }
}

/// Convert a schema into a `SettingDefinition::validate` style
/// closure: returns `None` when valid, `Some(message)` when invalid.
pub fn schema_to_message(schema: &ConfigSchema, value: &JsonValue) -> Option<String> {
    let result = validate_config(value, schema);
    if result.valid {
        return None;
    }
    Some(
        result
            .errors
            .iter()
            .map(|e| {
                if e.path.is_empty() {
                    e.message.clone()
                } else {
                    format!("[{}] {}", e.path, e.message)
                }
            })
            .collect::<Vec<_>>()
            .join("; "),
    )
}

/// Convenience: build an object schema from a slice of
/// `(key, prop_schema)` pairs.
pub fn object_schema(properties: &[(&str, ConfigSchema)]) -> ObjectSchema {
    let mut map = BTreeMap::new();
    for (k, v) in properties {
        map.insert((*k).to_string(), v.clone());
    }
    ObjectSchema {
        properties: map,
        required: Vec::new(),
    }
}

/// Convenience: build a JSON object from a slice of `(key, value)`
/// pairs. Used in docs + tests; exported since it's ergonomic.
pub fn json_obj(pairs: &[(&str, JsonValue)]) -> JsonMap<String, JsonValue> {
    let mut out = JsonMap::new();
    for (k, v) in pairs {
        out.insert((*k).to_string(), v.clone());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn string_type_check() {
        let schema = ConfigSchema::String(StringSchema::default());
        assert!(validate_config(&json!("hi"), &schema).valid);
        let err = validate_config(&json!(42), &schema);
        assert!(!err.valid);
        assert!(err.errors[0].message.contains("Expected string"));
    }

    #[test]
    fn string_min_max_length() {
        let schema = ConfigSchema::String(StringSchema {
            min_length: Some(2),
            max_length: Some(5),
            ..Default::default()
        });
        assert!(validate_config(&json!("abc"), &schema).valid);
        assert!(!validate_config(&json!("a"), &schema).valid);
        assert!(!validate_config(&json!("abcdef"), &schema).valid);
    }

    #[test]
    fn string_enum() {
        let schema = ConfigSchema::String(StringSchema {
            enum_values: Some(vec!["dark".into(), "light".into()]),
            ..Default::default()
        });
        assert!(validate_config(&json!("dark"), &schema).valid);
        let res = validate_config(&json!("neon"), &schema);
        assert!(!res.valid);
        assert!(res.errors[0].message.contains("Must be one of"));
    }

    #[test]
    fn number_min_max() {
        let schema = ConfigSchema::Number(NumberSchema {
            min: Some(0.0),
            max: Some(10.0),
            integer: false,
        });
        assert!(validate_config(&json!(5.5), &schema).valid);
        assert!(!validate_config(&json!(-1.0), &schema).valid);
        assert!(!validate_config(&json!(11.0), &schema).valid);
    }

    #[test]
    fn number_integer_flag() {
        let schema = ConfigSchema::Number(NumberSchema {
            integer: true,
            ..Default::default()
        });
        assert!(validate_config(&json!(3), &schema).valid);
        assert!(!validate_config(&json!(3.5), &schema).valid);
    }

    #[test]
    fn array_items_recursed() {
        let schema = ConfigSchema::Array(ArraySchema {
            items: Some(Box::new(ConfigSchema::Number(NumberSchema::default()))),
        });
        assert!(validate_config(&json!([1, 2, 3]), &schema).valid);
        let res = validate_config(&json!([1, "oops"]), &schema);
        assert!(!res.valid);
        assert_eq!(res.errors[0].path, "[1]");
    }

    #[test]
    fn object_required_and_properties() {
        let mut props = BTreeMap::new();
        props.insert("name".into(), ConfigSchema::String(StringSchema::default()));
        props.insert("age".into(), ConfigSchema::Number(NumberSchema::default()));
        let schema = ConfigSchema::Object(ObjectSchema {
            properties: props,
            required: vec!["name".into()],
        });
        assert!(validate_config(&json!({"name": "bob"}), &schema).valid);
        let res = validate_config(&json!({"age": 42}), &schema);
        assert!(!res.valid);
        assert_eq!(res.errors[0].path, "name");
    }

    #[test]
    fn coerce_values() {
        let num = ConfigSchema::Number(NumberSchema::default());
        assert_eq!(coerce_config_value("42", &num).unwrap(), json!(42.0));
        let boolean = ConfigSchema::Boolean;
        assert_eq!(coerce_config_value("true", &boolean).unwrap(), json!(true));
        assert_eq!(coerce_config_value("1", &boolean).unwrap(), json!(true));
        assert_eq!(coerce_config_value("0", &boolean).unwrap(), json!(false));
        let arr = ConfigSchema::Array(ArraySchema::default());
        assert_eq!(coerce_config_value("[1,2]", &arr).unwrap(), json!([1, 2]));
    }

    #[test]
    fn schema_to_message_joins_errors() {
        let schema = ConfigSchema::Number(NumberSchema {
            min: Some(0.0),
            ..Default::default()
        });
        assert!(schema_to_message(&schema, &json!(5)).is_none());
        let msg = schema_to_message(&schema, &json!(-1)).unwrap();
        assert!(msg.contains("Too small"));
    }
}
