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

/// Tags whose body the PRUI parser must scan verbatim (no nested
/// elements / interpolations / comments). The Wave A landing of
/// `prui-luau-fusion.md` §7.1 introduces `<script>`; the
/// same raw-text mode applies to `<style>` blocks (Wave H), so both
/// are listed up front — mirrors HTML's "raw text element" set.
fn is_raw_text_tag(tag: &str) -> bool {
    // `language` (Wave E sub-dialect blocks, §7.8) joins
    // `script`/`style`: a dialect body is foreign source (markdown,
    // SQL, …) the PRUI parser must not interpret.
    matches!(tag, "script" | "style" | "language")
}

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

            // **Wave E (§7.8)** — `~name{ … }` sub-dialect sigil,
            // sugar for `<language name="name"> … </language>`. Only
            // a `~ident{` run (no space) is a sigil; a bare `~` in
            // prose flows to the text path untouched.
            if self.scanner.peek() == Some('~') {
                if let Some(node) = self.try_parse_sigil() {
                    out.push(node);
                    continue;
                }
            }

            // Text / interpolation run.
            self.parse_text_or_interpolation(&mut out);
        }
        out
    }

    /// **Wave E** — `~name{ body }` → `<language name="name">body
    /// </language>`. Lookahead-validated: returns `None` (consuming
    /// nothing) unless the `~ident{` shape matches, so non-sigil
    /// `~` in prose is left for the text path. The body is a raw run
    /// with brace balancing; `\{` / `\}` escape a literal brace.
    fn try_parse_sigil(&mut self) -> Option<Node> {
        let rest = &self.scanner.source()[self.scanner.offset()..];
        let mut chars = rest.char_indices();
        // `~`
        match chars.next() {
            Some((_, '~')) => {}
            _ => return None,
        }
        // ident: alpha/_ then alnum/_/-
        let mut name_end = 1;
        let mut first = true;
        for (i, c) in chars.by_ref() {
            let ok = if first {
                c.is_ascii_alphabetic() || c == '_'
            } else {
                c.is_ascii_alphanumeric() || c == '_' || c == '-'
            };
            if ok {
                name_end = i + c.len_utf8();
                first = false;
            } else {
                // The char that ended the ident must be `{`.
                if c == '{' && !first {
                    break;
                }
                return None;
            }
        }
        let name = rest[1..name_end].to_string();
        if name.is_empty() || !rest[name_end..].starts_with('{') {
            return None;
        }
        let start = self.scanner.position();
        // Consume `~name{`.
        for _ in 0..rest[..name_end].chars().count() {
            self.scanner.advance();
        }
        self.scanner.advance(); // `{`

        let mut body = String::new();
        let mut depth = 1usize;
        loop {
            match self.scanner.peek() {
                None => {
                    self.errors.push(ParseError {
                        message: format!("Unterminated `~{name}{{ … }}` sigil"),
                        range: SourceRange {
                            start,
                            end: self.scanner.position(),
                        },
                        code: "unterminated-sigil",
                    });
                    break;
                }
                Some('\\') => {
                    self.scanner.advance();
                    match self.scanner.peek() {
                        Some(c @ ('{' | '}')) => {
                            body.push(c);
                            self.scanner.advance();
                        }
                        _ => body.push('\\'),
                    }
                }
                Some('{') => {
                    depth += 1;
                    body.push('{');
                    self.scanner.advance();
                }
                Some('}') => {
                    depth -= 1;
                    self.scanner.advance();
                    if depth == 0 {
                        break;
                    }
                    body.push('}');
                }
                Some(c) => {
                    body.push(c);
                    self.scanner.advance_unicode();
                }
            }
        }
        let range = SourceRange {
            start,
            end: self.scanner.position(),
        };
        Some(Node::Element(Element {
            tag: "language".to_string(),
            attributes: vec![Attribute {
                name: AttributeName {
                    raw: "name".to_string(),
                    local: "name".to_string(),
                    namespace: AttributeNamespace::Bare,
                    range,
                },
                value: AttributeValue::String { value: name, range },
                range,
            }],
            children: vec![Node::Text { value: body, range }],
            self_closing: false,
            range,
            tag_range: range,
        }))
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
            self.scanner.advance_unicode();
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
        let mut attributes = self.parse_attributes();

        // **§5.10** — `<script>` is Luau and `<style>` is PRSS; the
        // block tag *is* the language. A `lang=` attribute is no
        // longer a synonym — there is exactly one spelling. Recorded
        // as a recoverable diagnostic (parsing continues; the body is
        // still collected) so the one-spelling rule is enforced
        // without detonating documents that still carry the old form.
        if matches!(tag.as_str(), "script" | "style") {
            if let Some(bad) = attributes.iter().find(|a| a.name.raw == "lang") {
                self.errors.push(ParseError {
                    message: format!(
                        "`<{tag}>` carries no `lang=` attribute — \
                         the block tag is the language (§5.10)"
                    ),
                    range: bad.name.range,
                    code: "unexpected-lang-attr",
                });
            }
        }

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
            // **§5.10** — `<import …/> as <name>` postfix. The
            // namespace reads at the end of the line, not buried
            // mid-tag. Synthesised into an `as` attribute so the
            // downstream import collector (which reads `as`) is
            // unchanged.
            if tag == "import" {
                if let Some((alias, range)) = self.try_parse_import_alias() {
                    attributes.push(Attribute {
                        name: AttributeName {
                            raw: "as".into(),
                            local: "as".into(),
                            namespace: AttributeNamespace::Bare,
                            range,
                        },
                        value: AttributeValue::String {
                            value: alias,
                            range,
                        },
                        range,
                    });
                }
            }
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

        // **Raw-text elements** (`<script>` / `<style>`). Their bodies
        // are foreign-language source (Luau, PRSS) that the PRUI
        // parser must not interpret — `{`, `<`, `&`, etc. occur as
        // ordinary syntax in those languages. Mirrors HTML's
        // `script`/`style` parsing mode: scan verbatim until the
        // matching `</tag>`, stash as a single text child. The
        // §7.1 (`prui-luau-fusion.md`) loader extracts these bodies
        // before the rest of the AST flows into lowering.
        let children = if is_raw_text_tag(&tag) {
            self.parse_raw_text_body(&tag)
        } else {
            self.parse_nodes(Some(&tag))
        };

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
            // **§5.10** — attributes are comma-separated (the space
            // that used to delimit them can no longer, now that
            // unquoted values may themselves contain whitespace, e.g.
            // `padding=12 16`). The comma is consumed as a separator;
            // whitespace separation still parses so the existing
            // single-token / quoted corpus keeps working.
            while self.scanner.peek() == Some(',') {
                self.scanner.advance();
                self.scanner.skip_whitespace_and_newlines();
            }
            match self.scanner.peek() {
                None => break,
                Some('/') | Some('>') => break,
                Some(_) => {}
            }

            let name_start = self.scanner.position();
            let name_start_offset = self.scanner.offset();
            // Attribute name: identifiers + `:` + `-` are all valid.
            // **Wave 14.3** — `.` is allowed too so `on:click.once`
            // / `on:click.stop` parses cleanly; the dot becomes a
            // dash at the `data-on-<key>` lowering step so the
            // hit-test cache reads it uniformly.
            // **Sugar (`@event`)** — `@` is allowed as a first
            // character (and incidentally anywhere) so `@click`
            // parses as a single attribute name. The classifier
            // routes `@<event>` to the `On` namespace.
            let name_raw = self
                .scanner
                .scan_while(|c| {
                    c.is_ascii_alphanumeric()
                        || c == '_'
                        || c == '-'
                        || c == ':'
                        || c == '.'
                        || c == '@'
                })
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

    /// **§5.10** — six value forms. `"…"` / `{expr}` are the historic
    /// pair; `$ stmt` is an unquoted action/handler body, `[a, b]` a
    /// multi-value list, and a leading non-sigil char begins a bare
    /// token value (`gap=8`, `direction=row`, `padding=12 16`). Bare /
    /// `$` values still surface as `String` / `Template` so lowering
    /// is unchanged — the change is purely about dropping the quotes.
    fn parse_attribute_value(&mut self) -> AttributeValue {
        match self.scanner.peek() {
            Some('"') | Some('\'') => self.parse_quoted_value(),
            Some('{') => {
                let expr = self.parse_interpolation();
                AttributeValue::Expression(expr)
            }
            Some('[') => self.parse_list_value(),
            Some('$') => self.parse_action_value(),
            Some(c) if c != '/' && c != '>' && c != ',' => self.parse_bare_value(),
            _ => {
                let start = self.scanner.position();
                self.errors.push(ParseError {
                    message: "Expected attribute value (`\"…\"`, `{expr}`, `$stmt`, `[…]`, or a bare token)".into(),
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

    /// Scan a raw run up to the next *top-level* attribute terminator
    /// — `,` (separator), `>` / `/>` (tag close) — tracking
    /// `()[]{}` nesting and quote state so commas / `>` inside a call
    /// or string don't terminate early. Returns the trimmed slice and
    /// its range.
    fn scan_value_run(&mut self) -> (String, SourceRange) {
        let start = self.scanner.position();
        let start_off = self.scanner.offset();
        let mut depth = 0i32;
        let mut quote: Option<char> = None;
        while let Some(c) = self.scanner.peek() {
            if let Some(q) = quote {
                self.scanner.advance();
                if c == '\\' {
                    self.scanner.advance();
                } else if c == q {
                    quote = None;
                }
                continue;
            }
            match c {
                '"' | '\'' => {
                    quote = Some(c);
                    self.scanner.advance();
                }
                '(' | '[' | '{' => {
                    depth += 1;
                    self.scanner.advance();
                }
                ')' | ']' | '}' => {
                    depth -= 1;
                    self.scanner.advance();
                }
                ',' if depth <= 0 => break,
                '>' if depth <= 0 => break,
                '/' if depth <= 0 && self.scanner.peek_ahead(1) == Some('>') => break,
                _ => {
                    self.scanner.advance();
                }
            }
        }
        let raw = self.scanner.source()[start_off..self.scanner.offset()].to_string();
        let trimmed = raw.trim().to_string();
        (
            trimmed,
            SourceRange {
                start,
                end: self.scanner.position(),
            },
        )
    }

    /// `$ stmt` — an unquoted Luau action/handler body (§5.10). It
    /// surfaces as the canonical `luau { … }` action string so the
    /// existing dispatcher (`prism_builder::signal::parse_action`)
    /// routes it to `ParsedAction::Luau` unchanged — `$emit("save")`
    /// and `$state.x = !state.x` are Luau, not the space-delimited
    /// `verb rest` micro-grammar (`emit save` / `cmd help.show`).
    fn parse_action_value(&mut self) -> AttributeValue {
        self.scanner.advance(); // `$`
        self.scanner.skip_whitespace();
        let (body, range) = self.scan_value_run();
        if body.is_empty() {
            self.errors.push(ParseError {
                message: "Empty `$` action body".into(),
                range,
                code: "expected-value",
            });
            return AttributeValue::String {
                value: String::new(),
                range,
            };
        }
        AttributeValue::String {
            value: format!("luau {{ {body} }}"),
            range,
        }
    }

    /// Bare token value — `gap=8`, `direction=row`, `padding=12 16`,
    /// `class=card`. Equivalent to the old quoted string with the
    /// quotes removed; surfaces as `String` so lowering is unchanged.
    fn parse_bare_value(&mut self) -> AttributeValue {
        let (value, range) = self.scan_value_run();
        if value.is_empty() {
            self.errors.push(ParseError {
                message: "Expected attribute value".into(),
                range,
                code: "expected-value",
            });
            return AttributeValue::Empty;
        }
        AttributeValue::String { value, range }
    }

    /// `[a, {expr}, c]` — a comma-separated multi-value list. Lowers
    /// to the same shape a space-joined `class="a {expr} c"` produced,
    /// so the existing class / list handling is unchanged.
    fn parse_list_value(&mut self) -> AttributeValue {
        let start = self.scanner.position();
        self.scanner.advance(); // `[`
        let mut parts: Vec<TemplatePart> = Vec::new();
        let mut first = true;
        loop {
            self.scanner.skip_whitespace_and_newlines();
            while self.scanner.peek() == Some(',') {
                self.scanner.advance();
                self.scanner.skip_whitespace_and_newlines();
            }
            match self.scanner.peek() {
                None => {
                    self.errors.push(ParseError {
                        message: "Unterminated `[…]` list value".into(),
                        range: SourceRange {
                            start,
                            end: self.scanner.position(),
                        },
                        code: "unterminated-list",
                    });
                    break;
                }
                Some(']') => {
                    self.scanner.advance();
                    break;
                }
                _ => {}
            }
            if !first {
                parts.push(TemplatePart::Literal {
                    value: " ".into(),
                    range: SourceRange {
                        start: self.scanner.position(),
                        end: self.scanner.position(),
                    },
                });
            }
            first = false;
            if self.scanner.peek() == Some('{') {
                parts.push(TemplatePart::Expression(self.parse_interpolation()));
            } else {
                let el_start = self.scanner.position();
                let el_off = self.scanner.offset();
                while !matches!(self.scanner.peek(), None | Some(',') | Some(']')) {
                    self.scanner.advance();
                }
                let raw = self.scanner.source()[el_off..self.scanner.offset()]
                    .trim()
                    .to_string();
                parts.push(TemplatePart::Literal {
                    value: raw,
                    range: SourceRange {
                        start: el_start,
                        end: self.scanner.position(),
                    },
                });
            }
        }
        let range = SourceRange {
            start,
            end: self.scanner.position(),
        };
        let all_literal = parts
            .iter()
            .all(|p| matches!(p, TemplatePart::Literal { .. }));
        if all_literal {
            let value = parts
                .into_iter()
                .map(|p| match p {
                    TemplatePart::Literal { value, .. } => value,
                    TemplatePart::Expression(_) => unreachable!(),
                })
                .collect::<String>();
            AttributeValue::String { value, range }
        } else {
            AttributeValue::Template { parts, range }
        }
    }

    /// `<import …/> as <name>` — the §5.10 postfix namespace. Only
    /// consumed when an `as` keyword is immediately followed by a
    /// name; otherwise the scanner is left untouched so a following
    /// sibling run is not eaten.
    fn try_parse_import_alias(&mut self) -> Option<(String, SourceRange)> {
        let saved = self.scanner.save();
        let mut spaces = 0usize;
        while matches!(self.scanner.peek(), Some(' ') | Some('\t')) {
            self.scanner.advance();
            spaces += 1;
        }
        let start = self.scanner.position();
        if spaces == 0
            || self.scanner.peek() != Some('a')
            || self.scanner.peek_ahead(1) != Some('s')
            || !matches!(self.scanner.peek_ahead(2), Some(' ') | Some('\t'))
        {
            self.scanner.restore(saved);
            return None;
        }
        self.scanner.advance(); // a
        self.scanner.advance(); // s
        self.scanner.skip_whitespace();
        let name = self
            .scanner
            .scan_while(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.')
            .to_string();
        if name.is_empty() {
            self.scanner.restore(saved);
            return None;
        }
        Some((
            name,
            SourceRange {
                start,
                end: self.scanner.position(),
            },
        ))
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
            self.scanner.advance_unicode();
        }
        let end_offset = self.scanner.offset();
        if end_offset == start_offset {
            // Defensive: shouldn't happen, but advance one char to
            // guarantee progress.
            self.scanner.advance_unicode();
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

    /// Scan the body of a raw-text element (`<script>` / `<style>`)
    /// verbatim until the matching `</tag>`. Emits a single
    /// [`Node::Text`] containing the body — the PRUI walker doesn't
    /// recurse into it. Consumes the closing tag, mirroring
    /// `parse_element`'s normal close path; an unterminated body
    /// records an `unclosed-element` error and stops at EOF.
    fn parse_raw_text_body(&mut self, tag: &str) -> Vec<Node> {
        let body_start_offset = self.scanner.offset();
        let body_start_pos = self.scanner.position();
        let close = format!("</{tag}");
        loop {
            if self.scanner.is_at_end() {
                self.errors.push(ParseError {
                    message: format!("Unclosed raw-text element '<{tag}>'"),
                    range: SourceRange {
                        start: body_start_pos,
                        end: self.scanner.position(),
                    },
                    code: "unclosed-element",
                });
                break;
            }
            if self.peek_str(&close) {
                break;
            }
            self.scanner.advance_unicode();
        }
        let body_end_offset = self.scanner.offset();
        let value = self.scanner.source()[body_start_offset..body_end_offset].to_string();
        let body_node = Node::Text {
            value,
            range: SourceRange {
                start: body_start_pos,
                end: self.scanner.position(),
            },
        };

        // Consume the closing tag — `parse_element`'s outer body
        // already drives that branch for normal elements, so leave it
        // to handle the close after we return. Stash a single text
        // child as the only descendant.
        vec![body_node]
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
    fn comment_with_non_ascii_content_round_trips() {
        // Em-dash, curly quotes, accented chars — anything multibyte
        // in a comment body would previously panic the scanner.
        let doc = parse_ok("<!-- résumé — “smart” quotes -->\n<spacer/>");
        match &doc.nodes[0] {
            Node::Comment { value, .. } => {
                assert_eq!(value, " résumé — “smart” quotes ");
            }
            other => panic!("expected comment, got {other:?}"),
        }
    }

    #[test]
    fn text_node_with_non_ascii_content_round_trips() {
        let doc = parse_ok("<text>résumé — naïve façade</text>");
        let Node::Element(el) = &doc.nodes[0] else {
            panic!()
        };
        match &el.children[0] {
            Node::Text { value, .. } => assert_eq!(value, "résumé — naïve façade"),
            other => panic!("expected text, got {other:?}"),
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
    fn parses_script_block_as_raw_text() {
        // `local`, `{`, `<`, `function` — every Luau-shaped token the
        // PRUI parser would otherwise misinterpret. Wave A of
        // `prui-luau-fusion.md` §7.1 requires the body to survive
        // verbatim as a single text child.
        let src = r##"<script>
local function priority_color(p)
  if p == "high" then return "#ff0000" end
  return "#888888"
end
local state = prism.state { expanded = false }
</script>"##;
        let doc = parse_ok(src);
        let Node::Element(el) = &doc.nodes[0] else {
            panic!("expected element");
        };
        assert_eq!(el.tag, "script");
        // §5.10 — `<script>` carries no `lang=` attribute.
        assert!(el.attributes.is_empty());
        assert_eq!(el.children.len(), 1);
        let Node::Text { value, .. } = &el.children[0] else {
            panic!("expected raw text body");
        };
        assert!(value.contains("local function priority_color"));
        assert!(value.contains("prism.state { expanded = false }"));
    }

    #[test]
    fn parses_style_block_as_raw_text() {
        let src = r##"<style>
[class.card]
background = "#ffffff"
radius = 8
</style>"##;
        let doc = parse_ok(src);
        let Node::Element(el) = &doc.nodes[0] else {
            panic!();
        };
        assert_eq!(el.tag, "style");
        let Node::Text { value, .. } = &el.children[0] else {
            panic!();
        };
        assert!(value.contains("[class.card]"));
    }

    fn attr<'a>(el: &'a Element, name: &str) -> &'a AttributeValue {
        &el.attributes
            .iter()
            .find(|a| a.name.raw == name)
            .unwrap_or_else(|| panic!("missing attr {name}"))
            .value
    }
    fn as_str(v: &AttributeValue) -> &str {
        match v {
            AttributeValue::String { value, .. } => value,
            other => panic!("expected String, got {other:?}"),
        }
    }
    fn el0(doc: &Document) -> &Element {
        match &doc.nodes[0] {
            Node::Element(el) => el,
            n => panic!("expected element, got {n:?}"),
        }
    }

    #[test]
    fn s510_comma_separated_bare_values() {
        let doc = parse_ok(r#"<container direction=row, gap=8, padding=12 16>x</container>"#);
        let el = el0(&doc);
        assert_eq!(as_str(attr(el, "direction")), "row");
        assert_eq!(as_str(attr(el, "gap")), "8");
        assert_eq!(as_str(attr(el, "padding")), "12 16");
    }

    #[test]
    fn s510_quoted_and_whitespace_still_parse() {
        // Back-compat: the existing single-token / quoted,
        // whitespace-separated corpus keeps working unchanged.
        let doc = parse_ok(r#"<container class="card" title="Hi"/>"#);
        let el = el0(&doc);
        assert_eq!(as_str(attr(el, "class")), "card");
        assert_eq!(as_str(attr(el, "title")), "Hi");
    }

    #[test]
    fn s510_action_body_dollar() {
        // `$` wraps to the canonical `luau { … }` action so the
        // dispatcher routes it to ParsedAction::Luau.
        let doc = parse_ok(r#"<button on:click=$state.x = !state.x>Go</button>"#);
        assert_eq!(
            as_str(attr(el0(&doc), "on:click")),
            "luau { state.x = !state.x }"
        );
        // Comma / `>` inside a string or call must not terminate.
        let doc = parse_ok(r#"<button on:click=$emit("a, b")>Go</button>"#);
        assert_eq!(
            as_str(attr(el0(&doc), "on:click")),
            r#"luau { emit("a, b") }"#
        );
    }

    #[test]
    fn s510_list_value() {
        let doc = parse_ok(r#"<container class=[card, {priority_class(p)}]>x</container>"#);
        let AttributeValue::Template { parts, .. } = attr(el0(&doc), "class") else {
            panic!("expected Template");
        };
        assert!(matches!(&parts[0], TemplatePart::Literal { value, .. } if value == "card"));
        assert!(parts.iter().any(
            |p| matches!(p, TemplatePart::Expression(e) if e.body.contains("priority_class"))
        ));

        let doc = parse_ok(r#"<container class=[a, b]>x</container>"#);
        assert_eq!(as_str(attr(el0(&doc), "class")), "a b");
    }

    #[test]
    fn s510_lang_attr_is_an_error() {
        let (_, errs) = parse(r#"<script lang="luau"></script>"#);
        assert!(
            errs.iter().any(|e| e.code == "unexpected-lang-attr"),
            "expected unexpected-lang-attr, got {errs:?}"
        );
    }

    #[test]
    fn s510_import_postfix_as() {
        let doc = parse_ok(r#"<import script="./fmt.luau"/> as fmt"#);
        let el = el0(&doc);
        assert_eq!(el.tag, "import");
        assert_eq!(as_str(attr(el, "script")), "./fmt.luau");
        assert_eq!(as_str(attr(el, "as")), "fmt");

        // No postfix → no synthetic `as`, scanner not over-consumed.
        let doc = parse_ok(r#"<import script="./x.luau"/><spacer/>"#);
        assert!(doc
            .nodes
            .iter()
            .any(|n| matches!(n, Node::Element(e) if e.tag == "spacer")));
    }

    #[test]
    fn script_block_then_sibling_element() {
        // After the raw-text body closes, the parser must resume
        // normal mode and read the sibling element. Regression guard
        // against a sticky "still in raw text" state.
        let src = r#"<script>local x = 1</script>
<container/>"#;
        let doc = parse_ok(src);
        assert!(doc
            .nodes
            .iter()
            .any(|n| matches!(n, Node::Element(el) if el.tag == "script")));
        assert!(doc
            .nodes
            .iter()
            .any(|n| matches!(n, Node::Element(el) if el.tag == "container")));
    }

    #[test]
    fn parses_language_block_as_raw_text() {
        let src = "<language name=\"sql\">select * from t where x < 3 and y = '{a}'</language>";
        let doc = parse_ok(src);
        let Node::Element(el) = &doc.nodes[0] else {
            panic!()
        };
        assert_eq!(el.tag, "language");
        assert_eq!(el.attributes[0].name.local, "name");
        let Node::Text { value, .. } = &el.children[0] else {
            panic!()
        };
        assert!(value.contains("select * from t where x < 3"));
        assert!(value.contains("'{a}'"));
    }

    #[test]
    fn parses_dialect_sigil_sugar() {
        let doc = parse_ok("<container>~md{**bold** and _it_}</container>");
        let Node::Element(c) = &doc.nodes[0] else {
            panic!()
        };
        let Node::Element(lang) = &c.children[0] else {
            panic!("expected <language>, got {:?}", c.children[0]);
        };
        assert_eq!(lang.tag, "language");
        match &lang.attributes[0].value {
            AttributeValue::String { value, .. } => assert_eq!(value, "md"),
            o => panic!("{o:?}"),
        }
        let Node::Text { value, .. } = &lang.children[0] else {
            panic!()
        };
        assert_eq!(value, "**bold** and _it_");
    }

    #[test]
    fn sigil_balances_braces_and_escapes() {
        let doc = parse_ok(r"<container>~tex{a {b} c \{lit\}}</container>");
        let Node::Element(c) = &doc.nodes[0] else {
            panic!()
        };
        let Node::Element(lang) = &c.children[0] else {
            panic!()
        };
        let Node::Text { value, .. } = &lang.children[0] else {
            panic!()
        };
        assert_eq!(value, "a {b} c {lit}");
    }

    #[test]
    fn bare_tilde_in_prose_is_not_a_sigil() {
        let doc = parse_ok("<text>about ~5 items</text>");
        let Node::Element(t) = &doc.nodes[0] else {
            panic!()
        };
        let Node::Text { value, .. } = &t.children[0] else {
            panic!()
        };
        assert_eq!(value, "about ~5 items");
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
