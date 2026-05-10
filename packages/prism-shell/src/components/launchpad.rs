//! `shell.launchpad` — Studio home screen. Centres a vertical column
//! consisting of a hero title and a row of [`shell.app-card`]
//! children authored from `.prism-ui` source.
//!
//! Slint origin: the launchpad branch of `AppWindow` in
//! `ui/app.slint` around line 1872.
//!
//! Smart pattern: composition over `host_children` for the card row.
//! The hero title is a single `title` prop string lowered as a
//! plain text node — no embedded chrome dispatch since the launchpad
//! is the *first* surface a user sees and we don't want a registry
//! roundtrip on the boot path.

use prism_builder::{
    document::Node,
    registry::FieldSpec,
    style::StyleProperties,
    ui_lower::{bare_container, colored_text_node, prop_string, LowerCtx},
};
use prism_ui_runtime::layout::{Direction, Node as UiNode, Padding, Semantic, Sizing};

const TITLE_FONT: f32 = 24.0;
const TITLE_COLOR: &str = "#000000";
const CARDS_GAP: f32 = 16.0;

fn launchpad_schema() -> Vec<FieldSpec> {
    vec![FieldSpec::text("title", "Hero title")]
}

fn launchpad_lower(ctx: &LowerCtx<'_>, node: &Node, _style: &StyleProperties) -> UiNode {
    let title = prop_string(node, "title");
    let title_node = colored_text_node(
        format!("{}::title", node.id),
        if title.is_empty() {
            "Welcome to Prism".into()
        } else {
            title
        },
        &StyleProperties::default(),
        TITLE_FONT,
        TITLE_COLOR,
    );

    let card_children = ctx
        .host_children()
        .map(|s| s.to_vec())
        .unwrap_or_else(|| ctx.lower_children(&node.children));
    let cards_row = bare_container(format!("{}::cards", node.id), card_children, |p| {
        p.direction = Direction::Row;
        p.gap = CARDS_GAP;
        p.semantic = Semantic::tag("div").with_attr("data-role", "app-card-row");
    });

    bare_container(node.id.clone(), vec![title_node, cards_row], |p| {
        p.direction = Direction::Column;
        p.gap = 24.0;
        p.padding = Padding {
            left: 32.0,
            right: 32.0,
            top: 64.0,
            bottom: 32.0,
        };
        p.width = Sizing::Grow;
        p.height = Sizing::Grow;
        p.semantic = Semantic::tag("section").with_attr("data-role", "launchpad");
    })
}

pub const LAUNCHPAD_SPEC: prism_builder::BlockSpec =
    prism_builder::BlockSpec::new("shell.launchpad", launchpad_schema).lower(launchpad_lower);

#[cfg(test)]
mod tests {
    use super::*;

    use prism_builder::document::Node as BuilderNode;
    use prism_builder::layout::LayoutMode;
    use prism_core::foundation::spatial::Transform2D;
    use serde_json::json;

    fn lower(props: serde_json::Value) -> UiNode {
        let n = BuilderNode {
            id: "lp".into(),
            component: "shell.launchpad".into(),
            props,
            children: vec![],
            layout_mode: LayoutMode::default(),
            transform: Transform2D::default(),
            modifiers: vec![],
            style: StyleProperties::default(),
        };
        let cascade = StyleProperties::default();
        let ctx = LowerCtx::new(None, &cascade);
        launchpad_lower(&ctx, &n, &cascade)
    }

    #[test]
    fn renders_title_and_card_row() {
        let ui = lower(json!({ "title": "Prism Studio" }));
        let UiNode::Container { children, .. } = ui else {
            panic!()
        };
        assert_eq!(children.len(), 2);
        let UiNode::Text { content, .. } = &children[0] else {
            panic!()
        };
        assert_eq!(content, "Prism Studio");
    }

    #[test]
    fn default_title_when_unset() {
        let ui = lower(json!({}));
        let UiNode::Container { children, .. } = ui else {
            panic!()
        };
        let UiNode::Text { content, .. } = &children[0] else {
            panic!()
        };
        assert_eq!(content, "Welcome to Prism");
    }
}
