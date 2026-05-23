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

/// Built-in component variant axes. One source of truth feeds the
/// unified `lower_ui` walk that both the live `prism-ui-runtime`
/// renderer and the relay `lower_semantic_html` SSR consume —
/// `apply_variant_defaults` is called once on the shared path.
pub mod presets {
    use super::{VariantAxis, VariantOption};
    use serde_json::json;

    pub fn button() -> Vec<VariantAxis> {
        vec![VariantAxis {
            key: "variant".into(),
            label: "Variant".into(),
            options: vec![
                VariantOption {
                    value: "primary".into(),
                    label: "Primary".into(),
                    overrides: json!({ "bg": "#3b82f6", "color": "#ffffff" }),
                },
                VariantOption {
                    value: "secondary".into(),
                    label: "Secondary".into(),
                    overrides: json!({ "bg": "#4b5563", "color": "#ffffff" }),
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
        }]
    }

    pub fn input() -> Vec<VariantAxis> {
        vec![VariantAxis {
            key: "state".into(),
            label: "State".into(),
            options: vec![
                VariantOption {
                    value: "default".into(),
                    label: "Default".into(),
                    overrides: json!({ "border_color": "#3b4252" }),
                },
                VariantOption {
                    value: "error".into(),
                    label: "Error".into(),
                    overrides: json!({ "border_color": "#ef4444", "bg": "#fef2f2" }),
                },
                VariantOption {
                    value: "success".into(),
                    label: "Success".into(),
                    overrides: json!({ "border_color": "#10b981", "bg": "#f0fdf4" }),
                },
            ],
        }]
    }

    pub fn container() -> Vec<VariantAxis> {
        vec![VariantAxis {
            key: "style".into(),
            label: "Style".into(),
            options: vec![
                VariantOption {
                    value: "none".into(),
                    label: "None".into(),
                    overrides: json!({}),
                },
                VariantOption {
                    value: "card".into(),
                    label: "Card".into(),
                    overrides: json!({ "padding": 16, "border_width": 0 }),
                },
                VariantOption {
                    value: "outlined".into(),
                    label: "Outlined".into(),
                    overrides: json!({
                        "padding": 16,
                        "border_width": 1,
                        "border_color": "#3b4252"
                    }),
                },
                VariantOption {
                    value: "elevated".into(),
                    label: "Elevated".into(),
                    overrides: json!({ "padding": 20, "border_width": 0 }),
                },
            ],
        }]
    }

    pub fn tabs() -> Vec<VariantAxis> {
        vec![VariantAxis {
            key: "style".into(),
            label: "Style".into(),
            options: vec![
                VariantOption {
                    value: "underline".into(),
                    label: "Underline".into(),
                    overrides: json!({}),
                },
                VariantOption {
                    value: "pill".into(),
                    label: "Pill".into(),
                    overrides: json!({}),
                },
                VariantOption {
                    value: "segmented".into(),
                    label: "Segmented".into(),
                    overrides: json!({}),
                },
            ],
        }]
    }

    pub fn table() -> Vec<VariantAxis> {
        vec![VariantAxis {
            key: "density".into(),
            label: "Density".into(),
            options: vec![
                VariantOption {
                    value: "compact".into(),
                    label: "Compact".into(),
                    overrides: json!({ "row_padding": 4 }),
                },
                VariantOption {
                    value: "normal".into(),
                    label: "Normal".into(),
                    overrides: json!({ "row_padding": 8 }),
                },
                VariantOption {
                    value: "spacious".into(),
                    label: "Spacious".into(),
                    overrides: json!({ "row_padding": 14 }),
                },
            ],
        }]
    }

    pub fn image() -> Vec<VariantAxis> {
        vec![VariantAxis {
            key: "shape".into(),
            label: "Shape".into(),
            options: vec![
                VariantOption {
                    value: "square".into(),
                    label: "Square".into(),
                    overrides: json!({ "border_radius": 0 }),
                },
                VariantOption {
                    value: "rounded".into(),
                    label: "Rounded".into(),
                    overrides: json!({ "border_radius": 8 }),
                },
                VariantOption {
                    value: "circle".into(),
                    label: "Circle".into(),
                    overrides: json!({ "border_radius": 9999 }),
                },
            ],
        }]
    }

    pub fn code() -> Vec<VariantAxis> {
        vec![VariantAxis {
            key: "theme".into(),
            label: "Theme".into(),
            options: vec![
                VariantOption {
                    value: "dark".into(),
                    label: "Dark".into(),
                    overrides: json!({ "bg": "#1a1e28", "color": "#a3be8c" }),
                },
                VariantOption {
                    value: "light".into(),
                    label: "Light".into(),
                    overrides: json!({ "bg": "#f8f9fa", "color": "#2e3440" }),
                },
            ],
        }]
    }

    pub fn columns() -> Vec<VariantAxis> {
        vec![VariantAxis {
            key: "spacing".into(),
            label: "Spacing".into(),
            options: vec![
                VariantOption {
                    value: "compact".into(),
                    label: "Compact".into(),
                    overrides: json!({ "gap": 8 }),
                },
                VariantOption {
                    value: "normal".into(),
                    label: "Normal".into(),
                    overrides: json!({ "gap": 16 }),
                },
                VariantOption {
                    value: "wide".into(),
                    label: "Wide".into(),
                    overrides: json!({ "gap": 32 }),
                },
            ],
        }]
    }

    pub fn list() -> Vec<VariantAxis> {
        vec![VariantAxis {
            key: "density".into(),
            label: "Density".into(),
            options: vec![
                VariantOption {
                    value: "compact".into(),
                    label: "Compact".into(),
                    overrides: json!({ "item_spacing": 2 }),
                },
                VariantOption {
                    value: "normal".into(),
                    label: "Normal".into(),
                    overrides: json!({ "item_spacing": 4 }),
                },
                VariantOption {
                    value: "spacious".into(),
                    label: "Spacious".into(),
                    overrides: json!({ "item_spacing": 10 }),
                },
            ],
        }]
    }

    pub fn accordion() -> Vec<VariantAxis> {
        vec![VariantAxis {
            key: "style".into(),
            label: "Style".into(),
            options: vec![
                VariantOption {
                    value: "bordered".into(),
                    label: "Bordered".into(),
                    overrides: json!({ "border_width": 1, "border_color": "#3b4252" }),
                },
                VariantOption {
                    value: "flush".into(),
                    label: "Flush".into(),
                    overrides: json!({ "border_width": 0 }),
                },
                VariantOption {
                    value: "separated".into(),
                    label: "Separated".into(),
                    overrides: json!({ "border_width": 0, "section_gap": 8 }),
                },
            ],
        }]
    }
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

    #[test]
    fn presets_match_expected_axis_keys() {
        assert_eq!(presets::button()[0].key, "variant");
        assert_eq!(presets::input()[0].key, "state");
        assert_eq!(presets::container()[0].key, "style");
        assert_eq!(presets::tabs()[0].key, "style");
        assert_eq!(presets::table()[0].key, "density");
        assert_eq!(presets::image()[0].key, "shape");
        assert_eq!(presets::code()[0].key, "theme");
        assert_eq!(presets::columns()[0].key, "spacing");
        assert_eq!(presets::list()[0].key, "density");
        assert_eq!(presets::accordion()[0].key, "style");
    }

    #[test]
    fn container_card_preset_applies_padding_default() {
        let props = json!({ "style": "card" });
        let result = apply_variant_defaults(&props, &presets::container());
        assert_eq!(result["padding"], 16);
        assert_eq!(result["border_width"], 0);
    }

    #[test]
    fn input_error_preset_applies_red_border() {
        let props = json!({ "state": "error" });
        let result = apply_variant_defaults(&props, &presets::input());
        assert_eq!(result["border_color"], "#ef4444");
    }

    #[test]
    fn image_circle_preset_applies_max_radius() {
        let props = json!({ "shape": "circle" });
        let result = apply_variant_defaults(&props, &presets::image());
        assert_eq!(result["border_radius"], 9999);
    }

    #[test]
    fn code_light_preset_applies_colors() {
        let props = json!({ "theme": "light" });
        let result = apply_variant_defaults(&props, &presets::code());
        assert_eq!(result["bg"], "#f8f9fa");
        assert_eq!(result["color"], "#2e3440");
    }

    #[test]
    fn columns_wide_preset_applies_gap() {
        let props = json!({ "spacing": "wide" });
        let result = apply_variant_defaults(&props, &presets::columns());
        assert_eq!(result["gap"], 32);
    }

    #[test]
    fn list_spacious_preset_applies_item_spacing() {
        let props = json!({ "density": "spacious" });
        let result = apply_variant_defaults(&props, &presets::list());
        assert_eq!(result["item_spacing"], 10);
    }

    #[test]
    fn accordion_bordered_preset_applies_border() {
        let props = json!({ "style": "bordered" });
        let result = apply_variant_defaults(&props, &presets::accordion());
        assert_eq!(result["border_width"], 1);
        assert_eq!(result["border_color"], "#3b4252");
    }
}
