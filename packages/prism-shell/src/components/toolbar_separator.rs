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
};
use prism_ui_runtime::layout::{Node as UiNode, Sizing};

const SEPARATOR_WIDTH: f32 = 1.0;
const SEPARATOR_HEIGHT: f32 = 20.0;
/// Foreground transparentize(80%) — `Palette.foreground` is the shell's
/// dark text colour, so 20% opacity over the toolbar background reads
/// as a faint stroke. Until the design-tokens cascade lands, hard-code
/// the resolved value.
const SEPARATOR_DEFAULT_COLOR: &str = "#33000000";

fn toolbar_separator_schema() -> Vec<FieldSpec> {
    vec![]
}

fn toolbar_separator_lower(ctx: &LowerCtx<'_>, node: &Node, style: &StyleProperties) -> UiNode {
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

pub const TOOLBAR_SEPARATOR_SPEC: prism_builder::BlockSpec =
    prism_builder::BlockSpec::new("shell.toolbar-separator", toolbar_separator_schema)
        .lower(toolbar_separator_lower);

#[cfg(test)]
mod tests {
    use super::*;

    use crate::components::testing::{lower_with, test_node};
    use prism_builder::document::Node as BuilderNode;
    use serde_json::json;

    fn lower_one(node: &BuilderNode) -> UiNode {
        lower_with(node, toolbar_separator_lower)
    }

    fn separator() -> BuilderNode {
        test_node("sep", "shell.toolbar-separator", json!({}))
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
