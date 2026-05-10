//! `shell.docs-content` — title + summary + optional body block used in
//! the launchpad, the docs panel, and any "what is this thing?" surface.
//!
//! Slint origin: `DocsContent` in `ui/app.slint` (lines 1302-1345).
//!
//! Lowering: a column of three text rows separated by spacers, plus an
//! optional hairline + body when `doc-body` is non-empty. The `compact`
//! prop folds in at lower-time — purely a font-size / spacing dial,
//! resolved by a tiny `metrics` table rather than per-row branching.
//!
//! Everything composes from existing helpers (`bare_container`,
//! `colored_text_node`, `parse_color`, `prop_*`); no hand-rolled
//! `UiNode::Container { … }` literals.

use prism_builder::{
    common_signals,
    document::Node,
    registry::FieldSpec,
    style::StyleProperties,
    ui_lower::{bare_container, colored_text_node, parse_color, prop_bool, prop_string, LowerCtx},
};
use prism_ui_runtime::layout::{Direction, Node as UiNode, Semantic, Sizing};
use serde_json::Value;

const TITLE_COLOR: &str = "#000000";
const SUMMARY_COLOR: &str = "#cc000000";
const BODY_COLOR: &str = "#b3000000";
const HAIRLINE_COLOR: &str = "#26000000";

/// Pixel + font sizing for the two display modes. Single source of
/// truth — every row that varies between compact and full reads from
/// here, so the lowering body itself stays branch-free.
struct Metrics {
    title_size: f32,
    summary_size: f32,
    body_size: f32,
    title_to_summary_gap: f32,
    summary_to_body_gap: f32,
}

const FULL: Metrics = Metrics {
    title_size: 22.0,
    summary_size: 14.0,
    body_size: 13.0,
    title_to_summary_gap: 12.0,
    summary_to_body_gap: 16.0,
};

const COMPACT: Metrics = Metrics {
    title_size: 14.0,
    summary_size: 12.0,
    body_size: 12.0,
    title_to_summary_gap: 8.0,
    summary_to_body_gap: 10.0,
};

fn docs_content_schema() -> Vec<FieldSpec> {
    vec![
        FieldSpec::text("doc-title", "Title").required(),
        FieldSpec::text("doc-summary", "Summary"),
        FieldSpec::textarea("doc-body", "Body"),
        FieldSpec::boolean("compact", "Compact").with_default(Value::Bool(false)),
    ]
}

fn docs_content_signals() -> Vec<prism_builder::signal::SignalDef> {
    common_signals()
}

fn docs_content_lower(ctx: &LowerCtx<'_>, node: &Node, style: &StyleProperties) -> UiNode {
    let title = prop_string(node, "doc-title");
    let summary = prop_string(node, "doc-summary");
    let body = prop_string(node, "doc-body");
    let compact = prop_bool(node, "compact", false);
    let m = if compact { &COMPACT } else { &FULL };

    let mut children = vec![
        colored_text_node(
            format!("{}::title", node.id),
            title,
            style,
            m.title_size,
            TITLE_COLOR,
        ),
        spacer_h(format!("{}::g1", node.id), m.title_to_summary_gap),
        colored_text_node(
            format!("{}::summary", node.id),
            summary,
            style,
            m.summary_size,
            SUMMARY_COLOR,
        ),
    ];
    if !body.is_empty() {
        children.push(spacer_h(format!("{}::g2", node.id), m.summary_to_body_gap));
        children.push(hairline(format!("{}::rule", node.id)));
        children.push(spacer_h(format!("{}::g3", node.id), m.summary_to_body_gap));
        children.push(colored_text_node(
            format!("{}::body", node.id),
            body,
            style,
            m.body_size,
            BODY_COLOR,
        ));
    }

    // Use the registry-aware default `synthetic_container` so cascade
    // sizing and grow/fit semantics from the parent flow through
    // unchanged. Compact / full only varies font + gap, so all
    // structural sizing stays cascade-driven.
    ctx.synthetic_container(node, style, children, |props| {
        props.direction = Direction::Column;
        // Wrap in <article> for SSR — semantically a self-contained
        // composition (title + body) that screen readers should
        // announce as a unit.
        props.semantic = Semantic::tag("article");
    })
}

pub const DOCS_CONTENT_SPEC: prism_builder::BlockSpec =
    prism_builder::BlockSpec::new("shell.docs-content", docs_content_schema)
        .lower(docs_content_lower)
        .signals(docs_content_signals);

fn spacer_h(id: String, h: f32) -> UiNode {
    bare_container(id, vec![], |p| {
        p.height = Sizing::Fixed(h);
    })
}

fn hairline(id: String) -> UiNode {
    bare_container(id, vec![], |p| {
        p.height = Sizing::Fixed(1.0);
        p.background = parse_color(HAIRLINE_COLOR);
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use prism_builder::document::Node as BuilderNode;
    use prism_builder::layout::LayoutMode;
    use prism_builder::style::StyleProperties as Cascade;
    use prism_builder::Block;
    use prism_core::foundation::spatial::Transform2D;
    use serde_json::json;

    fn lower_one(node: &BuilderNode) -> UiNode {
        let cascade = Cascade::default();
        let ctx = LowerCtx::new(None, &cascade);
        docs_content_lower(&ctx, node, &cascade)
    }

    fn doc_node(props: Value) -> BuilderNode {
        BuilderNode {
            id: "d".into(),
            component: "shell.docs-content".into(),
            props,
            children: vec![],
            layout_mode: LayoutMode::default(),
            transform: Transform2D::default(),
            modifiers: vec![],
            style: Cascade::default(),
        }
    }

    #[test]
    fn full_mode_omits_body_section_when_body_blank() {
        let ui = lower_one(&doc_node(json!({
            "doc-title": "Hi", "doc-summary": "Sub"
        })));
        if let UiNode::Container {
            children, props, ..
        } = ui
        {
            assert_eq!(props.semantic.tag.as_deref(), Some("article"));
            // 3 rows: title + gap + summary, no body half.
            assert_eq!(children.len(), 3);
        } else {
            panic!("not a container")
        }
    }

    #[test]
    fn body_mode_adds_hairline_and_body_text() {
        let ui = lower_one(&doc_node(json!({
            "doc-title": "Hi", "doc-summary": "Sub", "doc-body": "Long body text"
        })));
        if let UiNode::Container { children, .. } = ui {
            // 3 + spacer + hairline + spacer + body = 7
            assert_eq!(children.len(), 7);
            // last child is the body text
            assert!(matches!(&children[6], UiNode::Text { .. }));
        } else {
            panic!("not a container")
        }
    }

    #[test]
    fn compact_mode_uses_smaller_title_size() {
        let ui = lower_one(&doc_node(json!({
            "doc-title": "Hi", "doc-summary": "Sub", "compact": true
        })));
        if let UiNode::Container { children, .. } = ui {
            if let UiNode::Text { props, .. } = &children[0] {
                assert_eq!(props.font_size, COMPACT.title_size);
            } else {
                panic!("first child not text")
            }
        }
    }

    #[test]
    fn schema_declares_four_fields() {
        let block = prism_builder::SpecBlock::new(&super::DOCS_CONTENT_SPEC);
        let keys: Vec<String> = block.schema().into_iter().map(|f| f.key).collect();
        assert_eq!(
            keys,
            vec!["doc-title", "doc-summary", "doc-body", "compact"]
        );
    }
}
