//! `shell.menu-dropdown` — column of `shell.menu-item` rows.
//! Reads an `items` JSON array and dispatches each through
//! `lower_as`. Used both as the menu-bar dropdown overlay and as the
//! base structure for [`super::context_menu::ContextMenu`].

use prism_builder::{
    document::Node,
    registry::FieldSpec,
    style::StyleProperties,
    ui_lower::{bare_container, parse_color, uniform_radius, LowerCtx},
};
use prism_ui_runtime::layout::{Direction, Node as UiNode, Padding, Semantic, Sizing};

use crate::components::chrome::hidden_overlay;

const MIN_WIDTH: f32 = 200.0;
const RADIUS: f32 = 6.0;
const BG: &str = "#ffffff";

fn menu_dropdown_schema() -> Vec<FieldSpec> {
    vec![FieldSpec::text("items", "Items (JSON array)")]
}

fn menu_dropdown_lower(ctx: &LowerCtx<'_>, node: &Node, _style: &StyleProperties) -> UiNode {
    // Visibility gate (§43 A2): an empty `items` array means the
    // dropdown isn't open — render a 0×0 placeholder so it occupies no
    // layout space.
    let items_arr = node.props.get("items").and_then(|v| v.as_array());
    if items_arr.is_none_or(|a| a.is_empty()) {
        return hidden_overlay(node.id.clone(), "menu-dropdown");
    }
    let items: Vec<UiNode> = items_arr
        .map(|arr| {
            arr.iter()
                .enumerate()
                .filter_map(|(idx, item)| {
                    ctx.lower_as(
                        "shell.menu-item",
                        format!("{}::item::{}", node.id, idx),
                        item.clone(),
                    )
                })
                .collect()
        })
        .unwrap_or_default();

    bare_container(node.id.clone(), items, |p| {
        p.direction = Direction::Column;
        p.padding = Padding {
            left: 4.0,
            right: 4.0,
            top: 4.0,
            bottom: 4.0,
        };
        p.width = Sizing::Fixed(MIN_WIDTH);
        p.radius = uniform_radius(RADIUS);
        p.background = parse_color(BG);
        p.semantic = Semantic::tag("div")
            .with_attr("role", "menu")
            .with_attr("data-role", "menu-dropdown");
    })
}

pub const MENU_DROPDOWN_SPEC: prism_builder::BlockSpec =
    prism_builder::BlockSpec::new("shell.menu-dropdown", menu_dropdown_schema)
        .lower(menu_dropdown_lower);

#[cfg(test)]
mod tests {
    use super::*;

    use crate::components::registry::{register_shell_builtins, ShellComponentRegistry};
    use crate::components::testing::test_node;
    use serde_json::json;

    #[test]
    fn closed_dropdown_is_hidden_zero_size() {
        // §43 A2: an empty `items` array means the dropdown is closed.
        let n = test_node("md", "shell.menu-dropdown", json!({ "items": [] }));
        let mut reg = ShellComponentRegistry::new();
        register_shell_builtins(&mut reg).expect("register");
        let owned = reg;
        let cascade = StyleProperties::default();
        let ctx = LowerCtx::new(Some(owned.as_component_registry()), &cascade);
        let UiNode::Container { props, .. } = menu_dropdown_lower(&ctx, &n, &cascade) else {
            panic!()
        };
        assert_eq!(props.width, Sizing::Fixed(0.0));
        assert_eq!(props.height, Sizing::Fixed(0.0));
    }

    #[test]
    fn dispatches_items_through_registry() {
        let n = test_node(
            "md",
            "shell.menu-dropdown",
            json!({
                "items": [
                    { "item-id": "save", "label": "Save", "shortcut": "Ctrl+S" },
                    { "item-id": "open", "label": "Open" },
                ]
            }),
        );
        let mut reg = ShellComponentRegistry::new();
        register_shell_builtins(&mut reg).expect("register");
        let owned = reg;
        let cascade = StyleProperties::default();
        let ctx = LowerCtx::new(Some(owned.as_component_registry()), &cascade);
        let UiNode::Container {
            children, props, ..
        } = menu_dropdown_lower(&ctx, &n, &cascade)
        else {
            panic!()
        };
        assert_eq!(children.len(), 2);
        assert!(props
            .semantic
            .attrs
            .iter()
            .any(|(k, v)| k == "role" && v == "menu"));
    }
}
