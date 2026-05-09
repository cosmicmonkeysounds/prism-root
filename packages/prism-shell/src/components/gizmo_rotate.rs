//! `shell.gizmo-rotate` — rotate gizmo overlay (orange ring with a
//! draggable handle dot). Sibling of `shell.gizmo-move`.
//!
//! Slint origin: rotate gizmo around `ui/app.slint:2603`.

use prism_builder::{
    common_signals,
    document::Node,
    registry::FieldSpec,
    signal::SignalDef,
    style::StyleProperties,
    ui_lower::{bare_container, parse_color, uniform_radius, LowerCtx},
    Block, ComponentId,
};
use prism_ui_runtime::layout::{Node as UiNode, Semantic, Sizing};

const RING_SIZE: f32 = 80.0;
const HANDLE_SIZE: f32 = 12.0;
const RING_COLOR: &str = "#ff8c1aff";
const HANDLE_COLOR: &str = "#ffffffff";

pub struct GizmoRotate {
    pub id: ComponentId,
}

impl Block for GizmoRotate {
    fn id(&self) -> &ComponentId {
        &self.id
    }

    fn schema(&self) -> Vec<FieldSpec> {
        vec![FieldSpec::text("active", "Active state")]
    }

    fn signals(&self) -> Vec<SignalDef> {
        let mut s = common_signals();
        s.push(SignalDef::new("ring-pressed", "Ring pressed."));
        s.push(SignalDef::new(
            "ring-dragged",
            "Ring dragged (degrees delta).",
        ));
        s
    }

    fn lower_ui(&self, _ctx: &LowerCtx<'_>, node: &Node, _style: &StyleProperties) -> UiNode {
        let ring = bare_container(format!("{}::ring", node.id), vec![], |p| {
            p.width = Sizing::Fixed(RING_SIZE);
            p.height = Sizing::Fixed(RING_SIZE);
            p.radius = uniform_radius(RING_SIZE / 2.0);
            p.background = parse_color("#00ff8c1a"); // ring is stroked, not filled
            p.semantic = Semantic::tag("span")
                .with_attr("role", "presentation")
                .with_attr("data-role", "gizmo-ring")
                .with_attr("data-stroke", RING_COLOR);
        });
        let handle = bare_container(format!("{}::handle", node.id), vec![], |p| {
            p.width = Sizing::Fixed(HANDLE_SIZE);
            p.height = Sizing::Fixed(HANDLE_SIZE);
            p.radius = uniform_radius(HANDLE_SIZE / 2.0);
            p.background = parse_color(HANDLE_COLOR);
            p.semantic = Semantic::tag("span")
                .with_attr("role", "button")
                .with_attr("aria-label", "Rotate handle")
                .with_attr("data-role", "gizmo-handle");
        });

        bare_container(node.id.clone(), vec![ring, handle], |p| {
            p.semantic = Semantic::tag("div")
                .with_attr("role", "group")
                .with_attr("aria-label", "Rotate gizmo")
                .with_attr("data-role", "gizmo")
                .with_attr("data-tool", "rotate");
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
    fn renders_ring_and_handle() {
        let block = GizmoRotate {
            id: "shell.gizmo-rotate".into(),
        };
        let n = BuilderNode {
            id: "g".into(),
            component: "shell.gizmo-rotate".into(),
            props: json!({}),
            children: vec![],
            layout_mode: LayoutMode::default(),
            transform: Transform2D::default(),
            modifiers: vec![],
            style: StyleProperties::default(),
        };
        let cascade = StyleProperties::default();
        let ctx = LowerCtx::new(None, &cascade);
        let UiNode::Container { children, .. } = block.lower_ui(&ctx, &n, &cascade) else {
            panic!()
        };
        assert_eq!(children.len(), 2);
    }
}
