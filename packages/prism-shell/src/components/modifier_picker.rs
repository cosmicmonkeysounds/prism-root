//! `shell.modifier-picker` — overlay listing every behaviour the
//! `ModifierRegistry` knows about, minus the ones already attached
//! to the selected node. Click a row → attach the modifier and
//! close the overlay.
//!
//! See `docs/dev/composable-builder-plan.md` Wave 1.4.
//!
//! Props:
//! - `open`        bool — when false the picker collapses to the §43-A2 hidden-overlay shape.
//! - `target-id`   string — owning node id; carried as `data-target-id`.
//! - `options`     JSON array of `{id, label, description}` — the
//!   filtered registry surface.
//!
//! Each option row carries `data-role="modifier-picker-select"` +
//! `data-target-id` + `data-modifier-id` for the §43-style hit-test
//! router (see `events::POINTER_ROUTES`).

use prism_builder::{
    document::Node,
    registry::FieldSpec,
    style::StyleProperties,
    ui_lower::{
        bare_container, colored_text_node, hover_bg, parse_color, uniform_radius, LowerCtx,
    },
};
use prism_ui_runtime::layout::{Direction, Node as UiNode, Padding, Semantic, Sizing};
use serde_json::Value;

use crate::components::chrome::hidden_overlay;

const POPUP_WIDTH: f32 = 280.0;
const POPUP_BG: &str = "#f0ffffff";
const POPUP_RADIUS: f32 = 8.0;
const ROW_HEIGHT: f32 = 40.0;
const ROW_HOVER: &str = "#0f000000";
const ROW_RADIUS: f32 = 4.0;
const LABEL_COLOR: &str = "#000000";
const DESC_COLOR: &str = "#7f000000";
const LABEL_FONT: f32 = 12.0;
const DESC_FONT: f32 = 10.0;

fn modifier_picker_schema() -> Vec<FieldSpec> {
    vec![
        FieldSpec::boolean("open", "Open").with_default(Value::Bool(false)),
        FieldSpec::text("target-id", "Owning node id"),
        FieldSpec::text(
            "options",
            "Options (JSON array of {id, label, description})",
        ),
    ]
}

fn modifier_picker_lower(ctx: &LowerCtx<'_>, node: &Node, style: &StyleProperties) -> UiNode {
    let open = ctx.prop_bool(node, "open", false);
    if !open {
        return hidden_overlay(node.id.clone(), "modifier-picker");
    }
    let target_id = ctx.prop_str(node, "target-id");
    let options = ctx
        .prop(node, "options")
        .as_array()
        .cloned()
        .unwrap_or_default();

    let rows: Vec<UiNode> = options
        .iter()
        .enumerate()
        .map(|(idx, item)| modifier_row(node, idx, item, &target_id, style))
        .collect();

    // Empty state — every behaviour already attached.
    let body = if rows.is_empty() {
        vec![colored_text_node(
            format!("{}::empty", node.id),
            "All behaviours attached.".into(),
            style,
            LABEL_FONT,
            DESC_COLOR,
        )]
    } else {
        rows
    };

    ctx.synthetic_container(node, style, body, |p| {
        p.direction = Direction::Column;
        p.gap = 2.0;
        p.padding = Padding {
            left: 6.0,
            right: 6.0,
            top: 6.0,
            bottom: 6.0,
        };
        p.width = Sizing::Fixed(POPUP_WIDTH);
        p.background = parse_color(POPUP_BG);
        p.radius = uniform_radius(POPUP_RADIUS);
        let mut sem = Semantic::tag("div");
        sem = sem.with_attr("data-role", "modifier-picker");
        sem = sem.with_attr("data-target-id", &target_id);
        sem = sem.with_attr("data-visible", "true");
        sem = sem.with_attr("role", "menu");
        sem = sem.with_attr("aria-label", "Add behaviour");
        p.semantic = sem;
    })
}

fn modifier_row(
    parent: &Node,
    idx: usize,
    item: &Value,
    target_id: &str,
    style: &StyleProperties,
) -> UiNode {
    let id = item.get("id").and_then(|v| v.as_str()).unwrap_or("");
    let label = item.get("label").and_then(|v| v.as_str()).unwrap_or(id);
    let description = item
        .get("description")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let label_node = colored_text_node(
        format!("{}::row{idx}::label", parent.id),
        label.into(),
        style,
        LABEL_FONT,
        LABEL_COLOR,
    );

    let mut col_children = vec![label_node];
    if !description.is_empty() {
        col_children.push(colored_text_node(
            format!("{}::row{idx}::desc", parent.id),
            description.into(),
            style,
            DESC_FONT,
            DESC_COLOR,
        ));
    }

    let text_col = bare_container(format!("{}::row{idx}::col", parent.id), col_children, |p| {
        p.direction = Direction::Column;
        p.gap = 2.0;
    });

    bare_container(format!("{}::row{idx}", parent.id), vec![text_col], |p| {
        p.direction = Direction::Row;
        p.gap = 8.0;
        p.padding = Padding {
            left: 8.0,
            right: 8.0,
            top: 6.0,
            bottom: 6.0,
        };
        p.height = Sizing::Fixed(ROW_HEIGHT);
        p.radius = uniform_radius(ROW_RADIUS);
        p.hover = hover_bg(ROW_HOVER);
        let mut sem = Semantic::tag("button");
        sem = sem.with_attr("data-role", "modifier-picker-select");
        sem = sem.with_attr("data-target-id", target_id);
        sem = sem.with_attr("data-modifier-id", id);
        sem = sem.with_attr("aria-label", label);
        sem = sem.with_attr("role", "menuitem");
        p.semantic = sem;
    })
}

pub const MODIFIER_PICKER_SPEC: prism_builder::BlockSpec =
    prism_builder::BlockSpec::new("shell.modifier-picker", modifier_picker_schema)
        .lower(modifier_picker_lower);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::testing::{lower_with, test_node};
    use serde_json::json;

    fn lower_one(props: Value) -> UiNode {
        lower_with(
            &test_node("mp", "shell.modifier-picker", props),
            modifier_picker_lower,
        )
    }

    fn outer_attrs(ui: &UiNode) -> &Vec<(String, String)> {
        match ui {
            UiNode::Container { props, .. } => &props.semantic.attrs,
            _ => panic!(),
        }
    }

    #[test]
    fn closed_picker_collapses_to_hidden_overlay() {
        let ui = lower_one(json!({
            "open": false,
            "target-id": "btn1",
            "options": [],
        }));
        let attrs = outer_attrs(&ui);
        assert!(attrs
            .iter()
            .any(|(k, v)| k == "data-visible" && v == "false"));
        assert!(attrs.iter().any(|(k, v)| k == "aria-hidden" && v == "true"));
    }

    #[test]
    fn open_picker_emits_one_row_per_option() {
        let ui = lower_one(json!({
            "open": true,
            "target-id": "btn1",
            "options": [
                {"id": "tooltip", "label": "Tooltip", "description": "Hover hint"},
                {"id": "hover-effect", "label": "Hover Effect", "description": "Visual feedback"},
            ],
        }));
        let UiNode::Container { children, .. } = &ui else {
            panic!()
        };
        assert_eq!(children.len(), 2);
    }

    #[test]
    fn open_picker_with_empty_options_shows_empty_state() {
        let ui = lower_one(json!({
            "open": true,
            "target-id": "btn1",
            "options": [],
        }));
        let UiNode::Container { children, .. } = &ui else {
            panic!()
        };
        assert_eq!(children.len(), 1);
        assert!(matches!(children[0], UiNode::Text { .. }));
    }

    #[test]
    fn each_row_carries_picker_select_route() {
        let ui = lower_one(json!({
            "open": true,
            "target-id": "btn1",
            "options": [
                {"id": "tooltip", "label": "Tooltip"},
            ],
        }));
        let UiNode::Container { children, .. } = &ui else {
            panic!()
        };
        let row = &children[0];
        let UiNode::Container { props, .. } = row else {
            panic!()
        };
        let attrs = &props.semantic.attrs;
        assert!(attrs
            .iter()
            .any(|(k, v)| k == "data-role" && v == "modifier-picker-select"));
        assert!(attrs
            .iter()
            .any(|(k, v)| k == "data-target-id" && v == "btn1"));
        assert!(attrs
            .iter()
            .any(|(k, v)| k == "data-modifier-id" && v == "tooltip"));
    }
}
