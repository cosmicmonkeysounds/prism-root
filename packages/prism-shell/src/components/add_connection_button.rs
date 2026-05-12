//! `shell.add-connection-button` — ghost "+ Add Connection" affordance
//! at the bottom of the Signals panel. Click opens the
//! [`shell.connection-picker`] overlay.
//!
//! See `docs/dev/composable-builder-plan.md` Wave 4.3.
//!
//! Routing attrs: `data-on-click="cmd signals.open-connection-picker"`
//! routes through the existing `route_on_click` command-table
//! dispatch — no new POINTER_ROUTES entry needed.

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

fn add_connection_button_schema() -> Vec<FieldSpec> {
    // No props — the button always dispatches the same command. The
    // schema row exists so the registry's catalogue stays uniform.
    Vec::new()
}

fn add_connection_button_lower(ctx: &LowerCtx<'_>, node: &Node, style: &StyleProperties) -> UiNode {
    let label = colored_text_node(
        format!("{}::label", node.id),
        "+ Add Connection".into(),
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
        let sem = Semantic::tag("button")
            .with_attr("data-role", "add-connection")
            .with_attr("data-on-click", "cmd signals.open-connection-picker")
            .with_attr("aria-label", "Add connection");
        p.semantic = sem;
    })
}

pub const ADD_CONNECTION_BUTTON_SPEC: prism_builder::BlockSpec =
    prism_builder::BlockSpec::new("shell.add-connection-button", add_connection_button_schema)
        .lower(add_connection_button_lower);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::testing::{lower_with, test_node};
    use serde_json::json;

    #[test]
    fn carries_open_picker_command_via_data_on_click() {
        let n = test_node("ac", "shell.add-connection-button", json!({}));
        let UiNode::Container { props, .. } = lower_with(&n, add_connection_button_lower) else {
            panic!()
        };
        let attrs = &props.semantic.attrs;
        assert!(attrs
            .iter()
            .any(|(k, v)| k == "data-on-click" && v == "cmd signals.open-connection-picker"));
        assert!(attrs
            .iter()
            .any(|(k, v)| k == "data-role" && v == "add-connection"));
    }
}
