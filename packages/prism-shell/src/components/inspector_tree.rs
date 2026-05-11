//! `shell.inspector-tree` — composition wrapper hosting
//! [`shell.inspector-row`] children. Owns the outer scroll-friendly
//! column shape, a `<ul role="tree">` SSR semantic, and a 4px gap
//! between rows.
//!
//! Smart pattern: composition over `host_children` *and* a `nodes`
//! JSON-array prop. The JSON path is the §43 C3 wiring — the
//! `BuilderSlot::inspector_tree_props()` binding emits a `nodes`
//! array, the tree dispatches each entry through
//! [`shell.inspector-row`] via `ctx.lower_as`, and the resulting
//! rows carry `data-role="inspector-row"` + `data-target-id` so the
//! hit-test surface can route clicks back to `SelectionService`.

use prism_builder::{
    document::Node,
    registry::FieldSpec,
    style::StyleProperties,
    ui_lower::{bare_container, LowerCtx},
};
use prism_ui_runtime::layout::{Direction, Node as UiNode, Semantic, Sizing};
use serde_json::{json, Value};

fn inspector_tree_schema() -> Vec<FieldSpec> {
    vec![
        FieldSpec::text("aria-label", "ARIA label"),
        FieldSpec::text("nodes", "Inspector rows (JSON array)"),
    ]
}

fn inspector_tree_lower(ctx: &LowerCtx<'_>, node: &Node, _style: &StyleProperties) -> UiNode {
    // Three composition paths kept in lockstep: explicit host_children
    // (binding-driven), source-authored AST children (when the
    // `.prism-ui` author embeds them inline), and the `nodes` JSON
    // array (the §43 C3 selection-driven path).
    let mut kids = ctx
        .host_children()
        .map(|s| s.to_vec())
        .unwrap_or_else(|| ctx.lower_children(&node.children));
    if let Some(arr) = node.props.get("nodes").and_then(|v| v.as_array()) {
        for (idx, item) in arr.iter().enumerate() {
            if let Some(row) = lower_inspector_row(ctx, node, idx, item) {
                kids.push(row);
            }
        }
    }
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

/// Project one `BuilderSlot::inspector_tree_props().nodes[i]` entry
/// onto an [`shell.inspector-row`] dispatch. Reads the friendly
/// `label` as the component-id text and forwards the doc-node-id
/// as `node-id` so the row's `data-target-id` attr carries the
/// click-routing key. Returns `None` when the resolver is missing
/// (headless render paths).
fn lower_inspector_row(
    ctx: &LowerCtx<'_>,
    tree_node: &Node,
    idx: usize,
    item: &Value,
) -> Option<UiNode> {
    let node_id = item.get("node-id").and_then(|v| v.as_str()).unwrap_or("");
    let label = item.get("label").and_then(|v| v.as_str()).unwrap_or("");
    let depth = item.get("depth").and_then(|v| v.as_u64()).unwrap_or(0);
    let selected = item
        .get("selected")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let row_props = json!({
        "node-id": node_id,
        "component-id": label,
        "kind": "node",
        "depth": depth,
        "selected": selected,
    });
    ctx.lower_as(
        "shell.inspector-row",
        format!("{}::row::{}", tree_node.id, idx),
        row_props,
    )
}

pub const INSPECTOR_TREE_SPEC: prism_builder::BlockSpec =
    prism_builder::BlockSpec::new("shell.inspector-tree", inspector_tree_schema)
        .lower(inspector_tree_lower);

#[cfg(test)]
mod tests {
    use super::*;

    use crate::components::registry::{register_shell_builtins, ShellComponentRegistry};
    use crate::components::testing::{lower_with, test_node};

    fn lower_with_registry(props: Value) -> UiNode {
        let mut reg = ShellComponentRegistry::new();
        register_shell_builtins(&mut reg).expect("register");
        let n = test_node("it", "shell.inspector-tree", props);
        let cascade = StyleProperties::default();
        let ctx = LowerCtx::new(Some(reg.as_component_registry()), &cascade);
        inspector_tree_lower(&ctx, &n, &cascade)
    }

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

    #[test]
    fn nodes_array_dispatches_one_row_per_entry() {
        // §43 C3 keystone: the binding emits `nodes` and the tree
        // composes one `shell.inspector-row` per entry. Without a
        // registry the rows drop silently (headless paths); with one,
        // they appear as children.
        let ui = lower_with_registry(json!({
            "nodes": [
                { "node-id": "a", "label": "container · a", "depth": 0, "selected": false },
                { "node-id": "b", "label": "text · b", "depth": 1, "selected": true },
            ],
        }));
        let UiNode::Container { children, .. } = ui else {
            panic!()
        };
        assert_eq!(children.len(), 2);
    }
}
