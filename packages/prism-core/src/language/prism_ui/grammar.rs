//! `Scanner`-driven parser for the `prism-ui` DSL.
//!
//! Builds on `language::syntax::Scanner` — the project's standing rule
//! is that all parsers go through Prism Syntax, so this module never
//! reaches for regex or hand-rolled string indexing.
//!
//! Recognises:
//!
//! - `<tag attr="value" ns:name="value">...</tag>` element pairs
//! - `<tag/>` self-closing elements
//! - `<!-- ... -->` comments
//! - `{expr}` interpolations inside attribute values and child runs
//!
//! Returns a typed [`Document`] plus a parallel generic [`RootNode`]
//! adapter for `LanguageContribution::parse` consumers.

use serde_json::json;

use crate::language::syntax::{
    pos_at, range as range_at, Position, RootKind, RootNode, ScanError, Scanner, SourceRange,
    SyntaxNode,
};

use super::ast::{
    Attribute, AttributeName, AttributeNamespace, AttributeValue, Document, Element, Expression,
    Node, ParseError, TemplatePart,
};

/// Parse a `.prism-ui` source file into a typed [`Document`].
///
/// Returns the document plus a list of recoverable [`ParseError`]s —
/// the parser tries to keep going after a malformed element so the
/// editor can show every problem at once.
pub fn parse(source: &str) -> (Document, Vec<ParseError>) {
    let mut parser = Parser::new(source);
    let nodes = parser.parse_nodes(None);
    (Document { nodes }, parser.errors)
}

/// Parse and surface the typed [`Document`] as a generic [`RootNode`]
/// for `LanguageContribution::parse`. The element tree is re-encoded
/// into [`SyntaxNode`]s so generic tooling (TextEmitter, the codegen
/// pipeline, the syntax engine) can walk the same shape.
pub fn parse_to_root(source: &str) -> RootNode {
    let (document, _errors) = parse(source);
    let children = document.nodes.iter().map(node_to_syntax).collect();
    RootNode {
        kind: RootKind::Root,
        position: Some(SourceRange {
            start: pos_at(source, 0),
            end: pos_at(source, source.len()),
        }),
        children,
    }
}

fn node_to_syntax(node: &Node) -> SyntaxNode {
    match node {
        Node::Element(el) => {
            let mut data = indexmap::IndexMap::new();
            data.insert("tag".into(), json!(el.tag));
            data.insert("self_closing".into(), json!(el.self_closing));
            let attrs: Vec<_> = el
                .attributes
                .iter()
                .map(|a| {
                    json!({
                        "name": a.name.raw,
                        "namespace": format!("{:?}", a.name.namespace),
                        "local": a.name.local,
                    })
                })
                .collect();
            data.insert("attributes".into(), json!(attrs));
            SyntaxNode {
                kind: "element".into(),
                position: Some(el.range),
                children: el.children.iter().map(node_to_syntax).collect(),
                value: Some(el.tag.clone()),
                data,
            }
        }
        Node::Text { value, range } => SyntaxNode {
            kind: "text".into(),
            position: Some(*range),
            children: Vec::new(),
            value: Some(value.clone()),
            data: indexmap::IndexMap::new(),
        },
        Node::Interpolation(expr) => SyntaxNode {
            kind: "interpolation".into(),
            position: Some(expr.range),
            children: Vec::new(),
            value: Some(expr.body.clone()),
            data: indexmap::IndexMap::new(),
        },
        Node::Comment { value, range } => SyntaxNode {
            kind: "comment".into(),
            position: Some(*range),
            children: Vec::new(),
            value: Some(value.clone()),
            data: indexmap::IndexMap::new(),
        },
    }
}

struct Parser<'s> {
    scanner: Scanner<'s>,
    errors: Vec<ParseError>,
}

impl<'s> Parser<'s> {
    fn new(source: &'s str) -> Self {
        Self {
            scanner: Scanner::new(source),
            errors: Vec::new(),
        }
    }

    /// Parse a sequence of nodes until either EOF or a closing tag for
    /// `parent_tag` is seen. When `parent_tag` is `None` the loop runs
    /// to EOF (top-level invocation).
    fn parse_nodes(&mut self, parent_tag: Option<&str>) -> Vec<Node> {
        let mut out = Vec::new();
        loop {
            if self.scanner.is_at_end() {
                break;
            }

            // Closing tag for the current parent? Stop here without
            // consuming — the caller pops it.
            if self.peek_str("</") {
                if parent_tag.is_some() {
                    return out;
                }
                let start = self.scanner.position();
                // Stray closing tag at the top level. Consume it as
                // text so we don't loop forever, but record the error.
                let consumed = self.consume_until_gt();
                self.errors.push(ParseError {
                    message: format!("Unexpected closing tag '{}'", consumed.trim()),
                    range: SourceRange {
                        start,
                        end: self.scanner.position(),
                    },
                    code: "stray-close-tag",
                });
                continue;
            }

            if self.peek_str("<!--") {
                out.push(self.parse_comment());
                continue;
            }

            if self.peek_str("<") {
                match self.parse_element() {
                    Some(node) => out.push(node),
                    None => continue,
                }
                continue;
            }

            // Text / interpolation run.
            self.parse_text_or_interpolation(&mut out);
        }
        out
    }

    fn parse_comment(&mut self) -> Node {
        let start = self.scanner.position();
        // `<!--` — the peek already verified the prefix.
        for _ in 0..4 {
            self.scanner.advance();
        }
        let body_start = self.scanner.offset();
        loop {
            if self.scanner.is_at_end() {
                self.errors.push(ParseError {
                    message: "Unterminated comment".into(),
                    range: SourceRange {
                        start,
                        end: self.scanner.position(),
                    },
                    code: "unterminated-comment",
                });
                let body = self.scanner.source()[body_start..self.scanner.offset()].to_string();
                return Node::Comment {
                    value: body,
                    range: SourceRange {
                        start,
                        end: self.scanner.position(),
                    },
                };
            }
            if self.peek_str("-->") {
                let body = self.scanner.source()[body_start..self.scanner.offset()].to_string();
                for _ in 0..3 {
                    self.scanner.advance();
                }
                return Node::Comment {
                    value: body,
                    range: SourceRange {
                        start,
                        end: self.scanner.position(),
                    },
                };
            }
            self.scanner.advance();
        }
    }

    fn parse_element(&mut self) -> Option<Node> {
        let start = self.scanner.position();
        // Consume `<`.
        self.scanner.advance();

        // Tag name. Tags may carry namespaced ids (`shell.icon-button`,
        // `app.foo-bar`) so we scan a wider character class than the
        // generic identifier scanner — alpha start, then alphanumeric
        // / `_` / `-` / `.`. Keeps the registry-resolved component
        // tags addressable from `.prism-ui` source without forcing the
        // host to encode dots as some other separator.
        let tag_start = self.scanner.position();
        let first = self.scanner.peek();
        if !matches!(first, Some(c) if c.is_ascii_alphabetic() || c == '_') {
            let err = self.scanner.error(&format!(
                "Expected tag name, got '{}'",
                first.map(|c| c.to_string()).unwrap_or_else(|| "EOF".into())
            ));
            self.push_scan_error(err);
            self.consume_until_gt();
            return None;
        }
        let tag = self
            .scanner
            .scan_while(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.')
            .to_string();
        let tag_range = SourceRange {
            start: tag_start,
            end: self.scanner.position(),
        };

        // Attributes.
        let attributes = self.parse_attributes();

        self.scanner.skip_whitespace_and_newlines();

        let self_closing = self.scanner.match_str("/>");
        if !self_closing && !self.scanner.match_str(">") {
            self.errors.push(ParseError {
                message: format!("Expected '>' to close opening tag '<{tag}>'"),
                range: SourceRange {
                    start,
                    end: self.scanner.position(),
                },
                code: "expected-gt",
            });
            self.consume_until_gt();
        }

        if self_closing {
            return Some(Node::Element(Element {
                tag,
                attributes,
                children: Vec::new(),
                self_closing: true,
                range: SourceRange {
                    start,
                    end: self.scanner.position(),
                },
                tag_range,
            }));
        }

        // Children.
        let children = self.parse_nodes(Some(&tag));

        // Closing tag.
        if self.peek_str("</") {
            self.scanner.advance();
            self.scanner.advance();
            // Closing tag uses the same wider char class as the
            // opening tag so `</shell.icon-button>` round-trips.
            let close_name = self
                .scanner
                .scan_while(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.')
                .to_string();
            if close_name.is_empty() {
                let err = self.scanner.error("Expected tag name in closing tag");
                self.push_scan_error(err);
                self.consume_until_gt();
            }
            self.scanner.skip_whitespace_and_newlines();
            if !self.scanner.match_str(">") {
                self.errors.push(ParseError {
                    message: format!("Expected '>' to close '</{close_name}>'"),
                    range: SourceRange {
                        start: tag_range.start,
                        end: self.scanner.position(),
                    },
                    code: "expected-gt",
                });
                self.consume_until_gt();
            }
            if !close_name.is_empty() && close_name != tag {
                self.errors.push(ParseError {
                    message: format!(
                        "Mismatched closing tag: expected '</{tag}>', found '</{close_name}>'"
                    ),
                    range: SourceRange {
                        start: tag_range.start,
                        end: self.scanner.position(),
                    },
                    code: "mismatched-close",
                });
            }
        } else {
            self.errors.push(ParseError {
                message: format!("Unclosed element '<{tag}>'"),
                range: SourceRange {
                    start,
                    end: self.scanner.position(),
                },
                code: "unclosed-element",
            });
        }

        Some(Node::Element(Element {
            tag,
            attributes,
            children,
            self_closing: false,
            range: SourceRange {
                start,
                end: self.scanner.position(),
            },
            tag_range,
        }))
    }

    fn parse_attributes(&mut self) -> Vec<Attribute> {
        let mut attrs = Vec::new();
        loop {
            self.scanner.skip_whitespace_and_newlines();
            match self.scanner.peek() {
                None => break,
                Some('/') | Some('>') => break,
                Some(_) => {}
            }

            let name_start = self.scanner.position();
            let name_start_offset = self.scanner.offset();
            // Attribute name: identifiers + `:` + `-` are all valid.
            let name_raw = self
                .scanner
                .scan_while(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == ':')
                .to_string();
            if name_raw.is_empty() {
                // Couldn't make progress — bail to avoid an infinite
                // loop. Consume one char and record an error.
                let bad = self.scanner.advance().unwrap_or('?');
                self.errors.push(ParseError {
                    message: format!("Unexpected character '{bad}' in attribute list"),
                    range: SourceRange {
                        start: name_start,
                        end: self.scanner.position(),
                    },
                    code: "unexpected-char",
                });
                continue;
            }
            let name_range = SourceRange {
                start: name_start,
                end: self.scanner.position(),
            };
            let (namespace, local) = AttributeNamespace::classify(&name_raw);

            // `=` and value, or boolean (no value) attribute.
            self.scanner.skip_whitespace();
            let value = if self.scanner.match_str("=") {
                self.scanner.skip_whitespace();
                self.parse_attribute_value()
            } else {
                AttributeValue::Empty
            };

            let attr_range = SourceRange {
                start: pos_at(self.scanner.source(), name_start_offset),
                end: self.scanner.position(),
            };
            attrs.push(Attribute {
                name: AttributeName {
                    raw: name_raw,
                    local,
                    namespace,
                    range: name_range,
                },
                value,
                range: attr_range,
            });
        }
        attrs
    }

    fn parse_attribute_value(&mut self) -> AttributeValue {
        match self.scanner.peek() {
            Some('"') | Some('\'') => self.parse_quoted_value(),
            Some('{') => {
                let expr = self.parse_interpolation();
                AttributeValue::Expression(expr)
            }
            _ => {
                let start = self.scanner.position();
                self.errors.push(ParseError {
                    message: "Expected attribute value (quoted string or `{expr}`)".into(),
                    range: SourceRange {
                        start,
                        end: self.scanner.position(),
                    },
                    code: "expected-value",
                });
                AttributeValue::Empty
            }
        }
    }

    fn parse_quoted_value(&mut self) -> AttributeValue {
        let quote = self.scanner.peek().unwrap();
        let start = self.scanner.position();
        self.scanner.advance(); // open quote
        let mut parts: Vec<TemplatePart> = Vec::new();
        let mut buf = String::new();
        let mut buf_start = self.scanner.position();

        loop {
            match self.scanner.peek() {
                None => {
                    self.errors.push(ParseError {
                        message: "Unterminated attribute value".into(),
                        range: SourceRange {
                            start,
                            end: self.scanner.position(),
                        },
                        code: "unterminated-string",
                    });
                    break;
                }
                Some(c) if c == quote => {
                    self.scanner.advance();
                    break;
                }
                Some('{') => {
                    if !buf.is_empty() {
                        parts.push(TemplatePart::Literal {
                            value: std::mem::take(&mut buf),
                            range: SourceRange {
                                start: buf_start,
                                end: self.scanner.position(),
                            },
                        });
                    }
                    let expr = self.parse_interpolation();
                    parts.push(TemplatePart::Expression(expr));
                    buf_start = self.scanner.position();
                }
                Some('\\') => {
                    self.scanner.advance();
                    if let Some(esc) = self.scanner.advance() {
                        match esc {
                            'n' => buf.push('\n'),
                            't' => buf.push('\t'),
                            'r' => buf.push('\r'),
                            '\\' => buf.push('\\'),
                            c if c == quote => buf.push(quote),
                            c => {
                                buf.push('\\');
                                buf.push(c);
                            }
                        }
                    }
                }
                Some(_) => {
                    if let Some(c) = self.scanner.advance() {
                        buf.push(c);
                    }
                }
            }
        }
        if !buf.is_empty() {
            parts.push(TemplatePart::Literal {
                value: buf,
                range: SourceRange {
                    start: buf_start,
                    end: self.scanner.position(),
                },
            });
        }

        let end = self.scanner.position();
        let range = SourceRange { start, end };

        match parts.len() {
            0 => AttributeValue::String {
                value: String::new(),
                range,
            },
            1 => match parts.into_iter().next().unwrap() {
                TemplatePart::Literal { value, .. } => AttributeValue::String { value, range },
                TemplatePart::Expression(expr) => AttributeValue::Expression(expr),
            },
            _ => AttributeValue::Template { parts, range },
        }
    }

    /// Parse `{...}` with brace-balance awareness. The body is kept
    /// raw — it's handed to the Prism Syntax expression scanner
    /// downstream.
    fn parse_interpolation(&mut self) -> Expression {
        let start = self.scanner.position();
        self.scanner.advance(); // `{`
        let body_start = self.scanner.offset();
        let mut depth = 1usize;
        while let Some(ch) = self.scanner.peek() {
            match ch {
                '{' => {
                    depth += 1;
                    self.scanner.advance();
                }
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        let body =
                            self.scanner.source()[body_start..self.scanner.offset()].to_string();
                        self.scanner.advance(); // closing `}`
                        return Expression {
                            body,
                            range: SourceRange {
                                start,
                                end: self.scanner.position(),
                            },
                        };
                    }
                    self.scanner.advance();
                }
                _ => {
                    self.scanner.advance();
                }
            }
        }
        self.errors.push(ParseError {
            message: "Unterminated `{...}` interpolation".into(),
            range: SourceRange {
                start,
                end: self.scanner.position(),
            },
            code: "unterminated-interpolation",
        });
        Expression {
            body: self.scanner.source()[body_start..self.scanner.offset()].to_string(),
            range: SourceRange {
                start,
                end: self.scanner.position(),
            },
        }
    }

    fn parse_text_or_interpolation(&mut self, out: &mut Vec<Node>) {
        if self.scanner.peek() == Some('{') {
            let expr = self.parse_interpolation();
            out.push(Node::Interpolation(expr));
            return;
        }
        let text_start = self.scanner.position();
        let start_offset = self.scanner.offset();
        while let Some(ch) = self.scanner.peek() {
            if ch == '<' || ch == '{' {
                break;
            }
            self.scanner.advance();
        }
        let end_offset = self.scanner.offset();
        if end_offset == start_offset {
            // Defensive: shouldn't happen, but advance one char to
            // guarantee progress.
            self.scanner.advance();
            return;
        }
        let value = self.scanner.source()[start_offset..end_offset].to_string();
        out.push(Node::Text {
            value,
            range: SourceRange {
                start: text_start,
                end: self.scanner.position(),
            },
        });
    }

    fn peek_str(&self, expected: &str) -> bool {
        self.scanner.source()[self.scanner.offset()..].starts_with(expected)
    }

    /// Advance past the next `>` (inclusive). Returns the consumed
    /// substring for diagnostics. Used as a recovery primitive.
    fn consume_until_gt(&mut self) -> String {
        let start = self.scanner.offset();
        while let Some(ch) = self.scanner.peek() {
            self.scanner.advance();
            if ch == '>' {
                break;
            }
        }
        self.scanner.source()[start..self.scanner.offset()].to_string()
    }

    fn push_scan_error(&mut self, err: ScanError) {
        let pos: Position = err.position;
        let range = range_at(self.scanner.source(), pos.offset, pos.offset);
        self.errors.push(ParseError {
            message: err.message,
            range,
            code: "scan-error",
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_ok(src: &str) -> Document {
        let (doc, errs) = parse(src);
        assert!(errs.is_empty(), "unexpected parse errors: {errs:?}");
        doc
    }

    #[test]
    fn parses_self_closing_element() {
        let doc = parse_ok(r#"<spacer/>"#);
        assert_eq!(doc.nodes.len(), 1);
        let Node::Element(el) = &doc.nodes[0] else {
            panic!("expected element");
        };
        assert_eq!(el.tag, "spacer");
        assert!(el.self_closing);
        assert!(el.children.is_empty());
        assert!(el.attributes.is_empty());
    }

    #[test]
    fn parses_element_with_string_attribute() {
        let doc = parse_ok(r#"<button label="Save"/>"#);
        let Node::Element(el) = &doc.nodes[0] else {
            panic!()
        };
        assert_eq!(el.tag, "button");
        assert_eq!(el.attributes.len(), 1);
        assert_eq!(el.attributes[0].name.raw, "label");
        assert_eq!(el.attributes[0].name.namespace, AttributeNamespace::Bare);
        match &el.attributes[0].value {
            AttributeValue::String { value, .. } => assert_eq!(value, "Save"),
            other => panic!("expected string value, got {other:?}"),
        }
    }

    #[test]
    fn parses_attribute_namespaces() {
        let doc =
            parse_ok(r#"<container on:click="emit save" style:tone="primary" class="card"/>"#);
        let Node::Element(el) = &doc.nodes[0] else {
            panic!()
        };
        assert_eq!(el.attributes.len(), 3);
        assert_eq!(el.attributes[0].name.namespace, AttributeNamespace::On);
        assert_eq!(el.attributes[0].name.local, "click");
        assert_eq!(el.attributes[1].name.namespace, AttributeNamespace::Style);
        assert_eq!(el.attributes[1].name.local, "tone");
        assert_eq!(
            el.attributes[2].name.namespace,
            AttributeNamespace::Identifier
        );
    }

    #[test]
    fn parses_open_close_with_text_child() {
        let doc = parse_ok(r#"<heading level="3">Hello</heading>"#);
        let Node::Element(el) = &doc.nodes[0] else {
            panic!()
        };
        assert_eq!(el.tag, "heading");
        assert!(!el.self_closing);
        assert_eq!(el.children.len(), 1);
        match &el.children[0] {
            Node::Text { value, .. } => assert_eq!(value, "Hello"),
            other => panic!("expected text child, got {other:?}"),
        }
    }

    #[test]
    fn parses_interpolation_in_attr_and_body() {
        let doc = parse_ok(r#"<heading title="{user.name}">{user.name}</heading>"#);
        let Node::Element(el) = &doc.nodes[0] else {
            panic!()
        };
        match &el.attributes[0].value {
            AttributeValue::Expression(expr) => assert_eq!(expr.body, "user.name"),
            other => panic!("expected expression, got {other:?}"),
        }
        match &el.children[0] {
            Node::Interpolation(expr) => assert_eq!(expr.body, "user.name"),
            other => panic!("expected interpolation, got {other:?}"),
        }
    }

    #[test]
    fn parses_template_attribute_value() {
        let doc = parse_ok(r#"<a href="/users/{id}/edit"/>"#);
        let Node::Element(el) = &doc.nodes[0] else {
            panic!()
        };
        match &el.attributes[0].value {
            AttributeValue::Template { parts, .. } => {
                assert_eq!(parts.len(), 3);
                matches!(parts[0], TemplatePart::Literal { .. });
                matches!(parts[1], TemplatePart::Expression(_));
                matches!(parts[2], TemplatePart::Literal { .. });
            }
            other => panic!("expected template, got {other:?}"),
        }
    }

    #[test]
    fn parses_nested_elements() {
        let doc = parse_ok(r#"<container><heading>Hi</heading></container>"#);
        let Node::Element(outer) = &doc.nodes[0] else {
            panic!()
        };
        assert_eq!(outer.tag, "container");
        assert_eq!(outer.children.len(), 1);
        let Node::Element(inner) = &outer.children[0] else {
            panic!()
        };
        assert_eq!(inner.tag, "heading");
    }

    #[test]
    fn parses_html_comment() {
        let doc = parse_ok(r#"<!-- a note --><spacer/>"#);
        assert_eq!(doc.nodes.len(), 2);
        match &doc.nodes[0] {
            Node::Comment { value, .. } => assert_eq!(value, " a note "),
            other => panic!("expected comment, got {other:?}"),
        }
    }

    #[test]
    fn parses_boolean_attribute() {
        let doc = parse_ok(r#"<input disabled/>"#);
        let Node::Element(el) = &doc.nodes[0] else {
            panic!()
        };
        assert_eq!(el.attributes[0].name.raw, "disabled");
        assert!(matches!(el.attributes[0].value, AttributeValue::Empty));
    }

    #[test]
    fn parses_control_flow_attribute() {
        let doc = parse_ok(r#"<pill if="{badge}">{badge}</pill>"#);
        let Node::Element(el) = &doc.nodes[0] else {
            panic!()
        };
        assert_eq!(
            el.attributes[0].name.namespace,
            AttributeNamespace::ControlFlow
        );
    }

    #[test]
    fn reports_unclosed_element() {
        let (_doc, errs) = parse(r#"<container>"#);
        assert!(!errs.is_empty());
        assert!(errs.iter().any(|e| e.code == "unclosed-element"));
    }

    #[test]
    fn reports_mismatched_close() {
        let (_doc, errs) = parse(r#"<container></heading>"#);
        assert!(errs.iter().any(|e| e.code == "mismatched-close"));
    }

    #[test]
    fn reports_unterminated_interpolation() {
        let (_doc, errs) = parse(r#"<a title="{foo"/>"#);
        assert!(errs
            .iter()
            .any(|e| e.code == "unterminated-interpolation" || e.code == "unterminated-string"));
    }

    #[test]
    fn reports_stray_closing_tag() {
        let (_doc, errs) = parse(r#"</orphan>"#);
        assert!(errs.iter().any(|e| e.code == "stray-close-tag"));
    }

    #[test]
    fn parse_to_root_emits_element_node() {
        let root = parse_to_root(r#"<button label="Save"/>"#);
        assert_eq!(root.children.len(), 1);
        assert_eq!(root.children[0].kind, "element");
        assert_eq!(root.children[0].value.as_deref(), Some("button"));
    }

    #[test]
    fn parses_full_strawman_card() {
        let src = r#"
<component name="Card">
  <container layout="flow" gap="{tokens.spacing.md}" on:click="emit clicked">
    <heading level="3">{title}</heading>
    <pill tone="accent" if="{badge}">{badge}</pill>
  </container>
</component>
"#;
        let (doc, errs) = parse(src);
        assert!(errs.is_empty(), "errors: {errs:?}");
        // Top-level: one whitespace text node + the <component>.
        let component = doc
            .nodes
            .iter()
            .find_map(|n| match n {
                Node::Element(el) if el.tag == "component" => Some(el),
                _ => None,
            })
            .expect("component element");
        assert_eq!(component.attributes[0].name.raw, "name");
    }
}
