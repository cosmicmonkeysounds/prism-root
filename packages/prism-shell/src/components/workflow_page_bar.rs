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
    Block, ComponentId,
};
use prism_ui_runtime::layout::{Direction, Node as UiNode, Node as RuntimeNode, Semantic, Sizing};

const BAR_HEIGHT: f32 = 32.0;
/// `Palette.alternate-background` — chrome strip.
const BAR_BG: &str = "#f4000000";

pub struct WorkflowPageBar {
    pub id: ComponentId,
}

impl Block for WorkflowPageBar {
    fn id(&self) -> &ComponentId {
        &self.id
    }

    fn schema(&self) -> Vec<FieldSpec> {
        vec![
            // `pages` is a JSON array of `{ page-id, label, active }`
            // entries; matches `WorkflowPageButton::schema`. Untyped at
            // the schema layer because the host populates it from
            // `DockWorkspace`.
            FieldSpec::text("pages", "Pages (JSON array)"),
        ]
    }

    fn lower_ui(&self, ctx: &LowerCtx<'_>, node: &Node, _style: &StyleProperties) -> UiNode {
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

        bare_container(node.id.clone(), buttons, |p| {
            p.direction = Direction::Row;
            p.width = Sizing::Grow;
            p.height = Sizing::Fixed(BAR_HEIGHT);
            p.background = parse_color(BAR_BG);
            p.semantic = Semantic::tag("nav")
                .with_attr("role", "tablist")
                .with_attr("aria-label", "Workflow pages");
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

    fn node(props: serde_json::Value) -> BuilderNode {
        BuilderNode {
            id: "wpb".into(),
            component: "shell.workflow-page-bar".into(),
            props,
            children: vec![],
            layout_mode: LayoutMode::default(),
            transform: Transform2D::default(),
            modifiers: vec![],
            style: StyleProperties::default(),
        }
    }

    fn lower_no_registry(props: serde_json::Value) -> UiNode {
        let block = WorkflowPageBar {
            id: "shell.workflow-page-bar".into(),
        };
        let cascade = StyleProperties::default();
        let ctx = LowerCtx::new(None, &cascade);
        block.lower_ui(&ctx, &node(props), &cascade)
    }

    fn lower_with_registry(props: serde_json::Value) -> UiNode {
        let block = WorkflowPageBar {
            id: "shell.workflow-page-bar".into(),
        };
        let mut reg = ShellComponentRegistry::new();
        register_shell_builtins(&mut reg).expect("register");
        let cascade = StyleProperties::default();
        let ctx = LowerCtx::new(Some(reg.as_component_registry()), &cascade);
        block.lower_ui(&ctx, &node(props), &cascade)
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
        assert_eq!(children.len(), 3);
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
