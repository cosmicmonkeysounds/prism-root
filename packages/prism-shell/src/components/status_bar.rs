//! `shell.status-bar` — 26px footer strip at the bottom of the app
//! window. Renders either a single status label (back-compat with
//! the `text` / `status` prop shape used since the §16 frame-chrome
//! step) or — when the host emits a `segments` array — a multi-
//! segment pipe-separated strip ("Editor | Flux | Canvas | Selected
//! | 1 node | Prism Studio") matching the DaVinci-style footer the
//! Slint shell carried. §43 D5.
//!
//! Smart pattern: leaf primitive — no embedded chrome, no
//! `host_children`. The kind-driven body picks `single` vs.
//! `segments` from the props bag; adding a new layout (icon-prefixed
//! segments, e.g.) extends `kind_body` without touching the outer
//! frame.

use prism_builder::{
    document::Node,
    registry::FieldSpec,
    style::StyleProperties,
    ui_lower::{bare_container, colored_text_node, parse_color, LowerCtx},
};
use prism_ui_runtime::layout::{Direction, Node as UiNode, Padding, Semantic, Sizing};

const STATUS_BAR_HEIGHT: f32 = 26.0;
const STATUS_BAR_BG: &str = "#08000000";
const STATUS_TEXT_COLOR: &str = "#99000000";
const STATUS_SEP_COLOR: &str = "#44000000";
const STATUS_FONT_SIZE: f32 = 11.0;
const SEGMENT_GAP: f32 = 10.0;
const SEPARATOR_GLYPH: &str = "|";

fn status_bar_schema() -> Vec<FieldSpec> {
    vec![
        FieldSpec::text("text", "Status text"),
        FieldSpec::text("segments", "Segments (JSON array)"),
    ]
}

fn status_bar_lower(ctx: &LowerCtx<'_>, node: &Node, _style: &StyleProperties) -> UiNode {
    let cascade = StyleProperties::default();
    // The §43 D5 binding emits a `segments` array; legacy / headless
    // hosts may still pass a single `text` (or `status`) string. The
    // segments path wins when populated, falls through to the single-
    // label path otherwise.
    let children = segment_children(node, &cascade).unwrap_or_else(|| {
        let text = single_status_text(ctx, node);
        vec![colored_text_node(
            format!("{}::label", node.id),
            text,
            &cascade,
            STATUS_FONT_SIZE,
            STATUS_TEXT_COLOR,
        )]
    });
    bare_container(node.id.clone(), children, |p| {
        p.direction = Direction::Row;
        p.height = Sizing::Fixed(STATUS_BAR_HEIGHT);
        p.gap = SEGMENT_GAP;
        p.padding = Padding {
            left: 12.0,
            right: 12.0,
            top: 0.0,
            bottom: 0.0,
        };
        p.background = parse_color(STATUS_BAR_BG);
        p.semantic = Semantic::tag("footer").with_attr("role", "contentinfo");
    })
}

/// Build the pipe-separated body when the host supplies a `segments`
/// JSON array. Returns `None` when the prop is missing / empty so the
/// caller can fall through to the single-label path.
fn segment_children(node: &Node, cascade: &StyleProperties) -> Option<Vec<UiNode>> {
    let arr = node.props.get("segments")?.as_array()?;
    if arr.is_empty() {
        return None;
    }
    let mut out: Vec<UiNode> = Vec::with_capacity(arr.len() * 2);
    for (idx, item) in arr.iter().enumerate() {
        let text = match item {
            serde_json::Value::String(s) => s.clone(),
            other => other.to_string(),
        };
        if text.is_empty() {
            continue;
        }
        if !out.is_empty() {
            out.push(colored_text_node(
                format!("{}::sep::{}", node.id, idx),
                SEPARATOR_GLYPH.into(),
                cascade,
                STATUS_FONT_SIZE,
                STATUS_SEP_COLOR,
            ));
        }
        out.push(colored_text_node(
            format!("{}::seg::{}", node.id, idx),
            text,
            cascade,
            STATUS_FONT_SIZE,
            STATUS_TEXT_COLOR,
        ));
    }
    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

/// Single-label fallback. The §43 D5 binding emits both `status` and
/// `segments`; older hosts emit only `text`. Reading both keys keeps
/// the single-label call sites working without per-host branching.
fn single_status_text(ctx: &LowerCtx<'_>, node: &Node) -> String {
    let text = ctx.prop_str(node, "text");
    if !text.is_empty() {
        return text;
    }
    ctx.prop_str(node, "status")
}

pub const STATUS_BAR_SPEC: prism_builder::BlockSpec =
    prism_builder::BlockSpec::new("shell.status-bar", status_bar_schema).lower(status_bar_lower);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::testing::{lower_with, test_node};
    use serde_json::json;

    fn lower(props: serde_json::Value) -> UiNode {
        lower_with(
            &test_node("sb", "shell.status-bar", props),
            status_bar_lower,
        )
    }

    #[test]
    fn renders_text_at_fixed_height() {
        let ui = lower(json!({ "text": "Saved." }));
        let UiNode::Container {
            children, props, ..
        } = ui
        else {
            panic!()
        };
        assert_eq!(props.height, Sizing::Fixed(STATUS_BAR_HEIGHT));
        let UiNode::Text { content, .. } = &children[0] else {
            panic!()
        };
        assert_eq!(content, "Saved.");
    }

    #[test]
    fn empty_text_when_prop_missing() {
        let ui = lower(json!({}));
        let UiNode::Container { children, .. } = ui else {
            panic!()
        };
        let UiNode::Text { content, .. } = &children[0] else {
            panic!()
        };
        assert_eq!(content, "");
    }

    #[test]
    fn footer_semantic_role() {
        let ui = lower(json!({}));
        let UiNode::Container { props, .. } = ui else {
            panic!()
        };
        assert_eq!(props.semantic.tag.as_deref(), Some("footer"));
    }

    #[test]
    fn segments_array_renders_pipe_separated_strip() {
        // §43 D5: when `segments` is non-empty, the body becomes a
        // pipe-separated list (segment / separator / segment / …).
        let ui = lower(json!({
            "segments": ["Editor", "Flux", "Canvas", "1 node", "Prism Studio"],
        }));
        let UiNode::Container { children, .. } = ui else {
            panic!()
        };
        // 5 segments + 4 separators between them = 9 children.
        assert_eq!(children.len(), 9);
        let texts: Vec<String> = children
            .iter()
            .map(|c| match c {
                UiNode::Text { content, .. } => content.clone(),
                _ => "<non-text>".into(),
            })
            .collect();
        assert_eq!(texts[0], "Editor");
        assert_eq!(texts[1], SEPARATOR_GLYPH);
        assert_eq!(texts[2], "Flux");
        assert_eq!(texts[3], SEPARATOR_GLYPH);
        assert_eq!(texts[4], "Canvas");
        assert_eq!(texts[8], "Prism Studio");
    }

    #[test]
    fn empty_segments_falls_back_to_text_label() {
        let ui = lower(json!({ "segments": [], "text": "Hello" }));
        let UiNode::Container { children, .. } = ui else {
            panic!()
        };
        // No segments → single-label path.
        assert_eq!(children.len(), 1);
        let UiNode::Text { content, .. } = &children[0] else {
            panic!()
        };
        assert_eq!(content, "Hello");
    }

    #[test]
    fn segments_skip_empty_strings() {
        // Empty selection / unset segment renders as a skipped slot —
        // never a stray double separator.
        let ui = lower(json!({
            "segments": ["Editor", "", "Canvas"],
        }));
        let UiNode::Container { children, .. } = ui else {
            panic!()
        };
        // 2 visible segments + 1 separator between them.
        assert_eq!(children.len(), 3);
    }
}
