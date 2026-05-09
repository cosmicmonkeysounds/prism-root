//! `shell.dock-tab` — single tab inside a [`super::dock_tab_bar::DockTabBar`].
//! Active tabs paint a tinted background + 2px accent underline; inactive
//! tabs use a hover-bg swap.
//!
//! Slint origin: the inline `Rectangle` inside the dock-tab-bar repeater
//! in `ui/app.slint` (around line 1952).

use prism_builder::{
    common_signals,
    document::Node,
    registry::FieldSpec,
    signal::SignalDef,
    style::StyleProperties,
    ui_lower::{
        bare_container, colored_text_node, hover_bg, parse_color, prop_bool, prop_string, LowerCtx,
    },
    Block, ComponentId,
};
use prism_ui_runtime::layout::{Direction, Node as UiNode, Padding, Semantic, Sizing};
use serde_json::Value;

const TAB_HEIGHT: f32 = 26.0;
const UNDERLINE_HEIGHT: f32 = 2.0;
const LABEL_FONT_SIZE: f32 = 12.0;
const LABEL_ACTIVE: &str = "#000000";
const LABEL_RESTING: &str = "#99000000";
const ACTIVE_BG: &str = "#19000000";
const HOVER_BG: &str = "#0f000000";
const ACCENT: &str = "#0060c0";

pub struct DockTab {
    pub id: ComponentId,
}

impl Block for DockTab {
    fn id(&self) -> &ComponentId {
        &self.id
    }

    fn schema(&self) -> Vec<FieldSpec> {
        vec![
            FieldSpec::text("tab-id", "Tab id").required(),
            FieldSpec::text("label", "Label").required(),
            FieldSpec::boolean("active", "Active").with_default(Value::Bool(false)),
        ]
    }

    fn signals(&self) -> Vec<SignalDef> {
        common_signals()
    }

    fn lower_ui(&self, ctx: &LowerCtx<'_>, node: &Node, style: &StyleProperties) -> UiNode {
        let label = prop_string(node, "label");
        let active = prop_bool(node, "active", false);
        let color = if active { LABEL_ACTIVE } else { LABEL_RESTING };

        let label_node = colored_text_node(
            format!("{}::label", node.id),
            label,
            style,
            LABEL_FONT_SIZE,
            color,
        );

        let underline = bare_container(format!("{}::underline", node.id), vec![], |p| {
            p.width = Sizing::Grow;
            p.height = Sizing::Fixed(UNDERLINE_HEIGHT);
            if active {
                p.background = parse_color(ACCENT);
            }
        });

        ctx.synthetic_container(node, style, vec![label_node, underline], |p| {
            p.direction = Direction::Column;
            p.height = Sizing::Fixed(TAB_HEIGHT);
            p.padding = Padding {
                left: 12.0,
                right: 12.0,
                top: 6.0,
                bottom: 0.0,
            };
            if active {
                p.background = parse_color(ACTIVE_BG);
            } else {
                p.hover = hover_bg(HOVER_BG);
            }
            p.semantic = Semantic::button().with_attr("role", "tab").with_attr_if(
                active,
                "aria-selected",
                "true",
            );
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use prism_builder::document::Node as BuilderNode;
    use prism_builder::layout::LayoutMode;
    use prism_core::foundation::spatial::Transform2D;
    use serde_json::json;

    fn lower(props: Value) -> UiNode {
        let block = DockTab {
            id: "shell.dock-tab".into(),
        };
        let n = BuilderNode {
            id: "t".into(),
            component: "shell.dock-tab".into(),
            props,
            children: vec![],
            layout_mode: LayoutMode::default(),
            transform: Transform2D::default(),
            modifiers: vec![],
            style: StyleProperties::default(),
        };
        let cascade = StyleProperties::default();
        let ctx = LowerCtx::new(None, &cascade);
        block.lower_ui(&ctx, &n, &cascade)
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
}
