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
//! - **Canonical declarations** (Phase 2 of the expressiveness
//!   roadmap, `docs/dev/prui-expressiveness-roadmap.md` §6): the
//!   `component Card(...) = <…/>` family, alongside `trait` /
//!   `mixin` / `macro` / `class` / `type` / `fn` / `let` /
//!   `import` / `namespace`. Dispatch is by first non-whitespace
//!   token — `<` opens the XML reader, a lowercase keyword opens
//!   the canonical reader in [`canonical`]. Both produce the same
//!   [`Document`] / [`Node`] shapes, so downstream consumers (the
//!   `prism-ui-runtime::interpret` walker, `prism-ui-build`, the
//!   syntax provider) read either surface uniformly.
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

pub mod canonical;
pub mod migrate;

pub use migrate::rewrite_xml_to_canonical;

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

/// Parse a `.prui` source file into a typed [`Document`].
///
/// Returns the document plus a list of recoverable [`ParseError`]s —
/// the parser tries to keep going after a malformed element so the
/// editor can show every problem at once.
///
/// Phase 2 dispatch (`docs/dev/prui-expressiveness-roadmap.md` §6 +
/// §8): the first non-whitespace, non-comment token of the source
/// picks the reader. A `<` opens the XML-shape parser (the historic
/// path); a lowercase ASCII keyword that names a canonical
/// declaration — `component` / `trait` / `mixin` / `macro` /
/// `class` / `type` / `fn` / `let` / `import` / `namespace` — opens
/// the canonical parser in [`canonical`]. Anything else falls
/// through to the XML reader so legacy text-leading documents keep
/// parsing. Both readers produce the same [`Document`] shape so
/// downstream consumers don't branch.
pub fn parse(source: &str) -> (Document, Vec<ParseError>) {
    if canonical::looks_canonical(source) {
        return canonical::parse(source);
    }
    let mut parser = Parser::new(source);
    let nodes = parser.parse_nodes(None);
    (Document { nodes }, parser.errors)
}

/// XML-form-only parse path, exposed for the migration tool +
/// tests that need to read the legacy surface unconditionally
/// (regardless of dispatch). Callers should prefer [`parse`].
pub fn parse_xml(source: &str) -> (Document, Vec<ParseError>) {
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
        // tags addressable from `.prui` source without forcing the
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
mod tests;
