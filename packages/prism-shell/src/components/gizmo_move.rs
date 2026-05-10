//! `shell.gizmo-move` — Godot-style XY move gizmo overlay. Pure
//! retained-mode emission: red horizontal arm, green vertical arm,
//! and a white center hitbox, all positioned around the (0, 0)
//! origin at the gizmo's mount point. The actual painter is the
//! renderer's job; this block emits nameable subnodes the painter
//! can pick up.
//!
//! Slint origin: gizmo overlay block in `ui/app.slint:2593`.

use super::chrome::{gizmo_axis_arm, gizmo_handle, gizmo_root};
use prism_builder::{
    document::Node, registry::FieldSpec, signal::SignalDef, style::StyleProperties,
    ui_lower::LowerCtx, with_common_signals,
};
use prism_ui_runtime::layout::Node as UiNode;

const ARM_LEN: f32 = 60.0;
const ARM_THICK: f32 = 4.0;
const HUB_SIZE: f32 = 12.0;
const X_AXIS_COLOR: &str = "#ff4040ff";
const Y_AXIS_COLOR: &str = "#40c060ff";
const HUB_COLOR: &str = "#ffffffff";

fn gizmo_move_schema() -> Vec<FieldSpec> {
    vec![FieldSpec::text("axis-active", "Active axis (x|y|hub)")]
}

fn gizmo_move_signals() -> Vec<prism_builder::signal::SignalDef> {
    with_common_signals(vec![
        SignalDef::new("axis-pressed", "Axis arm pressed."),
        SignalDef::new("axis-dragged", "Axis arm dragged."),
    ])
}

fn gizmo_move_lower(_ctx: &LowerCtx<'_>, node: &Node, _style: &StyleProperties) -> UiNode {
    let arm_x = gizmo_axis_arm(
        format!("{}::arm-x", node.id),
        'x',
        ARM_LEN,
        ARM_THICK,
        X_AXIS_COLOR,
        true,
    );
    let arm_y = gizmo_axis_arm(
        format!("{}::arm-y", node.id),
        'y',
        ARM_LEN,
        ARM_THICK,
        Y_AXIS_COLOR,
        true,
    );
    let hub = gizmo_handle(
        format!("{}::hub", node.id),
        HUB_SIZE,
        HUB_SIZE / 2.0,
        HUB_COLOR,
        "Move gizmo center",
        "gizmo-hub",
        None,
    );
    gizmo_root(
        node.id.clone(),
        vec![arm_x, arm_y, hub],
        "Move gizmo",
        "move",
        true,
    )
}

pub const GIZMO_MOVE_SPEC: prism_builder::BlockSpec =
    prism_builder::BlockSpec::new("shell.gizmo-move", gizmo_move_schema)
        .lower(gizmo_move_lower)
        .signals(gizmo_move_signals);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::testing::{lower_with, test_node};
    use serde_json::json;

    #[test]
    fn renders_three_subnodes() {
        let n = test_node("g", "shell.gizmo-move", json!({}));
        let UiNode::Container {
            children, props, ..
        } = lower_with(&n, gizmo_move_lower)
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
