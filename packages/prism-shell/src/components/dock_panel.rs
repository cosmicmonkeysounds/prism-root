//! `shell.dock-panel` — composition block hosting one panel of the
//! Studio dock layout. Optional tab bar at the top, body area below
//! that adopts the inner subtree as `host_children` (so authors write
//! `<shell.dock-panel><shell.builder-canvas/></shell.dock-panel>` from
//! source).
//!
//! Slint origin: the absolutely-positioned panel rectangles in
//! `ui/app.slint` around line 1938.
//!
//! Smart pattern: composition over `host_children` (§14) for the body
//! and `lower_as` (§15) for the optional tab bar. No knowledge of
//! which content blocks exist — the tab bar is dispatched by id, the
//! body is whatever the caller authored.

use prism_builder::{
    document::Node,
    registry::FieldSpec,
    style::StyleProperties,
    ui_lower::{bare_container, parse_color, LowerCtx},
};
use prism_ui_runtime::layout::{Direction, Node as UiNode, Semantic, Sizing};
use serde_json::Value;

const PANEL_BG: &str = "#fafafa";

fn dock_panel_schema() -> Vec<FieldSpec> {
    vec![
        FieldSpec::text("panel-id", "Panel id"),
        FieldSpec::text("title", "Panel title"),
        // JSON array — same shape as `shell.dock-tab-bar` consumes.
        FieldSpec::text("tabs", "Tabs (JSON array)"),
    ]
}

fn dock_panel_lower(ctx: &LowerCtx<'_>, node: &Node, _style: &StyleProperties) -> UiNode {
    let panel_id = ctx.prop_str(node, "panel-id");
    let mut sections: Vec<UiNode> = Vec::with_capacity(2);

    // Optional tab bar: present when the host gave us a `tabs`
    // array. Dispatched by id so a host-supplied alternative tab
    // bar transparently takes over.
    if node
        .props
        .get("tabs")
        .and_then(|v| v.as_array())
        .map(|a| !a.is_empty())
        .unwrap_or(false)
    {
        let tabs_props =
            serde_json::json!({ "tabs": node.props.get("tabs").cloned().unwrap_or(Value::Null) });
        if let Some(bar) = ctx.lower_as(
            "shell.dock-tab-bar",
            format!("{}::tabs", node.id),
            tabs_props,
        ) {
            sections.push(bar);
        }
    }

    // Body: the inner subtree from `<shell.dock-panel>…</shell.dock-panel>`,
    // or the builder-Node children when constructed by the host. When
    // neither is authored, fall through to the `panel-id` routing
    // table on `prism_dock::PanelKind` — `shell.dock-panel
    // panel-id="builder"` with no body dispatches to
    // `shell.builder-canvas` automatically. Adding a new dockable
    // panel is one row in `PanelKind::ALL` (its `tag` field carries
    // the shell content tag); §16 panel-by-panel discipline extended
    // to composition.
    let body_children = if let Some(slice) = ctx.host_children() {
        slice.to_vec()
    } else if !node.children.is_empty() {
        ctx.lower_children(&node.children)
    } else if !panel_id.is_empty() {
        // DSL self-bootstrap Loop 4: prefer the pre-resolved
        // `content-tag` prop from `shell.dock-workspace` (which has
        // the live `DockCatalog`). Fall back to built-ins for
        // directly-authored `<shell.dock-panel panel-id="...">`
        // outside a workspace context.
        let resolved_tag = ctx.prop_str(node, "content-tag");
        let tag_owned: String = if !resolved_tag.is_empty() {
            resolved_tag.to_string()
        } else {
            // Fallback: built-in catalog only — app-registered panels
            // won't surface through direct authoring without a
            // workspace wrapper, but every built-in still does.
            prism_dock::DockCatalog::with_builtins()
                .tag_for(&panel_id)
                .map(str::to_string)
                .unwrap_or_default()
        };
        if tag_owned.is_empty() {
            Vec::new()
        } else {
            ctx.lower_as(
                &tag_owned,
                format!("{}::content", node.id),
                serde_json::json!({}),
            )
            .map(|child| vec![child])
            .unwrap_or_default()
        }
    } else {
        Vec::new()
    };
    let body = bare_container(format!("{}::body", node.id), body_children, |p| {
        p.width = Sizing::Grow;
        p.height = Sizing::Grow;
        p.semantic = Semantic::tag("div").with_attr("data-role", "dock-body");
    });
    sections.push(body);

    bare_container(node.id.clone(), sections, |p| {
        p.direction = Direction::Column;
        p.width = Sizing::Grow;
        p.height = Sizing::Grow;
        p.background = parse_color(PANEL_BG);
        let semantic = Semantic::tag("section").with_attr("data-role", "dock-panel");
        p.semantic = if !panel_id.is_empty() {
            semantic.with_attr("data-panel", panel_id)
        } else {
            semantic
        };
    })
}

pub const DOCK_PANEL_SPEC: prism_builder::BlockSpec =
    prism_builder::BlockSpec::new("shell.dock-panel", dock_panel_schema).lower(dock_panel_lower);

#[cfg(test)]
mod tests {
    use super::*;

    use crate::components::registry::{register_full_shell_chrome, ShellComponentRegistry};
    use crate::components::testing::test_node_with_children;
    use prism_builder::document::Node as BuilderNode;
    use serde_json::json;

    fn node(props: Value, kids: Vec<BuilderNode>) -> BuilderNode {
        test_node_with_children("dp", "shell.dock-panel", props, kids)
    }

    fn lower(props: Value, kids: Vec<BuilderNode>) -> UiNode {
        let cascade = StyleProperties::default();
        let mut r = ShellComponentRegistry::new();
        // Wave 11.2 batch: `shell.dock-tab-bar` is DSL-authored, so the
        // panel's dispatch path needs the full chrome registry.
        register_full_shell_chrome(&mut r).expect("register");
        let owned = r;
        let ctx = LowerCtx::new(Some(owned.as_component_registry()), &cascade);
        dock_panel_lower(&ctx, &node(props, kids), &cascade)
    }

    #[test]
    fn body_only_when_no_tabs() {
        let ui = lower(json!({ "panel-id": "builder" }), vec![]);
        let UiNode::Container { children, .. } = ui else {
            panic!()
        };
        assert_eq!(children.len(), 1, "body only");
    }

    #[test]
    fn tabs_then_body() {
        let ui = lower(
            json!({
                "panel-id": "builder",
                "tabs": [{ "tab-id": "a", "label": "A", "active": true }],
            }),
            vec![],
        );
        let UiNode::Container { children, .. } = ui else {
            panic!()
        };
        assert_eq!(children.len(), 2, "tab-bar + body");
    }

    #[test]
    fn empty_body_dispatches_to_routed_content_tag() {
        // §16 panel-routing: a `<shell.dock-panel panel-id="builder"/>`
        // with no authored body and no `host_children` falls through
        // to `PanelKind::tag_for("builder")` and embeds the matching
        // content tag (`shell.builder-canvas`). Adding a new panel
        // is one row in `PanelKind::ALL` — never a router-arm edit
        // in this block.
        let ui = lower(json!({ "panel-id": "builder" }), vec![]);
        let UiNode::Container { children, .. } = ui else {
            panic!()
        };
        // Single body section (no tabs) containing the routed content.
        let body = children.last().expect("body section");
        let UiNode::Container {
            children: body_kids,
            ..
        } = body
        else {
            panic!()
        };
        assert_eq!(body_kids.len(), 1, "auto-dispatched content child");
        let UiNode::Container { id: content_id, .. } = &body_kids[0] else {
            panic!("routed content not a container")
        };
        assert!(
            content_id.starts_with("dp::content"),
            "content id derives from dock-panel id; got {content_id}"
        );
    }

    #[test]
    fn unknown_panel_id_renders_empty_body() {
        // Defence-in-depth: unknown panel-id yields an empty body
        // rather than panicking. Live state may surface an unknown
        // panel during a partial migration.
        let ui = lower(json!({ "panel-id": "no-such-panel" }), vec![]);
        let UiNode::Container { children, .. } = ui else {
            panic!()
        };
        let UiNode::Container {
            children: body_kids,
            ..
        } = children.last().expect("body")
        else {
            panic!()
        };
        assert!(body_kids.is_empty());
    }

    #[test]
    fn data_panel_attr_propagates() {
        let ui = lower(json!({ "panel-id": "inspector" }), vec![]);
        let UiNode::Container { props, .. } = ui else {
            panic!()
        };
        assert!(props
            .semantic
            .attrs
            .iter()
            .any(|(k, v)| k == "data-panel" && v == "inspector"));
    }
}
