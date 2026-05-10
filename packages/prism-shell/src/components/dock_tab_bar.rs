//! `shell.dock-tab-bar` — horizontal strip of tabs above a dock panel.
//! Reads a `tabs` JSON array of `{ tab-id, label, active }` entries
//! and dispatches each through `shell.dock-tab` via `lower_as`.
//!
//! Slint origin: the dock-panel header bar around `ui/app.slint:1952`.

use prism_builder::{
    document::Node,
    registry::FieldSpec,
    style::StyleProperties,
    ui_lower::{bare_container, parse_color, LowerCtx},
};
use prism_ui_runtime::layout::{Direction, Node as UiNode, Semantic, Sizing};

const BAR_HEIGHT: f32 = 28.0;
const BAR_BG: &str = "#08000000";

fn dock_tab_bar_schema() -> Vec<FieldSpec> {
    vec![FieldSpec::text("tabs", "Tabs (JSON array)")]
}

fn dock_tab_bar_lower(ctx: &LowerCtx<'_>, node: &Node, _style: &StyleProperties) -> UiNode {
    let tabs: Vec<UiNode> = node
        .props
        .get("tabs")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .enumerate()
                .filter_map(|(idx, item)| {
                    ctx.lower_as(
                        "shell.dock-tab",
                        format!("{}::tab::{}", node.id, idx),
                        item.clone(),
                    )
                })
                .collect()
        })
        .unwrap_or_default();

    bare_container(node.id.clone(), tabs, |p| {
        p.direction = Direction::Row;
        p.width = Sizing::Grow;
        p.height = Sizing::Fixed(BAR_HEIGHT);
        p.background = parse_color(BAR_BG);
        p.semantic = Semantic::tag("div")
            .with_attr("role", "tablist")
            .with_attr("data-role", "dock-tab-bar");
    })
}

pub const DOCK_TAB_BAR_SPEC: prism_builder::BlockSpec =
    prism_builder::BlockSpec::new("shell.dock-tab-bar", dock_tab_bar_schema)
        .lower(dock_tab_bar_lower);

#[cfg(test)]
mod tests {
    use super::*;

    use crate::components::registry::{register_shell_builtins, ShellComponentRegistry};
    use crate::components::testing::test_node;
    use serde_json::json;

    fn lower(props: serde_json::Value, with_reg: bool) -> UiNode {
        let n = test_node("tb", "shell.dock-tab-bar", props);
        let cascade = StyleProperties::default();
        let owned;
        let ctx = if with_reg {
            let mut r = ShellComponentRegistry::new();
            register_shell_builtins(&mut r).expect("register");
            owned = r;
            LowerCtx::new(Some(owned.as_component_registry()), &cascade)
        } else {
            LowerCtx::new(None, &cascade)
        };
        dock_tab_bar_lower(&ctx, &n, &cascade)
    }

    #[test]
    fn dispatches_each_tab() {
        let ui = lower(
            json!({ "tabs": [
                { "tab-id": "a", "label": "A", "active": true },
                { "tab-id": "b", "label": "B" },
            ]}),
            true,
        );
        let UiNode::Container { children, .. } = ui else {
            panic!()
        };
        assert_eq!(children.len(), 2);
    }

    #[test]
    fn empty_strip_when_no_tabs() {
        let ui = lower(json!({}), false);
        let UiNode::Container {
            children, props, ..
        } = ui
        else {
            panic!()
        };
        assert!(children.is_empty());
        assert_eq!(props.height, Sizing::Fixed(BAR_HEIGHT));
    }
}
