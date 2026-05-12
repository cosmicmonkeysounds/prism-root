//! `shell.code-editor` — code editor panel content. Reads a
//! `lines` JSON array (`[{ number, text, fold-state? }, …]`) plus
//! optional `language`, `cursor-line`, `cursor-column`, and renders
//! a monospaced column with a left-rail line-number gutter. The
//! actual syntax-highlight pass and caret rendering are the
//! renderer's job; this block emits a deterministic retained tree
//! suitable for SSR / accessibility traversal.
//!
//! Slint origin: code editor panel content (`ui/app.slint` panel
//! routing around line 1981).

use prism_builder::{
    document::Node,
    registry::FieldSpec,
    signal::SignalDef,
    style::StyleProperties,
    ui_lower::{bare_container, colored_text_node, parse_color, uniform_radius, LowerCtx},
    with_common_signals,
};
use prism_ui_runtime::layout::{Direction, Node as UiNode, Padding, Semantic, Sizing};
use serde_json::Value;

const GUTTER_BG: &str = "#08000000";
const GUTTER_COLOR: &str = "#80000000";
const TEXT_COLOR: &str = "#101218";
const STATUS_BG: &str = "#06000000";
const STATUS_COLOR: &str = "#80000000";
const LINE_HEIGHT: f32 = 18.0;

fn code_editor_schema() -> Vec<FieldSpec> {
    vec![
        FieldSpec::text("language", "Language"),
        FieldSpec::text("lines", "Lines (JSON array of {number, text, fold-state?})"),
        FieldSpec::number(
            "cursor-line",
            "Cursor line (1-based)",
            prism_builder::registry::NumericBounds::min(0.0),
        )
        .with_default(Value::from(0.0)),
        FieldSpec::number(
            "cursor-column",
            "Cursor column (1-based)",
            prism_builder::registry::NumericBounds::min(0.0),
        )
        .with_default(Value::from(0.0)),
    ]
}

fn code_editor_signals() -> Vec<prism_builder::signal::SignalDef> {
    with_common_signals(vec![
        SignalDef::new("line-clicked", "Line clicked."),
        SignalDef::new("fold-toggled", "Fold caret pressed."),
    ])
}

fn code_editor_lower(ctx: &LowerCtx<'_>, node: &Node, _style: &StyleProperties) -> UiNode {
    let style = StyleProperties::default();
    let language = ctx.prop_str(node, "language");
    let cursor_line = node
        .props
        .get("cursor-line")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0) as i64;
    let cursor_col = node
        .props
        .get("cursor-column")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0) as i64;

    let lines: Vec<&Value> = node
        .props
        .get("lines")
        .and_then(|v| v.as_array())
        .map(|a| a.iter().collect())
        .unwrap_or_default();

    let gutter_kids: Vec<UiNode> = lines
        .iter()
        .enumerate()
        .map(|(idx, item)| {
            let n = item
                .get("number")
                .and_then(|v| v.as_i64())
                .unwrap_or(idx as i64 + 1);
            colored_text_node(
                format!("{}::gutter::{}", node.id, idx),
                n.to_string(),
                &style,
                11.0,
                GUTTER_COLOR,
            )
        })
        .collect();

    let line_kids: Vec<UiNode> = lines
        .iter()
        .enumerate()
        .map(|(idx, item)| {
            let txt = item.get("text").and_then(|v| v.as_str()).unwrap_or("");
            let n = item
                .get("number")
                .and_then(|v| v.as_i64())
                .unwrap_or(idx as i64 + 1);
            let is_cursor = n == cursor_line && cursor_line > 0;
            bare_container(
                format!("{}::line::{}", node.id, idx),
                vec![colored_text_node(
                    format!("{}::line::{}::text", node.id, idx),
                    txt.into(),
                    &style,
                    12.0,
                    TEXT_COLOR,
                )],
                |p| {
                    p.direction = Direction::Row;
                    p.height = Sizing::Fixed(LINE_HEIGHT);
                    p.padding = Padding {
                        left: 8.0,
                        right: 8.0,
                        top: 0.0,
                        bottom: 0.0,
                    };
                    let mut s = Semantic::tag("div")
                        .with_attr("role", "row")
                        .with_attr("data-role", "code-line")
                        .with_attr("data-line", n.to_string());
                    if is_cursor {
                        s = s.with_attr("data-cursor", "true");
                    }
                    p.semantic = s;
                },
            )
        })
        .collect();

    let gutter = bare_container(format!("{}::gutter", node.id), gutter_kids, |p| {
        p.direction = Direction::Column;
        p.padding = Padding {
            left: 8.0,
            right: 8.0,
            top: 6.0,
            bottom: 6.0,
        };
        p.width = Sizing::Fixed(48.0);
        p.height = Sizing::Grow;
        p.background = parse_color(GUTTER_BG);
        p.semantic = Semantic::tag("div")
            .with_attr("role", "presentation")
            .with_attr("data-role", "code-gutter");
    });

    let body = bare_container(format!("{}::body", node.id), line_kids, |p| {
        p.direction = Direction::Column;
        p.padding = Padding {
            left: 0.0,
            right: 0.0,
            top: 6.0,
            bottom: 6.0,
        };
        p.width = Sizing::Grow;
        p.height = Sizing::Grow;
        p.semantic = Semantic::tag("div")
            .with_attr("role", "rowgroup")
            .with_attr("data-role", "code-body");
    });

    let editor_row = bare_container(format!("{}::editor", node.id), vec![gutter, body], |p| {
        p.direction = Direction::Row;
        p.width = Sizing::Grow;
        p.height = Sizing::Grow;
        p.radius = uniform_radius(4.0);
    });

    let status = bare_container(
        format!("{}::status", node.id),
        vec![colored_text_node(
            format!("{}::status::label", node.id),
            format!(
                "{}{}",
                if language.is_empty() {
                    "plain".to_string()
                } else {
                    language
                },
                if cursor_line > 0 {
                    format!(" · Ln {cursor_line}, Col {cursor_col}")
                } else {
                    String::new()
                }
            ),
            &style,
            10.0,
            STATUS_COLOR,
        )],
        |p| {
            p.direction = Direction::Row;
            p.padding = Padding {
                left: 10.0,
                right: 10.0,
                top: 4.0,
                bottom: 4.0,
            };
            p.height = Sizing::Fixed(22.0);
            p.background = parse_color(STATUS_BG);
            p.semantic = Semantic::tag("div")
                .with_attr("role", "status")
                .with_attr("data-role", "code-status");
        },
    );

    bare_container(node.id.clone(), vec![editor_row, status], |p| {
        p.direction = Direction::Column;
        p.width = Sizing::Grow;
        p.height = Sizing::Grow;
        p.semantic = Semantic::tag("section")
            .with_attr("role", "textbox")
            .with_attr("aria-multiline", "true")
            .with_attr("aria-label", "Code editor")
            .with_attr("data-role", "code-editor");
    })
}

pub const CODE_EDITOR_SPEC: prism_builder::BlockSpec =
    prism_builder::BlockSpec::new("shell.code-editor", code_editor_schema)
        .lower(code_editor_lower)
        .signals(code_editor_signals);

#[cfg(test)]
mod tests {
    use super::*;

    use crate::components::testing::{lower_with, test_node};
    use serde_json::json;

    fn lower(props: Value) -> UiNode {
        let n = test_node("ce", "shell.code-editor", props);
        lower_with(&n, code_editor_lower)
    }

    #[test]
    fn renders_lines_and_gutter() {
        let ui = lower(json!({
            "language": "rust",
            "cursor-line": 2,
            "cursor-column": 5,
            "lines": [
                { "number": 1, "text": "fn main() {" },
                { "number": 2, "text": "    println!(\"hi\");" },
                { "number": 3, "text": "}" },
            ],
        }));
        let UiNode::Container { children, .. } = ui else {
            panic!()
        };
        // editor + status
        assert_eq!(children.len(), 2);
        let UiNode::Container {
            children: editor, ..
        } = &children[0]
        else {
            panic!()
        };
        // gutter + body
        assert_eq!(editor.len(), 2);
        let UiNode::Container {
            children: body_kids,
            ..
        } = &editor[1]
        else {
            panic!()
        };
        assert_eq!(body_kids.len(), 3);
    }

    #[test]
    fn cursor_line_marked_in_data_attr() {
        let ui = lower(json!({
            "cursor-line": 1,
            "lines": [{ "number": 1, "text": "x" }],
        }));
        let UiNode::Container { children, .. } = ui else {
            panic!()
        };
        let UiNode::Container {
            children: editor, ..
        } = &children[0]
        else {
            panic!()
        };
        let UiNode::Container {
            children: body_kids,
            ..
        } = &editor[1]
        else {
            panic!()
        };
        let UiNode::Container { props, .. } = &body_kids[0] else {
            panic!()
        };
        assert!(props
            .semantic
            .attrs
            .iter()
            .any(|(k, v)| k == "data-cursor" && v == "true"));
    }

    #[test]
    fn empty_lines_still_render_status_strip() {
        let ui = lower(json!({}));
        let UiNode::Container { children, .. } = ui else {
            panic!()
        };
        assert_eq!(children.len(), 2);
    }
}
