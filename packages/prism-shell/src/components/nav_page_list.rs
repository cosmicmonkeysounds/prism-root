//! `shell.nav-page-list` — thin host that columns nav-page-row entries.
//! Reads a `pages` JSON array; each entry forwards verbatim to
//! `shell.nav-page-row` via `lower_as`.

use prism_builder::{
    document::Node,
    registry::FieldSpec,
    style::StyleProperties,
    ui_lower::{bare_container, LowerCtx},
};
use prism_ui_runtime::layout::{Direction, Node as UiNode, Padding, Semantic, Sizing};

fn nav_page_list_schema() -> Vec<FieldSpec> {
    vec![FieldSpec::text(
        "pages",
        "Pages (JSON array of nav-page-row props)",
    )]
}

fn nav_page_list_lower(ctx: &LowerCtx<'_>, node: &Node, _style: &StyleProperties) -> UiNode {
    let rows: Vec<UiNode> = node
        .props
        .get("pages")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .enumerate()
                .filter_map(|(idx, item)| {
                    ctx.lower_as(
                        "shell.nav-page-row",
                        format!("{}::row::{}", node.id, idx),
                        item.clone(),
                    )
                })
                .collect()
        })
        .unwrap_or_default();

    bare_container(node.id.clone(), rows, |p| {
        p.direction = Direction::Column;
        p.gap = 2.0;
        p.padding = Padding {
            left: 6.0,
            right: 6.0,
            top: 6.0,
            bottom: 6.0,
        };
        p.width = Sizing::Grow;
        p.height = Sizing::Grow;
        p.semantic = Semantic::tag("ul")
            .with_attr("role", "list")
            .with_attr("aria-label", "Pages")
            .with_attr("data-role", "nav-page-list");
    })
}

pub const NAV_PAGE_LIST_SPEC: prism_builder::BlockSpec =
    prism_builder::BlockSpec::new("shell.nav-page-list", nav_page_list_schema)
        .lower(nav_page_list_lower);

#[cfg(test)]
mod tests {
    use super::*;

    use crate::components::registry::{register_shell_builtins, ShellComponentRegistry};
    use crate::components::testing::test_node;
    use serde_json::json;

    #[test]
    fn dispatches_pages_through_registry() {
        let n = test_node(
            "npl",
            "shell.nav-page-list",
            json!({ "pages": [
                { "page-title": "Home", "route": "/" },
                { "page-title": "About", "route": "/about" },
            ]}),
        );
        let mut reg = ShellComponentRegistry::new();
        register_shell_builtins(&mut reg).expect("register");
        let cascade = StyleProperties::default();
        let ctx = LowerCtx::new(Some(reg.as_component_registry()), &cascade);
        let UiNode::Container { children, .. } = nav_page_list_lower(&ctx, &n, &cascade) else {
            panic!()
        };
        assert_eq!(children.len(), 2);
    }
}
