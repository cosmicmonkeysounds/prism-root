//! `shell.docs-sidebar` — slim docs panel hosted in the right rail.
//! Composes [`shell.docs-content`] inside a fixed-width chrome
//! container with a left hairline border.

use prism_builder::{
    document::Node,
    registry::FieldSpec,
    style::StyleProperties,
    ui_lower::{bare_container, parse_color, prop_string, LowerCtx},
};
use prism_ui_runtime::layout::{Direction, Node as UiNode, Padding, Semantic, Sizing};
use serde_json::json;

const SIDEBAR_WIDTH: f32 = 320.0;
const SIDEBAR_BG: &str = "#fafafa";

fn docs_sidebar_schema() -> Vec<FieldSpec> {
    vec![
        FieldSpec::text("title", "Title").required(),
        FieldSpec::text("summary", "Summary"),
        FieldSpec::text("body", "Body"),
        FieldSpec::text("mode", "Mode (full|compact)"),
    ]
}

fn docs_sidebar_lower(ctx: &LowerCtx<'_>, node: &Node, _style: &StyleProperties) -> UiNode {
    let mode = prop_string(node, "mode");
    let mode = if mode.is_empty() {
        "compact".into()
    } else {
        mode
    };
    let content_props = json!({
        "title": node.props.get("title").cloned().unwrap_or_default(),
        "summary": node.props.get("summary").cloned().unwrap_or_default(),
        "body": node.props.get("body").cloned().unwrap_or_default(),
        "mode": mode,
    });
    let inner = ctx
        .lower_as(
            "shell.docs-content",
            format!("{}::content", node.id),
            content_props,
        )
        .unwrap_or_else(|| bare_container(format!("{}::content", node.id), vec![], |_| {}));

    bare_container(node.id.clone(), vec![inner], |p| {
        p.direction = Direction::Column;
        p.padding = Padding {
            left: 16.0,
            right: 16.0,
            top: 16.0,
            bottom: 16.0,
        };
        p.width = Sizing::Fixed(SIDEBAR_WIDTH);
        p.height = Sizing::Grow;
        p.background = parse_color(SIDEBAR_BG);
        p.semantic = Semantic::tag("aside")
            .with_attr("role", "complementary")
            .with_attr("aria-label", "Documentation")
            .with_attr("data-role", "docs-sidebar");
    })
}

pub const DOCS_SIDEBAR_SPEC: prism_builder::BlockSpec =
    prism_builder::BlockSpec::new("shell.docs-sidebar", docs_sidebar_schema)
        .lower(docs_sidebar_lower);

#[cfg(test)]
mod tests {
    use super::*;

    use crate::components::registry::{register_shell_builtins, ShellComponentRegistry};
    use crate::components::testing::test_node;

    #[test]
    fn renders_aside_with_docs_content_inside() {
        let n = test_node("ds", "shell.docs-sidebar", json!({ "title": "Hi" }));
        let mut reg = ShellComponentRegistry::new();
        register_shell_builtins(&mut reg).expect("register");
        let owned = reg;
        let cascade = StyleProperties::default();
        let ctx = LowerCtx::new(Some(owned.as_component_registry()), &cascade);
        let UiNode::Container {
            children, props, ..
        } = docs_sidebar_lower(&ctx, &n, &cascade)
        else {
            panic!()
        };
        assert_eq!(props.semantic.tag.as_deref(), Some("aside"));
        assert_eq!(children.len(), 1);
        assert_eq!(props.width, Sizing::Fixed(SIDEBAR_WIDTH));
    }
}
