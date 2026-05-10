//! `shell.transform-editor` — Godot-style Position / Rotation / Scale /
//! Anchor stack used in the right-sidebar properties panel.
//!
//! Slint origin: `TransformEditor` in `ui/app.slint` (lines 643-817).
//!
//! Smart pattern: a single declarative `ROW_SPECS` table drives the
//! whole layout. Each row is a `RowSpec { label, fields }` where
//! `fields` is a slice of `FieldSpec { axis_label, axis_color,
//! key_suffix, value_prop }` — e.g. the Position row carries `x` and
//! `y`, the Rotation row a single unlabelled field, the Scale row two
//! labelled fields. Adding a row (Skew, Pivot, …) is one struct
//! literal at the top of the file; the lowering body never branches
//! on row identity. Each numeric field flows through the shared
//! [`super::chrome::drag_number_field_node`] helper so the visual
//! recipe lives exactly once.

use prism_builder::{
    document::Node,
    registry::FieldSpec,
    signal::SignalDef,
    style::StyleProperties,
    ui_lower::{
        bare_container, colored_text_node, hover_bg, parse_color, prop_str, uniform_radius,
        LowerCtx,
    },
    with_common_signals,
};
use prism_ui_runtime::layout::{Direction, Node as UiNode, Padding, Semantic, Sizing};
use serde_json::Value;

use super::chrome::{
    color_or_transparent, drag_number_field_node, format_drag_value, DRAG_NUMBER_LABEL_COLOR,
};

/// Per-axis field within a row. `axis_label` is the optional letter
/// painted in `axis_color` left of the value (e.g. red `x` / green `y`).
struct AxisSpec {
    axis_label: &'static str,
    axis_color: &'static str,
    key_suffix: &'static str,
    value_prop: &'static str,
}

struct RowSpec {
    label: &'static str,
    fields: &'static [AxisSpec],
}

const ROW_SPECS: &[RowSpec] = &[
    RowSpec {
        label: "Position",
        fields: &[
            AxisSpec {
                axis_label: "x",
                axis_color: "#e06060",
                key_suffix: "transform.position.0",
                value_prop: "pos-x",
            },
            AxisSpec {
                axis_label: "y",
                axis_color: "#60c060",
                key_suffix: "transform.position.1",
                value_prop: "pos-y",
            },
        ],
    },
    RowSpec {
        label: "Rotation",
        fields: &[AxisSpec {
            axis_label: "",
            axis_color: DRAG_NUMBER_LABEL_COLOR,
            key_suffix: "transform.rotation",
            value_prop: "rotation",
        }],
    },
    RowSpec {
        label: "Scale",
        fields: &[
            AxisSpec {
                axis_label: "x",
                axis_color: "#e06060",
                key_suffix: "transform.scale.0",
                value_prop: "scale-x",
            },
            AxisSpec {
                axis_label: "y",
                axis_color: "#60c060",
                key_suffix: "transform.scale.1",
                value_prop: "scale-y",
            },
        ],
    },
];

const LABEL_CELL_WIDTH: f32 = 52.0;
const LABEL_FONT_SIZE: f32 = 10.0;
const LABEL_COLOR: &str = "#99000000";
const ROW_GAP: f32 = 4.0;
const STACK_GAP: f32 = 2.0;
const ANCHOR_HEIGHT: f32 = 24.0;
const ANCHOR_RADIUS: f32 = 3.0;
const ANCHOR_BG: &str = "#08000000";
const ANCHOR_HOVER_BG: &str = "#14000000";

fn transform_editor_schema() -> Vec<FieldSpec> {
    vec![
        FieldSpec::number("pos-x", "Position X", Default::default()).with_default(0.0.into()),
        FieldSpec::number("pos-y", "Position Y", Default::default()).with_default(0.0.into()),
        FieldSpec::number("rotation", "Rotation", Default::default()).with_default(0.0.into()),
        FieldSpec::number("scale-x", "Scale X", Default::default()).with_default(1.0.into()),
        FieldSpec::number("scale-y", "Scale Y", Default::default()).with_default(1.0.into()),
        FieldSpec::text("anchor", "Anchor").with_default(Value::from("top-left")),
    ]
}

fn transform_editor_signals() -> Vec<prism_builder::signal::SignalDef> {
    with_common_signals(vec![
        SignalDef::new(
            "field-edited",
            "Anchor / committed-text edits — payload mirrors the Slint (key, text) callback.",
        ),
        SignalDef::new(
            "field-edited-number",
            "Numeric drag tick — payload mirrors the Slint (key, value) callback.",
        ),
    ])
}

fn transform_editor_lower(_ctx: &LowerCtx<'_>, node: &Node, _style: &StyleProperties) -> UiNode {
    let mut rows: Vec<UiNode> = Vec::with_capacity(ROW_SPECS.len() + 1);
    for row_spec in ROW_SPECS {
        rows.push(build_row(node, row_spec));
    }
    rows.push(build_anchor_row(node));

    bare_container(node.id.clone(), rows, |props| {
        props.direction = Direction::Column;
        props.gap = STACK_GAP;
        props.padding = Padding {
            left: 4.0,
            right: 4.0,
            top: 4.0,
            bottom: 4.0,
        };
        props.semantic = Semantic::tag("section").with_attr("data-role", "transform-editor");
    })
}

pub const TRANSFORM_EDITOR_SPEC: prism_builder::BlockSpec =
    prism_builder::BlockSpec::new("shell.transform-editor", transform_editor_schema)
        .lower(transform_editor_lower)
        .signals(transform_editor_signals);

fn build_row(node: &Node, row: &RowSpec) -> UiNode {
    let style = StyleProperties::default();
    let label = colored_text_node(
        format!("{}::row::{}::label", node.id, row.label),
        row.label.into(),
        &style,
        LABEL_FONT_SIZE,
        LABEL_COLOR,
    );

    let mut children: Vec<UiNode> = Vec::with_capacity(row.fields.len() + 1);
    children.push(bare_container(
        format!("{}::row::{}::label-cell", node.id, row.label),
        vec![label],
        |p| {
            p.width = Sizing::Fixed(LABEL_CELL_WIDTH);
            p.height = Sizing::Grow;
        },
    ));
    for f in row.fields {
        let value = node
            .props
            .get(f.value_prop)
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);
        children.push(drag_number_field_node(
            format!("{}::field::{}", node.id, f.key_suffix),
            f.axis_label,
            f.axis_color,
            format_drag_value(value),
            f.key_suffix,
        ));
    }

    bare_container(format!("{}::row::{}", node.id, row.label), children, |p| {
        p.direction = Direction::Row;
        p.gap = ROW_GAP;
    })
}

fn build_anchor_row(node: &Node) -> UiNode {
    let anchor_value = prop_str(node, "anchor");
    let style = StyleProperties::default();
    let label = colored_text_node(
        format!("{}::anchor-label", node.id),
        "Anchor".into(),
        &style,
        LABEL_FONT_SIZE,
        LABEL_COLOR,
    );
    let label_cell = bare_container(
        format!("{}::anchor::label-cell", node.id),
        vec![label],
        |p| {
            p.width = Sizing::Fixed(LABEL_CELL_WIDTH);
            p.height = Sizing::Grow;
        },
    );

    let value_text = colored_text_node(
        format!("{}::anchor::value", node.id),
        anchor_value.into(),
        &style,
        11.0,
        "#000000",
    );
    let pill = bare_container(
        format!("{}::anchor::pill", node.id),
        vec![value_text],
        |p| {
            p.height = Sizing::Fixed(ANCHOR_HEIGHT);
            p.padding = Padding {
                left: 6.0,
                right: 6.0,
                top: 0.0,
                bottom: 0.0,
            };
            p.radius = uniform_radius(ANCHOR_RADIUS);
            p.background = parse_color(ANCHOR_BG).or(Some(color_or_transparent(ANCHOR_BG)));
            p.hover = hover_bg(ANCHOR_HOVER_BG);
            p.semantic = Semantic::tag("button")
                .with_attr("type", "button")
                .with_attr("data-role", "anchor-picker");
        },
    );

    bare_container(
        format!("{}::anchor", node.id),
        vec![label_cell, pill],
        |p| {
            p.direction = Direction::Row;
            p.gap = ROW_GAP;
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::testing::{lower_with, test_node};
    use prism_builder::document::Node as BuilderNode;
    use prism_builder::Block;
    use serde_json::json;

    fn node(props: Value) -> BuilderNode {
        test_node("te", "shell.transform-editor", props)
    }

    fn lower(n: &BuilderNode) -> UiNode {
        lower_with(n, transform_editor_lower)
    }

    #[test]
    fn lowers_to_four_rows_in_a_column() {
        let ui = lower(&node(json!({ "anchor": "center" })));
        let UiNode::Container {
            children, props, ..
        } = ui
        else {
            panic!("not a container")
        };
        assert_eq!(props.direction, Direction::Column);
        assert_eq!(children.len(), 4, "Position + Rotation + Scale + Anchor");
    }

    #[test]
    fn position_row_has_two_drag_fields() {
        let ui = lower(&node(json!({ "pos-x": 12.0, "pos-y": 34.0 })));
        let UiNode::Container { children, .. } = ui else {
            panic!()
        };
        let UiNode::Container {
            children: pos_kids, ..
        } = &children[0]
        else {
            panic!("position row not a container")
        };
        // label-cell + 2 drag fields
        assert_eq!(pos_kids.len(), 3);
    }

    #[test]
    fn rotation_row_has_single_unlabelled_field() {
        let ui = lower(&node(json!({ "rotation": 45.0 })));
        let UiNode::Container { children, .. } = ui else {
            panic!()
        };
        let UiNode::Container {
            children: rot_kids, ..
        } = &children[1]
        else {
            panic!()
        };
        assert_eq!(rot_kids.len(), 2, "label-cell + single drag field");
    }

    #[test]
    fn anchor_row_paints_value_pill() {
        let ui = lower(&node(json!({ "anchor": "center-left" })));
        let UiNode::Container { children, .. } = ui else {
            panic!()
        };
        let UiNode::Container {
            children: anchor_kids,
            ..
        } = &children[3]
        else {
            panic!()
        };
        // label-cell + pill
        assert_eq!(anchor_kids.len(), 2);
        let UiNode::Container { props, .. } = &anchor_kids[1] else {
            panic!("pill not a container")
        };
        assert_eq!(props.height, Sizing::Fixed(ANCHOR_HEIGHT));
        assert_eq!(props.semantic.tag.as_deref(), Some("button"));
    }

    #[test]
    fn semantic_section_attr() {
        let ui = lower(&node(json!({})));
        let UiNode::Container { props, .. } = ui else {
            panic!()
        };
        assert_eq!(props.semantic.tag.as_deref(), Some("section"));
        assert!(props
            .semantic
            .attrs
            .iter()
            .any(|(k, v)| k == "data-role" && v == "transform-editor"));
    }

    #[test]
    fn schema_declares_six_fields() {
        let block = prism_builder::SpecBlock::new(&super::TRANSFORM_EDITOR_SPEC);
        let keys: Vec<String> = block.schema().into_iter().map(|f| f.key).collect();
        assert_eq!(
            keys,
            vec!["pos-x", "pos-y", "rotation", "scale-x", "scale-y", "anchor"]
        );
    }
}
