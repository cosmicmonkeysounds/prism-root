//! `shell.gizmo-move` — Godot-style XY move gizmo overlay. Pure
//! retained-mode emission: red horizontal arm, green vertical arm,
//! and a white center hitbox, all positioned around the (0, 0)
//! origin at the gizmo's mount point. The actual painter is the
//! renderer's job; this block emits nameable subnodes the painter
//! can pick up.
//!
//! Slint origin: gizmo overlay block in `ui/app.slint:2593`.

use prism_builder::{
    common_signals,
    document::Node,
    registry::FieldSpec,
    signal::SignalDef,
    style::StyleProperties,
    ui_lower::{bare_container, parse_color, uniform_radius, LowerCtx},
    Block, ComponentId,
};
use prism_ui_runtime::layout::{Direction, Node as UiNode, Semantic, Sizing};

const ARM_LEN: f32 = 60.0;
const ARM_THICK: f32 = 4.0;
const HUB_SIZE: f32 = 12.0;
const X_AXIS_COLOR: &str = "#ff4040ff";
const Y_AXIS_COLOR: &str = "#40c060ff";
const HUB_COLOR: &str = "#ffffffff";

pub struct GizmoMove {
    pub id: ComponentId,
}

impl Block for GizmoMove {
    fn id(&self) -> &ComponentId {
        &self.id
    }

    fn schema(&self) -> Vec<FieldSpec> {
        vec![FieldSpec::text("axis-active", "Active axis (x|y|hub)")]
    }

    fn signals(&self) -> Vec<SignalDef> {
        let mut s = common_signals();
        s.push(SignalDef::new("axis-pressed", "Axis arm pressed."));
        s.push(SignalDef::new("axis-dragged", "Axis arm dragged."));
        s
    }

    fn lower_ui(&self, _ctx: &LowerCtx<'_>, node: &Node, _style: &StyleProperties) -> UiNode {
        let arm_x = bare_container(format!("{}::arm-x", node.id), vec![], |p| {
            p.width = Sizing::Fixed(ARM_LEN);
            p.height = Sizing::Fixed(ARM_THICK);
            p.background = parse_color(X_AXIS_COLOR);
            p.radius = uniform_radius(ARM_THICK / 2.0);
            p.semantic = Semantic::tag("span")
                .with_attr("role", "presentation")
                .with_attr("data-role", "gizmo-axis")
                .with_attr("data-axis", "x");
        });
        let arm_y = bare_container(format!("{}::arm-y", node.id), vec![], |p| {
            p.width = Sizing::Fixed(ARM_THICK);
            p.height = Sizing::Fixed(ARM_LEN);
            p.background = parse_color(Y_AXIS_COLOR);
            p.radius = uniform_radius(ARM_THICK / 2.0);
            p.semantic = Semantic::tag("span")
                .with_attr("role", "presentation")
                .with_attr("data-role", "gizmo-axis")
                .with_attr("data-axis", "y");
        });
        let hub = bare_container(format!("{}::hub", node.id), vec![], |p| {
            p.width = Sizing::Fixed(HUB_SIZE);
            p.height = Sizing::Fixed(HUB_SIZE);
            p.background = parse_color(HUB_COLOR);
            p.radius = uniform_radius(HUB_SIZE / 2.0);
            p.semantic = Semantic::tag("span")
                .with_attr("role", "button")
                .with_attr("aria-label", "Move gizmo center")
                .with_attr("data-role", "gizmo-hub");
        });

        bare_container(node.id.clone(), vec![arm_x, arm_y, hub], |p| {
            p.direction = Direction::Row;
            p.semantic = Semantic::tag("div")
                .with_attr("role", "group")
                .with_attr("aria-label", "Move gizmo")
                .with_attr("data-role", "gizmo")
                .with_attr("data-tool", "move");
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use prism_builder::document::Node as BuilderNode;
    use prism_builder::layout::LayoutMode;
    use prism_core::foundation::spatial::Transform2D;
    use serde_json::json;

    #[test]
    fn renders_three_subnodes() {
        let block = GizmoMove {
            id: "shell.gizmo-move".into(),
        };
        let n = BuilderNode {
            id: "g".into(),
            component: "shell.gizmo-move".into(),
            props: json!({}),
            children: vec![],
            layout_mode: LayoutMode::default(),
            transform: Transform2D::default(),
            modifiers: vec![],
            style: StyleProperties::default(),
        };
        let cascade = StyleProperties::default();
        let ctx = LowerCtx::new(None, &cascade);
        let UiNode::Container {
            children, props, ..
        } = block.lower_ui(&ctx, &n, &cascade)
        else {
            panic!()
        };
        assert_eq!(children.len(), 3);
        assert!(props
            .semantic
            .attrs
            .iter()
            .any(|(k, v)| k == "data-tool" && v == "move"));
    }
}
