//! `shell.resize-handle` — single 8px square handle on a selection
//! rect. Reads `direction` (one of `tl|t|tr|r|br|b|bl|l`) and
//! emits a small button with the corresponding `data-direction`
//! and CSS-cursor hint. Eight instances make a complete selection
//! handle frame; the host typically composes them via a JSON-array
//! prop on a parent container, dispatched through `lower_as`.
//!
//! Slint origin: resize handles around `ui/app.slint:2676`.

use prism_builder::{
    document::Node,
    registry::FieldSpec,
    signal::SignalDef,
    style::StyleProperties,
    ui_lower::{bare_container, parse_color, uniform_radius, LowerCtx},
    with_common_signals,
};
use prism_ui_runtime::layout::{Node as UiNode, Semantic, Sizing};

const HANDLE_SIZE: f32 = 8.0;
const HANDLE_BG: &str = "#ffffffff";
const HANDLE_BORDER: &str = "#0060c0";

fn resize_handle_schema() -> Vec<FieldSpec> {
    vec![FieldSpec::text("direction", "Handle direction").required()]
}

fn resize_handle_signals() -> Vec<prism_builder::signal::SignalDef> {
    with_common_signals(vec![
        SignalDef::new("handle-pressed", "Handle pressed."),
        SignalDef::new("handle-dragged", "Handle dragged."),
    ])
}

fn resize_handle_lower(ctx: &LowerCtx<'_>, node: &Node, _style: &StyleProperties) -> UiNode {
    let dir = ctx.prop_str(node, "direction");
    let cursor = cursor_for(&dir);

    bare_container(node.id.clone(), vec![], |p| {
        p.width = Sizing::Fixed(HANDLE_SIZE);
        p.height = Sizing::Fixed(HANDLE_SIZE);
        p.background = parse_color(HANDLE_BG);
        p.radius = uniform_radius(1.0);
        p.semantic = Semantic::tag("span")
            .with_attr("role", "button")
            .with_attr("aria-label", aria_for(&dir))
            .with_attr("data-role", "resize-handle")
            .with_attr("data-direction", dir.clone())
            .with_attr("data-cursor", cursor)
            .with_attr("data-stroke", HANDLE_BORDER);
    })
}

pub const RESIZE_HANDLE_SPEC: prism_builder::BlockSpec =
    prism_builder::BlockSpec::new("shell.resize-handle", resize_handle_schema)
        .lower(resize_handle_lower)
        .signals(resize_handle_signals);

fn cursor_for(dir: &str) -> &'static str {
    match dir {
        "tl" | "br" => "nwse-resize",
        "tr" | "bl" => "nesw-resize",
        "t" | "b" => "ns-resize",
        "l" | "r" => "ew-resize",
        _ => "default",
    }
}

fn aria_for(dir: &str) -> &'static str {
    match dir {
        "tl" => "Resize top-left",
        "t" => "Resize top",
        "tr" => "Resize top-right",
        "r" => "Resize right",
        "br" => "Resize bottom-right",
        "b" => "Resize bottom",
        "bl" => "Resize bottom-left",
        "l" => "Resize left",
        _ => "Resize",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::components::testing::{lower_with, test_node};
    use serde_json::json;

    fn lower(direction: &str) -> UiNode {
        let n = test_node(
            "rh",
            "shell.resize-handle",
            json!({ "direction": direction }),
        );
        lower_with(&n, resize_handle_lower)
    }

    #[test]
    fn corner_handle_uses_diagonal_cursor() {
        let UiNode::Container { props, .. } = lower("tl") else {
            panic!()
        };
        assert!(props
            .semantic
            .attrs
            .iter()
            .any(|(k, v)| k == "data-cursor" && v == "nwse-resize"));
    }

    #[test]
    fn edge_handle_uses_axis_cursor() {
        let UiNode::Container { props, .. } = lower("t") else {
            panic!()
        };
        assert!(props
            .semantic
            .attrs
            .iter()
            .any(|(k, v)| k == "data-cursor" && v == "ns-resize"));
    }

    #[test]
    fn unknown_direction_falls_back() {
        let UiNode::Container { props, .. } = lower("???") else {
            panic!()
        };
        assert!(props
            .semantic
            .attrs
            .iter()
            .any(|(k, v)| k == "data-cursor" && v == "default"));
    }
}
