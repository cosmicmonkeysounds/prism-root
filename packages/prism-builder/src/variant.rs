//! Variant system — named bundles of property overrides on a component.
//!
//! A component declares variant axes (e.g., "variant", "size") via
//! [`Component::variants()`]. Each axis has named options that carry
//! prop overrides. The render walker merges: base defaults -> variant
//! overrides -> instance props.

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VariantAxis {
    pub key: String,
    pub label: String,
    pub options: Vec<VariantOption>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VariantOption {
    pub value: String,
    pub label: String,
    #[serde(default)]
    pub overrides: Value,
}

/// Apply variant overrides to a props map. For each axis, if the props
/// contain a value matching one of the axis options, merge that option's
/// overrides into the result. Instance-level props take precedence over
/// variant overrides.
pub fn apply_variant_overrides(props: &Value, variants: &[VariantAxis]) -> Value {
    if variants.is_empty() {
        return props.clone();
    }

    let mut base = match props {
        Value::Object(map) => map.clone(),
        _ => return props.clone(),
    };

    for axis in variants {
        let selected = base.get(&axis.key).and_then(|v| v.as_str()).unwrap_or("");

        if let Some(option) = axis.options.iter().find(|o| o.value == selected) {
            if let Value::Object(overrides) = &option.overrides {
                for (k, v) in overrides {
                    if !base.contains_key(k) || k == &axis.key {
                        // Don't override explicitly set instance props,
                        // but always keep the axis selector itself.
                        continue;
                    }
                    base.insert(k.clone(), v.clone());
                }
            }
        }
    }

    Value::Object(base)
}

/// Like `apply_variant_overrides` but variant overrides fill in
/// missing props rather than overwriting them. Instance props always
/// win.
pub fn apply_variant_defaults(props: &Value, variants: &[VariantAxis]) -> Value {
    if variants.is_empty() {
        return props.clone();
    }

    let mut base = match props {
        Value::Object(map) => map.clone(),
        _ => return props.clone(),
    };

    for axis in variants {
        let selected = base.get(&axis.key).and_then(|v| v.as_str()).unwrap_or("");

        if let Some(option) = axis.options.iter().find(|o| o.value == selected) {
            if let Value::Object(overrides) = &option.overrides {
                for (k, v) in overrides {
                    base.entry(k.clone()).or_insert_with(|| v.clone());
                }
            }
        }
    }

    Value::Object(base)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn button_variants() -> Vec<VariantAxis> {
        vec![
            VariantAxis {
                key: "variant".into(),
                label: "Variant".into(),
                options: vec![
                    VariantOption {
                        value: "primary".into(),
                        label: "Primary".into(),
                        overrides: json!({ "bg": "#3b82f6", "color": "#ffffff" }),
                    },
                    VariantOption {
                        value: "danger".into(),
                        label: "Danger".into(),
                        overrides: json!({ "bg": "#ef4444", "color": "#ffffff" }),
                    },
                    VariantOption {
                        value: "ghost".into(),
                        label: "Ghost".into(),
                        overrides: json!({ "bg": "transparent", "color": "#d8dee9" }),
                    },
                ],
            },
            VariantAxis {
                key: "size".into(),
                label: "Size".into(),
                options: vec![
                    VariantOption {
                        value: "sm".into(),
                        label: "Small".into(),
                        overrides: json!({ "height": 28, "font-size": 12 }),
                    },
                    VariantOption {
                        value: "md".into(),
                        label: "Medium".into(),
                        overrides: json!({ "height": 36, "font-size": 14 }),
                    },
                    VariantOption {
                        value: "lg".into(),
                        label: "Large".into(),
                        overrides: json!({ "height": 44, "font-size": 16 }),
                    },
                ],
            },
        ]
    }

    #[test]
    fn applies_matching_variant() {
        let props = json!({ "variant": "danger", "text": "Delete" });
        let result = apply_variant_defaults(&props, &button_variants());
        assert_eq!(result["bg"], "#ef4444");
        assert_eq!(result["text"], "Delete");
    }

    #[test]
    fn instance_props_override_variant_defaults() {
        let props = json!({ "variant": "primary", "text": "Go", "bg": "#custom" });
        let result = apply_variant_defaults(&props, &button_variants());
        assert_eq!(result["bg"], "#custom");
    }

    #[test]
    fn multiple_axes_stack() {
        let props = json!({ "variant": "primary", "size": "lg", "text": "Big" });
        let result = apply_variant_defaults(&props, &button_variants());
        assert_eq!(result["bg"], "#3b82f6");
        assert_eq!(result["height"], 44);
    }

    #[test]
    fn unknown_variant_value_is_no_op() {
        let props = json!({ "variant": "unknown", "text": "X" });
        let result = apply_variant_defaults(&props, &button_variants());
        assert_eq!(result["text"], "X");
        assert!(result.get("bg").is_none());
    }

    #[test]
    fn empty_variants_is_passthrough() {
        let props = json!({ "text": "hello" });
        let result = apply_variant_defaults(&props, &[]);
        assert_eq!(result, props);
    }

    #[test]
    fn variant_axis_round_trips() {
        let axis = &button_variants()[0];
        let json = serde_json::to_string(axis).unwrap();
        let back: VariantAxis = serde_json::from_str(&json).unwrap();
        assert_eq!(back.key, "variant");
        assert_eq!(back.options.len(), 3);
    }
}
