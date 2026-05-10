//! `shell.status-bar` — 26px footer strip at the bottom of the app
//! window. Renders a single status label.
//!
//! Slint origin: the inline status-bar Rectangle in `ui/app.slint`
//! (lines 4011-4037). The legacy Slint version composes additional
//! breadcrumb / selection / line-col / project-name segments; those
//! are *additive* prop-driven extensions queued as future authoring
//! changes (one new field per segment) and not part of this v1
//! promotion. The shape produced here matches AppWindow's previous
//! `synth_status_bar` byte-for-byte so the §16 frame-chrome step is
//! a pure registry-routing change, not a redesign.
//!
//! Smart pattern: leaf primitive — no embedded chrome, no
//! `host_children`. Schema is a single `text` field; AppWindow
//! dispatches via `ctx.lower_as("shell.status-bar", …, { "text": status })`.

use prism_builder::{
    document::Node,
    registry::FieldSpec,
    style::StyleProperties,
    ui_lower::{bare_container, colored_text_node, parse_color, prop_str, LowerCtx},
};
use prism_ui_runtime::layout::{Direction, Node as UiNode, Padding, Semantic, Sizing};

const STATUS_BAR_HEIGHT: f32 = 26.0;
const STATUS_BAR_BG: &str = "#08000000";
const STATUS_TEXT_COLOR: &str = "#99000000";
const STATUS_FONT_SIZE: f32 = 11.0;

fn status_bar_schema() -> Vec<FieldSpec> {
    vec![FieldSpec::text("text", "Status text")]
}

fn status_bar_lower(_ctx: &LowerCtx<'_>, node: &Node, _style: &StyleProperties) -> UiNode {
    let text = prop_str(node, "text");
    let cascade = StyleProperties::default();
    let label = colored_text_node(
        format!("{}::label", node.id),
        text.into(),
        &cascade,
        STATUS_FONT_SIZE,
        STATUS_TEXT_COLOR,
    );
    bare_container(node.id.clone(), vec![label], |p| {
        p.direction = Direction::Row;
        p.height = Sizing::Fixed(STATUS_BAR_HEIGHT);
        p.padding = Padding {
            left: 12.0,
            right: 12.0,
            top: 0.0,
            bottom: 0.0,
        };
        p.background = parse_color(STATUS_BAR_BG);
        p.semantic = Semantic::tag("footer").with_attr("role", "contentinfo");
    })
}

pub const STATUS_BAR_SPEC: prism_builder::BlockSpec =
    prism_builder::BlockSpec::new("shell.status-bar", status_bar_schema).lower(status_bar_lower);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::testing::{lower_with, test_node};
    use serde_json::json;

    fn lower(props: serde_json::Value) -> UiNode {
        lower_with(
            &test_node("sb", "shell.status-bar", props),
            status_bar_lower,
        )
    }

    #[test]
    fn renders_text_at_fixed_height() {
        let ui = lower(json!({ "text": "Saved." }));
        let UiNode::Container {
            children, props, ..
        } = ui
        else {
            panic!()
        };
        assert_eq!(props.height, Sizing::Fixed(STATUS_BAR_HEIGHT));
        let UiNode::Text { content, .. } = &children[0] else {
            panic!()
        };
        assert_eq!(content, "Saved.");
    }

    #[test]
    fn empty_text_when_prop_missing() {
        let ui = lower(json!({}));
        let UiNode::Container { children, .. } = ui else {
            panic!()
        };
        let UiNode::Text { content, .. } = &children[0] else {
            panic!()
        };
        assert_eq!(content, "");
    }

    #[test]
    fn footer_semantic_role() {
        let ui = lower(json!({}));
        let UiNode::Container { props, .. } = ui else {
            panic!()
        };
        assert_eq!(props.semantic.tag.as_deref(), Some("footer"));
    }
}
