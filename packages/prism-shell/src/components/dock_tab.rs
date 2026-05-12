//! `shell.dock-tab` — single tab inside a [`super::dock_tab_bar::DockTabBar`].
//! Active tabs paint a tinted background + 2px accent underline; inactive
//! tabs use a hover-bg swap.
//!
//! Slint origin: the inline `Rectangle` inside the dock-tab-bar repeater
//! in `ui/app.slint` (around line 1952).

use super::chrome::{active_underline_tab, TabStyle};
use prism_builder::{
    document::Node, registry::FieldSpec, style::StyleProperties, ui_lower::LowerCtx,
};
use prism_ui_runtime::layout::{Node as UiNode, Padding};
use serde_json::Value;

const TAB_STYLE: TabStyle = TabStyle {
    height: 26.0,
    padding: Padding {
        left: 12.0,
        right: 12.0,
        top: 6.0,
        bottom: 0.0,
    },
    label_size: 12.0,
    label_active: "#000000",
    label_resting: "#99000000",
    active_bg: "#19000000",
    hover_bg: "#0f000000",
    underline_height: 2.0,
    underline_active: "#0060c0",
    data_role: "dock-tab",
};

fn dock_tab_schema() -> Vec<FieldSpec> {
    vec![
        FieldSpec::text("tab-id", "Tab id").required(),
        FieldSpec::text("label", "Label").required(),
        FieldSpec::boolean("active", "Active").with_default(Value::Bool(false)),
    ]
}

fn dock_tab_lower(ctx: &LowerCtx<'_>, node: &Node, style: &StyleProperties) -> UiNode {
    active_underline_tab(
        ctx,
        node,
        style,
        ctx.prop_str(node, "label"),
        ctx.prop_bool(node, "active", false),
        &ctx.prop_str(node, "tab-id"),
        &TAB_STYLE,
    )
}

pub const DOCK_TAB_SPEC: prism_builder::BlockSpec =
    prism_builder::BlockSpec::new("shell.dock-tab", dock_tab_schema).lower(dock_tab_lower);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::testing::{lower_with, test_node};
    use serde_json::json;

    fn lower(props: Value) -> UiNode {
        lower_with(&test_node("t", "shell.dock-tab", props), dock_tab_lower)
    }

    #[test]
    fn active_paints_underline_and_bg() {
        let ui = lower(json!({ "tab-id": "x", "label": "X", "active": true }));
        let UiNode::Container {
            props, children, ..
        } = ui
        else {
            panic!()
        };
        assert!(props.background.is_some());
        let UiNode::Container { props: u, .. } = &children[1] else {
            panic!()
        };
        assert!(u.background.is_some());
    }

    #[test]
    fn resting_uses_hover_swap() {
        let ui = lower(json!({ "tab-id": "x", "label": "X" }));
        let UiNode::Container { props, .. } = ui else {
            panic!()
        };
        assert!(props.background.is_none());
        assert!(props.hover.is_some());
    }

    #[test]
    fn carries_data_role_and_target_id_for_click_routing() {
        let ui = lower(json!({ "tab-id": "builder", "label": "Builder" }));
        let UiNode::Container { props, .. } = ui else {
            panic!()
        };
        assert!(props
            .semantic
            .attrs
            .iter()
            .any(|(k, v)| k == "data-role" && v == "dock-tab"));
        assert!(props
            .semantic
            .attrs
            .iter()
            .any(|(k, v)| k == "data-target-id" && v == "builder"));
    }
}
