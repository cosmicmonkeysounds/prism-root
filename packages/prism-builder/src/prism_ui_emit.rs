//! `BuilderDocument` / `Node` → `.prism-ui` source emitter.
//!
//! Post-Slint replacement for the deleted source emitter. A single
//! declarative walker turns the authoritative `BuilderDocument` tree
//! into well-formed `.prism-ui` text suitable for round-tripping
//! through `prism_core::language::prism_ui::parse`. The legacy
//! `Page::source` auto-emit path was retired alongside the Slint
//! runtime in the Phase 5 cutover — `prism_ui_emit::emit_document` is
//! the only seam left, called explicitly by hosts that want source.
//!
//! ## Shape
//!
//! - One pass, depth-first. No two-step IR. No registry dependency —
//!   the `Node` already carries its `component` string.
//! - Two-space indent per nesting level.
//! - Self-closing tags when `children.is_empty()`.
//! - Attributes are emitted in alphabetical order so two equivalent
//!   trees produce byte-identical sources (deterministic for diff /
//!   golden tests).
//!
//! ## Attribute serialisation (declarative table)
//!
//! | `Value` shape | Emitted as |
//! |---|---|
//! | `Null` | omitted |
//! | `Bool(false)` | omitted (boolean-attribute semantics: absent = false) |
//! | `Bool(true)` | bare attribute name (`disabled`) |
//! | `Number(n)` | `key="<n>"` |
//! | `String(s)` | `key="<escaped>"` |
//! | `Array(_)` / `Object(_)` | `key={<compact JSON>}` (parser sees an `{expr}` interpolation) |
//!
//! Object/array values use `{...}` interpolation rather than a quoted
//! string because the grammar's `parse_quoted_value` treats `{` as the
//! start of an interpolation regardless of which quote opened the
//! value — so `key="{...}"` would mis-parse. Wrapping the JSON in
//! `{...}` lets the parser take it as an `AttributeValue::Expression`
//! whose body is the JSON literal, which is the only round-trip-safe
//! shape today.

use serde_json::Value;

use crate::document::{BuilderDocument, Node};

/// Emit a `BuilderDocument` as `.prism-ui` source text. Empty
/// document → empty string.
pub fn emit_document(doc: &BuilderDocument) -> String {
    let mut out = String::new();
    if let Some(root) = doc.root.as_ref() {
        emit_node_into(root, 0, &mut out);
    }
    out
}

/// Emit a single `Node` (and its children) as `.prism-ui` source text.
pub fn emit_node(node: &Node) -> String {
    let mut out = String::new();
    emit_node_into(node, 0, &mut out);
    out
}

fn emit_node_into(node: &Node, depth: usize, out: &mut String) {
    push_indent(out, depth);
    out.push('<');
    out.push_str(&node.component);

    if !node.id.is_empty() {
        out.push_str(" id=\"");
        push_attr_escaped(out, &node.id);
        out.push('"');
    }

    if let Value::Object(map) = &node.props {
        let mut entries: Vec<(&String, &Value)> = map.iter().collect();
        entries.sort_by(|a, b| a.0.cmp(b.0));
        for (k, v) in entries {
            emit_attr(out, k, v);
        }
    }

    if node.children.is_empty() {
        out.push_str("/>\n");
        return;
    }

    out.push_str(">\n");
    for child in &node.children {
        emit_node_into(child, depth + 1, out);
    }
    push_indent(out, depth);
    out.push_str("</");
    out.push_str(&node.component);
    out.push_str(">\n");
}

fn emit_attr(out: &mut String, key: &str, value: &Value) {
    match value {
        Value::Null => {}
        Value::Bool(false) => {}
        Value::Bool(true) => {
            out.push(' ');
            out.push_str(key);
        }
        Value::Number(n) => {
            out.push(' ');
            out.push_str(key);
            out.push_str("=\"");
            out.push_str(&n.to_string());
            out.push('"');
        }
        Value::String(s) => {
            out.push(' ');
            out.push_str(key);
            out.push_str("=\"");
            push_attr_escaped(out, s);
            out.push('"');
        }
        Value::Array(_) | Value::Object(_) => {
            let json = serde_json::to_string(value).unwrap_or_else(|_| "null".into());
            out.push(' ');
            out.push_str(key);
            out.push_str("={");
            out.push_str(&json);
            out.push('}');
        }
    }
}

fn push_indent(out: &mut String, depth: usize) {
    for _ in 0..depth {
        out.push_str("  ");
    }
}

fn push_attr_escaped(out: &mut String, s: &str) {
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use prism_core::language::prism_ui::parse;
    use serde_json::json;

    fn n(component: &str, id: &str, props: Value, children: Vec<Node>) -> Node {
        Node {
            id: id.into(),
            component: component.into(),
            props,
            children,
            ..Default::default()
        }
    }

    #[test]
    fn empty_document_emits_empty_string() {
        let doc = BuilderDocument::default();
        assert_eq!(emit_document(&doc), "");
    }

    #[test]
    fn leaf_self_closes() {
        let node = n("text", "t1", json!({ "value": "hi" }), vec![]);
        let out = emit_node(&node);
        assert_eq!(out, "<text id=\"t1\" value=\"hi\"/>\n");
    }

    #[test]
    fn parent_emits_open_close() {
        let node = n(
            "container",
            "c1",
            Value::Null,
            vec![n("text", "t1", json!({ "value": "a" }), vec![])],
        );
        let out = emit_node(&node);
        assert_eq!(
            out,
            "<container id=\"c1\">\n  <text id=\"t1\" value=\"a\"/>\n</container>\n"
        );
    }

    #[test]
    fn boolean_true_is_bare_false_omitted() {
        let node = n(
            "input",
            "i1",
            json!({ "disabled": true, "readonly": false }),
            vec![],
        );
        let out = emit_node(&node);
        assert_eq!(out, "<input id=\"i1\" disabled/>\n");
    }

    #[test]
    fn null_attrs_omitted() {
        let node = n(
            "text",
            "t1",
            json!({ "value": "hi", "color": null }),
            vec![],
        );
        let out = emit_node(&node);
        assert_eq!(out, "<text id=\"t1\" value=\"hi\"/>\n");
    }

    #[test]
    fn numbers_emit_numerically() {
        let node = n("spacer", "s1", json!({ "size": 16, "ratio": 0.5 }), vec![]);
        let out = emit_node(&node);
        assert_eq!(out, "<spacer id=\"s1\" ratio=\"0.5\" size=\"16\"/>\n");
    }

    #[test]
    fn attrs_alphabetised_for_determinism() {
        let n1 = n("x", "id", json!({ "z": "1", "a": "2", "m": "3" }), vec![]);
        let out = emit_node(&n1);
        // 'a' < 'm' < 'z'
        assert_eq!(out, "<x id=\"id\" a=\"2\" m=\"3\" z=\"1\"/>\n");
    }

    #[test]
    fn quotes_and_backslashes_escape() {
        let node = n("text", "t1", json!({ "value": "a\"b\\c" }), vec![]);
        let out = emit_node(&node);
        assert_eq!(out, "<text id=\"t1\" value=\"a\\\"b\\\\c\"/>\n");
    }

    #[test]
    fn newlines_escape() {
        let node = n("text", "t1", json!({ "value": "line1\nline2" }), vec![]);
        let out = emit_node(&node);
        assert_eq!(out, "<text id=\"t1\" value=\"line1\\nline2\"/>\n");
    }

    #[test]
    fn arrays_emit_as_brace_interpolation() {
        let node = n("tabs", "tb", json!({ "tabs": ["a", "b"] }), vec![]);
        let out = emit_node(&node);
        assert_eq!(out, "<tabs id=\"tb\" tabs={[\"a\",\"b\"]}/>\n");
    }

    #[test]
    fn objects_emit_as_brace_interpolation() {
        let node = n("widget", "w", json!({ "config": { "k": 1 } }), vec![]);
        let out = emit_node(&node);
        assert_eq!(out, "<widget id=\"w\" config={{\"k\":1}}/>\n");
    }

    #[test]
    fn nested_indents_two_spaces_per_level() {
        let leaf = n("text", "t", json!({ "value": "leaf" }), vec![]);
        let mid = n("container", "m", Value::Null, vec![leaf]);
        let root = n("container", "r", Value::Null, vec![mid]);
        let out = emit_node(&root);
        assert_eq!(
            out,
            concat!(
                "<container id=\"r\">\n",
                "  <container id=\"m\">\n",
                "    <text id=\"t\" value=\"leaf\"/>\n",
                "  </container>\n",
                "</container>\n",
            )
        );
    }

    #[test]
    fn document_with_root_emits_at_depth_zero() {
        let doc = BuilderDocument {
            root: Some(n(
                "container",
                "root",
                json!({ "spacing": 16 }),
                vec![n("text", "h1", json!({ "value": "Hello" }), vec![])],
            )),
            ..Default::default()
        };
        let out = emit_document(&doc);
        assert_eq!(
            out,
            concat!(
                "<container id=\"root\" spacing=\"16\">\n",
                "  <text id=\"h1\" value=\"Hello\"/>\n",
                "</container>\n",
            )
        );
    }

    // ── Round-trip discipline ───────────────────────────────────────
    // The output must parse cleanly through the canonical
    // `.prism-ui` parser.

    #[test]
    fn emitted_source_parses_without_errors() {
        let doc = BuilderDocument {
            root: Some(n(
                "container",
                "root",
                json!({ "spacing": 16, "title": "demo" }),
                vec![
                    n("text", "t1", json!({ "value": "with \"quotes\"" }), vec![]),
                    n("button", "b1", json!({ "disabled": true }), vec![]),
                    n("widget", "w1", json!({ "tabs": [1, 2, 3] }), vec![]),
                ],
            )),
            ..Default::default()
        };
        let source = emit_document(&doc);
        let (parsed, errs) = parse(&source);
        assert!(
            errs.is_empty(),
            "parse errors: {errs:?}\n--- source ---\n{source}"
        );
        let elem_count = parsed
            .nodes
            .iter()
            .filter(|n| matches!(n, prism_core::language::prism_ui::ast::Node::Element(_)))
            .count();
        assert_eq!(elem_count, 1);
    }

    #[test]
    fn emit_round_trips_components_and_ids_through_parse() {
        use prism_core::language::prism_ui::ast::Node as AstNode;

        let original = n(
            "container",
            "root",
            json!({ "title": "Hello" }),
            vec![
                n("text", "t1", json!({ "value": "a" }), vec![]),
                n(
                    "card",
                    "c1",
                    Value::Null,
                    vec![n("text", "t2", json!({ "value": "b" }), vec![])],
                ),
            ],
        );
        let source = emit_node(&original);
        let (doc, errs) = parse(&source);
        assert!(errs.is_empty());
        let root = doc
            .nodes
            .iter()
            .find_map(|n| match n {
                AstNode::Element(e) => Some(e),
                _ => None,
            })
            .expect("root element");
        assert_eq!(root.tag, "container");
        // id attr present
        assert!(root.attributes.iter().any(|a| a.name.raw == "id"));
        // two element children survive
        let elem_children: Vec<_> = root
            .children
            .iter()
            .filter(|n| matches!(n, AstNode::Element(_)))
            .collect();
        assert_eq!(elem_children.len(), 2);
    }
}
