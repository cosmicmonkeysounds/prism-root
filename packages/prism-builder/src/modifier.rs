//! Modifiers — attachable behavior descriptors on any node.
//!
//! A modifier augments a node without changing its component type.
//! The render walker applies modifiers as wrapper layers around the
//! component's output (e.g., `ScrollOverflow` wraps in `Flickable`).

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::registry::{FieldSpec, NumericBounds, SelectOption};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ModifierKind {
    ScrollOverflow,
    HoverEffect,
    EnterAnimation,
    ResponsiveVisibility,
    Tooltip,
    AccessibilityOverride,
}

impl ModifierKind {
    pub const ALL: &'static [ModifierKind] = &[
        ModifierKind::ScrollOverflow,
        ModifierKind::HoverEffect,
        ModifierKind::EnterAnimation,
        ModifierKind::ResponsiveVisibility,
        ModifierKind::Tooltip,
        ModifierKind::AccessibilityOverride,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::ScrollOverflow => "Scroll Overflow",
            Self::HoverEffect => "Hover Effect",
            Self::EnterAnimation => "Enter Animation",
            Self::ResponsiveVisibility => "Responsive Visibility",
            Self::Tooltip => "Tooltip",
            Self::AccessibilityOverride => "Accessibility Override",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Self::ScrollOverflow => {
                "Makes content scrollable when it exceeds the container bounds."
            }
            Self::HoverEffect => "Applies a visual effect when the user hovers over the element.",
            Self::EnterAnimation => "Animates the element when it first appears in the viewport.",
            Self::ResponsiveVisibility => "Controls visibility at different viewport breakpoints.",
            Self::Tooltip => "Shows a tooltip on hover with configurable text and placement.",
            Self::AccessibilityOverride => "Overrides ARIA attributes for assistive technology.",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Modifier {
    pub kind: ModifierKind,
    #[serde(default)]
    pub props: Value,
}

pub fn modifier_schema(kind: ModifierKind) -> Vec<FieldSpec> {
    match kind {
        ModifierKind::ScrollOverflow => vec![FieldSpec::select(
            "direction",
            "Direction",
            vec![
                SelectOption::new("vertical", "Vertical"),
                SelectOption::new("horizontal", "Horizontal"),
                SelectOption::new("both", "Both"),
            ],
        )],
        ModifierKind::HoverEffect => vec![
            FieldSpec::select(
                "effect",
                "Effect",
                vec![
                    SelectOption::new("scale", "Scale up"),
                    SelectOption::new("fade", "Fade"),
                    SelectOption::new("lift", "Lift (shadow)"),
                    SelectOption::new("glow", "Glow"),
                ],
            ),
            FieldSpec::number(
                "duration_ms",
                "Duration (ms)",
                NumericBounds::min_max(50.0, 2000.0),
            )
            .with_default(Value::from(200)),
        ],
        ModifierKind::EnterAnimation => vec![
            FieldSpec::select(
                "animation",
                "Animation",
                vec![
                    SelectOption::new("fade-in", "Fade in"),
                    SelectOption::new("slide-up", "Slide up"),
                    SelectOption::new("slide-left", "Slide left"),
                    SelectOption::new("scale-up", "Scale up"),
                ],
            ),
            FieldSpec::number(
                "duration_ms",
                "Duration (ms)",
                NumericBounds::min_max(50.0, 3000.0),
            )
            .with_default(Value::from(300)),
            FieldSpec::number(
                "delay_ms",
                "Delay (ms)",
                NumericBounds::min_max(0.0, 5000.0),
            )
            .with_default(Value::from(0)),
        ],
        ModifierKind::ResponsiveVisibility => vec![
            FieldSpec::boolean("show_mobile", "Show on mobile (<640px)"),
            FieldSpec::boolean("show_tablet", "Show on tablet (640\u{2013}1024px)"),
            FieldSpec::boolean("show_desktop", "Show on desktop (>1024px)"),
        ],
        ModifierKind::Tooltip => vec![
            FieldSpec::text("text", "Tooltip text").required(),
            FieldSpec::select(
                "placement",
                "Placement",
                vec![
                    SelectOption::new("top", "Top"),
                    SelectOption::new("bottom", "Bottom"),
                    SelectOption::new("left", "Left"),
                    SelectOption::new("right", "Right"),
                ],
            ),
        ],
        ModifierKind::AccessibilityOverride => vec![
            FieldSpec::text("role", "ARIA role"),
            FieldSpec::text("label", "ARIA label"),
            FieldSpec::text("description", "ARIA description"),
            FieldSpec::boolean("hidden", "ARIA hidden"),
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn modifier_round_trips_through_serde() {
        let m = Modifier {
            kind: ModifierKind::ScrollOverflow,
            props: json!({ "direction": "vertical" }),
        };
        let json = serde_json::to_string(&m).unwrap();
        let back: Modifier = serde_json::from_str(&json).unwrap();
        assert_eq!(back.kind, ModifierKind::ScrollOverflow);
    }

    #[test]
    fn all_kinds_have_labels() {
        for kind in ModifierKind::ALL {
            assert!(!kind.label().is_empty());
            assert!(!kind.description().is_empty());
        }
    }

    #[test]
    fn all_kinds_have_schemas() {
        for kind in ModifierKind::ALL {
            let _schema = modifier_schema(*kind);
        }
    }

    #[test]
    fn scroll_overflow_schema_has_direction() {
        let schema = modifier_schema(ModifierKind::ScrollOverflow);
        assert_eq!(schema.len(), 1);
        assert_eq!(schema[0].key, "direction");
    }

    #[test]
    fn hover_effect_schema_has_duration() {
        let schema = modifier_schema(ModifierKind::HoverEffect);
        assert!(schema.iter().any(|f| f.key == "duration_ms"));
    }

    #[test]
    fn responsive_visibility_schema_has_three_breakpoints() {
        let schema = modifier_schema(ModifierKind::ResponsiveVisibility);
        assert_eq!(schema.len(), 3);
    }
}
