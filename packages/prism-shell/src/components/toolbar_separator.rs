//! `shell.toolbar-separator` — 1×20 vertical stroke that separates
//! groups in toolbars and menu rows.
//!
//! Slint origin: `ToolbarSeparator` in `ui/app.slint` (lines 375-379).
//! Lowering: a 1×20 fixed-size [`prism_ui_runtime::layout::Node::Container`]
//! with a translucent foreground background. No interaction, no schema.

use prism_builder::{
    document::Node,
    registry::FieldSpec,
    style::StyleProperties,
    ui_lower::{parse_color, LowerCtx},
    Block, ComponentId,
};
use prism_ui_runtime::layout::{Node as UiNode, Sizing};

const SEPARATOR_WIDTH: f32 = 1.0;
const SEPARATOR_HEIGHT: f32 = 20.0;
/// Foreground transparentize(80%) — `Palette.foreground` is the shell's
/// dark text colour, so 20% opacity over the toolbar background reads
/// as a faint stroke. Until the design-tokens cascade lands, hard-code
/// the resolved value.
const SEPARATOR_DEFAULT_COLOR: &str = "#33000000";

pub struct ToolbarSeparator {
    pub id: ComponentId,
}

impl Block for ToolbarSeparator {
    fn id(&self) -> &ComponentId {
        &self.id
    }

    fn schema(&self) -> Vec<FieldSpec> {
        vec![]
    }

    fn lower_ui(&self, ctx: &LowerCtx<'_>, node: &Node, style: &StyleProperties) -> UiNode {
        ctx.synthetic_container(node, style, vec![], |props| {
            props.width = Sizing::Fixed(SEPARATOR_WIDTH);
            props.height = Sizing::Fixed(SEPARATOR_HEIGHT);
            if props.background.is_none() {
                props.background = parse_color(SEPARATOR_DEFAULT_COLOR);
            }
            // SSR: a presentation-only stroke. `role="separator"` +
            // `aria-orientation="vertical"` is the WAI-ARIA pattern for
            // toolbar dividers; the walker keeps the default `<div>`
            // tag since there's no native HTML element for "vertical
            // 1-pixel divider".
            props.semantic = prism_ui_runtime::layout::Semantic::default()
                .with_role("separator")
                .with_attr("aria-orientation", "vertical");
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use prism_builder::document::Node as BuilderNode;
    use prism_builder::layout::LayoutMode;
    use prism_builder::style::StyleProperties as Cascade;
    use prism_core::foundation::spatial::Transform2D;
    use serde_json::json;

    fn lower_one(node: &BuilderNode) -> UiNode {
        let block = ToolbarSeparator {
            id: "shell.toolbar-separator".into(),
        };
        let cascade = Cascade::default();
        let ctx = LowerCtx::new(None, &cascade);
        block.lower_ui(&ctx, node, &cascade)
    }

    fn separator() -> BuilderNode {
        BuilderNode {
            id: "sep".into(),
            component: "shell.toolbar-separator".into(),
            props: json!({}),
            children: vec![],
            layout_mode: LayoutMode::default(),
            transform: Transform2D::default(),
            modifiers: vec![],
            style: Cascade::default(),
        }
    }

    #[test]
    fn lowers_to_1x20_translucent_stroke() {
        let ui = lower_one(&separator());
        if let UiNode::Container {
            props, children, ..
        } = ui
        {
            assert_eq!(props.width, Sizing::Fixed(1.0));
            assert_eq!(props.height, Sizing::Fixed(20.0));
            assert!(props.background.is_some());
            assert!(children.is_empty());
            assert_eq!(props.semantic.role.as_deref(), Some("separator"));
        } else {
            panic!("not a container")
        }
    }
}
