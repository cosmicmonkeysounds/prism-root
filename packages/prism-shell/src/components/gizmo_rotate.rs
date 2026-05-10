//! `shell.gizmo-rotate` — rotate gizmo overlay (orange ring with a
//! draggable handle dot). Sibling of `shell.gizmo-move`.
//!
//! Slint origin: rotate gizmo around `ui/app.slint:2603`.

use super::chrome::{gizmo_handle, gizmo_root};
use prism_builder::{
    document::Node,
    registry::FieldSpec,
    signal::SignalDef,
    style::StyleProperties,
    ui_lower::{bare_container, parse_color, uniform_radius, LowerCtx},
    with_common_signals,
};
use prism_ui_runtime::layout::{Node as UiNode, Semantic, Sizing};

const RING_SIZE: f32 = 80.0;
const HANDLE_SIZE: f32 = 12.0;
const RING_COLOR: &str = "#ff8c1aff";
const HANDLE_COLOR: &str = "#ffffffff";

fn gizmo_rotate_schema() -> Vec<FieldSpec> {
    vec![FieldSpec::text("active", "Active state")]
}

fn gizmo_rotate_signals() -> Vec<prism_builder::signal::SignalDef> {
    with_common_signals(vec![
        SignalDef::new("ring-pressed", "Ring pressed."),
        SignalDef::new("ring-dragged", "Ring dragged (degrees delta)."),
    ])
}

fn gizmo_rotate_lower(_ctx: &LowerCtx<'_>, node: &Node, _style: &StyleProperties) -> UiNode {
    // The ring is a stroked circle, not a filled handle — keep its
    // recipe inline rather than forcing a `fill_color` parameter on
    // `gizmo_handle` for a one-off shape.
    let ring = bare_container(format!("{}::ring", node.id), vec![], |p| {
        p.width = Sizing::Fixed(RING_SIZE);
        p.height = Sizing::Fixed(RING_SIZE);
        p.radius = uniform_radius(RING_SIZE / 2.0);
        p.background = parse_color("#00ff8c1a");
        p.semantic = Semantic::tag("span")
            .with_attr("role", "presentation")
            .with_attr("data-role", "gizmo-ring")
            .with_attr("data-stroke", RING_COLOR);
    });
    let handle = gizmo_handle(
        format!("{}::handle", node.id),
        HANDLE_SIZE,
        HANDLE_SIZE / 2.0,
        HANDLE_COLOR,
        "Rotate handle",
        "gizmo-handle",
        None,
    );
    gizmo_root(
        node.id.clone(),
        vec![ring, handle],
        "Rotate gizmo",
        "rotate",
        false,
    )
}

pub const GIZMO_ROTATE_SPEC: prism_builder::BlockSpec =
    prism_builder::BlockSpec::new("shell.gizmo-rotate", gizmo_rotate_schema)
        .lower(gizmo_rotate_lower)
        .signals(gizmo_rotate_signals);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::testing::{lower_with, test_node};
    use serde_json::json;

    #[test]
    fn renders_ring_and_handle() {
        let n = test_node("g", "shell.gizmo-rotate", json!({}));
        let UiNode::Container { children, .. } = lower_with(&n, gizmo_rotate_lower) else {
            panic!()
        };
        assert_eq!(children.len(), 2);
    }
}
