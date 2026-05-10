//! `shell.signals-panel` — composition panel for the signals editor.
//! Reads a `connections` JSON array of row props (forwarded as-is to
//! `shell.signal-connection-row`) and dispatches each through the
//! registry. Optional `host_children` route lets `.prism-ui` source
//! author bespoke headers / footers above the dispatched rows.

use prism_builder::{
    document::Node,
    registry::FieldSpec,
    style::StyleProperties,
    ui_lower::{bare_container, colored_text_node, prop_string, LowerCtx},
};
use prism_ui_runtime::layout::{Direction, Node as UiNode, Padding, Semantic, Sizing};

const HEADER_COLOR: &str = "#80000000";

fn signals_panel_schema() -> Vec<FieldSpec> {
    vec![
        FieldSpec::text("title", "Section title"),
        FieldSpec::text(
            "connections",
            "Connections (JSON array of signal-connection-row props)",
        ),
    ]
}

fn signals_panel_lower(ctx: &LowerCtx<'_>, node: &Node, _style: &StyleProperties) -> UiNode {
    let style = StyleProperties::default();
    let title = prop_string(node, "title");

    let mut kids = ctx
        .host_children()
        .map(|s| s.to_vec())
        .unwrap_or_else(|| ctx.lower_children(&node.children));

    if !title.is_empty() {
        kids.insert(
            0,
            colored_text_node(
                format!("{}::title", node.id),
                title,
                &style,
                11.0,
                HEADER_COLOR,
            ),
        );
    }

    if let Some(arr) = node.props.get("connections").and_then(|v| v.as_array()) {
        for (idx, item) in arr.iter().enumerate() {
            if let Some(child) = ctx.lower_as(
                "shell.signal-connection-row",
                format!("{}::row::{}", node.id, idx),
                item.clone(),
            ) {
                kids.push(child);
            }
        }
    }

    bare_container(node.id.clone(), kids, |p| {
        p.direction = Direction::Column;
        p.gap = 4.0;
        p.padding = Padding {
            left: 12.0,
            right: 12.0,
            top: 12.0,
            bottom: 12.0,
        };
        p.width = Sizing::Grow;
        p.height = Sizing::Grow;
        p.semantic = Semantic::tag("section")
            .with_attr("aria-label", "Signals")
            .with_attr("data-role", "signals-panel");
    })
}

pub const SIGNALS_PANEL_SPEC: prism_builder::BlockSpec =
    prism_builder::BlockSpec::new("shell.signals-panel", signals_panel_schema)
        .lower(signals_panel_lower);

#[cfg(test)]
mod tests {
    use super::*;

    use crate::components::registry::{register_shell_builtins, ShellComponentRegistry};
    use prism_builder::document::Node as BuilderNode;
    use prism_builder::layout::LayoutMode;
    use prism_core::foundation::spatial::Transform2D;
    use serde_json::json;

    fn lower(props: serde_json::Value) -> UiNode {
        let n = BuilderNode {
            id: "sp".into(),
            component: "shell.signals-panel".into(),
            props,
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
        signals_panel_lower(&ctx, &n, &cascade)
    }

    #[test]
    fn empty_panel_has_section_role() {
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
    fn connections_dispatch_to_rows() {
        let ui = lower(json!({
            "title": "Wired up",
            "connections": [
                { "source-signal": "clicked", "action-kind": "SetProperty", "target-label": "x" },
                { "source-signal": "hovered", "action-kind": "EmitSignal", "target-label": "y" },
            ]
        }));
        let UiNode::Container { children, .. } = ui else {
            panic!()
        };
        // title + 2 rows
        assert_eq!(children.len(), 3);
    }
}
