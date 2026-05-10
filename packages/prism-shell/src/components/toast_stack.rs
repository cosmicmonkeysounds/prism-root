//! `shell.toast-stack` — composition wrapper that hosts a vertical
//! stack of [`shell.toast`] children. The host typically mounts the
//! whole stack onto the runtime's overlay z-layer with a
//! `BottomRight` anchor.
//!
//! Authoring shape:
//!
//! ```prism-ui
//! <shell.toast-stack id="toasts">
//!   <shell.toast id="t1" title="Saved" kind="success"/>
//! </shell.toast-stack>
//! ```
//!
//! Smart pattern: composition over `host_children` (§14). Plain leaves
//! such as `shell.toast` ignore the slot; this wrapper consumes it
//! so the toast arrangement (gap, ordering, semantic role) lives
//! exactly once.

use prism_builder::{
    document::Node,
    registry::FieldSpec,
    style::StyleProperties,
    ui_lower::{bare_container, LowerCtx},
};
use prism_ui_runtime::layout::{Direction, Node as UiNode, Padding, Semantic};

const STACK_GAP: f32 = 8.0;

fn toast_stack_schema() -> Vec<FieldSpec> {
    vec![]
}

fn toast_stack_lower(ctx: &LowerCtx<'_>, node: &Node, _style: &StyleProperties) -> UiNode {
    let kids = ctx
        .host_children()
        .map(|s| s.to_vec())
        .unwrap_or_else(|| ctx.lower_children(&node.children));
    bare_container(node.id.clone(), kids, |p| {
        p.direction = Direction::Column;
        p.gap = STACK_GAP;
        p.padding = Padding {
            left: 12.0,
            right: 12.0,
            top: 12.0,
            bottom: 12.0,
        };
        p.semantic = Semantic::tag("div")
            .with_attr("role", "region")
            .with_attr("aria-label", "Notifications")
            .with_attr("data-role", "toast-stack");
    })
}

pub const TOAST_STACK_SPEC: prism_builder::BlockSpec =
    prism_builder::BlockSpec::new("shell.toast-stack", toast_stack_schema).lower(toast_stack_lower);

#[cfg(test)]
mod tests {
    use super::*;

    use prism_builder::document::Node as BuilderNode;
    use prism_builder::layout::LayoutMode;
    use prism_core::foundation::spatial::Transform2D;
    use serde_json::json;

    #[test]
    fn empty_stack() {
        let n = BuilderNode {
            id: "ts".into(),
            component: "shell.toast-stack".into(),
            props: json!({}),
            children: vec![],
            layout_mode: LayoutMode::default(),
            transform: Transform2D::default(),
            modifiers: vec![],
            style: StyleProperties::default(),
        };
        let cascade = StyleProperties::default();
        let ctx = LowerCtx::new(None, &cascade);
        let UiNode::Container {
            children, props, ..
        } = toast_stack_lower(&ctx, &n, &cascade)
        else {
            panic!()
        };
        assert!(children.is_empty());
        assert_eq!(props.direction, Direction::Column);
        assert!(props
            .semantic
            .attrs
            .iter()
            .any(|(k, v)| k == "data-role" && v == "toast-stack"));
    }
}
