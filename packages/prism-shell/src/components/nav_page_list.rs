//! `shell.nav-page-list` — thin host that columns nav-page-row entries.
//! Reads a `pages` JSON array; each entry forwards verbatim to
//! `shell.nav-page-row` via `lower_as`.

use prism_builder::{
    document::Node,
    registry::FieldSpec,
    style::StyleProperties,
    ui_lower::{bare_container, LowerCtx},
    Block, ComponentId,
};
use prism_ui_runtime::layout::{Direction, Node as UiNode, Padding, Semantic, Sizing};

pub struct NavPageList {
    pub id: ComponentId,
}

impl Block for NavPageList {
    fn id(&self) -> &ComponentId {
        &self.id
    }

    fn schema(&self) -> Vec<FieldSpec> {
        vec![FieldSpec::text(
            "pages",
            "Pages (JSON array of nav-page-row props)",
        )]
    }

    fn lower_ui(&self, ctx: &LowerCtx<'_>, node: &Node, _style: &StyleProperties) -> UiNode {
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::registry::{register_shell_builtins, ShellComponentRegistry};
    use prism_builder::document::Node as BuilderNode;
    use prism_builder::layout::LayoutMode;
    use prism_core::foundation::spatial::Transform2D;
    use serde_json::json;

    #[test]
    fn dispatches_pages_through_registry() {
        let block = NavPageList {
            id: "shell.nav-page-list".into(),
        };
        let n = BuilderNode {
            id: "npl".into(),
            component: "shell.nav-page-list".into(),
            props: json!({ "pages": [
                { "page-title": "Home", "route": "/" },
                { "page-title": "About", "route": "/about" },
            ]}),
            children: vec![],
            layout_mode: LayoutMode::default(),
            transform: Transform2D::default(),
            modifiers: vec![],
            style: StyleProperties::default(),
        };
        let mut reg = ShellComponentRegistry::new();
        register_shell_builtins(&mut reg).expect("register");
        let cascade = StyleProperties::default();
        let ctx = LowerCtx::new(Some(reg.as_component_registry()), &cascade);
        let UiNode::Container { children, .. } = block.lower_ui(&ctx, &n, &cascade) else {
            panic!()
        };
        assert_eq!(children.len(), 2);
    }
}
