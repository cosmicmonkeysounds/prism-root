//! `shell.workflow-page-button` — single labelled tab inside the
//! DaVinci-style bottom workflow page bar. Selected state shows a
//! 2px accent underline plus a tinted background; resting state
//! gets a translucent hover tint.
//!
//! Slint origin: the inner `Rectangle` inside the workflow-page-bar
//! `for wp[i] in root.workflow-pages` loop in `ui/app.slint`
//! (lines 3991-4007).
//!
//! Smart pattern: this is the second consumer of
//! [`super::chrome::active_underline_tab`] — same column/label/underline
//! recipe as `shell.dock-tab`, just with a different metric/colour
//! [`TabStyle`]. Adding a third tab-shaped chrome primitive is one
//! more `TabStyle` literal.

use super::chrome::{active_underline_tab, TabStyle};
use prism_builder::{
    document::Node,
    registry::FieldSpec,
    style::StyleProperties,
    ui_lower::{prop_bool, prop_str, prop_string, LowerCtx},
};
use prism_ui_runtime::layout::{Node as UiNode, Padding};
use serde_json::Value;

const TAB_STYLE: TabStyle = TabStyle {
    height: 32.0,
    padding: Padding {
        left: 16.0,
        right: 16.0,
        top: 8.0,
        bottom: 0.0,
    },
    label_size: 12.0,
    label_active: "#000000",
    label_resting: "#99000000",
    active_bg: "#190060c0",
    hover_bg: "#1f000000",
    underline_height: 2.0,
    underline_active: "#0060c0",
    data_role: "workflow-page-button",
};

fn workflow_page_button_schema() -> Vec<FieldSpec> {
    vec![
        FieldSpec::text("page-id", "Page ID").required(),
        FieldSpec::text("label", "Label").required(),
        FieldSpec::boolean("active", "Active").with_default(Value::Bool(false)),
    ]
}

fn workflow_page_button_lower(ctx: &LowerCtx<'_>, node: &Node, style: &StyleProperties) -> UiNode {
    active_underline_tab(
        ctx,
        node,
        style,
        prop_string(node, "label"),
        prop_bool(node, "active", false),
        prop_str(node, "page-id"),
        &TAB_STYLE,
    )
}

pub const WORKFLOW_PAGE_BUTTON_SPEC: prism_builder::BlockSpec =
    prism_builder::BlockSpec::new("shell.workflow-page-button", workflow_page_button_schema)
        .lower(workflow_page_button_lower);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::testing::{lower_with, test_node};
    use prism_builder::Block;
    use serde_json::json;

    fn lower(props: Value) -> UiNode {
        lower_with(
            &test_node("wp", "shell.workflow-page-button", props),
            workflow_page_button_lower,
        )
    }

    #[test]
    fn active_button_paints_underline_and_bg() {
        let ui = lower(json!({ "page-id": "edit", "label": "Edit", "active": true }));
        let UiNode::Container {
            props, children, ..
        } = ui
        else {
            panic!()
        };
        assert!(props.background.is_some(), "active gets bg");
        assert!(props.hover.is_none());
        let UiNode::Container { props: under, .. } = &children[1] else {
            panic!()
        };
        assert!(under.background.is_some(), "underline painted when active");
        assert!(props
            .semantic
            .attrs
            .iter()
            .any(|(k, v)| k == "aria-selected" && v == "true"));
    }

    #[test]
    fn resting_button_uses_hover_override() {
        let ui = lower(json!({ "page-id": "edit", "label": "Edit", "active": false }));
        let UiNode::Container {
            props, children, ..
        } = ui
        else {
            panic!()
        };
        assert!(props.background.is_none());
        assert!(props.hover.is_some());
        let UiNode::Container { props: under, .. } = &children[1] else {
            panic!()
        };
        assert!(under.background.is_none());
    }

    #[test]
    fn carries_data_role_and_target_id_for_click_routing() {
        let ui = lower(json!({ "page-id": "edit", "label": "Edit" }));
        let UiNode::Container { props, .. } = ui else {
            panic!()
        };
        assert!(props
            .semantic
            .attrs
            .iter()
            .any(|(k, v)| k == "data-role" && v == "workflow-page-button"));
        assert!(props
            .semantic
            .attrs
            .iter()
            .any(|(k, v)| k == "data-target-id" && v == "edit"));
    }

    #[test]
    fn schema_declares_three_fields() {
        let block = prism_builder::SpecBlock::new(&super::WORKFLOW_PAGE_BUTTON_SPEC);
        let keys: Vec<String> = block.schema().into_iter().map(|f| f.key).collect();
        assert_eq!(keys, vec!["page-id", "label", "active"]);
    }
}
