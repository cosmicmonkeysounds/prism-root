//! `shell.workflow-page-button` — single labelled tab inside the
//! DaVinci-style bottom workflow page bar. Selected state shows a
//! 2px accent underline plus a tinted background; resting state
//! gets a translucent hover tint.
//!
//! Slint origin: the inner `Rectangle` inside the workflow-page-bar
//! `for wp[i] in root.workflow-pages` loop in `ui/app.slint`
//! (lines 3991-4007).
//!
//! Smart pattern: leaf primitive — no embedded chrome, no
//! `host_children`. The parent `shell.workflow-page-bar` block
//! synthesises one of these per entry in its `pages` JSON prop via
//! `ctx.lower_as`. The id (used for the `workflow-page-clicked`
//! signal payload) is carried as the `page-id` prop.

use prism_builder::{
    common_signals,
    document::Node,
    registry::FieldSpec,
    style::StyleProperties,
    ui_lower::{
        bare_container, colored_text_node, hover_bg, parse_color, prop_bool, prop_string, LowerCtx,
    },
    Block, ComponentId,
};
use prism_ui_runtime::layout::{Direction, Node as UiNode, Padding, Semantic, Sizing};
use serde_json::Value;

const TAB_HEIGHT: f32 = 32.0;
const UNDERLINE_HEIGHT: f32 = 2.0;
const LABEL_FONT_SIZE: f32 = 12.0;
/// `Palette.foreground` — full opacity for the active tab label.
const LABEL_ACTIVE_COLOR: &str = "#000000";
/// `Palette.foreground.transparentize(40%)` — resting label colour.
const LABEL_RESTING_COLOR: &str = "#99000000";
/// `Palette.accent-background.transparentize(90%)` — selected bg.
const SELECTED_BG: &str = "#190060c0";
/// Translucent foreground tint for the resting hover state.
const HOVER_BG: &str = "#1f000000";
/// Accent underline colour when active.
const UNDERLINE_ACCENT: &str = "#0060c0";

pub struct WorkflowPageButton {
    pub id: ComponentId,
}

impl Block for WorkflowPageButton {
    fn id(&self) -> &ComponentId {
        &self.id
    }

    fn schema(&self) -> Vec<FieldSpec> {
        vec![
            FieldSpec::text("page-id", "Page ID").required(),
            FieldSpec::text("label", "Label").required(),
            FieldSpec::boolean("active", "Active").with_default(Value::Bool(false)),
        ]
    }

    fn signals(&self) -> Vec<prism_builder::signal::SignalDef> {
        common_signals()
    }

    fn lower_ui(&self, ctx: &LowerCtx<'_>, node: &Node, style: &StyleProperties) -> UiNode {
        let label_text = prop_string(node, "label");
        let active = prop_bool(node, "active", false);
        let color = if active {
            LABEL_ACTIVE_COLOR
        } else {
            LABEL_RESTING_COLOR
        };

        let label = colored_text_node(
            format!("{}::label", node.id),
            label_text,
            style,
            LABEL_FONT_SIZE,
            color,
        );

        // Bottom underline — always emitted; only painted when active.
        let underline = bare_container(format!("{}::underline", node.id), vec![], |p| {
            p.width = Sizing::Grow;
            p.height = Sizing::Fixed(UNDERLINE_HEIGHT);
            if active {
                p.background = parse_color(UNDERLINE_ACCENT);
            }
        });

        ctx.synthetic_container(node, style, vec![label, underline], |props| {
            props.direction = Direction::Column;
            props.height = Sizing::Fixed(TAB_HEIGHT);
            props.padding = Padding {
                left: 16.0,
                right: 16.0,
                top: 8.0,
                bottom: 0.0,
            };
            if active {
                props.background = parse_color(SELECTED_BG);
            } else {
                props.hover = hover_bg(HOVER_BG);
            }
            props.semantic = Semantic::button().with_attr("role", "tab").with_attr_if(
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
        let block = WorkflowPageButton {
            id: "shell.workflow-page-button".into(),
        };
        let n = BuilderNode {
            id: "wp".into(),
            component: "shell.workflow-page-button".into(),
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
    fn schema_declares_three_fields() {
        let block = WorkflowPageButton {
            id: "shell.workflow-page-button".into(),
        };
        let keys: Vec<String> = block.schema().into_iter().map(|f| f.key).collect();
        assert_eq!(keys, vec!["page-id", "label", "active"]);
    }
}
