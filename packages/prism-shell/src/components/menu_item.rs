//! `shell.menu-item` — single labelled row inside a dropdown / context
//! menu. Optional shortcut hint on the right; disabled rows skip the
//! hover swap.

use prism_builder::{
    document::Node,
    registry::FieldSpec,
    style::StyleProperties,
    ui_lower::{
        bare_container, colored_text_node, hover_bg, prop_bool, prop_string, uniform_radius,
        LowerCtx,
    },
};
use prism_ui_runtime::layout::{Direction, Node as UiNode, Padding, Semantic, Sizing};
use serde_json::Value;

const ROW_HEIGHT: f32 = 28.0;
const ROW_HOVER: &str = "#0f000000";
const LABEL_COLOR: &str = "#000000";
const SHORTCUT_COLOR: &str = "#88000000";
const DISABLED_COLOR: &str = "#66000000";

fn menu_item_schema() -> Vec<FieldSpec> {
    vec![
        FieldSpec::text("item-id", "Item id").required(),
        FieldSpec::text("label", "Label").required(),
        FieldSpec::text("shortcut", "Shortcut hint"),
        FieldSpec::boolean("disabled", "Disabled").with_default(Value::Bool(false)),
        FieldSpec::text("command", "Command id to dispatch on click"),
    ]
}

fn menu_item_lower(_ctx: &LowerCtx<'_>, node: &Node, _style: &StyleProperties) -> UiNode {
    let label = prop_string(node, "label");
    let shortcut = prop_string(node, "shortcut");
    // `MenuSlot::items_json` emits `enabled` (state-side struct uses
    // that name); the legacy schema field is `disabled`. Read both so
    // either authoring path disables the row.
    let enabled = prop_bool(node, "enabled", true);
    let disabled = prop_bool(node, "disabled", false) || !enabled;
    let command = prop_string(node, "command");
    let style = StyleProperties::default();
    let label_color = if disabled {
        DISABLED_COLOR
    } else {
        LABEL_COLOR
    };

    let mut kids = vec![colored_text_node(
        format!("{}::label", node.id),
        label,
        &style,
        12.0,
        label_color,
    )];
    if !shortcut.is_empty() {
        kids.push(colored_text_node(
            format!("{}::shortcut", node.id),
            shortcut,
            &style,
            11.0,
            SHORTCUT_COLOR,
        ));
    }

    bare_container(node.id.clone(), kids, |p| {
        p.direction = Direction::Row;
        p.gap = 12.0;
        p.padding = Padding {
            left: 10.0,
            right: 10.0,
            top: 6.0,
            bottom: 6.0,
        };
        p.height = Sizing::Fixed(ROW_HEIGHT);
        p.radius = uniform_radius(3.0);
        if !disabled {
            p.hover = hover_bg(ROW_HOVER);
        }
        let mut s = Semantic::tag("div")
            .with_attr("role", "menuitem")
            .with_attr_if(disabled, "aria-disabled", "true");
        // §43 A1 reuse: surface the command binding as a
        // `data-on-click="cmd <id>"` so the existing `route_on_click`
        // dispatches through the command table. Disabled rows skip
        // the attr so route_on_click falls through to the canvas /
        // drag chain (i.e. clicking a greyed-out item is a no-op).
        if !disabled && !command.is_empty() {
            s = s.with_attr("data-on-click", format!("cmd {command}"));
        }
        p.semantic = s;
    })
}

pub const MENU_ITEM_SPEC: prism_builder::BlockSpec =
    prism_builder::BlockSpec::new("shell.menu-item", menu_item_schema).lower(menu_item_lower);

#[cfg(test)]
mod tests {
    use super::*;

    use crate::components::testing::{lower_with, test_node};
    use serde_json::json;

    fn lower(props: Value) -> UiNode {
        let n = test_node("mi", "shell.menu-item", props);
        lower_with(&n, menu_item_lower)
    }

    #[test]
    fn enabled_has_hover() {
        let ui = lower(json!({ "item-id": "x", "label": "X" }));
        let UiNode::Container { props, .. } = ui else {
            panic!()
        };
        assert!(props.hover.is_some());
    }

    #[test]
    fn disabled_skips_hover_and_sets_aria() {
        let ui = lower(json!({ "item-id": "x", "label": "X", "disabled": true }));
        let UiNode::Container { props, .. } = ui else {
            panic!()
        };
        assert!(props.hover.is_none());
        assert!(props
            .semantic
            .attrs
            .iter()
            .any(|(k, v)| k == "aria-disabled" && v == "true"));
    }

    #[test]
    fn command_prop_emits_data_on_click_cmd_attribute() {
        let ui = lower(json!({
            "item-id": "save",
            "label": "Save",
            "command": "file.save",
        }));
        let UiNode::Container { props, .. } = ui else {
            panic!()
        };
        assert!(props
            .semantic
            .attrs
            .iter()
            .any(|(k, v)| k == "data-on-click" && v == "cmd file.save"));
    }

    #[test]
    fn disabled_item_omits_data_on_click() {
        let ui = lower(json!({
            "item-id": "save",
            "label": "Save",
            "command": "file.save",
            "disabled": true,
        }));
        let UiNode::Container { props, .. } = ui else {
            panic!()
        };
        assert!(props
            .semantic
            .attrs
            .iter()
            .all(|(k, _)| k != "data-on-click"));
    }

    #[test]
    fn enabled_false_also_omits_data_on_click() {
        // The state-side `MenuItem` struct emits `enabled` rather than
        // `disabled`; menu_item_lower honours either authoring path.
        let ui = lower(json!({
            "item-id": "save",
            "label": "Save",
            "command": "file.save",
            "enabled": false,
        }));
        let UiNode::Container { props, .. } = ui else {
            panic!()
        };
        assert!(props
            .semantic
            .attrs
            .iter()
            .all(|(k, _)| k != "data-on-click"));
    }
}
