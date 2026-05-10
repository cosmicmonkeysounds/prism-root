//! `shell.gizmo-scale` — Godot-style scale gizmo: red and green
//! arms capped with squares, plus a white center hub.
//!
//! Slint origin: scale gizmo around `ui/app.slint:2614`.

use prism_builder::{
    common_signals,
    document::Node,
    registry::FieldSpec,
    signal::SignalDef,
    style::StyleProperties,
    ui_lower::{bare_container, parse_color, uniform_radius, LowerCtx},
};
use prism_ui_runtime::layout::{Direction, Node as UiNode, Semantic, Sizing};

const ARM_LEN: f32 = 56.0;
const ARM_THICK: f32 = 4.0;
const CAP_SIZE: f32 = 12.0;
const HUB_SIZE: f32 = 12.0;
const X_AXIS_COLOR: &str = "#ff4040ff";
const Y_AXIS_COLOR: &str = "#40c060ff";
const HUB_COLOR: &str = "#ffffffff";

fn gizmo_scale_schema() -> Vec<FieldSpec> {
    vec![FieldSpec::text("axis-active", "Active axis (x|y|hub)")]
}

fn gizmo_scale_signals() -> Vec<prism_builder::signal::SignalDef> {
    let mut s = common_signals();
    s.push(SignalDef::new("axis-pressed", "Axis pressed."));
    s.push(SignalDef::new("axis-dragged", "Axis dragged."));
    s
}

fn gizmo_scale_lower(_ctx: &LowerCtx<'_>, node: &Node, _style: &StyleProperties) -> UiNode {
    let arm_x = bare_container(format!("{}::arm-x", node.id), vec![], |p| {
        p.width = Sizing::Fixed(ARM_LEN);
        p.height = Sizing::Fixed(ARM_THICK);
        p.background = parse_color(X_AXIS_COLOR);
        p.semantic = Semantic::tag("span")
            .with_attr("role", "presentation")
            .with_attr("data-role", "gizmo-axis")
            .with_attr("data-axis", "x");
    });
    let cap_x = bare_container(format!("{}::cap-x", node.id), vec![], |p| {
        p.width = Sizing::Fixed(CAP_SIZE);
        p.height = Sizing::Fixed(CAP_SIZE);
        p.background = parse_color(X_AXIS_COLOR);
        p.semantic = Semantic::tag("span")
            .with_attr("role", "button")
            .with_attr("aria-label", "Scale X handle")
            .with_attr("data-role", "gizmo-cap")
            .with_attr("data-axis", "x");
    });
    let arm_y = bare_container(format!("{}::arm-y", node.id), vec![], |p| {
        p.width = Sizing::Fixed(ARM_THICK);
        p.height = Sizing::Fixed(ARM_LEN);
        p.background = parse_color(Y_AXIS_COLOR);
        p.semantic = Semantic::tag("span")
            .with_attr("role", "presentation")
            .with_attr("data-role", "gizmo-axis")
            .with_attr("data-axis", "y");
    });
    let cap_y = bare_container(format!("{}::cap-y", node.id), vec![], |p| {
        p.width = Sizing::Fixed(CAP_SIZE);
        p.height = Sizing::Fixed(CAP_SIZE);
        p.background = parse_color(Y_AXIS_COLOR);
        p.semantic = Semantic::tag("span")
            .with_attr("role", "button")
            .with_attr("aria-label", "Scale Y handle")
            .with_attr("data-role", "gizmo-cap")
            .with_attr("data-axis", "y");
    });
    let hub = bare_container(format!("{}::hub", node.id), vec![], |p| {
        p.width = Sizing::Fixed(HUB_SIZE);
        p.height = Sizing::Fixed(HUB_SIZE);
        p.background = parse_color(HUB_COLOR);
        p.radius = uniform_radius(2.0);
        p.semantic = Semantic::tag("span")
            .with_attr("role", "button")
            .with_attr("aria-label", "Uniform scale hub")
            .with_attr("data-role", "gizmo-hub");
    });

    bare_container(
        node.id.clone(),
        vec![arm_x, cap_x, arm_y, cap_y, hub],
        |p| {
            p.direction = Direction::Row;
            p.semantic = Semantic::tag("div")
                .with_attr("role", "group")
                .with_attr("aria-label", "Scale gizmo")
                .with_attr("data-role", "gizmo")
                .with_attr("data-tool", "scale");
        },
    )
}

pub const GIZMO_SCALE_SPEC: prism_builder::BlockSpec =
    prism_builder::BlockSpec::new("shell.gizmo-scale", gizmo_scale_schema)
        .lower(gizmo_scale_lower)
        .signals(gizmo_scale_signals);

#[cfg(test)]
mod tests {
    use super::*;

    use prism_builder::document::Node as BuilderNode;
    use prism_builder::layout::LayoutMode;
    use prism_core::foundation::spatial::Transform2D;
    use serde_json::json;

    #[test]
    fn renders_arm_cap_pairs_plus_hub() {
        let n = BuilderNode {
            id: "g".into(),
            component: "shell.gizmo-scale".into(),
            props: json!({}),
            children: vec![],
            layout_mode: LayoutMode::default(),
            transform: Transform2D::default(),
            modifiers: vec![],
            style: StyleProperties::default(),
        };
        let cascade = StyleProperties::default();
        let ctx = LowerCtx::new(None, &cascade);
        let UiNode::Container { children, .. } = gizmo_scale_lower(&ctx, &n, &cascade) else {
            panic!()
        };
        assert_eq!(children.len(), 5);
    }
}
