//! Canonical PRUI grammar — Phase 2 of the expressiveness roadmap.
//!
//! The canonical surface (see `docs/dev/prui-expressiveness-roadmap.md`
//! §6) collapses every declaration into one gesture:
//!
//!   `<keyword> Name [params] = expression`
//!   `<keyword> Name [params] { body }`
//!
//! where `keyword` is one of `component` / `trait` / `mixin` / `macro`
//! / `class` / `type` / `fn` / `let`, plus the headerless `import`
//! and `namespace` directives.
//!
//! This module sits **next to** the existing XML parser. The pair
//! decide who runs at `parse()` time by peeking the first non-
//! whitespace token of a declaration: a `<` opens XML, a lowercase
//! keyword opens the canonical reader. Both flow into the same
//! [`Document`]; downstream consumers (`prism-ui-runtime::interpret`,
//! `prism-ui-build`) see the same `Element` / `Node` shapes.
//!
//! Phase 2 lands the **grammar + AST projection** only. Real feature
//! semantics (`extends`, `derive`, capability injection, mixin
//! flattening, trait conformance, computed defaults) ship in
//! Phases 7–18; this parser captures the surface faithfully so those
//! later passes can read the structure off the AST without rewriting
//! the parser.
//!
//! ## AST projection
//!
//! Each canonical declaration becomes a single [`Element`] whose
//! `tag` is the keyword (`component`, `trait`, …). Header data
//! lives on attributes:
//!
//! | Surface                              | AST                                                                      |
//! |--------------------------------------|--------------------------------------------------------------------------|
//! | `component Card(t: string) = <…/>`  | `<component name="Card"><property name="t" type="string"/><…/></…>`     |
//! | `namespace Foo`                      | `<namespace name="Foo"/>`                                                |
//! | `import "./theme.prss"`              | `<import stylesheet="./theme.prss"/>`                                    |
//! | `import "./db.luau" as db`           | `<import script="./db.luau" as="db"/>`                                   |
//! | `import "./card.prui"`               | `<import component="./card.prui"/>`                                      |
//! | `trait Focusable { … }`              | `<trait name="Focusable">…</trait>`                                      |
//! | `mixin Hoverable { … }`              | `<mixin name="Hoverable">…</mixin>`                                      |
//! | `macro Field(l: string) { … }`       | `<macro name="Field">…</macro>`                                          |
//! | `class card { … }`                   | `<style class="card">…</style>` (the existing PRSS body shape)           |
//! | `type Tone = info \| danger`         | `<type name="Tone" body="info \| danger"/>`                              |
//! | `fn double(x: int) → int = x * 2`    | `<fn name="double">…</fn>` with `<property>`s + an `<expr>` body         |
//! | `let MAX = 100`                      | `<let name="MAX" value="100"/>`                                          |
//!
//! Body statements (`use`, `requires`, `let`, `on`, `style`) inside a
//! declaration block become child elements (`<use>`, `<requires>`,
//! `<let>`, `<on>`, `<style>`). Render trees inside an expression
//! body parse through the existing XML reader so a tag like
//! `<container>{title}</container>` flows in unchanged.
//!
//! ## Forward references
//!
//! - §6.2 / §6.3 — `name = expression` and the body-shape rule.
//! - §6.4 / §6.5 — primitive + composite types (parsed opaque here).
//! - §6.7 — `use`, `with`, function-call composition.
//! - §6.13 — algebraic types (`a | b(c) | d`).
//! - §6.24 — XML → canonical translation cheatsheet (powers the
//!   `prism rewrite-canonical` migration tool in `migrate.rs`).

use crate::language::syntax::{Position, Scanner, SourceRange};

use super::super::ast::{
    Attribute, AttributeName, AttributeNamespace, AttributeValue, Document, Element, Node,
    ParseError,
};

/// Set of keywords that may begin a canonical top-level declaration.
/// The parser dispatcher uses this to decide whether a file (or a
/// document fragment) is canonical or XML.
pub(super) const TOP_LEVEL_KEYWORDS: &[&str] = &[
    "component",
    "trait",
    "mixin",
    "macro",
    "class",
    "type",
    "fn",
    "let",
    "import",
    "namespace",
];

/// Peek the first non-whitespace, non-comment token of `source` and
/// return `true` if it begins a canonical declaration. Returns
/// `false` for XML-shape input (starting with `<`), for raw text /
/// `~sigil{ … }`, or for empty input.
///
/// Comments scanned: `--` line comments (Lua / canonical style).
/// HTML `<!-- … -->` comments are *not* skipped here — anything
/// starting with `<` is XML by definition.
pub fn looks_canonical(source: &str) -> bool {
    let bytes = source.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b' ' || b == b'\t' || b == b'\n' || b == b'\r' {
            i += 1;
            continue;
        }
        // `--` line comment.
        if b == b'-' && bytes.get(i + 1) == Some(&b'-') {
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        // `<` opens XML.
        if b == b'<' {
            return false;
        }
        // Must be a lowercase ASCII letter starting the keyword.
        if !b.is_ascii_lowercase() {
            return false;
        }
        // Scan the candidate keyword.
        let start = i;
        while i < bytes.len()
            && (bytes[i].is_ascii_lowercase() || bytes[i].is_ascii_digit() || bytes[i] == b'_')
        {
            i += 1;
        }
        let kw = &source[start..i];
        return TOP_LEVEL_KEYWORDS.contains(&kw);
    }
    false
}

/// Entry point for the canonical parser. Returns the same
/// [`Document`] / [`ParseError`] pair the XML parser does so the
/// outer dispatcher can compose either reader's output uniformly.
pub fn parse(source: &str) -> (Document, Vec<ParseError>) {
    let mut parser = CanonicalParser::new(source);
    let nodes = parser.parse_top_level();
    (Document { nodes }, parser.errors)
}

struct CanonicalParser<'s> {
    scanner: Scanner<'s>,
    errors: Vec<ParseError>,
}

impl<'s> CanonicalParser<'s> {
    fn new(source: &'s str) -> Self {
        Self {
            scanner: Scanner::new(source),
            errors: Vec::new(),
        }
    }

    fn parse_top_level(&mut self) -> Vec<Node> {
        let mut nodes = Vec::new();
        loop {
            self.skip_whitespace_and_comments();
            if self.scanner.is_at_end() {
                break;
            }
            let start_offset = self.scanner.offset();
            let start_pos = self.scanner.position();
            match self.parse_top_level_one(start_pos) {
                Some(node) => nodes.push(node),
                None => {
                    // Safety net: if a sub-parser failed to make
                    // progress, advance one byte so the loop can't
                    // spin. The error has already been recorded.
                    if self.scanner.offset() == start_offset {
                        self.scanner.advance_unicode();
                    }
                }
            }
        }
        nodes
    }

    /// Parse one top-level declaration. Returns `None` on
    /// unrecoverable structural error (the caller advances).
    fn parse_top_level_one(&mut self, start_pos: Position) -> Option<Node> {
        // A canonical file can drop into XML at the top level —
        // §6.11 "tag-expression blend, both ways, freely". When the
        // first non-whitespace token is `<`, parse one balanced XML
        // element and emit its parsed `Node`(s) as siblings of the
        // surrounding canonical declarations. This lets a file
        // declare a `component Greeting(...) = ...` and immediately
        // invoke it via `<Greeting/>` at file scope.
        let first = self.scanner.peek()?;
        if first == '<' {
            let nodes = self.parse_inline_xml_tree(start_pos);
            return nodes.into_iter().next();
        }

        // Scan the keyword.
        let keyword = self.scan_keyword();
        if keyword.is_empty() {
            let bad = self
                .scanner
                .peek()
                .map(|c| c.to_string())
                .unwrap_or_default();
            self.errors.push(ParseError {
                message: format!("Expected declaration keyword, got `{bad}`"),
                range: self.range_from(start_pos),
                code: "expected-keyword",
            });
            // Skip the rest of the line so we make progress.
            self.consume_until_line_end();
            return None;
        }

        match keyword.as_str() {
            "namespace" => self.parse_namespace(start_pos),
            "import" => self.parse_import(start_pos),
            "component" => self.parse_decl_with_params("component", start_pos),
            "trait" => self.parse_decl_with_params("trait", start_pos),
            "mixin" => self.parse_decl_with_params("mixin", start_pos),
            "macro" => self.parse_decl_with_params("macro", start_pos),
            "class" => self.parse_class(start_pos),
            "type" => self.parse_type_decl(start_pos),
            "fn" => self.parse_decl_with_params("fn", start_pos),
            "let" => self.parse_let_decl(start_pos),
            other => {
                self.errors.push(ParseError {
                    message: format!("Unknown declaration keyword `{other}`"),
                    range: self.range_from(start_pos),
                    code: "unknown-keyword",
                });
                self.consume_until_line_end();
                None
            }
        }
    }

    // ── namespace ─────────────────────────────────────────────────

    fn parse_namespace(&mut self, start_pos: Position) -> Option<Node> {
        // `namespace Name` — single-segment only (§6.20).
        self.skip_inline_whitespace();
        let name = self.scan_simple_identifier();
        if name.is_empty() {
            self.errors.push(ParseError {
                message: "`namespace` requires an identifier".into(),
                range: self.range_from(start_pos),
                code: "expected-identifier",
            });
            return None;
        }
        // Namespace is single-segment (§6.20). A `.` here means the
        // author wrote `namespace Foo.Bar`; flag and consume the
        // trailing dotted part so it doesn't leak into the next
        // declaration.
        if self.scanner.peek() == Some('.') {
            self.errors.push(ParseError {
                message: "Namespace must be single-segment (§6.20); \
                         nest via the import graph instead"
                    .into(),
                range: self.range_from(start_pos),
                code: "namespace-multi-segment",
            });
            // Consume the trailing dotted part so the rest of the
            // declaration loop doesn't see it.
            while matches!(
                self.scanner.peek(),
                Some(c) if c == '.' || c.is_ascii_alphanumeric() || c == '_' || c == '-'
            ) {
                self.scanner.advance();
            }
        }
        let range = self.range_from(start_pos);
        Some(self.synth_element(
            "namespace",
            vec![("name", name, range)],
            Vec::new(),
            true,
            range,
        ))
    }

    // ── import ────────────────────────────────────────────────────

    fn parse_import(&mut self, start_pos: Position) -> Option<Node> {
        // `import "path" [as alias]`
        self.skip_inline_whitespace();
        let path_start = self.scanner.position();
        let q = self.scanner.peek();
        if q != Some('"') && q != Some('\'') {
            self.errors.push(ParseError {
                message: "`import` requires a quoted path".into(),
                range: self.range_from(start_pos),
                code: "expected-string",
            });
            self.consume_until_line_end();
            return None;
        }
        let quote = q.unwrap();
        self.scanner.advance();
        let path_body_start = self.scanner.offset();
        while let Some(c) = self.scanner.peek() {
            if c == quote {
                break;
            }
            if c == '\\' {
                self.scanner.advance();
            }
            self.scanner.advance_unicode();
        }
        let path = self.scanner.source()[path_body_start..self.scanner.offset()].to_string();
        if !self.scanner.match_str(&quote.to_string()) {
            self.errors.push(ParseError {
                message: "Unterminated import path".into(),
                range: self.range_from(path_start),
                code: "unterminated-string",
            });
        }
        let path_range = self.range_from(path_start);

        // Optional `as alias`.
        self.skip_inline_whitespace();
        let alias = if self.peek_word("as") {
            self.scan_keyword(); // consume `as`
            self.skip_inline_whitespace();
            let alias_start = self.scanner.position();
            let n = self.scan_identifier_run();
            if n.is_empty() {
                self.errors.push(ParseError {
                    message: "`as` requires an alias identifier".into(),
                    range: self.range_from(alias_start),
                    code: "expected-identifier",
                });
                None
            } else {
                Some((n, self.range_from(alias_start)))
            }
        } else {
            None
        };

        // The extension picks the kind (§6.19): `.prss` → stylesheet,
        // `.luau` → script, `.prui` → component. Anything else logs
        // a warning-shape error but still emits the import (the
        // resolver can flag).
        let kind = match extension_of(&path) {
            Some("prss") => "stylesheet",
            Some("luau") => "script",
            Some("prui") => "component",
            _ => "import",
        };

        let range = self.range_from(start_pos);
        let mut attrs = vec![(kind, path, path_range)];
        if let Some((alias_name, alias_range)) = alias {
            attrs.push(("as", alias_name, alias_range));
        }
        Some(self.synth_element("import", attrs, Vec::new(), true, range))
    }

    // ── component / trait / mixin / macro / fn ─────────────────────
    //
    // Shared declaration shape: `<keyword> Name [<generics>](params)
    // [: Trait, Trait] = expr` or `{ body }`.

    fn parse_decl_with_params(&mut self, keyword: &str, start_pos: Position) -> Option<Node> {
        self.skip_inline_whitespace();
        let name_start = self.scanner.position();
        let name = self.scan_identifier_run();
        if name.is_empty() && keyword != "macro" {
            self.errors.push(ParseError {
                message: format!("`{keyword}` requires a name"),
                range: self.range_from(start_pos),
                code: "expected-identifier",
            });
            return None;
        }

        // Optional generics — `<T, U: Bound>`. Captured verbatim so
        // Phase 7's type checker can read the raw text without our
        // having to model the generic AST here.
        let generics = self.try_parse_generics();

        // Optional parameter list `(...)`.
        self.skip_whitespace_and_comments();
        let params = if self.scanner.peek() == Some('(') {
            self.parse_param_list()
        } else {
            Vec::new()
        };

        // Optional `: Trait, Trait` for trait conformance (§6.14).
        self.skip_whitespace_and_comments();
        let impls = if self.scanner.peek() == Some(':') {
            self.scanner.advance();
            self.parse_impl_list()
        } else {
            Vec::new()
        };

        // Optional `→ ReturnType` for `fn` (and components, which we
        // accept loosely — the return-type slot exists in the
        // grammar but is rare on components).
        self.skip_whitespace_and_comments();
        let return_type = if self.peek_arrow() {
            self.consume_arrow();
            self.skip_whitespace_and_comments();
            Some(self.scan_type_expr())
        } else {
            None
        };

        // Body — either `= expr` or `{ … }`. A bare declaration
        // with neither (e.g. `trait Marker`) is also legal and
        // emits an empty body.
        self.skip_whitespace_and_comments();
        let body_nodes = self.parse_decl_body(start_pos);

        let range = self.range_from(start_pos);
        let name_range = if name.is_empty() {
            self.range_from(name_start)
        } else {
            range_at(
                self.scanner.source(),
                name_start.offset,
                name_start.offset + name.len(),
            )
        };

        let mut attrs: Vec<(&'static str, String, SourceRange)> = Vec::new();
        if !name.is_empty() {
            attrs.push(("name", name, name_range));
        }
        if let Some(g) = generics {
            attrs.push(("generics", g, range));
        }
        if !impls.is_empty() {
            attrs.push(("impls", impls.join(", "), range));
        }
        if let Some(rt) = return_type {
            attrs.push(("return", rt, range));
        }

        // Property children go first (mirrors the §7.1 doc shape
        // where `<property>` sub-tags lead the body in the
        // original XML form).
        let mut children = Vec::new();
        for p in params {
            children.push(self.synth_param_element(&p));
        }
        children.extend(body_nodes);

        let self_closing = children.is_empty();
        Some(self.synth_element(keyword, attrs, children, self_closing, range))
    }

    fn parse_let_decl(&mut self, start_pos: Position) -> Option<Node> {
        // `let name [: type] = expression`
        self.skip_inline_whitespace();
        let name_start = self.scanner.position();
        let name = self.scan_identifier_run();
        if name.is_empty() {
            self.errors.push(ParseError {
                message: "`let` requires a binding name".into(),
                range: self.range_from(start_pos),
                code: "expected-identifier",
            });
            return None;
        }
        self.skip_inline_whitespace();
        let ty = if self.scanner.peek() == Some(':') {
            self.scanner.advance();
            self.skip_inline_whitespace();
            Some(self.scan_type_expr())
        } else {
            None
        };
        self.skip_inline_whitespace();
        if !self.scanner.match_str("=") {
            self.errors.push(ParseError {
                message: "`let <name>` must be followed by `= expr`".into(),
                range: self.range_from(start_pos),
                code: "expected-equals",
            });
            return None;
        }
        self.skip_inline_whitespace();
        let value = self.scan_value_to_end_of_decl();
        let range = self.range_from(start_pos);
        let name_range = range_at(
            self.scanner.source(),
            name_start.offset,
            name_start.offset + name.len(),
        );
        let mut attrs = vec![("name", name, name_range), ("value", value, range)];
        if let Some(t) = ty {
            attrs.push(("type", t, range));
        }
        Some(self.synth_element("let", attrs, Vec::new(), true, range))
    }

    fn parse_type_decl(&mut self, start_pos: Position) -> Option<Node> {
        // `type Name [<generics>] = type-expr`
        self.skip_inline_whitespace();
        let name_start = self.scanner.position();
        let name = self.scan_identifier_run();
        if name.is_empty() {
            self.errors.push(ParseError {
                message: "`type` requires a name".into(),
                range: self.range_from(start_pos),
                code: "expected-identifier",
            });
            return None;
        }
        let generics = self.try_parse_generics();
        self.skip_whitespace_and_comments();
        if !self.scanner.match_str("=") {
            self.errors.push(ParseError {
                message: "`type <Name>` must be followed by `= type-expr`".into(),
                range: self.range_from(start_pos),
                code: "expected-equals",
            });
            return None;
        }
        self.skip_inline_whitespace();
        let body = self.scan_value_to_end_of_decl();
        let range = self.range_from(start_pos);
        let name_range = range_at(
            self.scanner.source(),
            name_start.offset,
            name_start.offset + name.len(),
        );
        let mut attrs = vec![("name", name, name_range), ("body", body, range)];
        if let Some(g) = generics {
            attrs.push(("generics", g, range));
        }
        Some(self.synth_element("type", attrs, Vec::new(), true, range))
    }

    fn parse_class(&mut self, start_pos: Position) -> Option<Node> {
        // `class Name { PRSS-body }` — the PRSS body is kept as a
        // single raw-text child so the downstream PRSS reader gets
        // it intact (mirrors `<style>` raw-text mode in XML form).
        self.skip_inline_whitespace();
        let name_start = self.scanner.position();
        let name = self.scan_identifier_run();
        if name.is_empty() {
            self.errors.push(ParseError {
                message: "`class` requires a name".into(),
                range: self.range_from(start_pos),
                code: "expected-identifier",
            });
            return None;
        }
        self.skip_whitespace_and_comments();
        if self.scanner.peek() != Some('{') {
            self.errors.push(ParseError {
                message: "`class <Name>` must be followed by `{ … }`".into(),
                range: self.range_from(start_pos),
                code: "expected-brace",
            });
            return None;
        }
        let body_start = self.scanner.position();
        let body = self.scan_balanced_braces();
        let body_range = SourceRange {
            start: body_start,
            end: self.scanner.position(),
        };
        let range = self.range_from(start_pos);
        let name_range = range_at(
            self.scanner.source(),
            name_start.offset,
            name_start.offset + name.len(),
        );

        let children = vec![Node::Text {
            value: body,
            range: body_range,
        }];
        // Emit as `<style class="Name">…</style>` so the existing PRSS
        // lowering picks up the body identically to an XML `<style>`
        // block.
        Some(self.synth_element(
            "style",
            vec![("class", name, name_range)],
            children,
            false,
            range,
        ))
    }

    // ── shared sub-parsers ─────────────────────────────────────────

    /// Parse a `(...)` parameter list. Each parameter is
    /// `name[: type][= default][ required]`. Defaults may contain
    /// trailing `required` keyword (§7.1).
    fn parse_param_list(&mut self) -> Vec<DeclParam> {
        self.scanner.advance(); // `(`
        let mut params = Vec::new();
        loop {
            self.skip_whitespace_and_comments();
            // Allow trailing comma + `)`.
            while self.scanner.peek() == Some(',') {
                self.scanner.advance();
                self.skip_whitespace_and_comments();
            }
            match self.scanner.peek() {
                None => {
                    let pos = self.scanner.position();
                    self.errors.push(ParseError {
                        message: "Unterminated parameter list".into(),
                        range: SourceRange {
                            start: pos,
                            end: pos,
                        },
                        code: "unterminated-params",
                    });
                    break;
                }
                Some(')') => {
                    self.scanner.advance();
                    break;
                }
                _ => {}
            }
            let p_start = self.scanner.position();
            let name = self.scan_identifier_run();
            if name.is_empty() {
                let bad = self
                    .scanner
                    .peek()
                    .map(|c| c.to_string())
                    .unwrap_or_default();
                self.errors.push(ParseError {
                    message: format!("Expected parameter name, got `{bad}`"),
                    range: self.range_from(p_start),
                    code: "expected-identifier",
                });
                // Skip to the next comma / close.
                self.scan_until_top_level(&[',', ')']);
                continue;
            }
            self.skip_inline_whitespace();
            let type_expr = if self.scanner.peek() == Some(':') {
                self.scanner.advance();
                self.skip_inline_whitespace();
                let t = self.scan_type_expr();
                Some(t)
            } else {
                None
            };

            // After the type, look for `= default` and / or
            // trailing `required`. Both are optional.
            self.skip_inline_whitespace();
            let mut default = None;
            let mut required = false;

            // Handle trailing `required` token, which can appear
            // either before or after a `= default`. Iterate.
            loop {
                if self.peek_word("required") {
                    self.scan_keyword();
                    required = true;
                    self.skip_inline_whitespace();
                    continue;
                }
                if self.scanner.peek() == Some('=') {
                    self.scanner.advance();
                    self.skip_inline_whitespace();
                    let d = self.scan_param_default();
                    default = Some(d);
                    self.skip_inline_whitespace();
                    continue;
                }
                break;
            }
            params.push(DeclParam {
                name,
                type_expr,
                default,
                required,
                range: self.range_from(p_start),
            });
        }
        params
    }

    /// Parse `: Trait, Trait, ...` (the conformance list after the
    /// parameter `()`). Each trait is captured as its raw textual
    /// form (no spaces / no `,`); generics inside one trait are
    /// kept intact via depth tracking.
    fn parse_impl_list(&mut self) -> Vec<String> {
        let mut traits = Vec::new();
        loop {
            self.skip_inline_whitespace();
            let mut name = String::new();
            let mut depth: i32 = 0;
            while let Some(c) = self.scanner.peek() {
                if depth == 0 && matches!(c, ',' | '=' | '{') {
                    break;
                }
                if depth == 0 && c == '\n' {
                    break;
                }
                if c == '<' {
                    depth += 1;
                }
                if c == '>' {
                    depth -= 1;
                }
                name.push(c);
                self.scanner.advance();
            }
            let t = name.trim().to_string();
            if !t.is_empty() {
                traits.push(t);
            }
            if self.scanner.peek() == Some(',') {
                self.scanner.advance();
                continue;
            }
            break;
        }
        traits
    }

    /// Parse the body of a declaration: `= expr` OR `{ body }` OR
    /// nothing. Returns the body's child node list.
    fn parse_decl_body(&mut self, start_pos: Position) -> Vec<Node> {
        if self.scanner.peek() == Some('=') {
            self.scanner.advance();
            self.skip_whitespace_and_comments();
            // `= { body }` and `= <tree/>` and `= expr` are all
            // valid. Brace → block body; `<` → XML tree; anything
            // else → opaque expression that we wrap in a single
            // `<expr>` child.
            match self.scanner.peek() {
                Some('{') => self.parse_block_body(start_pos),
                Some('<') => self.parse_inline_xml_tree(start_pos),
                _ => self.parse_inline_expression(start_pos),
            }
        } else if self.scanner.peek() == Some('{') {
            self.parse_block_body(start_pos)
        } else {
            Vec::new()
        }
    }

    fn parse_inline_expression(&mut self, _start_pos: Position) -> Vec<Node> {
        let pos = self.scanner.position();
        let body = self.scan_value_to_end_of_decl();
        let range = self.range_from(pos);
        if body.trim().is_empty() {
            return Vec::new();
        }
        // Capture as a single `<expr>` child carrying the raw text
        // verbatim. Phase 7's lowering takes this and re-parses
        // against the expression grammar.
        let tag_range = range;
        vec![Node::Element(Element {
            tag: "expr".into(),
            attributes: vec![Attribute {
                name: AttributeName {
                    raw: "body".into(),
                    local: "body".into(),
                    namespace: AttributeNamespace::Bare,
                    range,
                },
                value: AttributeValue::String { value: body, range },
                range,
            }],
            children: Vec::new(),
            self_closing: true,
            range,
            tag_range,
        })]
    }

    fn parse_inline_xml_tree(&mut self, start_pos: Position) -> Vec<Node> {
        // Scan exactly one balanced XML element (`<tag …/>` or
        // `<tag …>…</tag>`), then hand that single substring to
        // the XML reader. Limiting the slice prevents the XML
        // reader from running past the element into surrounding
        // canonical-form code (the closing `}` of the body, or a
        // sibling statement).
        let start_offset = self.scanner.offset();
        let element_len = match scan_balanced_xml_element(self.scanner.source(), start_offset) {
            Some(len) => len,
            None => {
                self.errors.push(ParseError {
                    message: "Malformed inline XML tree".into(),
                    range: self.range_from(start_pos),
                    code: "malformed-xml-tree",
                });
                // Advance one char so the loop makes progress.
                self.scanner.advance_unicode();
                return Vec::new();
            }
        };
        let slice = &self.scanner.source()[start_offset..start_offset + element_len];
        // Use the XML reader directly so a `<` inside a canonical
        // body doesn't bounce through `looks_canonical` (which
        // would route back here, but be explicit).
        let (frag_doc, frag_errs) = super::parse_xml(slice);
        for e in frag_errs {
            self.errors.push(ParseError {
                message: e.message,
                range: shift_range(e.range, start_offset),
                code: e.code,
            });
        }
        // Advance the scanner over the consumed bytes. The XML
        // element is ASCII at its tags; element_len is in bytes so
        // we walk that many bytes via advance_unicode (each step
        // moves one full codepoint).
        let end_offset = start_offset + element_len;
        while self.scanner.offset() < end_offset {
            self.scanner.advance_unicode();
        }
        frag_doc.nodes
    }

    /// Parse a `{ ... }` block body for a component / mixin /
    /// macro / fn. Statements inside are recognised line-by-line:
    ///
    /// - `let name = …`              → `<let name=… value=…/>`
    /// - `use Name [, Name…]`        → `<use names="Name, Name"/>`
    /// - `requires name: Type [, …]` → `<requires names="name: T,…"/>`
    /// - `on event[(args)] { … }`    → `<on event="event" args=…>body</on>`
    /// - `style { … }`               → `<style>…</style>` (raw-text PRSS body)
    /// - `match { … } expand { … }`  → for macros: `<match>…</match><expand>…</expand>`
    /// - `<tag …>…</tag>`            → XML tree (the body's value)
    /// - anything else                → `<expr body="…"/>`
    fn parse_block_body(&mut self, _start_pos: Position) -> Vec<Node> {
        let _brace_start = self.scanner.position();
        if self.scanner.peek() != Some('{') {
            return Vec::new();
        }
        self.scanner.advance(); // `{`
        let mut out = Vec::new();
        loop {
            self.skip_whitespace_and_comments();
            match self.scanner.peek() {
                None => {
                    self.errors.push(ParseError {
                        message: "Unterminated declaration body".into(),
                        range: self.range_from(self.scanner.position()),
                        code: "unterminated-block",
                    });
                    break;
                }
                Some('}') => {
                    self.scanner.advance();
                    break;
                }
                _ => {}
            }
            let stmt_start = self.scanner.position();
            if let Some(node) = self.parse_body_stmt(stmt_start) {
                out.push(node);
            } else if self.scanner.position().offset == stmt_start.offset {
                // No progress — bail to avoid infinite loop.
                self.scanner.advance_unicode();
            }
            // Optional `;` or `,` separator.
            self.skip_inline_whitespace();
            if matches!(self.scanner.peek(), Some(';') | Some(',')) {
                self.scanner.advance();
            }
        }
        out
    }

    fn parse_body_stmt(&mut self, stmt_start: Position) -> Option<Node> {
        // XML tree → delegate to the XML reader.
        if self.scanner.peek() == Some('<') {
            // Skip XML-comment / closing tag corner cases — the
            // XML reader handles them.
            let xml = self.parse_inline_xml_tree(stmt_start);
            return xml.into_iter().next();
        }
        // Keyword-led statements.
        let kw_start = self.scanner.position();
        let kw = self.peek_word_run();
        match kw.as_str() {
            "let" => {
                self.scan_keyword();
                self.parse_let_decl(kw_start)
            }
            "use" => {
                self.scan_keyword();
                self.parse_use_stmt(kw_start)
            }
            "requires" => {
                self.scan_keyword();
                self.parse_requires_stmt(kw_start)
            }
            "on" => {
                self.scan_keyword();
                self.parse_on_handler(kw_start)
            }
            "style" => {
                self.scan_keyword();
                self.parse_style_block(kw_start)
            }
            "match" => {
                self.scan_keyword();
                self.parse_macro_match(kw_start)
            }
            "expand" => {
                self.scan_keyword();
                self.parse_macro_expand(kw_start)
            }
            "fn" => {
                self.scan_keyword();
                self.parse_decl_with_params("fn", kw_start)
            }
            "type" => {
                self.scan_keyword();
                self.parse_type_decl(kw_start)
            }
            _ => {
                // Treat the rest of the line as an opaque expression
                // (effect / state-write / function-call etc.).
                let body = self.scan_value_to_end_of_decl();
                if body.trim().is_empty() {
                    return None;
                }
                let range = self.range_from(stmt_start);
                Some(synth_self_closing(
                    "expr",
                    vec![("body", body, range)],
                    range,
                ))
            }
        }
    }

    fn parse_use_stmt(&mut self, start_pos: Position) -> Option<Node> {
        // `use Name [, Name…] [as Alias]`
        self.skip_inline_whitespace();
        let line = self.scan_value_to_end_of_decl();
        let trimmed = line.trim().to_string();
        let range = self.range_from(start_pos);
        Some(synth_self_closing(
            "use",
            vec![("names", trimmed, range)],
            range,
        ))
    }

    fn parse_requires_stmt(&mut self, start_pos: Position) -> Option<Node> {
        // `requires name: Type [, name: Type…]`
        self.skip_inline_whitespace();
        let line = self.scan_value_to_end_of_decl();
        let trimmed = line.trim().to_string();
        let range = self.range_from(start_pos);
        Some(synth_self_closing(
            "requires",
            vec![("names", trimmed, range)],
            range,
        ))
    }

    fn parse_on_handler(&mut self, start_pos: Position) -> Option<Node> {
        // `on event[(args)] [if cond] { body }`
        // Capture the event name + the optional `(args)` and
        // `if cond` segments as separate attributes, then the
        // brace body as children (parsed recursively as a block
        // — handlers can contain nested statements).
        self.skip_inline_whitespace();
        let event_start = self.scanner.position();
        let event = self.scan_identifier_run();
        if event.is_empty() {
            self.errors.push(ParseError {
                message: "`on` requires an event name".into(),
                range: self.range_from(start_pos),
                code: "expected-identifier",
            });
            return None;
        }
        self.skip_inline_whitespace();
        let args = if self.scanner.peek() == Some('(') {
            let args_start = self.scanner.position();
            let raw = self.scan_balanced(('(', ')'));
            Some((raw, self.range_from(args_start)))
        } else {
            None
        };
        self.skip_inline_whitespace();
        let guard = if self.peek_word("if") {
            self.scan_keyword();
            self.skip_inline_whitespace();
            let g_start = self.scanner.position();
            let mut g = String::new();
            while let Some(c) = self.scanner.peek() {
                if c == '{' || c == '\n' {
                    break;
                }
                g.push(c);
                self.scanner.advance();
            }
            Some((g.trim().to_string(), self.range_from(g_start)))
        } else {
            None
        };
        self.skip_whitespace_and_comments();
        let body_nodes = if self.scanner.peek() == Some('{') {
            self.parse_block_body(start_pos)
        } else {
            Vec::new()
        };
        let range = self.range_from(start_pos);
        let event_range = range_at(
            self.scanner.source(),
            event_start.offset,
            event_start.offset + event.len(),
        );

        let mut attrs: Vec<(&'static str, String, SourceRange)> =
            vec![("event", event, event_range)];
        if let Some((a, r)) = args {
            attrs.push(("args", a, r));
        }
        if let Some((g, r)) = guard {
            attrs.push(("guard", g, r));
        }
        Some(self.synth_element("on", attrs, body_nodes, false, range))
    }

    fn parse_style_block(&mut self, start_pos: Position) -> Option<Node> {
        // `style { PRSS-body }`
        self.skip_inline_whitespace();
        if self.scanner.peek() != Some('{') {
            self.errors.push(ParseError {
                message: "`style` must be followed by `{ … }`".into(),
                range: self.range_from(start_pos),
                code: "expected-brace",
            });
            return None;
        }
        let body_start = self.scanner.position();
        let body = self.scan_balanced_braces();
        let body_range = SourceRange {
            start: body_start,
            end: self.scanner.position(),
        };
        let children = vec![Node::Text {
            value: body,
            range: body_range,
        }];
        let range = self.range_from(start_pos);
        Some(self.synth_element("style", Vec::new(), children, false, range))
    }

    fn parse_macro_match(&mut self, start_pos: Position) -> Option<Node> {
        // `match { tree }` — used inside a macro body
        self.skip_whitespace_and_comments();
        if self.scanner.peek() != Some('{') {
            return None;
        }
        let inner = self.parse_block_body(start_pos);
        let range = self.range_from(start_pos);
        Some(self.synth_element("match", Vec::new(), inner, false, range))
    }

    fn parse_macro_expand(&mut self, start_pos: Position) -> Option<Node> {
        // `expand { tree }` — used inside a macro body
        self.skip_whitespace_and_comments();
        if self.scanner.peek() != Some('{') {
            return None;
        }
        let inner = self.parse_block_body(start_pos);
        let range = self.range_from(start_pos);
        Some(self.synth_element("expand", Vec::new(), inner, false, range))
    }

    // ── scanning primitives ────────────────────────────────────────

    fn skip_inline_whitespace(&mut self) {
        while matches!(self.scanner.peek(), Some(' ') | Some('\t')) {
            self.scanner.advance();
        }
    }

    fn skip_whitespace_and_comments(&mut self) {
        loop {
            self.scanner.skip_whitespace_and_newlines();
            if self.scanner.peek() == Some('-') && self.scanner.peek_ahead(1) == Some('-') {
                // Line comment — also a `<!-- -->` block when the
                // next char is `[`. Roadmap doesn't lock that in,
                // so support `--` line comments only.
                while let Some(c) = self.scanner.peek() {
                    if c == '\n' {
                        break;
                    }
                    self.scanner.advance();
                }
                continue;
            }
            break;
        }
    }

    fn consume_until_line_end(&mut self) {
        while let Some(c) = self.scanner.peek() {
            if c == '\n' {
                break;
            }
            self.scanner.advance_unicode();
        }
    }

    /// Lowercase ASCII keyword (`component`, `let`, …). Returns
    /// empty string on miss.
    fn scan_keyword(&mut self) -> String {
        let start = self.scanner.offset();
        while let Some(c) = self.scanner.peek() {
            if c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' {
                self.scanner.advance();
            } else {
                break;
            }
        }
        self.scanner.source()[start..self.scanner.offset()].to_string()
    }

    /// Identifier (Pascal or kebab or camel — anything that scans
    /// like a name segment plus dot-paths).
    fn scan_identifier_run(&mut self) -> String {
        let start = self.scanner.offset();
        while let Some(c) = self.scanner.peek() {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.' {
                self.scanner.advance();
            } else {
                break;
            }
        }
        self.scanner.source()[start..self.scanner.offset()].to_string()
    }

    /// Single-segment identifier (no `.`). Used by namespaces and
    /// any other position where a dotted path is rejected (§6.20).
    fn scan_simple_identifier(&mut self) -> String {
        let start = self.scanner.offset();
        while let Some(c) = self.scanner.peek() {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                self.scanner.advance();
            } else {
                break;
            }
        }
        self.scanner.source()[start..self.scanner.offset()].to_string()
    }

    fn peek_word_run(&self) -> String {
        let mut i = self.scanner.offset();
        let bytes = self.scanner.source().as_bytes();
        while i < bytes.len()
            && (bytes[i].is_ascii_lowercase() || bytes[i].is_ascii_digit() || bytes[i] == b'_')
        {
            i += 1;
        }
        self.scanner.source()[self.scanner.offset()..i].to_string()
    }

    /// Compare the upcoming run against `word`. Matches when the
    /// run is exactly `word` AND the byte after is not an ident
    /// continuation.
    fn peek_word(&self, word: &str) -> bool {
        let src = self.scanner.source();
        let off = self.scanner.offset();
        if !src[off..].starts_with(word) {
            return false;
        }
        let after = off + word.len();
        match src.as_bytes().get(after) {
            None => true,
            Some(b) => !(b.is_ascii_alphanumeric() || *b == b'_' || *b == b'-'),
        }
    }

    fn peek_arrow(&self) -> bool {
        // Either `→` (U+2192) or `->`.
        let src = self.scanner.source();
        let off = self.scanner.offset();
        src[off..].starts_with('→') || src[off..].starts_with("->")
    }

    fn consume_arrow(&mut self) {
        let src = self.scanner.source();
        let off = self.scanner.offset();
        if src[off..].starts_with('→') {
            self.scanner.advance_unicode();
        } else if src[off..].starts_with("->") {
            self.scanner.advance();
            self.scanner.advance();
        }
    }

    fn try_parse_generics(&mut self) -> Option<String> {
        if self.scanner.peek() != Some('<') {
            return None;
        }
        // Heuristic: only treat `<…>` as generics if the run inside
        // could plausibly be a type list — i.e. it starts with an
        // ASCII alpha. (`<tag…>` is XML, but at this position
        // we're between a name and `(` / `:` / `=` / `{`, so a `<`
        // here is generics.)
        let start = self.scanner.offset();
        let saved = self.scanner.save();
        self.scanner.advance(); // `<`
        let mut depth: i32 = 1;
        while let Some(c) = self.scanner.peek() {
            if c == '<' {
                depth += 1;
                self.scanner.advance();
                continue;
            }
            if c == '>' {
                depth -= 1;
                self.scanner.advance();
                if depth == 0 {
                    let raw = self.scanner.source()[start + 1..self.scanner.offset() - 1]
                        .trim()
                        .to_string();
                    if raw.is_empty() {
                        return None;
                    }
                    return Some(raw);
                }
                continue;
            }
            if c == '\n' && depth > 0 {
                // Generics shouldn't span lines without a
                // continuation — bail and treat `<` as a syntax
                // error elsewhere.
                self.scanner.restore(saved);
                return None;
            }
            self.scanner.advance();
        }
        // Fell off the end — restore.
        self.scanner.restore(saved);
        None
    }

    /// Scan a type expression as a raw string. We don't attempt
    /// to model the type AST here; Phase 7's type checker reads
    /// the raw text back. Stops on top-level `,` / `)` / `=` /
    /// `{` / `;` / newline (with depth tracking for `<…>` / `(…)`
    /// / `[…]` / `{…}`).
    fn scan_type_expr(&mut self) -> String {
        let start = self.scanner.offset();
        let mut depth: i32 = 0;
        let mut bar_count: i32 = 0;
        while let Some(c) = self.scanner.peek() {
            if depth == 0 {
                if matches!(c, ',' | ')' | ';' | '\n') {
                    break;
                }
                // `= default` ends the type unless we're inside a
                // generic / record / paren.
                if c == '=' {
                    break;
                }
                // Trailing `required` keyword on a param — stop
                // before it so the param parser can pick it up.
                if c == ' ' || c == '\t' {
                    let saved = self.scanner.save();
                    self.scanner.skip_whitespace();
                    if self.peek_word("required") {
                        self.scanner.restore(saved);
                        break;
                    }
                    self.scanner.restore(saved);
                }
            }
            if c == '<' || c == '(' || c == '[' || c == '{' {
                depth += 1;
            }
            if c == '>' || c == ')' || c == ']' || c == '}' {
                if depth == 0 {
                    break;
                }
                depth -= 1;
            }
            if c == '|' && depth == 0 {
                // Union types use `|` at depth 0 — that's fine;
                // record it so the trailing trim doesn't kill it.
                bar_count += 1;
            }
            self.scanner.advance();
        }
        let _ = bar_count;
        self.scanner.source()[start..self.scanner.offset()]
            .trim()
            .to_string()
    }

    /// Scan a parameter default value. Stops on top-level `,` /
    /// `)`, honouring quote / brace / paren / bracket depth.
    fn scan_param_default(&mut self) -> String {
        let start = self.scanner.offset();
        let mut depth: i32 = 0;
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
            if depth == 0 && matches!(c, ',' | ')') {
                break;
            }
            match c {
                '"' | '\'' => {
                    quote = Some(c);
                    self.scanner.advance();
                }
                '(' | '[' | '{' | '<' => {
                    depth += 1;
                    self.scanner.advance();
                }
                ')' | ']' | '}' | '>' => {
                    if depth == 0 {
                        break;
                    }
                    depth -= 1;
                    self.scanner.advance();
                }
                _ => {
                    self.scanner.advance_unicode();
                }
            }
        }
        self.scanner.source()[start..self.scanner.offset()]
            .trim()
            .to_string()
    }

    /// Scan an opaque value run that runs to end-of-line OR an
    /// outer `;` / `}` / `,` at depth 0. Used for `let value`,
    /// `type body`, `expr` statements.
    fn scan_value_to_end_of_decl(&mut self) -> String {
        let start = self.scanner.offset();
        let mut depth: i32 = 0;
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
            if depth == 0 && (c == '\n' || c == ';' || c == '}') {
                break;
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
                    if depth == 0 {
                        break;
                    }
                    depth -= 1;
                    self.scanner.advance();
                }
                _ => {
                    self.scanner.advance_unicode();
                }
            }
        }
        self.scanner.source()[start..self.scanner.offset()]
            .trim()
            .to_string()
    }

    fn scan_until_top_level(&mut self, stops: &[char]) {
        let mut depth: i32 = 0;
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
            if depth == 0 && stops.contains(&c) {
                break;
            }
            match c {
                '"' | '\'' => {
                    quote = Some(c);
                    self.scanner.advance();
                }
                '(' | '[' | '{' | '<' => {
                    depth += 1;
                    self.scanner.advance();
                }
                ')' | ']' | '}' | '>' => {
                    if depth == 0 {
                        break;
                    }
                    depth -= 1;
                    self.scanner.advance();
                }
                _ => {
                    self.scanner.advance_unicode();
                }
            }
        }
    }

    /// Scan a `{ ... }` balanced run, returning the inner body.
    /// Consumes both braces.
    fn scan_balanced_braces(&mut self) -> String {
        if self.scanner.peek() != Some('{') {
            return String::new();
        }
        self.scanner.advance(); // `{`
        let start = self.scanner.offset();
        let mut depth: i32 = 1;
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
                '{' => {
                    depth += 1;
                    self.scanner.advance();
                }
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        let body = self.scanner.source()[start..self.scanner.offset()].to_string();
                        self.scanner.advance(); // closing `}`
                        return body;
                    }
                    self.scanner.advance();
                }
                _ => {
                    self.scanner.advance_unicode();
                }
            }
        }
        // Unterminated.
        self.errors.push(ParseError {
            message: "Unterminated `{ … }` block".into(),
            range: self.range_from(self.scanner.position()),
            code: "unterminated-block",
        });
        self.scanner.source()[start..self.scanner.offset()].to_string()
    }

    /// Scan a balanced run inside a `(…)` (or other) pair. The
    /// caller passes the open + close chars; we consume both
    /// brackets and return the inner text.
    fn scan_balanced(&mut self, pair: (char, char)) -> String {
        if self.scanner.peek() != Some(pair.0) {
            return String::new();
        }
        self.scanner.advance(); // open
        let start = self.scanner.offset();
        let mut depth: i32 = 1;
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
            if c == '"' || c == '\'' {
                quote = Some(c);
                self.scanner.advance();
                continue;
            }
            if c == pair.0 {
                depth += 1;
                self.scanner.advance();
                continue;
            }
            if c == pair.1 {
                depth -= 1;
                if depth == 0 {
                    let body = self.scanner.source()[start..self.scanner.offset()].to_string();
                    self.scanner.advance();
                    return body;
                }
                self.scanner.advance();
                continue;
            }
            self.scanner.advance_unicode();
        }
        // Unterminated.
        self.errors.push(ParseError {
            message: format!("Unterminated `{}…{}` pair", pair.0, pair.1),
            range: self.range_from(self.scanner.position()),
            code: "unterminated-pair",
        });
        self.scanner.source()[start..self.scanner.offset()].to_string()
    }

    // ── element synthesis ──────────────────────────────────────────

    fn synth_element(
        &self,
        tag: &str,
        attrs: Vec<(&'static str, String, SourceRange)>,
        children: Vec<Node>,
        self_closing: bool,
        range: SourceRange,
    ) -> Node {
        synth_element_node(tag, attrs, children, self_closing, range)
    }

    fn synth_param_element(&self, p: &DeclParam) -> Node {
        let mut attrs: Vec<(&'static str, String, SourceRange)> =
            vec![("name", p.name.clone(), p.range)];
        if let Some(t) = &p.type_expr {
            attrs.push(("type", t.clone(), p.range));
        }
        if let Some(d) = &p.default {
            attrs.push(("default", d.clone(), p.range));
        }
        if p.required {
            attrs.push(("required", "true".into(), p.range));
        }
        synth_element_node("property", attrs, Vec::new(), true, p.range)
    }

    fn range_from(&self, start: Position) -> SourceRange {
        SourceRange {
            start,
            end: self.scanner.position(),
        }
    }
}

// ── helpers ───────────────────────────────────────────────────────

#[derive(Debug, Clone)]
struct DeclParam {
    name: String,
    type_expr: Option<String>,
    default: Option<String>,
    required: bool,
    range: SourceRange,
}

fn extension_of(path: &str) -> Option<&str> {
    path.rsplit('.').next().and_then(|ext| {
        if ext.len() == path.len() {
            None
        } else {
            Some(ext)
        }
    })
}

fn synth_element_node(
    tag: &str,
    attrs: Vec<(&'static str, String, SourceRange)>,
    children: Vec<Node>,
    self_closing: bool,
    range: SourceRange,
) -> Node {
    let attributes = attrs
        .into_iter()
        .map(|(raw, value, r)| Attribute {
            name: AttributeName {
                raw: raw.to_string(),
                local: raw.to_string(),
                namespace: AttributeNamespace::Bare,
                range: r,
            },
            value: AttributeValue::String { value, range: r },
            range: r,
        })
        .collect();
    Node::Element(Element {
        tag: tag.to_string(),
        attributes,
        children,
        self_closing,
        range,
        tag_range: range,
    })
}

fn synth_self_closing(
    tag: &str,
    attrs: Vec<(&'static str, String, SourceRange)>,
    range: SourceRange,
) -> Node {
    synth_element_node(tag, attrs, Vec::new(), true, range)
}

/// Re-emit a `SourceRange` from start offset + a known end offset
/// (used when the originating substring fits a known byte length).
fn range_at(source: &str, start_off: usize, end_off: usize) -> SourceRange {
    use crate::language::syntax::pos_at;
    SourceRange {
        start: pos_at(source, start_off),
        end: pos_at(source, end_off),
    }
}

/// Shift a sub-document's `SourceRange` by `offset` bytes so it
/// reads correctly against the outer source. Line / column are
/// recomputed off the outer source's offsets — for now we offset
/// only the byte position; the consuming layer's diagnostic code
/// re-computes line / column from offsets as needed.
fn shift_range(r: SourceRange, _offset: usize) -> SourceRange {
    // Sub-document is parsed against a slice starting at `offset`,
    // so its `Position { offset, line, column }` is local. To
    // avoid double-counting, we leave the offset/line/column
    // numbers as-is here — callers that need outer coordinates
    // recompute via `pos_at` on the outer source. The outer
    // dispatcher records the slice start and can adjust if needed
    // (we keep this simple for Phase 2; full re-anchoring lands
    // with the schema-first cross-language unification in §7.14).
    r
}

/// Scan a single balanced XML element starting at `start_off` in
/// `source`. Returns the byte length consumed (so the caller can
/// slice `source[start_off..start_off + len]` and feed it to the
/// XML reader). `None` if the input doesn't look like a tag start
/// or if the open/close pair never balances out.
///
/// Tracks:
/// - Self-closing `<tag …/>` (one tag, returns length up to `/>`).
/// - Open / close `<tag …>…</tag>` with nested elements (depth
///   counter; nests on every `<tag>` and decrements on every
///   `</tag>`).
/// - Quoted attribute values (`"…"` / `'…'`) — `<` / `>` inside
///   a string don't move the depth counter.
/// - Inline `{ … }` interpolations — braces inside interpolations
///   don't affect tag scanning, but `<` / `>` inside the body of
///   an interpolation still need to be ignored because they may
///   appear as comparison operators in an expression body.
fn scan_balanced_xml_element(source: &str, start_off: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    if start_off >= bytes.len() || bytes[start_off] != b'<' {
        return None;
    }
    let mut i = start_off;
    let mut depth: i32 = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b'<' {
            // Could be `<tag>`, `</tag>`, or `<!--`.
            if i + 4 <= bytes.len() && &bytes[i..i + 4] == b"<!--" {
                // Skip to `-->`.
                let mut j = i + 4;
                while j + 3 <= bytes.len() && &bytes[j..j + 3] != b"-->" {
                    j += 1;
                }
                if j + 3 > bytes.len() {
                    return None;
                }
                i = j + 3;
                continue;
            }
            let closing = bytes.get(i + 1) == Some(&b'/');
            // Find the matching `>` (or `/>`).
            let mut j = i + 1;
            let mut self_closing = false;
            let mut in_quote: Option<u8> = None;
            let mut in_brace: i32 = 0;
            while j < bytes.len() {
                let c = bytes[j];
                if let Some(q) = in_quote {
                    if c == b'\\' && j + 1 < bytes.len() {
                        j += 2;
                        continue;
                    }
                    if c == q {
                        in_quote = None;
                    }
                    j += 1;
                    continue;
                }
                if c == b'"' || c == b'\'' {
                    in_quote = Some(c);
                    j += 1;
                    continue;
                }
                if c == b'{' {
                    in_brace += 1;
                    j += 1;
                    continue;
                }
                if c == b'}' {
                    if in_brace > 0 {
                        in_brace -= 1;
                    }
                    j += 1;
                    continue;
                }
                if in_brace > 0 {
                    j += 1;
                    continue;
                }
                if c == b'/' && bytes.get(j + 1) == Some(&b'>') {
                    self_closing = true;
                    j += 2;
                    break;
                }
                if c == b'>' {
                    j += 1;
                    break;
                }
                j += 1;
            }
            if j > bytes.len() {
                return None;
            }
            // After the tag header we're past `<…>` (or `<…/>`).
            i = j;
            if closing {
                if depth == 0 {
                    return None;
                }
                depth -= 1;
                if depth == 0 {
                    return Some(i - start_off);
                }
            } else if !self_closing {
                depth += 1;
            } else if depth == 0 {
                // Top-level self-closing element completes here.
                return Some(i - start_off);
            }
            continue;
        }
        i += 1;
    }
    None
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
    fn looks_canonical_keyword() {
        assert!(looks_canonical("component Foo() = <text/>"));
        assert!(looks_canonical("namespace Foo"));
        assert!(looks_canonical("import \"./theme.prss\""));
        assert!(looks_canonical("  let x = 1"));
        assert!(looks_canonical("-- comment\ncomponent Foo"));
    }

    #[test]
    fn looks_canonical_xml() {
        assert!(!looks_canonical("<container/>"));
        assert!(!looks_canonical("  <container/>"));
        assert!(!looks_canonical(""));
    }

    #[test]
    fn parses_namespace() {
        let doc = parse_ok("namespace TasksApp");
        assert_eq!(doc.nodes.len(), 1);
        let Node::Element(el) = &doc.nodes[0] else {
            panic!("expected element");
        };
        assert_eq!(el.tag, "namespace");
        assert_eq!(el.attributes.len(), 1);
        assert_eq!(el.attributes[0].name.raw, "name");
        match &el.attributes[0].value {
            AttributeValue::String { value, .. } => assert_eq!(value, "TasksApp"),
            _ => panic!("expected string"),
        }
    }

    #[test]
    fn parses_import_stylesheet() {
        let doc = parse_ok(r#"import "./theme.prss""#);
        let Node::Element(el) = &doc.nodes[0] else {
            panic!()
        };
        assert_eq!(el.tag, "import");
        assert_eq!(el.attributes[0].name.raw, "stylesheet");
    }

    #[test]
    fn parses_import_with_alias() {
        let doc = parse_ok(r#"import "./db.luau" as db"#);
        let Node::Element(el) = &doc.nodes[0] else {
            panic!()
        };
        assert_eq!(el.tag, "import");
        assert_eq!(el.attributes[0].name.raw, "script");
        assert_eq!(el.attributes[1].name.raw, "as");
        match &el.attributes[1].value {
            AttributeValue::String { value, .. } => assert_eq!(value, "db"),
            _ => panic!(),
        }
    }

    #[test]
    fn parses_import_component() {
        let doc = parse_ok(r#"import "./card.prui""#);
        let Node::Element(el) = &doc.nodes[0] else {
            panic!()
        };
        assert_eq!(el.tag, "import");
        assert_eq!(el.attributes[0].name.raw, "component");
    }

    #[test]
    fn parses_trivial_component_with_xml_body() {
        let doc = parse_ok(r#"component Avatar(src: string) = <image src={src}/>"#);
        let Node::Element(el) = &doc.nodes[0] else {
            panic!()
        };
        assert_eq!(el.tag, "component");
        // One property + one render-tree child.
        let prop = el
            .children
            .iter()
            .find(|n| matches!(n, Node::Element(e) if e.tag == "property"))
            .expect("missing property child");
        let Node::Element(prop_el) = prop else {
            panic!()
        };
        let name = prop_el
            .attributes
            .iter()
            .find(|a| a.name.raw == "name")
            .unwrap();
        let ty = prop_el
            .attributes
            .iter()
            .find(|a| a.name.raw == "type")
            .unwrap();
        match (&name.value, &ty.value) {
            (AttributeValue::String { value: n, .. }, AttributeValue::String { value: t, .. }) => {
                assert_eq!(n, "src");
                assert_eq!(t, "string");
            }
            _ => panic!(),
        }
        // Render tree child.
        let render = el
            .children
            .iter()
            .find(|n| matches!(n, Node::Element(e) if e.tag == "image"))
            .expect("missing image render child");
        let Node::Element(render_el) = render else {
            panic!()
        };
        assert_eq!(render_el.tag, "image");
    }

    #[test]
    fn parses_required_param() {
        let doc = parse_ok(r#"component Card(title: string required) = <text>{title}</text>"#);
        let Node::Element(el) = &doc.nodes[0] else {
            panic!()
        };
        let prop = el
            .children
            .iter()
            .find_map(|n| match n {
                Node::Element(e) if e.tag == "property" => Some(e),
                _ => None,
            })
            .unwrap();
        let req = prop.attributes.iter().find(|a| a.name.raw == "required");
        assert!(req.is_some(), "expected `required` flag");
    }

    #[test]
    fn parses_param_with_default() {
        let doc = parse_ok(r#"component Avatar(size: int = 32) = <image size={size}/>"#);
        let Node::Element(el) = &doc.nodes[0] else {
            panic!()
        };
        let prop = el
            .children
            .iter()
            .find_map(|n| match n {
                Node::Element(e) if e.tag == "property" => Some(e),
                _ => None,
            })
            .unwrap();
        let dflt = prop.attributes.iter().find(|a| a.name.raw == "default");
        assert!(dflt.is_some(), "expected default attr");
        match &dflt.unwrap().value {
            AttributeValue::String { value, .. } => assert_eq!(value, "32"),
            _ => panic!(),
        }
    }

    #[test]
    fn parses_trait_decl() {
        let doc = parse_ok(
            r#"trait Focusable {
  focus: action
  blur: action
}"#,
        );
        let Node::Element(el) = &doc.nodes[0] else {
            panic!()
        };
        assert_eq!(el.tag, "trait");
        let name = el.attributes.iter().find(|a| a.name.raw == "name").unwrap();
        match &name.value {
            AttributeValue::String { value, .. } => assert_eq!(value, "Focusable"),
            _ => panic!(),
        }
    }

    #[test]
    fn parses_type_decl_union() {
        let doc = parse_ok("type Priority = low | medium | high | critical");
        let Node::Element(el) = &doc.nodes[0] else {
            panic!()
        };
        assert_eq!(el.tag, "type");
        let body = el.attributes.iter().find(|a| a.name.raw == "body").unwrap();
        match &body.value {
            AttributeValue::String { value, .. } => {
                assert!(value.contains("low"));
                assert!(value.contains("critical"));
            }
            _ => panic!(),
        }
    }

    #[test]
    fn parses_let_decl() {
        let doc = parse_ok("let MAX = 100");
        let Node::Element(el) = &doc.nodes[0] else {
            panic!()
        };
        assert_eq!(el.tag, "let");
        let name = el.attributes.iter().find(|a| a.name.raw == "name").unwrap();
        let value = el
            .attributes
            .iter()
            .find(|a| a.name.raw == "value")
            .unwrap();
        match (&name.value, &value.value) {
            (AttributeValue::String { value: n, .. }, AttributeValue::String { value: v, .. }) => {
                assert_eq!(n, "MAX");
                assert_eq!(v, "100");
            }
            _ => panic!(),
        }
    }

    #[test]
    fn parses_class_as_style_block() {
        let doc = parse_ok("class card { padding = 8 }");
        let Node::Element(el) = &doc.nodes[0] else {
            panic!()
        };
        assert_eq!(el.tag, "style");
        let cls = el
            .attributes
            .iter()
            .find(|a| a.name.raw == "class")
            .unwrap();
        match &cls.value {
            AttributeValue::String { value, .. } => assert_eq!(value, "card"),
            _ => panic!(),
        }
        // Body text round-trips.
        let txt = el.children.iter().find_map(|n| match n {
            Node::Text { value, .. } => Some(value),
            _ => None,
        });
        assert!(txt.unwrap().contains("padding"));
    }

    #[test]
    fn parses_mixin_with_body_statements() {
        let doc = parse_ok(
            r#"mixin Hoverable {
  let hovered = state(false)
  on pointerenter { hovered <- true }
  on pointerleave { hovered <- false }
}"#,
        );
        let Node::Element(el) = &doc.nodes[0] else {
            panic!()
        };
        assert_eq!(el.tag, "mixin");
        let body: Vec<&str> = el
            .children
            .iter()
            .filter_map(|n| match n {
                Node::Element(e) => Some(e.tag.as_str()),
                _ => None,
            })
            .collect();
        assert!(body.contains(&"let"));
        // Two `on` handlers + one let.
        assert_eq!(body.iter().filter(|t| **t == "on").count(), 2);
        assert_eq!(body.iter().filter(|t| **t == "let").count(), 1);
    }

    #[test]
    fn parses_macro_body() {
        let doc = parse_ok(
            r#"macro Field(label: string, value: string) {
  match { <Field label={label} value={value}/> }
  expand {
    <container direction=row gap=8>
      <text>{label}</text>
      <text>{value}</text>
    </container>
  }
}"#,
        );
        let Node::Element(el) = &doc.nodes[0] else {
            panic!()
        };
        assert_eq!(el.tag, "macro");
        let body: Vec<&str> = el
            .children
            .iter()
            .filter_map(|n| match n {
                Node::Element(e) => Some(e.tag.as_str()),
                _ => None,
            })
            .collect();
        assert!(body.contains(&"match"));
        assert!(body.contains(&"expand"));
    }

    #[test]
    fn parses_component_with_trait_conformance() {
        let doc =
            parse_ok(r#"component TaskRow(task: Task) : Focusable, Pointable = <container/>"#);
        let Node::Element(el) = &doc.nodes[0] else {
            panic!()
        };
        let impls = el
            .attributes
            .iter()
            .find(|a| a.name.raw == "impls")
            .unwrap();
        match &impls.value {
            AttributeValue::String { value, .. } => {
                assert!(value.contains("Focusable"));
                assert!(value.contains("Pointable"));
            }
            _ => panic!(),
        }
    }

    #[test]
    fn parses_component_with_requires_in_body() {
        let doc = parse_ok(
            r#"component ShareButton(text: string required) {
  requires clipboard: Clipboard
  <button @click=$click>Copy</button>
}"#,
        );
        let Node::Element(el) = &doc.nodes[0] else {
            panic!()
        };
        let reqs = el
            .children
            .iter()
            .find_map(|n| match n {
                Node::Element(e) if e.tag == "requires" => Some(e),
                _ => None,
            })
            .expect("missing requires child");
        let names = reqs
            .attributes
            .iter()
            .find(|a| a.name.raw == "names")
            .unwrap();
        match &names.value {
            AttributeValue::String { value, .. } => {
                assert!(value.contains("clipboard"));
                assert!(value.contains("Clipboard"));
            }
            _ => panic!(),
        }
    }

    #[test]
    fn rejects_multi_segment_namespace() {
        let (_doc, errs) = parse("namespace Forms.Fields");
        assert!(errs.iter().any(|e| e.code == "namespace-multi-segment"));
    }

    #[test]
    fn parses_fn_with_arrow_return_type() {
        let doc = parse_ok("fn double(x: int) -> int = x * 2");
        let Node::Element(el) = &doc.nodes[0] else {
            panic!()
        };
        assert_eq!(el.tag, "fn");
        let ret = el
            .attributes
            .iter()
            .find(|a| a.name.raw == "return")
            .unwrap();
        match &ret.value {
            AttributeValue::String { value, .. } => assert_eq!(value, "int"),
            _ => panic!(),
        }
        // Body — an `expr` child carries the RHS.
        let expr = el
            .children
            .iter()
            .find_map(|n| match n {
                Node::Element(e) if e.tag == "expr" => Some(e),
                _ => None,
            })
            .unwrap();
        let body = expr
            .attributes
            .iter()
            .find(|a| a.name.raw == "body")
            .unwrap();
        match &body.value {
            AttributeValue::String { value, .. } => assert_eq!(value, "x * 2"),
            _ => panic!(),
        }
    }

    #[test]
    fn parses_fn_with_unicode_arrow() {
        let doc = parse_ok("fn double(x: int) → int = x * 2");
        let Node::Element(el) = &doc.nodes[0] else {
            panic!()
        };
        assert_eq!(el.tag, "fn");
    }

    #[test]
    fn parses_multiple_top_level_decls() {
        let doc = parse_ok(
            r#"namespace TasksApp

import "./theme.prss"

type Priority = low | high

component Chip(text: string) = <text>{text}</text>
"#,
        );
        let tags: Vec<&str> = doc
            .nodes
            .iter()
            .filter_map(|n| match n {
                Node::Element(e) => Some(e.tag.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(tags, vec!["namespace", "import", "type", "component"]);
    }

    #[test]
    fn allows_top_level_xml_after_canonical_decl() {
        // §6.11 — canonical declarations + XML invocations mix freely
        // at file scope. Phase 7 leans on this: declare `<component
        // Greeting>` then invoke `<Greeting/>` in the same file.
        let doc = parse_ok(
            r#"component Greeting(name: string) = <text>Hello, {name}</text>

<Greeting name="World"/>"#,
        );
        let tags: Vec<&str> = doc
            .nodes
            .iter()
            .filter_map(|n| match n {
                Node::Element(e) => Some(e.tag.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(tags, vec!["component", "Greeting"]);
    }

    #[test]
    fn skips_line_comments() {
        let doc = parse_ok(
            r#"-- a leading comment
component Card(t: string) = <text>{t}</text>
-- trailing
"#,
        );
        let tags: Vec<&str> = doc
            .nodes
            .iter()
            .filter_map(|n| match n {
                Node::Element(e) => Some(e.tag.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(tags, vec!["component"]);
    }
}
