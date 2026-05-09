//! `shell.drag-number-field` — 24px-tall horizontal-drag-to-edit
//! numeric scrubber. The pointer-drag delta increments / decrements
//! the value by `step`; double-click drops into a `TextInput` for
//! direct keyboard entry.
//!
//! Slint origin: `DragNumberField` in `ui/app.slint` (lines 568-641).
//!
//! Lowering: outer 24px container with a 3px radius and a hover-bg
//! swap (shows the user the row is interactive without painting any
//! resting chrome). Inner row carries the optional 11px label and the
//! current value, formatted to two decimal places matching the
//! original Slint version.
//!
//! The drag / edit / commit interactivity is pure host concern (input
//! dispatch + signal emission). The lowering is just visual structure
//! — exactly the split established by IconButton + NavButton.

use prism_builder::{
    common_signals,
    document::Node,
    registry::{FieldSpec, NumericBounds},
    signal::SignalDef,
    style::StyleProperties,
    ui_lower::{
        bare_container, colored_text_node, hover_bg, parse_color, prop_str, prop_string,
        uniform_radius, LowerCtx,
    },
    Block, ComponentId,
};
use prism_ui_runtime::layout::{Direction, Node as UiNode, Padding, Semantic, Sizing};
use serde_json::Value;

const FIELD_HEIGHT: f32 = 24.0;
const FIELD_RADIUS: f32 = 3.0;
const FIELD_RESTING_BG: &str = "#08000000";
const FIELD_HOVER_BG: &str = "#14000000";
const LABEL_COLOR: &str = "#99000000";
const VALUE_COLOR: &str = "#000000";
const LABEL_SIZE: f32 = 11.0;
const VALUE_SIZE: f32 = 11.0;

pub struct DragNumberField {
    pub id: ComponentId,
}

impl Block for DragNumberField {
    fn id(&self) -> &ComponentId {
        &self.id
    }

    fn schema(&self) -> Vec<FieldSpec> {
        vec![
            FieldSpec::text("key", "Key"),
            FieldSpec::text("label", "Label"),
            FieldSpec::number("value", "Value", NumericBounds::default())
                .with_default(Value::from(0.0)),
            FieldSpec::number("step", "Step", NumericBounds::min(0.0))
                .with_default(Value::from(1.0)),
            FieldSpec::number("min", "Minimum", NumericBounds::default())
                .with_default(Value::from(-99_999.0)),
            FieldSpec::number("max", "Maximum", NumericBounds::default())
                .with_default(Value::from(99_999.0)),
        ]
    }

    fn signals(&self) -> Vec<SignalDef> {
        let mut signals = common_signals();
        signals.push(
            SignalDef::new(
                "changed",
                "Drag updated the value — payload mirrors the (key, value) pair the original \
                 Slint callback emits on every drag tick.",
            )
            .with_payload(vec![
                FieldSpec::text("key", "Key"),
                FieldSpec::number("value", "Value", NumericBounds::default()),
            ]),
        );
        signals.push(
            SignalDef::new(
                "committed",
                "Inline edit accepted (Enter) — payload is the (key, raw text) the user typed.",
            )
            .with_payload(vec![
                FieldSpec::text("key", "Key"),
                FieldSpec::text("text", "Raw text"),
            ]),
        );
        signals
    }

    fn lower_ui(&self, _ctx: &LowerCtx<'_>, node: &Node, style: &StyleProperties) -> UiNode {
        let label = prop_str(node, "label");
        let value = node
            .props
            .get("value")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);
        let key = prop_string(node, "key");

        let mut row_children: Vec<UiNode> = Vec::with_capacity(2);
        if !label.is_empty() {
            row_children.push(colored_text_node(
                format!("{}::label", node.id),
                label.into(),
                style,
                LABEL_SIZE,
                LABEL_COLOR,
            ));
        }
        row_children.push(colored_text_node(
            format!("{}::value", node.id),
            format_value(value),
            style,
            VALUE_SIZE,
            VALUE_COLOR,
        ));

        let row = bare_container(format!("{}::row", node.id), row_children, |p| {
            p.direction = Direction::Row;
            p.gap = 4.0;
            p.padding = Padding {
                left: 6.0,
                right: 6.0,
                top: 0.0,
                bottom: 0.0,
            };
            p.height = Sizing::Grow;
        });

        bare_container(node.id.clone(), vec![row], |props| {
            props.height = Sizing::Fixed(FIELD_HEIGHT);
            props.radius = uniform_radius(FIELD_RADIUS);
            props.background = parse_color(FIELD_RESTING_BG);
            props.hover = hover_bg(FIELD_HOVER_BG);
            // SSR semantic: a labelled <input type="number"> wrapped by
            // <label> when a label prop is set. The void-tag walker
            // already handles the inner <input> as a self-closing tag,
            // and the outer <label> defers chrome to stylesheets.
            let mut semantic = Semantic::tag("label").with_attr("data-key", &key);
            if !label.is_empty() {
                semantic = semantic.with_attr("aria-label", label);
            }
            props.semantic = semantic;
        })
    }
}

/// Match the original Slint `Math.round(value * 100) / 100` formatting:
/// up to two decimals, trailing-zero / trailing-dot trimmed so integer
/// values render as `"3"` not `"3.00"`.
fn format_value(v: f64) -> String {
    let rounded = (v * 100.0).round() / 100.0;
    let raw = format!("{rounded:.2}");
    let trimmed = raw.trim_end_matches('0').trim_end_matches('.');
    if trimmed.is_empty() {
        "0".into()
    } else {
        trimmed.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use prism_builder::document::Node as BuilderNode;
    use prism_builder::layout::LayoutMode;
    use prism_builder::style::StyleProperties as Cascade;
    use prism_core::foundation::spatial::Transform2D;
    use serde_json::json;

    fn lower(node: &BuilderNode) -> UiNode {
        let block = DragNumberField {
            id: "shell.drag-number-field".into(),
        };
        let cascade = Cascade::default();
        let ctx = LowerCtx::new(None, &cascade);
        block.lower_ui(&ctx, node, &cascade)
    }

    fn field(props: Value) -> BuilderNode {
        BuilderNode {
            id: "f".into(),
            component: "shell.drag-number-field".into(),
            props,
            children: vec![],
            layout_mode: LayoutMode::default(),
            transform: Transform2D::default(),
            modifiers: vec![],
            style: Cascade::default(),
        }
    }

    #[test]
    fn lowers_to_24px_row_with_value_only_when_label_empty() {
        let ui = lower(&field(json!({ "key": "x", "value": 3.0 })));
        if let UiNode::Container {
            props, children, ..
        } = ui
        {
            assert_eq!(props.height, Sizing::Fixed(24.0));
            assert!(props.hover.is_some(), "hover-bg declared");
            // Single child: the row.
            assert_eq!(children.len(), 1);
            if let UiNode::Container {
                children: row_kids, ..
            } = &children[0]
            {
                assert_eq!(row_kids.len(), 1, "value-only row when label empty");
            } else {
                panic!("expected row container")
            }
        } else {
            panic!("not a container")
        }
    }

    #[test]
    fn label_prepends_when_set() {
        let ui = lower(&field(json!({ "key": "x", "label": "X", "value": 0.0 })));
        if let UiNode::Container { children, .. } = ui {
            if let UiNode::Container {
                children: row_kids, ..
            } = &children[0]
            {
                assert_eq!(row_kids.len(), 2, "label + value");
                if let UiNode::Text { content, .. } = &row_kids[0] {
                    assert_eq!(content, "X");
                } else {
                    panic!("expected label text")
                }
            } else {
                panic!("expected row container")
            }
        }
    }

    #[test]
    fn value_formats_with_at_most_two_decimals() {
        assert_eq!(format_value(3.0), "3");
        assert_eq!(format_value(3.1), "3.1");
        assert_eq!(format_value(3.12), "3.12");
        assert_eq!(format_value(3.129), "3.13");
        assert_eq!(format_value(0.0), "0");
        assert_eq!(format_value(-2.5), "-2.5");
    }

    #[test]
    fn semantic_carries_data_key_and_aria_label() {
        let ui = lower(&field(
            json!({ "key": "rotation", "label": "Rot", "value": 0.0 }),
        ));
        if let UiNode::Container { props, .. } = ui {
            assert_eq!(props.semantic.tag.as_deref(), Some("label"));
            assert!(props
                .semantic
                .attrs
                .iter()
                .any(|(k, v)| k == "data-key" && v == "rotation"));
            assert!(props
                .semantic
                .attrs
                .iter()
                .any(|(k, v)| k == "aria-label" && v == "Rot"));
        }
    }

    #[test]
    fn schema_declares_six_fields() {
        let block = DragNumberField {
            id: "shell.drag-number-field".into(),
        };
        let keys: Vec<String> = block.schema().into_iter().map(|f| f.key).collect();
        assert_eq!(keys, vec!["key", "label", "value", "step", "min", "max"]);
    }

    #[test]
    fn signals_include_changed_and_committed() {
        let block = DragNumberField {
            id: "shell.drag-number-field".into(),
        };
        let names: Vec<String> = block.signals().into_iter().map(|s| s.name).collect();
        assert!(names.contains(&"changed".into()));
        assert!(names.contains(&"committed".into()));
    }
}
