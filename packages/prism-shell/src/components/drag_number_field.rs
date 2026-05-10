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
    document::Node,
    registry::{FieldSpec, NumericBounds},
    signal::SignalDef,
    style::StyleProperties,
    ui_lower::{prop_str, prop_string, LowerCtx},
    with_common_signals,
};
use prism_ui_runtime::layout::Node as UiNode;
use serde_json::Value;

use super::chrome::{drag_number_field_node, format_drag_value, DRAG_NUMBER_LABEL_COLOR};

fn drag_number_field_schema() -> Vec<FieldSpec> {
    vec![
        FieldSpec::text("key", "Key"),
        FieldSpec::text("label", "Label"),
        FieldSpec::number("value", "Value", NumericBounds::default())
            .with_default(Value::from(0.0)),
        FieldSpec::number("step", "Step", NumericBounds::min(0.0)).with_default(Value::from(1.0)),
        FieldSpec::number("min", "Minimum", NumericBounds::default())
            .with_default(Value::from(-99_999.0)),
        FieldSpec::number("max", "Maximum", NumericBounds::default())
            .with_default(Value::from(99_999.0)),
    ]
}

fn drag_number_field_signals() -> Vec<prism_builder::signal::SignalDef> {
    with_common_signals(vec![
        SignalDef::new(
            "changed",
            "Drag updated the value — payload mirrors the (key, value) pair the original \
                 Slint callback emits on every drag tick.",
        )
        .with_payload(vec![
            FieldSpec::text("key", "Key"),
            FieldSpec::number("value", "Value", NumericBounds::default()),
        ]),
        SignalDef::new(
            "committed",
            "Inline edit accepted (Enter) — payload is the (key, raw text) the user typed.",
        )
        .with_payload(vec![
            FieldSpec::text("key", "Key"),
            FieldSpec::text("text", "Raw text"),
        ]),
    ])
}

fn drag_number_field_lower(_ctx: &LowerCtx<'_>, node: &Node, _style: &StyleProperties) -> UiNode {
    let label = prop_str(node, "label");
    let value = node
        .props
        .get("value")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    let key = prop_string(node, "key");
    drag_number_field_node(
        node.id.clone(),
        label,
        DRAG_NUMBER_LABEL_COLOR,
        format_drag_value(value),
        &key,
    )
}

pub const DRAG_NUMBER_FIELD_SPEC: prism_builder::BlockSpec =
    prism_builder::BlockSpec::new("shell.drag-number-field", drag_number_field_schema)
        .lower(drag_number_field_lower)
        .signals(drag_number_field_signals);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::testing::{lower_with, test_node};
    use prism_builder::document::Node as BuilderNode;
    use prism_builder::Block;
    use prism_ui_runtime::layout::Sizing;
    use serde_json::json;

    fn lower(node: &BuilderNode) -> UiNode {
        lower_with(node, drag_number_field_lower)
    }

    fn field(props: Value) -> BuilderNode {
        test_node("f", "shell.drag-number-field", props)
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
        assert_eq!(format_drag_value(3.0), "3");
        assert_eq!(format_drag_value(3.1), "3.1");
        assert_eq!(format_drag_value(3.12), "3.12");
        assert_eq!(format_drag_value(3.129), "3.13");
        assert_eq!(format_drag_value(0.0), "0");
        assert_eq!(format_drag_value(-2.5), "-2.5");
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
        let block = prism_builder::SpecBlock::new(&super::DRAG_NUMBER_FIELD_SPEC);
        let keys: Vec<String> = block.schema().into_iter().map(|f| f.key).collect();
        assert_eq!(keys, vec!["key", "label", "value", "step", "min", "max"]);
    }

    #[test]
    fn signals_include_changed_and_committed() {
        let block = prism_builder::SpecBlock::new(&super::DRAG_NUMBER_FIELD_SPEC);
        let names: Vec<String> = block.signals().into_iter().map(|s| s.name).collect();
        assert!(names.contains(&"changed".into()));
        assert!(names.contains(&"committed".into()));
    }
}
