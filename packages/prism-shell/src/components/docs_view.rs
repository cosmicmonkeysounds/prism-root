//! `shell.docs-view` — full-page docs reader. Same content as
//! [`super::docs_sidebar`] but rendered as a centred full-width
//! article instead of a side rail.

use prism_builder::{
    document::Node,
    registry::FieldSpec,
    style::StyleProperties,
    ui_lower::{bare_container, parse_color, LowerCtx},
};
use prism_ui_runtime::layout::{Direction, Node as UiNode, Padding, Semantic, Sizing};
use serde_json::json;

const VIEW_BG: &str = "#ffffff";

fn docs_view_schema() -> Vec<FieldSpec> {
    vec![
        FieldSpec::text("title", "Title").required(),
        FieldSpec::text("summary", "Summary"),
        FieldSpec::text("body", "Body"),
    ]
}

fn docs_view_lower(ctx: &LowerCtx<'_>, node: &Node, _style: &StyleProperties) -> UiNode {
    let content_props = json!({
        "title": node.props.get("title").cloned().unwrap_or_default(),
        "summary": node.props.get("summary").cloned().unwrap_or_default(),
        "body": node.props.get("body").cloned().unwrap_or_default(),
        "mode": "full",
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
            left: 64.0,
            right: 64.0,
            top: 48.0,
            bottom: 48.0,
        };
        p.width = Sizing::Grow;
        p.height = Sizing::Grow;
        p.background = parse_color(VIEW_BG);
        p.semantic = Semantic::tag("article").with_attr("data-role", "docs-view");
    })
}

pub const DOCS_VIEW_SPEC: prism_builder::BlockSpec =
    prism_builder::BlockSpec::new("shell.docs-view", docs_view_schema).lower(docs_view_lower);

#[cfg(test)]
mod tests {
    use super::*;

    use crate::components::registry::{register_shell_builtins, ShellComponentRegistry};
    use crate::components::testing::test_node;

    #[test]
    fn article_with_full_mode() {
        let n = test_node("dv", "shell.docs-view", json!({ "title": "Hi" }));
        let mut reg = ShellComponentRegistry::new();
        register_shell_builtins(&mut reg).expect("register");
        let owned = reg;
        let cascade = StyleProperties::default();
        let ctx = LowerCtx::new(Some(owned.as_component_registry()), &cascade);
        let UiNode::Container { props, .. } = docs_view_lower(&ctx, &n, &cascade) else {
            panic!()
        };
        assert_eq!(props.semantic.tag.as_deref(), Some("article"));
    }
}
