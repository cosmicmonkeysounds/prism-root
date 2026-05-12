//! `shell.dock-divider` — 6px draggable splitter between dock panels.
//!
//! Slint origin: the `dock-dividers` repeater in `ui/app.slint`
//! (line 3915+).
//!
//! Smart pattern: leaf primitive driven by an `orientation` prop
//! (`vertical` / `horizontal`) and explicit `length` for the
//! cross-axis sizing. Drag callbacks are host concerns — the lowering
//! is purely visual structure with a `data-divider-id` SSR hook.

use prism_builder::{
    document::Node,
    registry::FieldSpec,
    style::StyleProperties,
    ui_lower::{bare_container, parse_color, LowerCtx},
};
use prism_ui_runtime::layout::{Node as UiNode, Semantic, Sizing};

const THICKNESS: f32 = 6.0;
const BG: &str = "#10000000";

fn dock_divider_schema() -> Vec<FieldSpec> {
    vec![
        FieldSpec::text("orientation", "Orientation (vertical|horizontal)"),
        FieldSpec::text("length", "Cross-axis length"),
    ]
}

fn dock_divider_lower(ctx: &LowerCtx<'_>, node: &Node, _style: &StyleProperties) -> UiNode {
    let orientation = ctx.prop_str(node, "orientation");
    let length = node
        .props
        .get("length")
        .and_then(|v| v.as_f64())
        .map(|n| n as f32)
        .unwrap_or(1.0);

    bare_container(node.id.clone(), vec![], |p| {
        // "horizontal" divider sits between vertically stacked
        // panels — it spans the row width, with a fixed THICKNESS
        // height. "vertical" is the dual.
        if orientation == "horizontal" {
            p.width = if length > 1.0 {
                Sizing::Fixed(length)
            } else {
                Sizing::Grow
            };
            p.height = Sizing::Fixed(THICKNESS);
        } else {
            p.width = Sizing::Fixed(THICKNESS);
            p.height = if length > 1.0 {
                Sizing::Fixed(length)
            } else {
                Sizing::Grow
            };
        }
        p.background = parse_color(BG);
        p.semantic = Semantic::tag("div")
            .with_attr("role", "separator")
            .with_attr(
                "aria-orientation",
                if orientation == "horizontal" {
                    "horizontal"
                } else {
                    "vertical"
                },
            )
            .with_attr("data-divider", &node.id);
    })
}

pub const DOCK_DIVIDER_SPEC: prism_builder::BlockSpec =
    prism_builder::BlockSpec::new("shell.dock-divider", dock_divider_schema)
        .lower(dock_divider_lower);

#[cfg(test)]
mod tests {
    use super::*;

    use crate::components::testing::{lower_with, test_node};
    use serde_json::json;

    fn lower(props: serde_json::Value) -> UiNode {
        let n = test_node("d", "shell.dock-divider", props);
        lower_with(&n, dock_divider_lower)
    }

    #[test]
    fn vertical_default() {
        let ui = lower(json!({}));
        let UiNode::Container { props, .. } = ui else {
            panic!()
        };
        assert_eq!(props.width, Sizing::Fixed(THICKNESS));
        assert_eq!(props.height, Sizing::Grow);
    }

    #[test]
    fn horizontal_swaps_axes() {
        let ui = lower(json!({ "orientation": "horizontal" }));
        let UiNode::Container { props, .. } = ui else {
            panic!()
        };
        assert_eq!(props.height, Sizing::Fixed(THICKNESS));
    }

    #[test]
    fn aria_orientation_propagates() {
        let ui = lower(json!({ "orientation": "horizontal" }));
        let UiNode::Container { props, .. } = ui else {
            panic!()
        };
        assert!(props
            .semantic
            .attrs
            .iter()
            .any(|(k, v)| k == "aria-orientation" && v == "horizontal"));
    }
}
