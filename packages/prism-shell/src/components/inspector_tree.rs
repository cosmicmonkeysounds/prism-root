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
};
use prism_ui_runtime::layout::{Direction, Node as UiNode, Semantic, Sizing};

fn inspector_tree_schema() -> Vec<FieldSpec> {
    vec![FieldSpec::text("aria-label", "ARIA label")]
}

fn inspector_tree_lower(ctx: &LowerCtx<'_>, node: &Node, _style: &StyleProperties) -> UiNode {
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

pub const INSPECTOR_TREE_SPEC: prism_builder::BlockSpec =
    prism_builder::BlockSpec::new("shell.inspector-tree", inspector_tree_schema)
        .lower(inspector_tree_lower);

#[cfg(test)]
mod tests {
    use super::*;

    use crate::components::testing::{lower_with, test_node};
    use serde_json::json;

    #[test]
    fn renders_as_role_tree_ul() {
        let n = test_node("it", "shell.inspector-tree", json!({}));
        let UiNode::Container { props, .. } = lower_with(&n, inspector_tree_lower) else {
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
