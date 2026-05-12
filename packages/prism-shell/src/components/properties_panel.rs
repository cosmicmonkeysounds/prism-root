//! `shell.properties-panel` — right rail panel composing
//! [`shell.section-header`] / [`shell.field-editor`] /
//! [`shell.transform-editor`] children. The panel itself owns only
//! the outer scroll-friendly column shape; the actual property rows
//! are authored as children (or fed via the `rows` JSON prop, which
//! routes each entry through the registry by component id).

use prism_builder::{
    document::Node,
    registry::FieldSpec,
    style::StyleProperties,
    ui_lower::{bare_container, LowerCtx},
};
use prism_ui_runtime::layout::{Direction, Node as UiNode, Padding, Semantic, Sizing};

fn properties_panel_schema() -> Vec<FieldSpec> {
    vec![FieldSpec::text(
        "rows",
        "Rows (JSON array of {component, props})",
    )]
}

fn properties_panel_lower(ctx: &LowerCtx<'_>, node: &Node, _style: &StyleProperties) -> UiNode {
    // Either the source-driven path (host_children + recursed
    // lower_children) OR the JSON-driven path: each entry in
    // `rows` declares `{ component: "shell.section-header", props: { … }}`.
    let mut kids = ctx
        .host_children()
        .map(|s| s.to_vec())
        .unwrap_or_else(|| ctx.lower_children(&node.children));

    if let Some(arr) = node.props.get("rows").and_then(|v| v.as_array()) {
        for (idx, item) in arr.iter().enumerate() {
            let component = item.get("component").and_then(|v| v.as_str()).unwrap_or("");
            let props = item
                .get("props")
                .cloned()
                .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));
            if let Some(child) =
                ctx.lower_as(component, format!("{}::row::{}", node.id, idx), props)
            {
                kids.push(child);
            }
        }
    }

    bare_container(node.id.clone(), kids, |p| {
        p.direction = Direction::Column;
        p.gap = 8.0;
        p.padding = Padding {
            left: 12.0,
            right: 12.0,
            top: 12.0,
            bottom: 12.0,
        };
        p.width = Sizing::Grow;
        p.height = Sizing::Grow;
        p.semantic = Semantic::tag("section")
            .with_attr("data-role", "properties-panel")
            .with_attr("aria-label", "Properties");
    })
}

pub const PROPERTIES_PANEL_SPEC: prism_builder::BlockSpec =
    prism_builder::BlockSpec::new("shell.properties-panel", properties_panel_schema)
        .lower(properties_panel_lower);

#[cfg(test)]
mod tests {
    use super::*;

    use crate::components::registry::{register_full_shell_chrome, ShellComponentRegistry};
    use crate::components::testing::test_node;
    use serde_json::json;

    fn lower(props: serde_json::Value) -> UiNode {
        let n = test_node("pp", "shell.properties-panel", props);
        let mut reg = ShellComponentRegistry::new();
        register_full_shell_chrome(&mut reg).expect("register");
        let owned = reg;
        let cascade = StyleProperties::default();
        let ctx = LowerCtx::new(Some(owned.as_component_registry()), &cascade);
        properties_panel_lower(&ctx, &n, &cascade)
    }

    #[test]
    fn empty_panel() {
        let ui = lower(json!({}));
        let UiNode::Container {
            children, props, ..
        } = ui
        else {
            panic!()
        };
        assert!(children.is_empty());
        assert_eq!(props.semantic.tag.as_deref(), Some("section"));
    }

    #[test]
    fn rows_dispatch_through_registry() {
        let ui = lower(json!({
            "rows": [
                { "component": "shell.section-header", "props": { "label": "Layout" } },
                { "component": "shell.field-editor", "props": { "key": "x", "kind": "number", "value": 10 } },
            ]
        }));
        let UiNode::Container { children, .. } = ui else {
            panic!()
        };
        assert_eq!(children.len(), 2);
    }
}
