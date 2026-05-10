//! `shell.context-menu` — right-click menu. Same lowering shape as
//! [`super::menu_dropdown::MenuDropdown`] (a column of menu items),
//! distinct registered tag so authors can address it explicitly and
//! a host can swap one impl without affecting the other.

use prism_builder::{
    document::Node,
    registry::FieldSpec,
    style::StyleProperties,
    ui_lower::{bare_container, parse_color, uniform_radius, LowerCtx},
};
use prism_ui_runtime::layout::{Direction, Node as UiNode, Padding, Semantic, Sizing};

const MIN_WIDTH: f32 = 220.0;
const RADIUS: f32 = 6.0;
const BG: &str = "#ffffff";

fn context_menu_schema() -> Vec<FieldSpec> {
    vec![FieldSpec::text("items", "Items (JSON array)")]
}

fn context_menu_lower(ctx: &LowerCtx<'_>, node: &Node, _style: &StyleProperties) -> UiNode {
    let items: Vec<UiNode> = node
        .props
        .get("items")
        .and_then(|v| v.as_array())
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
            .with_attr("data-role", "context-menu");
    })
}

pub const CONTEXT_MENU_SPEC: prism_builder::BlockSpec =
    prism_builder::BlockSpec::new("shell.context-menu", context_menu_schema)
        .lower(context_menu_lower);

#[cfg(test)]
mod tests {
    use super::*;

    use crate::components::registry::{register_shell_builtins, ShellComponentRegistry};
    use crate::components::testing::test_node;
    use serde_json::json;

    #[test]
    fn data_role_is_context_menu() {
        let n = test_node("cm", "shell.context-menu", json!({ "items": [] }));
        let mut reg = ShellComponentRegistry::new();
        register_shell_builtins(&mut reg).expect("register");
        let owned = reg;
        let cascade = StyleProperties::default();
        let ctx = LowerCtx::new(Some(owned.as_component_registry()), &cascade);
        let UiNode::Container { props, .. } = context_menu_lower(&ctx, &n, &cascade) else {
            panic!()
        };
        assert!(props
            .semantic
            .attrs
            .iter()
            .any(|(k, v)| k == "data-role" && v == "context-menu"));
    }
}
