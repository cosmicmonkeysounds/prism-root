//! Modifiers — attachable behavior descriptors on any node.
//!
//! A modifier augments a node without changing its component type.
//! The render walker applies modifiers as wrapper layers around the
//! component's output (e.g., `ScrollOverflow` wraps in `Flickable`).
//!
//! `RenderSlintContext::apply_slint_modifiers` delegates per-modifier
//! wrapping to the [`Modifier::wrap_slint`] method defined here so
//! kind-specific markup lives in one place.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::component::RenderError;
use crate::registry::{prop_str, prop_u64, FieldSpec, NumericBounds, SelectOption};
use crate::slint_source::SlintEmitter;

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

impl Modifier {
    /// Wrap inner Slint output with the modifier's effect. Each kind
    /// emits a wrapper element whose body is the next modifier in the
    /// chain (or the component itself when the chain is exhausted).
    pub fn wrap_slint<F>(&self, out: &mut SlintEmitter, body: F) -> Result<(), RenderError>
    where
        F: FnOnce(&mut SlintEmitter) -> Result<(), RenderError>,
    {
        match self.kind {
            ModifierKind::ScrollOverflow => out.block("Flickable", body),
            ModifierKind::HoverEffect => self.wrap_slint_hover(out, body),
            ModifierKind::EnterAnimation => self.wrap_slint_enter(out, body),
            ModifierKind::ResponsiveVisibility => self.wrap_slint_responsive(out, body),
            ModifierKind::Tooltip => self.wrap_slint_tooltip(out, body),
            ModifierKind::AccessibilityOverride => self.wrap_slint_accessibility(out, body),
        }
    }

    fn wrap_slint_hover<F>(&self, out: &mut SlintEmitter, body: F) -> Result<(), RenderError>
    where
        F: FnOnce(&mut SlintEmitter) -> Result<(), RenderError>,
    {
        let effect = prop_str(&self.props, "effect", "fade");
        let duration = prop_u64(&self.props, "duration_ms", 200);
        // TouchArea exposes `has-hover`; an inner Rectangle animates a
        // visual property based on that signal.
        out.block("TouchArea", |out| {
            out.block("Rectangle", |out| {
                out.line("horizontal-stretch: 1;");
                out.line("vertical-stretch: 1;");
                match effect {
                    "scale" => {
                        out.line("transform-scale-x: parent.has-hover ? 1.05 : 1.0;");
                        out.line("transform-scale-y: parent.has-hover ? 1.05 : 1.0;");
                        out.line(format!(
                            "animate transform-scale-x, transform-scale-y {{ duration: {duration}ms; easing: ease-out; }}"
                        ));
                    }
                    "lift" => {
                        out.line("drop-shadow-color: #00000033;");
                        out.line("drop-shadow-blur: parent.has-hover ? 12px : 0px;");
                        out.line(format!(
                            "animate drop-shadow-blur {{ duration: {duration}ms; }}"
                        ));
                    }
                    "glow" => {
                        out.line("border-width: 2px;");
                        out.line(
                            "border-color: parent.has-hover ? #ffffffaa : #00000000;",
                        );
                        out.line(format!(
                            "animate border-color {{ duration: {duration}ms; }}"
                        ));
                    }
                    _ => {
                        // "fade" and any unknown value
                        out.line("opacity: parent.has-hover ? 0.85 : 1.0;");
                        out.line(format!(
                            "animate opacity {{ duration: {duration}ms; }}"
                        ));
                    }
                }
                body(out)
            })
        })
    }

    fn wrap_slint_enter<F>(&self, out: &mut SlintEmitter, body: F) -> Result<(), RenderError>
    where
        F: FnOnce(&mut SlintEmitter) -> Result<(), RenderError>,
    {
        let animation = prop_str(&self.props, "animation", "fade-in");
        let duration = prop_u64(&self.props, "duration_ms", 300);
        let delay = prop_u64(&self.props, "delay_ms", 0);

        out.block("Rectangle", |out| {
            out.line("horizontal-stretch: 1;");
            out.line("vertical-stretch: 1;");
            // `played` flips on init; the bound visual properties
            // animate from the not-played value to the played value
            // exactly once when the element first appears.
            out.line("property <bool> played: false;");
            out.line("init => { self.played = true; }");
            match animation {
                "slide-up" => {
                    out.line("y: played ? 0px : 20px;");
                    out.line(format!(
                        "animate y {{ duration: {duration}ms; delay: {delay}ms; easing: ease-out; }}"
                    ));
                    out.line("opacity: played ? 1.0 : 0.0;");
                    out.line(format!(
                        "animate opacity {{ duration: {duration}ms; delay: {delay}ms; }}"
                    ));
                }
                "slide-left" => {
                    out.line("x: played ? 0px : 20px;");
                    out.line(format!(
                        "animate x {{ duration: {duration}ms; delay: {delay}ms; easing: ease-out; }}"
                    ));
                    out.line("opacity: played ? 1.0 : 0.0;");
                    out.line(format!(
                        "animate opacity {{ duration: {duration}ms; delay: {delay}ms; }}"
                    ));
                }
                "scale-up" => {
                    out.line("transform-scale-x: played ? 1.0 : 0.9;");
                    out.line("transform-scale-y: played ? 1.0 : 0.9;");
                    out.line(format!(
                        "animate transform-scale-x, transform-scale-y {{ duration: {duration}ms; delay: {delay}ms; easing: ease-out; }}"
                    ));
                    out.line("opacity: played ? 1.0 : 0.0;");
                    out.line(format!(
                        "animate opacity {{ duration: {duration}ms; delay: {delay}ms; }}"
                    ));
                }
                _ => {
                    // "fade-in" and any unknown value
                    out.line("opacity: played ? 1.0 : 0.0;");
                    out.line(format!(
                        "animate opacity {{ duration: {duration}ms; delay: {delay}ms; easing: ease-out; }}"
                    ));
                }
            }
            body(out)
        })
    }

    fn wrap_slint_responsive<F>(&self, out: &mut SlintEmitter, body: F) -> Result<(), RenderError>
    where
        F: FnOnce(&mut SlintEmitter) -> Result<(), RenderError>,
    {
        // Defaults: visible everywhere unless explicitly hidden.
        let mobile = prop_bool_or(&self.props, "show_mobile", true);
        let tablet = prop_bool_or(&self.props, "show_tablet", true);
        let desktop = prop_bool_or(&self.props, "show_desktop", true);
        let visible = match (mobile, tablet, desktop) {
            (true, true, true) => "true".to_string(),
            (false, false, false) => "false".to_string(),
            _ => format!(
                "(parent.width < 640px ? {mobile} : (parent.width < 1024px ? {tablet} : {desktop}))"
            ),
        };
        out.block("Rectangle", |out| {
            out.line(format!("visible: {visible};"));
            out.line("horizontal-stretch: 1;");
            out.line("vertical-stretch: 1;");
            body(out)
        })
    }

    fn wrap_slint_tooltip<F>(&self, out: &mut SlintEmitter, body: F) -> Result<(), RenderError>
    where
        F: FnOnce(&mut SlintEmitter) -> Result<(), RenderError>,
    {
        let text = prop_str(&self.props, "text", "");
        out.block("Rectangle", |out| {
            out.line("horizontal-stretch: 1;");
            out.line("vertical-stretch: 1;");
            // Slint's accessible-description surfaces as the platform
            // tooltip / screen-reader hint. A first-class popup with
            // configurable placement is a future enhancement.
            if !text.is_empty() {
                out.prop_string("accessible-description", text);
            }
            body(out)
        })
    }

    fn wrap_slint_accessibility<F>(
        &self,
        out: &mut SlintEmitter,
        body: F,
    ) -> Result<(), RenderError>
    where
        F: FnOnce(&mut SlintEmitter) -> Result<(), RenderError>,
    {
        let label = prop_str(&self.props, "label", "");
        let description = prop_str(&self.props, "description", "");
        let hidden = prop_bool_or(&self.props, "hidden", false);
        out.block("Rectangle", |out| {
            out.line("horizontal-stretch: 1;");
            out.line("vertical-stretch: 1;");
            if !label.is_empty() {
                out.prop_string("accessible-label", label);
            }
            if !description.is_empty() {
                out.prop_string("accessible-description", description);
            }
            // Slint has no first-class `accessible-hidden`; collapse
            // visually so screen readers and pointer events both miss
            // it.
            if hidden {
                out.line("opacity: 0;");
            }
            body(out)
        })
    }
}

fn prop_bool_or(props: &Value, key: &str, default: bool) -> bool {
    props.get(key).and_then(|v| v.as_bool()).unwrap_or(default)
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

    fn run_slint(modifier: Modifier) -> String {
        let mut out = SlintEmitter::new();
        modifier
            .wrap_slint(&mut out, |out| {
                out.line("// inner");
                Ok(())
            })
            .unwrap();
        out.build()
    }

    #[test]
    fn slint_scroll_overflow_emits_flickable() {
        let s = run_slint(Modifier {
            kind: ModifierKind::ScrollOverflow,
            props: json!({}),
        });
        assert!(s.contains("Flickable"));
        assert!(s.contains("// inner"));
    }

    #[test]
    fn slint_hover_fade_animates_opacity() {
        let s = run_slint(Modifier {
            kind: ModifierKind::HoverEffect,
            props: json!({ "effect": "fade", "duration_ms": 250 }),
        });
        assert!(s.contains("TouchArea"));
        assert!(s.contains("opacity"));
        assert!(s.contains("animate opacity"));
        assert!(s.contains("250ms"));
    }

    #[test]
    fn slint_hover_scale_animates_transform() {
        let s = run_slint(Modifier {
            kind: ModifierKind::HoverEffect,
            props: json!({ "effect": "scale" }),
        });
        assert!(s.contains("transform-scale-x"));
        assert!(s.contains("animate transform-scale-x, transform-scale-y"));
    }

    #[test]
    fn slint_enter_fade_in_uses_played_property() {
        let s = run_slint(Modifier {
            kind: ModifierKind::EnterAnimation,
            props: json!({ "animation": "fade-in" }),
        });
        assert!(s.contains("property <bool> played"));
        assert!(s.contains("init => { self.played = true; }"));
        assert!(s.contains("opacity: played ?"));
    }

    #[test]
    fn slint_responsive_all_true_emits_constant() {
        let s = run_slint(Modifier {
            kind: ModifierKind::ResponsiveVisibility,
            props: json!({
                "show_mobile": true,
                "show_tablet": true,
                "show_desktop": true,
            }),
        });
        assert!(s.contains("visible: true"));
    }

    #[test]
    fn slint_responsive_mixed_emits_breakpoint_check() {
        let s = run_slint(Modifier {
            kind: ModifierKind::ResponsiveVisibility,
            props: json!({
                "show_mobile": false,
                "show_tablet": true,
                "show_desktop": true,
            }),
        });
        assert!(s.contains("parent.width < 640px"));
        assert!(s.contains("parent.width < 1024px"));
    }

    #[test]
    fn slint_tooltip_sets_accessible_description() {
        let s = run_slint(Modifier {
            kind: ModifierKind::Tooltip,
            props: json!({ "text": "Save document" }),
        });
        assert!(s.contains("accessible-description: \"Save document\""));
    }

    #[test]
    fn slint_accessibility_emits_label_and_hidden() {
        let s = run_slint(Modifier {
            kind: ModifierKind::AccessibilityOverride,
            props: json!({ "label": "Close", "hidden": true }),
        });
        assert!(s.contains("accessible-label: \"Close\""));
        assert!(s.contains("opacity: 0"));
    }
}
