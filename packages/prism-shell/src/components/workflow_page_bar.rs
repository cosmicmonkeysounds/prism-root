//! `shell.workflow-page-bar` — DaVinci-style 32px bottom strip
//! containing centred workflow-page tabs (Edit / Design / Code /
//! Fusion / etc.).
//!
//! Slint origin: the `if !root.is-launchpad : Rectangle` block in
//! `ui/app.slint` (lines 3984-4009).
//!
//! Smart pattern: composition block over a `pages` JSON array prop.
//! Each entry dispatches to `shell.workflow-page-button` via
//! `ctx.lower_as` — same DI seam (§15) the menu-bar / activity-bar
//! already use, so a host-supplied alternative tab impl
//! transparently takes effect. No `host_children` here: workflow
//! tabs are entirely prop-driven (the legacy Slint version reads
//! from a `[WorkflowPageItem]` model), and prop-driven composition
//! has the rule-of-three precedent set by AppWindow's nav-buttons.

use prism_builder::{
    document::Node,
    registry::FieldSpec,
    style::StyleProperties,
    ui_lower::{bare_container, parse_color, LowerCtx},
};
use prism_ui_runtime::layout::{Direction, Node as UiNode, Node as RuntimeNode, Semantic, Sizing};

const BAR_HEIGHT: f32 = 32.0;
/// `Palette.alternate-background` — chrome strip.
const BAR_BG: &str = "#f4000000";
const TAB_GAP: f32 = 0.0;

fn workflow_page_bar_schema() -> Vec<FieldSpec> {
    vec![
        // `pages` is a JSON array of `{ page-id, label, active }`
        // entries; matches `WorkflowPageButton::schema`. Untyped at
        // the schema layer because the host populates it from
        // `DockWorkspace`.
        FieldSpec::text("pages", "Pages (JSON array)"),
    ]
}

fn workflow_page_bar_lower(ctx: &LowerCtx<'_>, node: &Node, _style: &StyleProperties) -> UiNode {
    let buttons: Vec<RuntimeNode> = node
        .props
        .get("pages")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .enumerate()
                .filter_map(|(idx, item)| {
                    ctx.lower_as(
                        "shell.workflow-page-button",
                        format!("{}::page::{}", node.id, idx),
                        item.clone(),
                    )
                })
                .collect()
        })
        .unwrap_or_default();

    // §43 D3: the DaVinci-style bar centres its tabs across the
    // full bar width. The runtime container doesn't expose a Taffy
    // `justify_content`, so we flank the button row with two
    // `Sizing::Grow` spacers — the canonical CSS-flex centering
    // technique. Empty bars (no pages) skip the spacers and render
    // as a bare strip.
    let mut children: Vec<RuntimeNode> = Vec::with_capacity(buttons.len() + 2);
    if !buttons.is_empty() {
        children.push(grow_spacer(format!("{}::spacer-left", node.id)));
        children.extend(buttons);
        children.push(grow_spacer(format!("{}::spacer-right", node.id)));
    }

    bare_container(node.id.clone(), children, |p| {
        p.direction = Direction::Row;
        p.gap = TAB_GAP;
        p.width = Sizing::Grow;
        p.height = Sizing::Fixed(BAR_HEIGHT);
        p.background = parse_color(BAR_BG);
        p.semantic = Semantic::tag("nav")
            .with_attr("role", "tablist")
            .with_attr("aria-label", "Workflow pages");
    })
}

/// One-axis grow spacer used to centre a row of tabs in the bar.
/// Bare container, no decoration — width pushes outward, height
/// flexes against the parent. Pure layout, no paint.
fn grow_spacer(id: String) -> RuntimeNode {
    bare_container(id, vec![], |p| {
        p.width = Sizing::Grow;
        p.height = Sizing::Grow;
        p.semantic = Semantic::tag("div").with_attr("data-role", "spacer");
    })
}

pub const WORKFLOW_PAGE_BAR_SPEC: prism_builder::BlockSpec =
    prism_builder::BlockSpec::new("shell.workflow-page-bar", workflow_page_bar_schema)
        .lower(workflow_page_bar_lower);

#[cfg(test)]
mod tests {
    use super::*;

    use crate::components::registry::{register_shell_builtins, ShellComponentRegistry};
    use crate::components::testing::{lower_with, test_node};
    use prism_builder::document::Node as BuilderNode;
    use serde_json::json;

    fn node(props: serde_json::Value) -> BuilderNode {
        test_node("wpb", "shell.workflow-page-bar", props)
    }

    fn lower_no_registry(props: serde_json::Value) -> UiNode {
        lower_with(&node(props), workflow_page_bar_lower)
    }

    fn lower_with_registry(props: serde_json::Value) -> UiNode {
        let mut reg = ShellComponentRegistry::new();
        register_shell_builtins(&mut reg).expect("register");
        let cascade = StyleProperties::default();
        let ctx = LowerCtx::new(Some(reg.as_component_registry()), &cascade);
        workflow_page_bar_lower(&ctx, &node(props), &cascade)
    }

    #[test]
    fn empty_pages_lowers_to_empty_strip() {
        let ui = lower_no_registry(json!({}));
        let UiNode::Container {
            props, children, ..
        } = ui
        else {
            panic!()
        };
        assert_eq!(props.height, Sizing::Fixed(BAR_HEIGHT));
        assert!(children.is_empty());
    }

    #[test]
    fn no_registry_drops_pages_silently() {
        // Without a registry, lower_as returns None; the bar still
        // renders its strip but with no children. Production paths
        // always have the registry attached.
        let ui = lower_no_registry(json!({
            "pages": [
                { "page-id": "edit", "label": "Edit", "active": true },
                { "page-id": "code", "label": "Code", "active": false },
            ]
        }));
        let UiNode::Container { children, .. } = ui else {
            panic!()
        };
        assert!(children.is_empty());
    }

    #[test]
    fn with_registry_dispatches_each_page_through_workflow_page_button() {
        let ui = lower_with_registry(json!({
            "pages": [
                { "page-id": "edit", "label": "Edit", "active": true },
                { "page-id": "code", "label": "Code", "active": false },
                { "page-id": "fusion", "label": "Fusion", "active": false },
            ]
        }));
        let UiNode::Container { children, .. } = ui else {
            panic!()
        };
        // §43 D3: spacer-flanked centering — 2 grow spacers + 3 tabs.
        assert_eq!(children.len(), 5);
    }

    #[test]
    fn populated_bar_centres_tabs_with_flanking_grow_spacers() {
        // §43 D3 keystone: the DaVinci-style bar centres its tabs. The
        // runtime has no `justify_content`, so the bar emits grow
        // spacers on each side of the tab row. This test pins the
        // shape so the centering doesn't regress.
        let ui = lower_with_registry(json!({
            "pages": [
                { "page-id": "edit", "label": "Edit", "active": true },
            ]
        }));
        let UiNode::Container { children, .. } = ui else {
            panic!()
        };
        // [spacer-left, tab, spacer-right]
        assert_eq!(children.len(), 3);
        let UiNode::Container {
            id: left_id,
            props: left_props,
            ..
        } = &children[0]
        else {
            panic!("first child not a container")
        };
        assert!(left_id.ends_with("::spacer-left"));
        assert_eq!(left_props.width, Sizing::Grow);
        assert!(left_props
            .semantic
            .attrs
            .iter()
            .any(|(k, v)| k == "data-role" && v == "spacer"));
        let UiNode::Container {
            id: right_id,
            props: right_props,
            ..
        } = &children[2]
        else {
            panic!("last child not a container")
        };
        assert!(right_id.ends_with("::spacer-right"));
        assert_eq!(right_props.width, Sizing::Grow);
    }

    #[test]
    fn semantic_role_is_tablist() {
        let ui = lower_no_registry(json!({}));
        let UiNode::Container { props, .. } = ui else {
            panic!()
        };
        assert_eq!(props.semantic.tag.as_deref(), Some("nav"));
        assert!(props
            .semantic
            .attrs
            .iter()
            .any(|(k, v)| k == "role" && v == "tablist"));
    }
}
