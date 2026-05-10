//! `shell.menu-item` — single labelled row inside a dropdown / context
//! menu. Optional shortcut hint on the right; disabled rows skip the
//! hover swap.

use prism_builder::{
    common_signals,
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
    ]
}

fn menu_item_signals() -> Vec<prism_builder::signal::SignalDef> {
    common_signals()
}

fn menu_item_lower(_ctx: &LowerCtx<'_>, node: &Node, _style: &StyleProperties) -> UiNode {
    let label = prop_string(node, "label");
    let shortcut = prop_string(node, "shortcut");
    let disabled = prop_bool(node, "disabled", false);
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
        p.semantic = Semantic::tag("div")
            .with_attr("role", "menuitem")
            .with_attr_if(disabled, "aria-disabled", "true");
    })
}

pub const MENU_ITEM_SPEC: prism_builder::BlockSpec =
    prism_builder::BlockSpec::new("shell.menu-item", menu_item_schema)
        .lower(menu_item_lower)
        .signals(menu_item_signals);

#[cfg(test)]
mod tests {
    use super::*;

    use prism_builder::document::Node as BuilderNode;
    use prism_builder::layout::LayoutMode;
    use prism_core::foundation::spatial::Transform2D;
    use serde_json::json;

    fn lower(props: Value) -> UiNode {
        let n = BuilderNode {
            id: "mi".into(),
            component: "shell.menu-item".into(),
            props,
            children: vec![],
            layout_mode: LayoutMode::default(),
            transform: Transform2D::default(),
            modifiers: vec![],
            style: StyleProperties::default(),
        };
        let cascade = StyleProperties::default();
        let ctx = LowerCtx::new(None, &cascade);
        menu_item_lower(&ctx, &n, &cascade)
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
}
