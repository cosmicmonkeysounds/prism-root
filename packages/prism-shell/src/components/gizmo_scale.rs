//! `shell.gizmo-scale` — Godot-style scale gizmo: red and green
//! arms capped with squares, plus a white center hub.
//!
//! Slint origin: scale gizmo around `ui/app.slint:2614`.

use super::chrome::{gizmo_axis_arm, gizmo_handle, gizmo_root};
use prism_builder::{
    document::Node, registry::FieldSpec, signal::SignalDef, style::StyleProperties,
    ui_lower::LowerCtx, with_common_signals,
};
use prism_ui_runtime::layout::Node as UiNode;

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
    with_common_signals(vec![
        SignalDef::new("axis-pressed", "Axis pressed."),
        SignalDef::new("axis-dragged", "Axis dragged."),
    ])
}

fn gizmo_scale_lower(_ctx: &LowerCtx<'_>, node: &Node, _style: &StyleProperties) -> UiNode {
    let arm_x = gizmo_axis_arm(
        format!("{}::arm-x", node.id),
        'x',
        ARM_LEN,
        ARM_THICK,
        X_AXIS_COLOR,
        false,
    );
    let cap_x = gizmo_handle(
        format!("{}::cap-x", node.id),
        CAP_SIZE,
        0.0,
        X_AXIS_COLOR,
        "Scale X handle",
        "gizmo-cap",
        Some('x'),
    );
    let arm_y = gizmo_axis_arm(
        format!("{}::arm-y", node.id),
        'y',
        ARM_LEN,
        ARM_THICK,
        Y_AXIS_COLOR,
        false,
    );
    let cap_y = gizmo_handle(
        format!("{}::cap-y", node.id),
        CAP_SIZE,
        0.0,
        Y_AXIS_COLOR,
        "Scale Y handle",
        "gizmo-cap",
        Some('y'),
    );
    let hub = gizmo_handle(
        format!("{}::hub", node.id),
        HUB_SIZE,
        2.0,
        HUB_COLOR,
        "Uniform scale hub",
        "gizmo-hub",
        None,
    );
    gizmo_root(
        node.id.clone(),
        vec![arm_x, cap_x, arm_y, cap_y, hub],
        "Scale gizmo",
        "scale",
        true,
    )
}

pub const GIZMO_SCALE_SPEC: prism_builder::BlockSpec =
    prism_builder::BlockSpec::new("shell.gizmo-scale", gizmo_scale_schema)
        .lower(gizmo_scale_lower)
        .signals(gizmo_scale_signals);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::testing::{lower_with, test_node};
    use serde_json::json;

    #[test]
    fn renders_arm_cap_pairs_plus_hub() {
        let n = test_node("g", "shell.gizmo-scale", json!({}));
        let UiNode::Container { children, .. } = lower_with(&n, gizmo_scale_lower) else {
            panic!()
        };
        assert_eq!(children.len(), 5);
    }
}
