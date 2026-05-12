//! `shell.add-modifier-button` — ghost "+ Add Behaviour" affordance
//! at the bottom of the Inspector's modifier stack. Click opens the
//! [`shell.modifier-picker`] overlay populated from the live
//! `ModifierRegistry`.
//!
//! See `docs/dev/composable-builder-plan.md` Wave 1.4.
//!
//! Routing attrs: `data-role="add-modifier-open"` + `data-target-id`.
//! The `attached` prop carries the ids already present on the
//! selected node so the picker can filter them out — the routing
//! handler in `events::POINTER_ROUTES` reads it through the lowered
//! attr.

use prism_builder::{
    document::Node,
    registry::FieldSpec,
    style::StyleProperties,
    ui_lower::{colored_text_node, uniform_radius, LowerCtx},
};
use prism_ui_runtime::layout::{Direction, Node as UiNode, Padding, Semantic, Sizing};

const HEIGHT: f32 = 32.0;
const LABEL_FONT_SIZE: f32 = 11.0;
const LABEL_COLOR: &str = "#7f000000";
const HOVER_BG: &str = "#0a000000";

fn add_modifier_button_schema() -> Vec<FieldSpec> {
    vec![
        FieldSpec::text("target-id", "Owning node id").required(),
        // `attached` is a JSON array of behaviour ids already on the
        // node; the picker filters them out.
        FieldSpec::text("attached", "Already-attached ids (JSON array)"),
    ]
}

fn add_modifier_button_lower(ctx: &LowerCtx<'_>, node: &Node, style: &StyleProperties) -> UiNode {
    let target_id = ctx.prop_str(node, "target-id");
    // Stringify the attached list so the lowered attr survives the
    // SSR pass. The picker route reads it back via `serde_json::from_str`.
    let attached_raw = ctx.prop(node, "attached");
    let attached_str = if attached_raw.is_array() {
        attached_raw.to_string()
    } else {
        "[]".to_string()
    };

    let label = colored_text_node(
        format!("{}::label", node.id),
        "+ Add Behaviour".into(),
        style,
        LABEL_FONT_SIZE,
        LABEL_COLOR,
    );

    ctx.synthetic_container(node, style, vec![label], |p| {
        p.direction = Direction::Row;
        p.gap = 6.0;
        p.padding = Padding {
            left: 12.0,
            right: 12.0,
            top: 0.0,
            bottom: 0.0,
        };
        p.height = Sizing::Fixed(HEIGHT);
        p.width = Sizing::Grow;
        p.radius = uniform_radius(6.0);
        p.hover = prism_builder::ui_lower::hover_bg(HOVER_BG);
        let mut sem = Semantic::tag("button");
        sem = sem.with_attr("data-role", "add-modifier-open");
        sem = sem.with_attr("data-target-id", &target_id);
        sem = sem.with_attr("data-attached", &attached_str);
        sem = sem.with_attr("aria-label", "Add behaviour");
        p.semantic = sem;
    })
}

pub const ADD_MODIFIER_BUTTON_SPEC: prism_builder::BlockSpec =
    prism_builder::BlockSpec::new("shell.add-modifier-button", add_modifier_button_schema)
        .lower(add_modifier_button_lower);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::testing::{lower_with, test_node};
    use serde_json::{json, Value};

    fn lower_one(props: Value) -> UiNode {
        lower_with(
            &test_node("amb", "shell.add-modifier-button", props),
            add_modifier_button_lower,
        )
    }

    #[test]
    fn carries_open_picker_route() {
        let ui = lower_one(json!({
            "target-id": "btn1",
            "attached": ["tooltip"],
        }));
        let UiNode::Container { props, .. } = &ui else {
            panic!()
        };
        let attrs = &props.semantic.attrs;
        assert!(attrs
            .iter()
            .any(|(k, v)| k == "data-role" && v == "add-modifier-open"));
        assert!(attrs
            .iter()
            .any(|(k, v)| k == "data-target-id" && v == "btn1"));
    }

    #[test]
    fn attached_array_round_trips_as_json_string_attr() {
        let ui = lower_one(json!({
            "target-id": "btn",
            "attached": ["tooltip", "hover-effect"],
        }));
        let UiNode::Container { props, .. } = &ui else {
            panic!()
        };
        let attached = props
            .semantic
            .attrs
            .iter()
            .find(|(k, _)| k == "data-attached")
            .map(|(_, v)| v.clone())
            .unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&attached).unwrap();
        let arr = parsed.as_array().unwrap();
        assert_eq!(arr.len(), 2);
        assert_eq!(arr[0], "tooltip");
        assert_eq!(arr[1], "hover-effect");
    }

    #[test]
    fn missing_attached_field_emits_empty_json_array() {
        let ui = lower_one(json!({
            "target-id": "btn",
        }));
        let UiNode::Container { props, .. } = &ui else {
            panic!()
        };
        let attached = props
            .semantic
            .attrs
            .iter()
            .find(|(k, _)| k == "data-attached")
            .map(|(_, v)| v.clone())
            .unwrap();
        assert_eq!(attached, "[]");
    }
}
