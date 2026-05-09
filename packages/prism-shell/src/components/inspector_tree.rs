//! `shell.inspector-tree` — composition wrapper hosting
//! [`shell.inspector-row`] children. Owns the outer scroll-friendly
//! column shape, a `<ul role="tree">` SSR semantic, and a 4px gap
//! between rows.
//!
//! Smart pattern: composition over `host_children`. The actual row
//! visuals already live in `shell.inspector-row` (§13 chrome
//! scoreboard), so this wrapper is a thin chrome shell.

use prism_builder::{
    document::Node,
    registry::FieldSpec,
    style::StyleProperties,
    ui_lower::{bare_container, LowerCtx},
    Block, ComponentId,
};
use prism_ui_runtime::layout::{Direction, Node as UiNode, Semantic, Sizing};

pub struct InspectorTree {
    pub id: ComponentId,
}

impl Block for InspectorTree {
    fn id(&self) -> &ComponentId {
        &self.id
    }

    fn schema(&self) -> Vec<FieldSpec> {
        vec![FieldSpec::text("aria-label", "ARIA label")]
    }

    fn lower_ui(&self, ctx: &LowerCtx<'_>, node: &Node, _style: &StyleProperties) -> UiNode {
        let kids = ctx
            .host_children()
            .map(|s| s.to_vec())
            .unwrap_or_else(|| ctx.lower_children(&node.children));
        let aria = node
            .props
            .get("aria-label")
            .and_then(|v| v.as_str())
            .unwrap_or("Document tree");
        bare_container(node.id.clone(), kids, |p| {
            p.direction = Direction::Column;
            p.gap = 0.0;
            p.width = Sizing::Grow;
            p.semantic = Semantic::tag("ul")
                .with_attr("role", "tree")
                .with_attr("aria-label", aria);
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use prism_builder::document::Node as BuilderNode;
    use prism_builder::layout::LayoutMode;
    use prism_core::foundation::spatial::Transform2D;
    use serde_json::json;

    #[test]
    fn renders_as_role_tree_ul() {
        let block = InspectorTree {
            id: "shell.inspector-tree".into(),
        };
        let n = BuilderNode {
            id: "it".into(),
            component: "shell.inspector-tree".into(),
            props: json!({}),
            children: vec![],
            layout_mode: LayoutMode::default(),
            transform: Transform2D::default(),
            modifiers: vec![],
            style: StyleProperties::default(),
        };
        let cascade = StyleProperties::default();
        let ctx = LowerCtx::new(None, &cascade);
        let UiNode::Container { props, .. } = block.lower_ui(&ctx, &n, &cascade) else {
            panic!()
        };
        assert_eq!(props.semantic.tag.as_deref(), Some("ul"));
        assert!(props
            .semantic
            .attrs
            .iter()
            .any(|(k, v)| k == "role" && v == "tree"));
    }
}
