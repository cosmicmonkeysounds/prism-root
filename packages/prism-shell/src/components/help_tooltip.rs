//! `shell.help-tooltip` — small floating tooltip with title + summary.
//!
//! Slint origin: the help-tooltip overlay block in `ui/app.slint`
//! around line 4165.
//!
//! Smart pattern: leaf — props carry the resolved `HelpEntry` shape.
//! Mounting onto an overlay anchor is the host's responsibility.

use prism_builder::{
    document::Node,
    registry::FieldSpec,
    style::StyleProperties,
    ui_lower::{
        bare_container, colored_text_node, parse_color, prop_string, uniform_radius, LowerCtx,
    },
};
use prism_ui_runtime::layout::{Direction, Node as UiNode, Padding, Semantic, Sizing};

const TOOLTIP_WIDTH: f32 = 280.0;
const TOOLTIP_RADIUS: f32 = 6.0;
const TOOLTIP_BG: &str = "#f0202020";
const TITLE_COLOR: &str = "#ffffff";
const SUMMARY_COLOR: &str = "#cccccc";

fn help_tooltip_schema() -> Vec<FieldSpec> {
    vec![
        FieldSpec::text("title", "Title").required(),
        FieldSpec::text("summary", "Summary"),
    ]
}

fn help_tooltip_lower(_ctx: &LowerCtx<'_>, node: &Node, _style: &StyleProperties) -> UiNode {
    let title = prop_string(node, "title");
    let summary = prop_string(node, "summary");
    let style = StyleProperties::default();

    let mut kids = vec![colored_text_node(
        format!("{}::title", node.id),
        title,
        &style,
        12.0,
        TITLE_COLOR,
    )];
    if !summary.is_empty() {
        kids.push(colored_text_node(
            format!("{}::summary", node.id),
            summary,
            &style,
            11.0,
            SUMMARY_COLOR,
        ));
    }

    bare_container(node.id.clone(), kids, |p| {
        p.direction = Direction::Column;
        p.gap = 4.0;
        p.padding = Padding {
            left: 10.0,
            right: 10.0,
            top: 8.0,
            bottom: 8.0,
        };
        p.width = Sizing::Fixed(TOOLTIP_WIDTH);
        p.radius = uniform_radius(TOOLTIP_RADIUS);
        p.background = parse_color(TOOLTIP_BG);
        p.semantic = Semantic::tag("div")
            .with_attr("role", "tooltip")
            .with_attr("data-role", "help-tooltip");
    })
}

pub const HELP_TOOLTIP_SPEC: prism_builder::BlockSpec =
    prism_builder::BlockSpec::new("shell.help-tooltip", help_tooltip_schema)
        .lower(help_tooltip_lower);

#[cfg(test)]
mod tests {
    use super::*;

    use crate::components::testing::{lower_with, test_node};
    use serde_json::json;

    fn lower(props: serde_json::Value) -> UiNode {
        let n = test_node("ht", "shell.help-tooltip", props);
        lower_with(&n, help_tooltip_lower)
    }

    #[test]
    fn title_only_when_summary_missing() {
        let ui = lower(json!({ "title": "Hi" }));
        let UiNode::Container { children, .. } = ui else {
            panic!()
        };
        assert_eq!(children.len(), 1);
    }

    #[test]
    fn title_and_summary_render() {
        let ui = lower(json!({ "title": "Hi", "summary": "There" }));
        let UiNode::Container { children, .. } = ui else {
            panic!()
        };
        assert_eq!(children.len(), 2);
    }

    #[test]
    fn role_is_tooltip() {
        let ui = lower(json!({ "title": "x" }));
        let UiNode::Container { props, .. } = ui else {
            panic!()
        };
        assert!(props
            .semantic
            .attrs
            .iter()
            .any(|(k, v)| k == "role" && v == "tooltip"));
    }
}
