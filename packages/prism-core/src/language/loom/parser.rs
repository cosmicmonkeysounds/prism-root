//! Loom parser — recursive descent from a [`lexer::Token`] stream to
//! a [`RootNode`] / [`SyntaxNode`] tree. Reference:
//! `docs/dev/loom-grammar.md`.
//!
//! The parser is error-tolerant: when it can't match a production it
//! emits an `ERROR` [`SyntaxNode`] (kind = [`node_kinds::ERROR`])
//! carrying a diagnostic id + message, then resyncs to the next
//! `Newline` / `Dedent` boundary. The whole parse always returns a
//! root — the LSP consumer expects partial trees for hover /
//! completion to work in unfinished files.
//!
//! Implementation note: the parser doesn't try to enforce the §14
//! "reserved space" rules (e.g. `=` in mutation context, mutation of
//! a role, knowledge field arithmetic) — those are validator
//! concerns. The parser's job is to **shape** the tree; the
//! validator turns shape into diagnostics.

use indexmap::IndexMap;
use serde_json::Value as JsonValue;

use crate::language::syntax::{Position, RootNode, SourceRange, SyntaxNode};

use super::lexer::{lex, Token, TokenKind};
use super::node_kinds as nk;

/// Severity of a parser-produced diagnostic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
    Info,
}

impl Severity {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Warning => "warning",
            Self::Info => "info",
        }
    }
}

/// One diagnostic — id matches `docs/dev/loom-grammar.md` §21.
#[derive(Debug, Clone)]
pub struct LoomDiagnostic {
    pub id: &'static str,
    pub severity: Severity,
    pub message: String,
    pub range: SourceRange,
}

/// Result of parsing one source string.
#[derive(Debug, Clone)]
pub struct ParseResult {
    pub root: RootNode,
    pub diagnostics: Vec<LoomDiagnostic>,
}

/// Parse a `.loom` source into an AST + diagnostics. Never panics —
/// even an empty source yields a valid (empty-body) root.
pub fn parse(source: &str) -> ParseResult {
    let tokens = match lex(source) {
        Ok(t) => t,
        Err(err) => {
            return ParseResult {
                root: RootNode::default(),
                diagnostics: vec![LoomDiagnostic {
                    id: "lex-error",
                    severity: Severity::Error,
                    message: err.message,
                    range: SourceRange {
                        start: err.position,
                        end: err.position,
                    },
                }],
            };
        }
    };
    let mut p = Parser::new(&tokens);
    let root = p.parse_source_file();
    ParseResult {
        root,
        diagnostics: p.diagnostics,
    }
}

// ═══════════════════════════════════════════════════════════════════
// Parser core
// ═══════════════════════════════════════════════════════════════════

struct Parser<'t> {
    tokens: &'t [Token],
    pos: usize,
    diagnostics: Vec<LoomDiagnostic>,
}

impl<'t> Parser<'t> {
    fn new(tokens: &'t [Token]) -> Self {
        Self {
            tokens,
            pos: 0,
            diagnostics: Vec::new(),
        }
    }

    // ── Cursor helpers ──────────────────────────────────────────────

    fn peek(&self) -> &Token {
        &self.tokens[self.pos]
    }

    fn peek_kind(&self) -> TokenKind {
        self.tokens[self.pos].kind
    }

    fn peek_n(&self, ahead: usize) -> &Token {
        let i = (self.pos + ahead).min(self.tokens.len() - 1);
        &self.tokens[i]
    }

    fn at(&self, kind: TokenKind) -> bool {
        self.peek_kind() == kind
    }

    fn at_word(&self, word: &str) -> bool {
        self.peek().is_word(word)
    }

    fn at_eof(&self) -> bool {
        matches!(self.peek_kind(), TokenKind::Eof)
    }

    fn advance(&mut self) -> &'t Token {
        let t = &self.tokens[self.pos];
        if self.pos + 1 < self.tokens.len() {
            self.pos += 1;
        }
        t
    }

    fn consume(&mut self, kind: TokenKind) -> bool {
        if self.at(kind) {
            self.advance();
            true
        } else {
            false
        }
    }

    fn consume_word(&mut self, word: &str) -> bool {
        if self.at_word(word) {
            self.advance();
            true
        } else {
            false
        }
    }

    fn expect(&mut self, kind: TokenKind, id: &'static str, msg: &str) -> bool {
        if self.consume(kind) {
            true
        } else {
            self.diag(id, Severity::Error, msg);
            false
        }
    }

    fn skip_newlines(&mut self) {
        while self.at(TokenKind::Newline) {
            self.advance();
        }
    }

    /// Skip tokens until we reach a Newline or Dedent boundary. Used
    /// for error recovery — the parser drops back to the next line
    /// boundary and resumes parsing top-down items.
    fn resync_to_line_boundary(&mut self) {
        while !self.at_eof() && !self.at(TokenKind::Newline) && !self.at(TokenKind::Dedent) {
            self.advance();
        }
        if self.at(TokenKind::Newline) {
            self.advance();
        }
    }

    fn diag(&mut self, id: &'static str, severity: Severity, message: impl Into<String>) {
        let range = self.peek().range;
        self.diagnostics.push(LoomDiagnostic {
            id,
            severity,
            message: message.into(),
            range,
        });
    }

    fn diag_at(
        &mut self,
        range: SourceRange,
        id: &'static str,
        severity: Severity,
        message: impl Into<String>,
    ) {
        self.diagnostics.push(LoomDiagnostic {
            id,
            severity,
            message: message.into(),
            range,
        });
    }

    /// Produce an ERROR node anchored at the current position and
    /// record a matching diagnostic. Used for graceful recovery.
    fn error_node(&mut self, id: &'static str, msg: impl Into<String>) -> SyntaxNode {
        let start = self.peek().range.start;
        let m = msg.into();
        self.diag(id, Severity::Error, m.clone());
        let mut data: IndexMap<String, JsonValue> = IndexMap::new();
        data.insert(
            nk::DATA_KEY_DIAGNOSTIC_ID.to_string(),
            JsonValue::String(id.to_string()),
        );
        data.insert(nk::DATA_KEY_HAS_ERROR.to_string(), JsonValue::Bool(true));
        SyntaxNode {
            kind: nk::ERROR.to_string(),
            position: Some(SourceRange { start, end: start }),
            value: Some(m),
            children: Vec::new(),
            data,
        }
    }
}

// ── Construction helpers ────────────────────────────────────────────

fn make_node(kind: &str, range: SourceRange, children: Vec<SyntaxNode>) -> SyntaxNode {
    SyntaxNode {
        kind: kind.to_string(),
        position: Some(range),
        children,
        value: None,
        data: IndexMap::new(),
    }
}

fn leaf(kind: &str, range: SourceRange, value: impl Into<String>) -> SyntaxNode {
    SyntaxNode {
        kind: kind.to_string(),
        position: Some(range),
        children: Vec::new(),
        value: Some(value.into()),
        data: IndexMap::new(),
    }
}

fn range_to(start: Position, end: Position) -> SourceRange {
    SourceRange { start, end }
}

// ═══════════════════════════════════════════════════════════════════
// §1, §4. Source file / document / header / properties / docstring
// ═══════════════════════════════════════════════════════════════════

impl Parser<'_> {
    fn parse_source_file(&mut self) -> RootNode {
        let start = self.peek().range.start;
        let mut documents = Vec::new();
        self.skip_newlines();
        while !self.at_eof() {
            if let Some(doc) = self.parse_document() {
                documents.push(doc);
            } else {
                // Parse made no progress — bail to avoid an infinite
                // loop. This shouldn't happen with the recovery logic
                // below but is defensive.
                self.advance();
            }
            self.skip_newlines();
        }
        let end = self.peek().range.end;
        RootNode {
            kind: Default::default(),
            position: Some(range_to(start, end)),
            children: documents,
        }
    }

    fn parse_document(&mut self) -> Option<SyntaxNode> {
        if !self.at(TokenKind::Hash) {
            // Recover by consuming tokens until we find a `#` at line
            // start. If we hit EOF first, return None.
            let start = self.peek().range.start;
            self.diag_at(
                range_to(start, start),
                "doc-header-missing",
                Severity::Error,
                "Expected `#` to start a Loom document",
            );
            while !self.at_eof() && !self.at(TokenKind::Hash) {
                self.advance();
            }
            if self.at_eof() {
                return None;
            }
        }

        let doc_start = self.peek().range.start;
        let header = self.parse_header();
        let mut children = vec![header];

        // Optional property block — only valid immediately after the
        // header (with no blank line, per §4). We allow blank lines
        // because the lexer collapses them.
        if self.at(TokenKind::Indent) {
            // Look one ahead: if it's `.something`, that's a property
            // block. Otherwise we're already in the body.
            let save = self.pos;
            self.advance(); // Indent
            if self.at(TokenKind::Dot) {
                self.pos = save;
                children.push(self.parse_property_block());
            } else {
                self.pos = save;
            }
        }

        // Optional docstring.
        if self.at(TokenKind::Docstring) {
            let tok = self.advance().clone();
            children.push(leaf(nk::DOCSTRING, tok.range, tok.text));
            self.skip_newlines();
        }

        // Body — top-level items until the next `# ...` document
        // header or EOF.
        while !self.at_eof() && !self.at(TokenKind::Hash) {
            if let Some(item) = self.parse_top_level_item() {
                children.push(item);
            }
            self.skip_newlines();
        }

        let doc_end = self.peek().range.start;
        Some(make_node(
            nk::DOCUMENT,
            range_to(doc_start, doc_end),
            children,
        ))
    }

    fn parse_header(&mut self) -> SyntaxNode {
        let start = self.peek().range.start;
        self.expect(TokenKind::Hash, "doc-header-id", "Expected `#`");

        let mut children = Vec::new();
        if matches!(self.peek_kind(), TokenKind::Ident | TokenKind::Speaker) {
            let tok = self.advance().clone();
            children.push(leaf(nk::IDENT, tok.range, tok.text));
        } else {
            children.push(self.error_node("doc-header-id", "Expected document identifier"));
        }

        if self.at(TokenKind::String) {
            let tok = self.advance().clone();
            children.push(leaf(nk::STRING, tok.range, tok.text));
        }

        // Doc tags: `: ident`, optional and repeating.
        while self.at(TokenKind::Colon) {
            let tag_start = self.peek().range.start;
            self.advance();
            if matches!(self.peek_kind(), TokenKind::Ident | TokenKind::Speaker) {
                let tok = self.advance().clone();
                let tag = make_node(
                    nk::DOC_TAG,
                    range_to(tag_start, tok.range.end),
                    vec![leaf(nk::IDENT, tok.range, tok.text)],
                );
                children.push(tag);
            } else {
                children.push(self.error_node("doc-type-unknown", "Expected tag identifier"));
            }
        }

        self.expect(
            TokenKind::Newline,
            "doc-header-id",
            "Expected newline after header",
        );
        let end = self.peek().range.start;
        make_node(nk::HEADER, range_to(start, end), children)
    }

    fn parse_property_block(&mut self) -> SyntaxNode {
        let start = self.peek().range.start;
        self.expect(
            TokenKind::Indent,
            "indent-jump",
            "Expected indented property block",
        );
        let mut props = Vec::new();
        while self.at(TokenKind::Dot) {
            props.push(self.parse_property());
            self.skip_newlines();
        }
        self.expect(
            TokenKind::Dedent,
            "indent-jump",
            "Expected dedent after property block",
        );
        let end = self.peek().range.start;
        // Properties become children of the property block grouping.
        make_node("property_block", range_to(start, end), props)
    }

    fn parse_property(&mut self) -> SyntaxNode {
        let start = self.peek().range.start;
        self.advance(); // `.`
        let mut children = Vec::new();
        if matches!(self.peek_kind(), TokenKind::Ident | TokenKind::Speaker) {
            let tok = self.advance().clone();
            children.push(leaf(nk::IDENT, tok.range, tok.text));
        } else {
            children.push(self.error_node("doc-type-unknown", "Expected property name"));
        }
        // Free-form value: any tokens up to the line break.
        let value_start = self.peek().range.start;
        let mut raw = String::new();
        while !self.at(TokenKind::Newline) && !self.at_eof() {
            let t = self.advance();
            if !raw.is_empty() {
                raw.push(' ');
            }
            raw.push_str(&t.text);
        }
        if !raw.is_empty() {
            let value_end = self.peek().range.start;
            children.push(leaf(
                nk::PROPERTY_VALUE,
                range_to(value_start, value_end),
                raw,
            ));
        }
        if self.at(TokenKind::Newline) {
            self.advance();
        }
        let end = self.peek().range.start;
        make_node(nk::PROPERTY, range_to(start, end), children)
    }
}

// ═══════════════════════════════════════════════════════════════════
// §4.2. Top-level item dispatch
// ═══════════════════════════════════════════════════════════════════

impl Parser<'_> {
    fn parse_top_level_item(&mut self) -> Option<SyntaxNode> {
        // Skip stray newlines / indents (defensive).
        while matches!(self.peek_kind(), TokenKind::Newline) {
            self.advance();
        }
        if self.at_eof() || self.at(TokenKind::Hash) {
            return None;
        }

        match self.peek_kind() {
            TokenKind::DashDash => return Some(self.parse_section()),
            TokenKind::HashHash => return Some(self.parse_slugline_scene()),
            TokenKind::LParen => return Some(self.parse_sexp_form_or_decl()),
            TokenKind::Tilde => return Some(self.parse_action_line()),
            TokenKind::Indent | TokenKind::Dedent => {
                // Layout out-of-place — recover by consuming and trying again.
                self.advance();
                return self.parse_top_level_item();
            }
            TokenKind::LineComment | TokenKind::BlockComment => {
                self.advance();
                return self.parse_top_level_item();
            }
            _ => {}
        }

        // Keyword-led top-level forms (§5).
        if self.at_word("cast") {
            return Some(self.parse_cast_decl());
        }
        if self.at_word("cue") {
            return Some(self.parse_cue_decl());
        }
        if self.at_word("location") {
            return Some(self.parse_location_decl());
        }
        if self.at_word("cohort") {
            return Some(self.parse_cohort_decl());
        }
        if self.at_word("knowledge") {
            return Some(self.parse_knowledge_block());
        }
        if self.at_word("goal") {
            return Some(self.parse_goal_decl());
        }
        if self.at_word("disposition") {
            return Some(self.parse_disposition_block());
        }
        if self.at_word("on") {
            return Some(self.parse_hook_decl());
        }
        if self.at_word("attribute") {
            return Some(self.parse_attribute_decl());
        }
        if self.at_word("axis") {
            return Some(self.parse_axis_decl());
        }
        if self.at_word("pool") {
            return Some(self.parse_pool_decl());
        }
        if self.at_word("stat") {
            return Some(self.parse_stat_decl());
        }
        if self.at_word("node") {
            return Some(self.parse_tree_node_decl());
        }
        if self.at_word("modify") {
            return Some(self.parse_action_line());
        }
        if self.at_word("generator") {
            return Some(self.parse_generator_decl());
        }
        if self.at_word("scene") {
            return Some(self.parse_scene_decl());
        }
        if self.at_word("compose") {
            return Some(self.parse_compose_decl());
        }
        if self.at_word("faction") {
            return Some(self.parse_inline_faction_decl());
        }
        if self.at_word("members") {
            return Some(self.parse_members_block());
        }
        if self.at_word("state") {
            return Some(self.parse_state_block());
        }
        if self.at_word("stance") {
            return Some(self.parse_stance_block());
        }
        if self.at_word("reveal") {
            return Some(self.parse_action_line());
        }
        if self.at_word("broadcast") {
            return Some(self.parse_broadcast_block());
        }
        if self.at_word("when") {
            return Some(self.parse_when_top_level());
        }
        if self.at_word("at") {
            // `at TIMECODE` opens a cutscene timecode block. `at TIMESPEC`
            // inside a generator/scene is a `time_stmt` — disambiguated
            // by context (generator/scene bodies handle it themselves).
            return Some(self.parse_timecode_block());
        }
        if self.at_word("let") {
            return Some(self.parse_let_binding());
        }
        if self.at_word("var") {
            return Some(self.parse_action_line()); // var $x := ... is an action
        }

        // Otherwise, treat as content (dialogue / flavor / divert / etc.)
        // attached to the implicit "preamble" of the document.
        self.parse_content_or_recover()
    }

    fn parse_content_or_recover(&mut self) -> Option<SyntaxNode> {
        if self.at_eof() {
            return None;
        }
        let before = self.pos;
        let node = self.parse_content_line();
        if self.pos == before {
            // No progress — emit an error and skip the offending line.
            let err = self.error_node(
                "unexpected-child",
                format!("Unexpected token `{}` at this position", self.peek().text),
            );
            self.resync_to_line_boundary();
            return Some(err);
        }
        Some(node)
    }
}

// ═══════════════════════════════════════════════════════════════════
// §5. Performance-model declarations
// ═══════════════════════════════════════════════════════════════════

impl Parser<'_> {
    fn parse_cast_decl(&mut self) -> SyntaxNode {
        let start = self.peek().range.start;
        self.advance(); // `cast`
        let mut children = Vec::new();
        if let Some(id) = self.parse_cast_or_location_id() {
            children.push(id);
        }
        if self.at(TokenKind::String) {
            let tok = self.advance().clone();
            children.push(leaf(nk::STRING, tok.range, tok.text));
        }
        self.expect(
            TokenKind::Newline,
            "doc-header-id",
            "Expected newline after cast id",
        );
        if self.at(TokenKind::Indent) {
            children.push(self.parse_property_block());
        }
        let end = self.peek().range.start;
        make_node(nk::CAST_DECL, range_to(start, end), children)
    }

    fn parse_cue_decl(&mut self) -> SyntaxNode {
        let start = self.peek().range.start;
        self.advance(); // `cue`
        let mut children = Vec::new();
        if matches!(self.peek_kind(), TokenKind::Ident | TokenKind::Speaker) {
            let t = self.advance().clone();
            children.push(leaf(nk::IDENT, t.range, t.text));
        } else {
            children.push(self.error_node("unknown-cue", "Expected cue identifier"));
        }
        self.expect(
            TokenKind::Newline,
            "doc-header-id",
            "Expected newline after cue id",
        );
        if self.at(TokenKind::Indent) {
            children.push(self.parse_property_block());
        }
        let end = self.peek().range.start;
        make_node(nk::CUE_DECL, range_to(start, end), children)
    }

    fn parse_location_decl(&mut self) -> SyntaxNode {
        let start = self.peek().range.start;
        self.advance(); // `location`
        let mut children = Vec::new();
        if let Some(id) = self.parse_cast_or_location_id() {
            children.push(id);
        }
        if self.at(TokenKind::String) {
            let t = self.advance().clone();
            children.push(leaf(nk::STRING, t.range, t.text));
        }
        self.expect(
            TokenKind::Newline,
            "doc-header-id",
            "Expected newline after location id",
        );
        if self.at(TokenKind::Indent) {
            children.push(self.parse_property_block());
        }
        let end = self.peek().range.start;
        make_node(nk::LOCATION_DECL, range_to(start, end), children)
    }

    fn parse_cohort_decl(&mut self) -> SyntaxNode {
        let start = self.peek().range.start;
        self.advance(); // `cohort`
        let mut children = Vec::new();
        if matches!(self.peek_kind(), TokenKind::Ident | TokenKind::Speaker) {
            let t = self.advance().clone();
            children.push(leaf(nk::IDENT, t.range, t.text));
        } else {
            children.push(self.error_node("unknown-cohort", "Expected cohort identifier"));
        }
        self.expect(
            TokenKind::Newline,
            "doc-header-id",
            "Expected newline after cohort id",
        );
        if self.at(TokenKind::Indent) {
            children.push(self.parse_property_block());
        }
        let end = self.peek().range.start;
        make_node(nk::COHORT_DECL, range_to(start, end), children)
    }

    /// Speaker token, `@ident` static ref, or error fallback.
    fn parse_cast_or_location_id(&mut self) -> Option<SyntaxNode> {
        match self.peek_kind() {
            TokenKind::Speaker => {
                let t = self.advance().clone();
                Some(leaf(nk::SPEAKER, t.range, t.text))
            }
            TokenKind::At => Some(self.parse_static_ref()),
            _ => Some(self.error_node("unknown-cast", "Expected SPEAKER or @ref")),
        }
    }

    fn parse_broadcast_block(&mut self) -> SyntaxNode {
        let start = self.peek().range.start;
        self.advance(); // `broadcast`
        let scope = self.parse_broadcast_scope();
        self.expect(
            TokenKind::Newline,
            "doc-header-id",
            "Expected newline after broadcast scope",
        );
        let mut children = vec![scope];
        if self.at(TokenKind::Indent) {
            children.push(self.parse_indented_content_block());
        }
        let end = self.peek().range.start;
        make_node(nk::BROADCAST_BLOCK, range_to(start, end), children)
    }

    fn parse_broadcast_scope(&mut self) -> SyntaxNode {
        let start = self.peek().range.start;
        let mut atoms = Vec::new();
        atoms.push(self.parse_scope_atom());
        while self.at_word("and") || self.at_word("but") {
            let connector_tok = self.advance().clone();
            atoms.push(leaf(nk::IDENT, connector_tok.range, connector_tok.text));
            atoms.push(self.parse_scope_atom());
        }
        let end = self.peek().range.start;
        make_node(nk::BROADCAST_SCOPE, range_to(start, end), atoms)
    }

    fn parse_scope_atom(&mut self) -> SyntaxNode {
        let start = self.peek().range.start;
        self.expect(
            TokenKind::Colon,
            "broadcast-empty-scope",
            "Expected `:` to introduce broadcast scope",
        );
        // `:all` shorthand
        if self.at_word("all") {
            let t = self.advance().clone();
            let end = t.range.end;
            return make_node(
                nk::SCOPE_ATOM,
                range_to(start, end),
                vec![leaf(nk::IDENT, t.range, t.text)],
            );
        }
        // cohort/location/participant/cast/faction(arg)
        let mut children = Vec::new();
        if matches!(self.peek_kind(), TokenKind::Ident | TokenKind::Speaker) {
            let t = self.advance().clone();
            children.push(leaf(nk::IDENT, t.range, t.text));
            if self.at(TokenKind::LParen) {
                self.advance();
                let arg = self.parse_expression();
                children.push(arg);
                self.expect(
                    TokenKind::RParen,
                    "bracket-unbalanced",
                    "Expected `)` after scope argument",
                );
            }
        }
        let end = self.peek().range.start;
        make_node(nk::SCOPE_ATOM, range_to(start, end), children)
    }

    fn parse_when_top_level(&mut self) -> SyntaxNode {
        // `when` is shared between several constructs at top level:
        // - `when participant joins/leaves [:cohort]`
        // - `when participant enters/exits @LOC`
        // - `when participant proposes/joins/leaves faction`
        // - `when faction emerges/dissolves/grows/shrinks [...]`
        // - `when stance changes [(a,b)]`
        // - `when stance/membership revealed to OBSERVER`
        // - `when <ident>` (event-driven block, §7.7)
        // - `when <expr>` (condition block, §7.7)
        //
        // We dispatch on what follows `when`.
        let start = self.peek().range.start;
        self.advance(); // `when`

        // participant lifecycle / location event / faction lifecycle
        if self.at_word("participant") {
            return self.parse_participant_when(start);
        }
        if self.at_word("faction") {
            return self.parse_faction_lifecycle_when(start);
        }
        if self.at_word("stance") {
            return self.parse_stance_change_or_reveal_when(start);
        }
        if self.at_word("membership") {
            return self.parse_membership_reveal_when(start);
        }

        // §7.7 plain `when EVENT|EXPR` block.
        let mut children = Vec::new();
        let discriminant = if matches!(self.peek_kind(), TokenKind::Ident | TokenKind::Speaker)
            && self.peek_n(1).kind == TokenKind::Newline
        {
            let t = self.advance().clone();
            leaf(nk::IDENT, t.range, t.text)
        } else {
            self.parse_expression()
        };
        children.push(discriminant);
        self.expect(
            TokenKind::Newline,
            "doc-header-id",
            "Expected newline after when discriminant",
        );
        if self.at(TokenKind::Indent) {
            children.push(self.parse_indented_content_block());
        }
        let end = self.peek().range.start;
        make_node(nk::WHEN_BLOCK, range_to(start, end), children)
    }

    fn parse_participant_when(&mut self, start: Position) -> SyntaxNode {
        self.advance(); // `participant`
                        // The next word picks the variant.
        let verb_tok = self.advance().clone();
        let verb = verb_tok.text.clone();
        let mut children = vec![leaf(nk::IDENT, verb_tok.range, verb.clone())];

        let kind = match verb.as_str() {
            "joins" | "leaves" => {
                // Optional cohort guard: `:ident`
                if self.consume(TokenKind::Colon)
                    && matches!(self.peek_kind(), TokenKind::Ident | TokenKind::Speaker)
                {
                    let t = self.advance().clone();
                    children.push(leaf(nk::IDENT, t.range, t.text));
                }
                // If the next thing is `faction`, this is a participant
                // faction lifecycle instead.
                if self.at_word("faction") {
                    self.advance();
                    nk::PARTICIPANT_FACTION_LIFECYCLE
                } else {
                    nk::PARTICIPANT_LIFECYCLE
                }
            }
            "enters" | "exits" => {
                // Expect @LOCATION.
                if self.at(TokenKind::At) {
                    children.push(self.parse_static_ref());
                }
                nk::LOCATION_EVENT
            }
            "proposes" => {
                if self.at_word("faction") {
                    self.advance();
                }
                nk::PARTICIPANT_FACTION_LIFECYCLE
            }
            _ => nk::PARTICIPANT_LIFECYCLE,
        };

        self.expect(
            TokenKind::Newline,
            "doc-header-id",
            "Expected newline after when-clause",
        );
        if self.at(TokenKind::Indent) {
            children.push(self.parse_indented_content_block());
        }
        let end = self.peek().range.start;
        make_node(kind, range_to(start, end), children)
    }

    fn parse_faction_lifecycle_when(&mut self, start: Position) -> SyntaxNode {
        self.advance(); // `faction`
        let mut children = Vec::new();
        // verb
        if matches!(self.peek_kind(), TokenKind::Ident | TokenKind::Speaker) {
            let t = self.advance().clone();
            children.push(leaf(nk::IDENT, t.range, t.text));
        }
        // Optional qualifier: `from template @X` or `in $F`
        if self.at_word("from") {
            self.advance();
            if self.at_word("template") {
                self.advance();
            }
            if self.at(TokenKind::At) {
                children.push(self.parse_static_ref());
            }
        } else if self.at_word("in") {
            self.advance();
            if self.at(TokenKind::Dollar) {
                children.push(self.parse_resolve_ref());
            }
        }
        self.expect(
            TokenKind::Newline,
            "doc-header-id",
            "Expected newline after faction event",
        );
        if self.at(TokenKind::Indent) {
            children.push(self.parse_indented_content_block());
        }
        let end = self.peek().range.start;
        make_node(nk::FACTION_EVENT, range_to(start, end), children)
    }

    fn parse_stance_change_or_reveal_when(&mut self, start: Position) -> SyntaxNode {
        self.advance(); // `stance`
        let mut children = Vec::new();
        if self.at_word("changes") {
            self.advance();
            // Optional (a, b) qualifier.
            if self.at(TokenKind::LParen) {
                self.advance();
                children.push(self.parse_expression());
                self.expect(
                    TokenKind::Comma,
                    "doc-header-id",
                    "Expected `,` in stance qualifier",
                );
                children.push(self.parse_expression());
                self.expect(TokenKind::RParen, "bracket-unbalanced", "Expected `)`");
            }
            self.expect(TokenKind::Newline, "doc-header-id", "Expected newline");
            if self.at(TokenKind::Indent) {
                children.push(self.parse_indented_content_block());
            }
            let end = self.peek().range.start;
            return make_node(nk::FACTION_EVENT, range_to(start, end), children);
        }

        // `when stance revealed to OBSERVER` / `when stance(a,b) revealed to OBSERVER`
        if self.at(TokenKind::LParen) {
            self.advance();
            children.push(self.parse_expression());
            self.expect(TokenKind::Comma, "doc-header-id", "Expected `,`");
            children.push(self.parse_expression());
            self.expect(TokenKind::RParen, "bracket-unbalanced", "Expected `)`");
        }
        if self.at_word("revealed") {
            self.advance();
        }
        if self.at_word("to") {
            self.advance();
            children.push(self.parse_observer());
        }
        self.expect(TokenKind::Newline, "doc-header-id", "Expected newline");
        if self.at(TokenKind::Indent) {
            children.push(self.parse_indented_content_block());
        }
        let end = self.peek().range.start;
        make_node(nk::DISCOVERY_EVENT, range_to(start, end), children)
    }

    fn parse_membership_reveal_when(&mut self, start: Position) -> SyntaxNode {
        self.advance(); // `membership`
        let mut children = Vec::new();
        // Optional `of X in @F`
        if self.at_word("of") {
            self.advance();
            children.push(self.parse_expression());
            if self.at_word("in") {
                self.advance();
                children.push(self.parse_expression());
            }
        }
        if self.at_word("revealed") {
            self.advance();
        }
        if self.at_word("to") {
            self.advance();
            children.push(self.parse_observer());
        }
        self.expect(TokenKind::Newline, "doc-header-id", "Expected newline");
        if self.at(TokenKind::Indent) {
            children.push(self.parse_indented_content_block());
        }
        let end = self.peek().range.start;
        make_node(nk::DISCOVERY_EVENT, range_to(start, end), children)
    }

    fn parse_observer(&mut self) -> SyntaxNode {
        let start = self.peek().range.start;
        let mut children = Vec::new();
        if self.at(TokenKind::Colon) && self.peek_n(1).is_word("all") {
            self.advance();
            let t = self.advance().clone();
            children.push(leaf(nk::IDENT, t.range, t.text));
        } else if self.at(TokenKind::LParen) {
            self.advance();
            children.push(self.parse_expression());
            self.expect(TokenKind::RParen, "bracket-unbalanced", "Expected `)`");
        } else {
            children.push(self.parse_primary_expression());
        }
        let end = self.peek().range.start;
        make_node(nk::OBSERVER, range_to(start, end), children)
    }
}

// ═══════════════════════════════════════════════════════════════════
// §6. Sections + slugline scenes + timecode blocks
// ═══════════════════════════════════════════════════════════════════

impl Parser<'_> {
    fn parse_section(&mut self) -> SyntaxNode {
        let start = self.peek().range.start;
        self.advance(); // `--`
        let mut children = Vec::new();

        // Optional id (anonymous sections allowed per §6.1).
        if matches!(self.peek_kind(), TokenKind::Ident | TokenKind::Speaker) {
            let t = self.advance().clone();
            children.push(leaf(nk::IDENT, t.range, t.text));
        }

        self.parse_modifiers_and_scope_and_guard(&mut children);
        self.expect(
            TokenKind::Newline,
            "doc-header-id",
            "Expected newline after section header",
        );

        if self.at(TokenKind::Docstring) {
            let t = self.advance().clone();
            children.push(leaf(nk::DOCSTRING, t.range, t.text));
            self.skip_newlines();
        }

        // Section body: consume content lines until we hit another
        // top-level item start (`# doc`, `-- section`, `## slugline`, or
        // a declaration keyword). Layout tokens — Indent/Dedent —
        // pass through silently, because content lines (dialogue,
        // choice, …) maintain their own indent semantics internally
        // and the boundaries between them don't always correspond
        // 1:1 with lexer indent transitions.
        loop {
            if self.at_eof() || self.at_section_boundary() {
                break;
            }
            match self.peek_kind() {
                TokenKind::Newline | TokenKind::Indent | TokenKind::Dedent => {
                    self.advance();
                    continue;
                }
                _ => {}
            }
            let before = self.pos;
            children.push(self.parse_content_line());
            if self.pos == before {
                self.advance();
            }
        }

        let end = self.peek().range.start;
        make_node(nk::SECTION, range_to(start, end), children)
    }

    /// True if the current token starts another top-level item that
    /// would end the current section's body.
    fn at_section_boundary(&self) -> bool {
        if matches!(
            self.peek_kind(),
            TokenKind::Hash | TokenKind::DashDash | TokenKind::HashHash
        ) {
            return true;
        }
        // Declaration keywords at top of a line.
        if matches!(self.peek_kind(), TokenKind::Ident | TokenKind::Speaker) {
            matches!(
                self.peek().text.as_str(),
                "cast"
                    | "cue"
                    | "location"
                    | "cohort"
                    | "broadcast"
                    | "generator"
                    | "scene"
                    | "compose"
                    | "faction"
                    | "knowledge"
                    | "goal"
                    | "disposition"
                    | "attribute"
                    | "axis"
                    | "pool"
                    | "stat"
                    | "node"
            )
        } else {
            false
        }
    }

    fn parse_slugline_scene(&mut self) -> SyntaxNode {
        let start = self.peek().range.start;
        self.advance(); // `##`
        let mut children = Vec::new();

        // Scene slug: ident or ident.ident
        if matches!(self.peek_kind(), TokenKind::Ident | TokenKind::Speaker) {
            let id_start = self.peek().range.start;
            let mut slug_children = Vec::new();
            let t1 = self.advance().clone();
            slug_children.push(leaf(nk::IDENT, t1.range, t1.text));
            if self.at(TokenKind::Dot) {
                self.advance();
                if matches!(self.peek_kind(), TokenKind::Ident | TokenKind::Speaker) {
                    let t2 = self.advance().clone();
                    slug_children.push(leaf(nk::IDENT, t2.range, t2.text));
                }
            }
            let id_end = self.peek().range.start;
            children.push(make_node(
                nk::SCENE_SLUG,
                range_to(id_start, id_end),
                slug_children,
            ));
        }

        if self.at(TokenKind::String) {
            let t = self.advance().clone();
            children.push(leaf(nk::STRING, t.range, t.text));
        }

        self.parse_modifiers_and_scope_and_guard(&mut children);
        self.expect(
            TokenKind::Newline,
            "doc-header-id",
            "Expected newline after slugline",
        );

        if self.at(TokenKind::Docstring) {
            let t = self.advance().clone();
            children.push(leaf(nk::DOCSTRING, t.range, t.text));
            self.skip_newlines();
        }

        if self.at(TokenKind::Indent) {
            children.push(self.parse_indented_content_block());
        }

        let end = self.peek().range.start;
        make_node(nk::SLUGLINE_SCENE, range_to(start, end), children)
    }

    fn parse_timecode_block(&mut self) -> SyntaxNode {
        let start = self.peek().range.start;
        self.advance(); // `at`
        let mut children = Vec::new();
        // Expect a number-looking token (the lexer produces e.g. "2.5"
        // as a single Number).
        if self.at(TokenKind::Number) {
            let t = self.advance().clone();
            children.push(leaf(nk::NUMBER, t.range, t.text));
        }
        self.expect(
            TokenKind::Newline,
            "doc-header-id",
            "Expected newline after timecode",
        );
        if self.at(TokenKind::Indent) {
            // Body: track commands OR dialogue lines.
            self.advance();
            while !self.at(TokenKind::Dedent) && !self.at_eof() {
                if self.at(TokenKind::Colon) {
                    children.push(self.parse_track_command());
                } else if self.at(TokenKind::Speaker)
                    || self.at(TokenKind::Dollar)
                    || self.at(TokenKind::At)
                {
                    children.push(self.parse_dialogue());
                } else if self.at(TokenKind::Newline) {
                    self.advance();
                } else {
                    children.push(self.error_node(
                        "unexpected-child",
                        format!("Unexpected token `{}` inside `at` block", self.peek().text),
                    ));
                    self.resync_to_line_boundary();
                }
            }
            self.consume(TokenKind::Dedent);
        }
        let end = self.peek().range.start;
        make_node(nk::TIMECODE_BLOCK, range_to(start, end), children)
    }

    fn parse_track_command(&mut self) -> SyntaxNode {
        let start = self.peek().range.start;
        self.advance(); // `:`
        let mut children = Vec::new();
        if matches!(self.peek_kind(), TokenKind::Ident | TokenKind::Speaker) {
            let t = self.advance().clone();
            children.push(leaf(nk::IDENT, t.range, t.text));
        }
        self.expect(
            TokenKind::Colon,
            "doc-header-id",
            "Expected `:` in track command",
        );
        if matches!(self.peek_kind(), TokenKind::Ident | TokenKind::Speaker) {
            let t = self.advance().clone();
            children.push(leaf(nk::IDENT, t.range, t.text));
        }
        if self.at(TokenKind::LParen) {
            self.advance();
            while !self.at(TokenKind::RParen) && !self.at_eof() {
                children.push(self.parse_expression());
                if !self.consume(TokenKind::Comma) {
                    break;
                }
            }
            self.expect(TokenKind::RParen, "bracket-unbalanced", "Expected `)`");
        }
        if self.at(TokenKind::Newline) {
            self.advance();
        }
        let end = self.peek().range.start;
        make_node(nk::TRACK_COMMAND, range_to(start, end), children)
    }

    fn parse_modifiers_and_scope_and_guard(&mut self, children: &mut Vec<SyntaxNode>) {
        loop {
            if self.at(TokenKind::Dot) {
                children.push(self.parse_modifier());
                continue;
            }
            if self.at_word("as") {
                children.push(self.parse_participant_scope());
                continue;
            }
            if self.at_word("if") {
                children.push(self.parse_guard());
                continue;
            }
            break;
        }
    }

    fn parse_modifier(&mut self) -> SyntaxNode {
        let start = self.peek().range.start;
        self.advance(); // `.`
        let mut children = Vec::new();
        if matches!(self.peek_kind(), TokenKind::Ident | TokenKind::Speaker) {
            let t = self.advance().clone();
            children.push(leaf(nk::IDENT, t.range, t.text));
        }
        if self.at(TokenKind::LParen) {
            self.advance();
            if !self.at(TokenKind::RParen) {
                let arg = match self.peek_kind() {
                    TokenKind::String => {
                        let t = self.advance().clone();
                        leaf(nk::STRING, t.range, t.text)
                    }
                    TokenKind::Number => {
                        let t = self.advance().clone();
                        leaf(nk::NUMBER, t.range, t.text)
                    }
                    _ => self.parse_primary_expression(),
                };
                children.push(arg);
            }
            self.expect(TokenKind::RParen, "bracket-unbalanced", "Expected `)`");
        }
        let end = self.peek().range.start;
        make_node(nk::MODIFIER, range_to(start, end), children)
    }

    fn parse_participant_scope(&mut self) -> SyntaxNode {
        let start = self.peek().range.start;
        self.advance(); // `as`
        let mut children = Vec::new();
        if self.at_word("participant") || self.at_word("faction") {
            let t = self.advance().clone();
            children.push(leaf(nk::IDENT, t.range, t.text));
        } else if self.at(TokenKind::Dollar) {
            children.push(self.parse_resolve_ref());
        } else {
            children.push(self.error_node(
                "as-faction-bad-target",
                "Expected `participant`, `faction`, or $ref",
            ));
        }
        let end = self.peek().range.start;
        make_node(nk::PARTICIPANT_SCOPE, range_to(start, end), children)
    }

    fn parse_guard(&mut self) -> SyntaxNode {
        let start = self.peek().range.start;
        self.advance(); // `if`
        let expr = self.parse_expression();
        let end = self.peek().range.start;
        make_node(nk::GUARD, range_to(start, end), vec![expr])
    }
}

// ═══════════════════════════════════════════════════════════════════
// §7. Content (inside sections)
// ═══════════════════════════════════════════════════════════════════

impl Parser<'_> {
    fn parse_indented_content_block(&mut self) -> SyntaxNode {
        let start = self.peek().range.start;
        self.expect(TokenKind::Indent, "indent-jump", "Expected indented block");
        let mut children = Vec::new();
        while !self.at(TokenKind::Dedent) && !self.at_eof() {
            if self.at(TokenKind::Newline) {
                self.advance();
                continue;
            }
            let before = self.pos;
            children.push(self.parse_content_line());
            if self.pos == before {
                // Defensive — drain a token to avoid infinite loop.
                self.advance();
            }
        }
        self.consume(TokenKind::Dedent);
        let end = self.peek().range.start;
        make_node("content_block", range_to(start, end), children)
    }

    fn parse_content_line(&mut self) -> SyntaxNode {
        // Comments / newlines pass through.
        if self.at(TokenKind::LineComment) || self.at(TokenKind::BlockComment) {
            let t = self.advance().clone();
            return leaf("comment", t.range, t.text);
        }
        if self.at(TokenKind::Newline) {
            self.advance();
            return make_node("blank", self.peek().range, Vec::new());
        }

        // Line-class dispatch.
        match self.peek_kind() {
            TokenKind::DashDash => return self.parse_section(),
            TokenKind::HashHash => return self.parse_slugline_scene(),
            TokenKind::Star | TokenKind::Plus => return self.parse_choice(),
            TokenKind::Arrow => return self.parse_divert(),
            TokenKind::BackArrow => return self.parse_return_line(),
            TokenKind::Gt => return self.parse_flavor_line(),
            TokenKind::Tilde => return self.parse_action_line(),
            TokenKind::At if matches!(self.peek_n(1).kind, TokenKind::Ident) => {
                // Annotation: @vo / @director / @note — only when at the
                // very start of a content line.
                return self.parse_annotation();
            }
            _ => {}
        }

        // Speaker line (dialogue).
        if matches!(self.peek_kind(), TokenKind::Speaker)
            || (self.at(TokenKind::Dollar) && self.peek_n(1).kind == TokenKind::Ident)
        {
            return self.parse_dialogue();
        }

        // Block keywords.
        if self.at_word("each") {
            return self.parse_each_visit();
        }
        if self.at_word("after") {
            return self.parse_after_block();
        }
        if self.at_word("otherwise") {
            return self.parse_otherwise_block();
        }
        if self.at_word("when") {
            return self.parse_when_top_level();
        }
        if self.at_word("match") {
            return self.parse_match_block();
        }
        if self.at_word("let") {
            return self.parse_let_binding();
        }
        if self.at_word("var")
            || self.at_word("fire")
            || self.at_word("advance")
            || self.at_word("trigger")
            || self.at_word("modify")
            || self.at_word("enroll")
            || self.at_word("cue")
            || self.at_word("reveal")
        {
            return self.parse_action_line();
        }
        if self.at(TokenKind::LParen) {
            return self.parse_sexp_form_or_decl();
        }

        // Stage direction — prose at content position not matching any
        // other line class.
        self.parse_stage_direction()
    }

    fn parse_dialogue(&mut self) -> SyntaxNode {
        let start = self.peek().range.start;
        let speaker = self.parse_speaker_ref();
        let mut children = vec![speaker];

        if self.at(TokenKind::LBrace) {
            children.push(self.parse_char_block());
        }
        if self.at(TokenKind::Caret) {
            let t = self.advance().clone();
            children.push(leaf("dual_marker", t.range, t.text));
        }

        // Optional improv parenthetical: `(improv ...)`
        if self.at(TokenKind::LParen) && self.peek_n(1).is_word("improv") {
            children.push(self.parse_improv_parenthetical());
        }

        self.expect(
            TokenKind::Newline,
            "doc-header-id",
            "Expected newline after speaker",
        );

        // Optional parenthetical (indented one level).
        if self.at(TokenKind::Indent) && self.peek_n(1).kind == TokenKind::LParen {
            self.advance();
            children.push(self.parse_parenthetical_inner());
            self.consume(TokenKind::Dedent);
        }

        // Text lines (indented block). Dialogue text lines are the
        // children of the dialogue node. The block ends when we see
        // either a real DEDENT or a line that starts with a
        // non-text-line sigil (`*`, `+`, `->`, `<-`, `~`, `>`, or
        // another speaker) — those are peer content items at the
        // same indent column, NOT dialogue text. We leave any pending
        // DEDENT for the section's body loop.
        if self.at(TokenKind::Indent) {
            self.advance();
            while !self.at(TokenKind::Dedent) && !self.at_eof() {
                if self.at(TokenKind::Newline) {
                    self.advance();
                    continue;
                }
                if self.is_non_text_line_opener() {
                    break;
                }
                let line_start = self.peek().range.start;
                let text = self.collect_inline_text_until_newline();
                let line_end = self.peek().range.start;
                children.push(make_node(
                    nk::TEXT_LINE,
                    range_to(line_start, line_end),
                    vec![text],
                ));
                if self.at(TokenKind::Newline) {
                    self.advance();
                }
            }
            // Only consume the DEDENT if we hit it naturally — if we
            // broke out for a peer content item, leave the DEDENT for
            // the outer loop.
            self.consume(TokenKind::Dedent);
        }

        let end = self.peek().range.start;
        make_node(nk::DIALOGUE, range_to(start, end), children)
    }

    /// `true` when the current token starts a content line that is
    /// NOT dialogue text (a choice, divert, action, etc.). Used to
    /// terminate dialogue's text-line block when a peer content item
    /// appears at the same indent column.
    fn is_non_text_line_opener(&self) -> bool {
        matches!(
            self.peek_kind(),
            TokenKind::Star
                | TokenKind::Plus
                | TokenKind::Arrow
                | TokenKind::BackArrow
                | TokenKind::Tilde
                | TokenKind::Gt
                | TokenKind::DashDash
                | TokenKind::HashHash
                | TokenKind::Speaker
        )
    }

    fn parse_speaker_ref(&mut self) -> SyntaxNode {
        let start = self.peek().range.start;
        let node = match self.peek_kind() {
            TokenKind::Speaker => {
                let t = self.advance().clone();
                leaf(nk::SPEAKER, t.range, t.text)
            }
            TokenKind::Dollar => self.parse_resolve_ref(),
            TokenKind::At => self.parse_static_ref(),
            _ => self.error_node("unknown-cast", "Expected speaker"),
        };
        let end = self.peek().range.start;
        make_node(nk::SPEAKER_REF, range_to(start, end), vec![node])
    }

    fn parse_char_block(&mut self) -> SyntaxNode {
        let start = self.peek().range.start;
        self.advance(); // `{`
        let mut children = Vec::new();
        while !self.at(TokenKind::RBrace) && !self.at_eof() {
            children.push(self.parse_char_item());
            if !self.consume(TokenKind::Comma) {
                break;
            }
        }
        self.expect(TokenKind::RBrace, "bracket-unbalanced", "Expected `}`");
        let end = self.peek().range.start;
        make_node(nk::CHAR_BLOCK, range_to(start, end), children)
    }

    fn parse_char_item(&mut self) -> SyntaxNode {
        let start = self.peek().range.start;
        let mut children = Vec::new();
        if matches!(self.peek_kind(), TokenKind::Ident | TokenKind::Speaker) {
            let t = self.advance().clone();
            children.push(leaf(nk::IDENT, t.range, t.text));
        }
        if self.consume(TokenKind::Colon) {
            match self.peek_kind() {
                TokenKind::String => {
                    let t = self.advance().clone();
                    children.push(leaf(nk::STRING, t.range, t.text));
                }
                TokenKind::Number => {
                    let t = self.advance().clone();
                    children.push(leaf(nk::NUMBER, t.range, t.text));
                }
                TokenKind::Ident | TokenKind::Speaker => {
                    let t = self.advance().clone();
                    children.push(leaf(nk::IDENT, t.range, t.text));
                }
                _ => {}
            }
        }
        let end = self.peek().range.start;
        make_node(nk::CHAR_ITEM, range_to(start, end), children)
    }

    fn parse_parenthetical_inner(&mut self) -> SyntaxNode {
        let start = self.peek().range.start;
        self.expect(TokenKind::LParen, "bracket-unbalanced", "Expected `(`");
        let text = self.collect_inline_text_until_close_paren();
        self.expect(TokenKind::RParen, "bracket-unbalanced", "Expected `)`");
        if self.at(TokenKind::Newline) {
            self.advance();
        }
        let end = self.peek().range.start;
        make_node(nk::PARENTHETICAL, range_to(start, end), vec![text])
    }

    fn parse_improv_parenthetical(&mut self) -> SyntaxNode {
        let start = self.peek().range.start;
        self.advance(); // `(`
        self.advance(); // `improv`
        let mut children = Vec::new();
        while !self.at(TokenKind::RParen) && !self.at_eof() {
            if self.at(TokenKind::Dot) {
                // .latitude(0.5) / .duration(45s) / etc.
                children.push(self.parse_modifier());
                continue;
            }
            if self.at_word("about") {
                self.advance();
                if self.consume(TokenKind::Colon) && self.at(TokenKind::String) {
                    let t = self.advance().clone();
                    children.push(leaf(nk::STRING, t.range, t.text));
                }
                continue;
            }
            // Skip stray tokens defensively.
            self.advance();
        }
        self.expect(TokenKind::RParen, "bracket-unbalanced", "Expected `)`");
        let end = self.peek().range.start;
        make_node(nk::IMPROV_PARENTHETICAL, range_to(start, end), children)
    }

    fn parse_flavor_line(&mut self) -> SyntaxNode {
        let start = self.peek().range.start;
        self.advance(); // `>`
        let text = self.collect_inline_text_until_newline();
        if self.at(TokenKind::Newline) {
            self.advance();
        }
        let end = self.peek().range.start;
        make_node(nk::FLAVOR_LINE, range_to(start, end), vec![text])
    }

    fn parse_stage_direction(&mut self) -> SyntaxNode {
        let start = self.peek().range.start;
        let text = self.collect_inline_text_until_newline();
        if self.at(TokenKind::Newline) {
            self.advance();
        }
        let end = self.peek().range.start;
        make_node(nk::STAGE_DIRECTION, range_to(start, end), vec![text])
    }

    fn parse_choice(&mut self) -> SyntaxNode {
        let start = self.peek().range.start;
        let marker_tok = self.advance().clone();
        let mut children = vec![leaf("choice_marker", marker_tok.range, marker_tok.text)];

        // Optional modifiers (.once / .sticky / .show("..."))
        while self.at(TokenKind::Dot) {
            children.push(self.parse_modifier());
        }

        // Label — collect inline text up to `if` guard or newline.
        let label_start = self.peek().range.start;
        let label = self.collect_inline_text_until_guard_or_newline();
        let label_end = self.peek().range.start;
        children.push(make_node(
            nk::CHOICE_LABEL,
            range_to(label_start, label_end),
            vec![label],
        ));

        // Optional inline divert: `-> target` directly on the same
        // line. Some authors write `* I'll help. -> investigate`.
        if self.at(TokenKind::Arrow) {
            children.push(self.parse_divert());
        }

        if self.at_word("if") {
            children.push(self.parse_guard());
        }

        self.expect(
            TokenKind::Newline,
            "doc-header-id",
            "Expected newline after choice",
        );
        if self.at(TokenKind::Indent) {
            children.push(self.parse_indented_content_block());
        }

        let end = self.peek().range.start;
        make_node(nk::CHOICE, range_to(start, end), children)
    }

    fn parse_divert(&mut self) -> SyntaxNode {
        let start = self.peek().range.start;
        self.advance(); // `->`
        let mut children = Vec::new();
        // Target.
        match self.peek_kind() {
            TokenKind::At => children.push(self.parse_static_ref()),
            TokenKind::Ident | TokenKind::Speaker => {
                let mut tgt_children = Vec::new();
                let t = self.advance().clone();
                tgt_children.push(leaf(nk::IDENT, t.range, t.text));
                if self.consume(TokenKind::Dot)
                    && matches!(self.peek_kind(), TokenKind::Ident | TokenKind::Speaker)
                {
                    let t2 = self.advance().clone();
                    tgt_children.push(leaf(nk::IDENT, t2.range, t2.text));
                }
                // Tunnel call: `name( args )->`
                if self.at(TokenKind::LParen) {
                    self.advance();
                    while !self.at(TokenKind::RParen) && !self.at_eof() {
                        tgt_children.push(self.parse_expression());
                        if !self.consume(TokenKind::Comma) {
                            break;
                        }
                    }
                    self.expect(TokenKind::RParen, "bracket-unbalanced", "Expected `)`");
                    if self.consume(TokenKind::Arrow) {
                        // Trailing `->` confirms tunnel call.
                        let end = self.peek().range.start;
                        let tunnel =
                            make_node(nk::TUNNEL_CALL, range_to(t.range.start, end), tgt_children);
                        children.push(tunnel);
                    } else {
                        children.extend(tgt_children);
                    }
                } else {
                    children.extend(tgt_children);
                }
            }
            _ => {
                children.push(self.error_node("divert-target-unknown", "Expected divert target"));
            }
        }

        // Modifiers + scope + guard.
        self.parse_modifiers_and_scope_and_guard(&mut children);
        self.expect(
            TokenKind::Newline,
            "doc-header-id",
            "Expected newline after divert",
        );
        let end = self.peek().range.start;
        make_node(nk::DIVERT, range_to(start, end), children)
    }

    fn parse_return_line(&mut self) -> SyntaxNode {
        let start = self.peek().range.start;
        self.advance(); // `<-`
        let mut children = Vec::new();
        if matches!(self.peek_kind(), TokenKind::Ident | TokenKind::Speaker) {
            let t = self.advance().clone();
            children.push(leaf(nk::IDENT, t.range, t.text));
        }
        self.expect(
            TokenKind::Newline,
            "doc-header-id",
            "Expected newline after return",
        );
        let end = self.peek().range.start;
        make_node(nk::RETURN_LINE, range_to(start, end), children)
    }

    fn parse_action_line(&mut self) -> SyntaxNode {
        let start = self.peek().range.start;
        let mut children = Vec::new();
        if self.at(TokenKind::Tilde) {
            let prefix_tok = self.advance().clone();
            // `~` prefix is optional — emit a marker child for tools
            // that care about it.
            children.push(leaf("action_prefix", prefix_tok.range, "~"));
        }
        // Single action (no `;` chaining for now — first cut).
        children.push(self.parse_action());
        while self.consume(TokenKind::Semi) {
            children.push(self.parse_action());
        }
        if self.at(TokenKind::Newline) {
            self.advance();
        }
        let end = self.peek().range.start;
        make_node(nk::ACTION_LINE, range_to(start, end), children)
    }

    fn parse_action(&mut self) -> SyntaxNode {
        // KeywordAction | NamespaceCall | MutationExpr
        let start = self.peek().range.start;

        // Mutation: starts with `$ident ...` followed by an assign op.
        if self.at(TokenKind::Dollar) {
            let save = self.pos;
            let lvalue = self.parse_resolve_ref();
            if matches!(
                self.peek_kind(),
                TokenKind::Walrus | TokenKind::PlusEq | TokenKind::MinusEq | TokenKind::PlusPlus
            ) {
                let op_tok = self.advance().clone();
                let op = leaf("assign_op", op_tok.range, op_tok.text);
                let rhs = if matches!(op_tok.kind, TokenKind::PlusPlus) {
                    // `++` has no RHS.
                    None
                } else {
                    Some(self.parse_expression())
                };
                let mut kids = vec![lvalue, op];
                if let Some(r) = rhs {
                    kids.push(r);
                }
                let end = self.peek().range.start;
                return make_node(nk::MUTATION_EXPR, range_to(start, end), kids);
            }
            // Not a mutation — rewind and fall through.
            self.pos = save;
        }

        // NamespaceCall: ident ('.' ident)+ '(' args? ')'
        if matches!(self.peek_kind(), TokenKind::Ident | TokenKind::Speaker)
            && self.peek_n(1).kind == TokenKind::Dot
        {
            return self.parse_namespace_call();
        }

        // KeywordAction
        if matches!(self.peek_kind(), TokenKind::Ident | TokenKind::Speaker) {
            let kw_tok = self.advance().clone();
            let mut children = vec![leaf(nk::IDENT, kw_tok.range, kw_tok.text)];
            // Action keywords have registry-driven arg shapes (grammar
            // §7.5). For the parser we treat the tail as a single
            // free-form payload — the validator splits it per the
            // registry entry. This is what keeps `var $x := value`,
            // `cue @lights`, `enroll $P into singers`, and
            // `fire bell_solved` all under one production.
            let payload_start = self.peek().range.start;
            let mut payload = String::new();
            while !self.at(TokenKind::Newline) && !self.at(TokenKind::Semi) && !self.at_eof() {
                let t = self.advance();
                if !payload.is_empty() {
                    payload.push(' ');
                }
                payload.push_str(&t.text);
            }
            if !payload.is_empty() {
                let payload_end = self.peek().range.start;
                children.push(leaf(
                    nk::PROPERTY_VALUE,
                    range_to(payload_start, payload_end),
                    payload,
                ));
            }
            let end = self.peek().range.start;
            return make_node(nk::KEYWORD_ACTION, range_to(start, end), children);
        }

        // Unrecognised — emit an error node.
        self.error_node("unknown-action-kw", "Expected action keyword")
    }

    fn parse_namespace_call(&mut self) -> SyntaxNode {
        let start = self.peek().range.start;
        let mut children = Vec::new();
        let head = self.advance().clone();
        children.push(leaf(nk::IDENT, head.range, head.text));
        while self.consume(TokenKind::Dot) {
            if matches!(self.peek_kind(), TokenKind::Ident | TokenKind::Speaker) {
                let t = self.advance().clone();
                children.push(leaf(nk::IDENT, t.range, t.text));
            } else {
                break;
            }
        }
        if self.at(TokenKind::LParen) {
            self.advance();
            while !self.at(TokenKind::RParen) && !self.at_eof() {
                children.push(self.parse_expression());
                if !self.consume(TokenKind::Comma) {
                    break;
                }
            }
            self.expect(TokenKind::RParen, "bracket-unbalanced", "Expected `)`");
        }
        let end = self.peek().range.start;
        make_node(nk::NAMESPACE_CALL, range_to(start, end), children)
    }

    fn parse_annotation(&mut self) -> SyntaxNode {
        let start = self.peek().range.start;
        self.advance(); // `@`
        let mut children = Vec::new();
        if matches!(self.peek_kind(), TokenKind::Ident | TokenKind::Speaker) {
            let t = self.advance().clone();
            children.push(leaf(nk::IDENT, t.range, t.text));
        }
        if self.consume(TokenKind::Colon)
            && matches!(self.peek_kind(), TokenKind::Ident | TokenKind::Speaker)
        {
            let t = self.advance().clone();
            children.push(leaf(nk::IDENT, t.range, t.text));
        }
        // Body — free-form to end of line.
        let mut body = String::new();
        let body_start = self.peek().range.start;
        while !self.at(TokenKind::Newline) && !self.at_eof() {
            let t = self.advance();
            if !body.is_empty() {
                body.push(' ');
            }
            body.push_str(&t.text);
        }
        if !body.is_empty() {
            let body_end = self.peek().range.start;
            children.push(leaf(
                nk::PROPERTY_VALUE,
                range_to(body_start, body_end),
                body,
            ));
        }
        if self.at(TokenKind::Newline) {
            self.advance();
        }
        let end = self.peek().range.start;
        make_node(nk::ANNOTATION, range_to(start, end), children)
    }
}

// ═══════════════════════════════════════════════════════════════════
// §7.7. Blocks (each visit / after / otherwise / match)
// ═══════════════════════════════════════════════════════════════════

impl Parser<'_> {
    fn parse_each_visit(&mut self) -> SyntaxNode {
        let start = self.peek().range.start;
        self.advance(); // `each`
        self.consume_word("visit");
        let mut children = Vec::new();
        while self.at(TokenKind::Dot) {
            children.push(self.parse_modifier());
        }
        self.expect(
            TokenKind::Newline,
            "doc-header-id",
            "Expected newline after `each visit`",
        );
        if self.at(TokenKind::Indent) {
            self.advance();
            while !self.at(TokenKind::Dedent) && !self.at_eof() {
                if self.at(TokenKind::Newline) {
                    self.advance();
                    continue;
                }
                if self.at_word("first")
                    || self.at_word("then")
                    || self.at_word("finally")
                    || self.at(TokenKind::Slash)
                {
                    children.push(self.parse_visit_branch());
                } else {
                    children.push(
                        self.error_node("unexpected-child", "Expected first/then/finally/`/`"),
                    );
                    self.resync_to_line_boundary();
                }
            }
            self.consume(TokenKind::Dedent);
        }
        let end = self.peek().range.start;
        make_node(nk::EACH_VISIT_BLOCK, range_to(start, end), children)
    }

    fn parse_visit_branch(&mut self) -> SyntaxNode {
        let start = self.peek().range.start;
        let mut children = Vec::new();
        if self.at(TokenKind::Slash) {
            let t = self.advance().clone();
            children.push(leaf("alt_separator", t.range, t.text));
        } else {
            let t = self.advance().clone();
            children.push(leaf(nk::IDENT, t.range, t.text));
        }
        self.expect(
            TokenKind::Newline,
            "doc-header-id",
            "Expected newline after branch label",
        );
        if self.at(TokenKind::Indent) {
            children.push(self.parse_indented_content_block());
        }
        let end = self.peek().range.start;
        make_node(nk::VISIT_BRANCH, range_to(start, end), children)
    }

    fn parse_after_block(&mut self) -> SyntaxNode {
        let start = self.peek().range.start;
        self.advance(); // `after`
        let mut children = vec![self.parse_expression()];
        self.expect(
            TokenKind::Newline,
            "doc-header-id",
            "Expected newline after `after`",
        );
        if self.at(TokenKind::Indent) {
            children.push(self.parse_indented_content_block());
        }
        let end = self.peek().range.start;
        make_node(nk::AFTER_BLOCK, range_to(start, end), children)
    }

    fn parse_otherwise_block(&mut self) -> SyntaxNode {
        let start = self.peek().range.start;
        self.advance(); // `otherwise`
        self.expect(
            TokenKind::Newline,
            "doc-header-id",
            "Expected newline after `otherwise`",
        );
        let mut children = Vec::new();
        if self.at(TokenKind::Indent) {
            children.push(self.parse_indented_content_block());
        }
        let end = self.peek().range.start;
        make_node(nk::OTHERWISE_BLOCK, range_to(start, end), children)
    }

    fn parse_match_block(&mut self) -> SyntaxNode {
        let start = self.peek().range.start;
        self.advance(); // `match`
        let mut children = vec![self.parse_expression()];
        self.expect(
            TokenKind::Newline,
            "doc-header-id",
            "Expected newline after `match`",
        );
        if self.at(TokenKind::Indent) {
            self.advance();
            while !self.at(TokenKind::Dedent) && !self.at_eof() {
                if self.at(TokenKind::Newline) {
                    self.advance();
                    continue;
                }
                children.push(self.parse_match_arm());
            }
            self.consume(TokenKind::Dedent);
        }
        let end = self.peek().range.start;
        make_node(nk::MATCH_BLOCK, range_to(start, end), children)
    }

    fn parse_match_arm(&mut self) -> SyntaxNode {
        let start = self.peek().range.start;
        let mut children = Vec::new();
        match self.peek_kind() {
            TokenKind::String => {
                let t = self.advance().clone();
                children.push(leaf(nk::STRING, t.range, t.text));
            }
            TokenKind::Number => {
                let t = self.advance().clone();
                children.push(leaf(nk::NUMBER, t.range, t.text));
            }
            TokenKind::Ident => {
                let t = self.advance().clone();
                children.push(leaf(nk::IDENT, t.range, t.text));
            }
            _ => {
                children
                    .push(self.error_node("unexpected-child", "Expected match arm discriminant"));
                self.advance();
            }
        }
        self.expect(
            TokenKind::Newline,
            "doc-header-id",
            "Expected newline after match arm",
        );
        if self.at(TokenKind::Indent) {
            children.push(self.parse_indented_content_block());
        }
        let end = self.peek().range.start;
        make_node(nk::MATCH_ARM, range_to(start, end), children)
    }
}

// ═══════════════════════════════════════════════════════════════════
// §16. Character archetype — knowledge / goal / disposition / hook
// ═══════════════════════════════════════════════════════════════════

impl Parser<'_> {
    fn parse_knowledge_block(&mut self) -> SyntaxNode {
        let start = self.peek().range.start;
        self.advance(); // `knowledge`
        self.expect(
            TokenKind::Newline,
            "doc-header-id",
            "Expected newline after `knowledge`",
        );
        let mut children = Vec::new();
        if self.at(TokenKind::Indent) {
            self.advance();
            while !self.at(TokenKind::Dedent) && !self.at_eof() {
                if self.at(TokenKind::Newline) {
                    self.advance();
                    continue;
                }
                children.push(self.parse_knowledge_field());
            }
            self.consume(TokenKind::Dedent);
        }
        let end = self.peek().range.start;
        make_node(nk::KNOWLEDGE_BLOCK, range_to(start, end), children)
    }

    fn parse_knowledge_field(&mut self) -> SyntaxNode {
        let start = self.peek().range.start;
        let mut children = Vec::new();
        if matches!(self.peek_kind(), TokenKind::Ident | TokenKind::Speaker) {
            let t = self.advance().clone();
            children.push(leaf(nk::IDENT, t.range, t.text));
        } else {
            children.push(self.error_node("knowledge-bad-type", "Expected knowledge field name"));
        }
        self.expect(
            TokenKind::Colon,
            "doc-header-id",
            "Expected `:` in knowledge field",
        );
        children.push(self.parse_knowledge_type());
        if self.consume(TokenKind::Eq) {
            children.push(self.parse_expression());
        }
        if self.at(TokenKind::Newline) {
            self.advance();
        }
        let end = self.peek().range.start;
        make_node(nk::KNOWLEDGE_FIELD, range_to(start, end), children)
    }

    fn parse_knowledge_type(&mut self) -> SyntaxNode {
        let start = self.peek().range.start;
        let mut children = Vec::new();
        // Closed enum: `{ A, B, C }`
        if self.at(TokenKind::LBrace) {
            self.advance();
            while !self.at(TokenKind::RBrace) && !self.at_eof() {
                if matches!(self.peek_kind(), TokenKind::Ident | TokenKind::Speaker) {
                    let t = self.advance().clone();
                    children.push(leaf(nk::IDENT, t.range, t.text));
                } else {
                    self.advance();
                }
                if !self.consume(TokenKind::Comma) {
                    break;
                }
            }
            self.expect(TokenKind::RBrace, "bracket-unbalanced", "Expected `}`");
        } else if matches!(self.peek_kind(), TokenKind::Ident | TokenKind::Speaker) {
            let head = self.advance().clone();
            children.push(leaf(nk::IDENT, head.range, head.text.clone()));
            // `list<T>` form.
            if head.text == "list" && self.consume(TokenKind::Lt) {
                children.push(self.parse_knowledge_type());
                self.expect(
                    TokenKind::Gt,
                    "bracket-unbalanced",
                    "Expected `>` to close `list<T>`",
                );
            }
        } else {
            children.push(self.error_node("knowledge-bad-type", "Expected type name"));
        }
        // Nilable suffix.
        if self.consume(TokenKind::QMark) {
            children.push(leaf("nilable", self.tokens[self.pos - 1].range, "?"));
        }
        let end = self.peek().range.start;
        make_node(nk::KNOWLEDGE_TYPE, range_to(start, end), children)
    }

    fn parse_goal_decl(&mut self) -> SyntaxNode {
        let start = self.peek().range.start;
        self.advance(); // `goal`
        let mut children = Vec::new();
        if matches!(self.peek_kind(), TokenKind::Ident | TokenKind::Speaker) {
            let t = self.advance().clone();
            children.push(leaf(nk::IDENT, t.range, t.text));
        }
        self.expect(
            TokenKind::Newline,
            "doc-header-id",
            "Expected newline after goal name",
        );
        if self.at(TokenKind::Indent) {
            self.advance();
            while !self.at(TokenKind::Dedent) && !self.at_eof() {
                if self.at(TokenKind::Newline) {
                    self.advance();
                    continue;
                }
                children.push(self.parse_goal_knob());
            }
            self.consume(TokenKind::Dedent);
        }
        let end = self.peek().range.start;
        make_node(nk::GOAL_DECL, range_to(start, end), children)
    }

    fn parse_goal_knob(&mut self) -> SyntaxNode {
        let start = self.peek().range.start;
        let mut children = Vec::new();
        let knob_name = if matches!(self.peek_kind(), TokenKind::Ident | TokenKind::Speaker) {
            let t = self.advance().clone();
            children.push(leaf(nk::IDENT, t.range, t.text.clone()));
            t.text
        } else {
            String::new()
        };
        // `priority = N` / `active_when = expr` / `drives generator IDENT`
        // / `on_complete <action chain>` etc.
        if self.consume(TokenKind::Eq) {
            children.push(self.parse_expression());
        } else if knob_name == "drives" {
            // `drives generator IDENT`
            if self.peek().is_word("generator") {
                let t = self.advance().clone();
                children.push(leaf(nk::IDENT, t.range, t.text));
            }
            if matches!(self.peek_kind(), TokenKind::Ident | TokenKind::Speaker) {
                let t = self.advance().clone();
                children.push(leaf(nk::IDENT, t.range, t.text));
            }
        } else {
            // ActionChain — slurp tokens up to newline / `;`.
            let chain_start = self.peek().range.start;
            let mut raw = String::new();
            while !self.at(TokenKind::Newline) && !self.at_eof() {
                let t = self.advance();
                if !raw.is_empty() {
                    raw.push(' ');
                }
                raw.push_str(&t.text);
            }
            if !raw.is_empty() {
                let chain_end = self.peek().range.start;
                children.push(leaf(
                    nk::ACTION_CHAIN,
                    range_to(chain_start, chain_end),
                    raw,
                ));
            }
        }
        if self.at(TokenKind::Newline) {
            self.advance();
        }
        let end = self.peek().range.start;
        make_node(nk::GOAL_KNOB, range_to(start, end), children)
    }

    fn parse_disposition_block(&mut self) -> SyntaxNode {
        let start = self.peek().range.start;
        self.advance(); // `disposition`
        let mut children = Vec::new();
        // Target — $ref or @ref.
        match self.peek_kind() {
            TokenKind::Dollar | TokenKind::DollarBrace | TokenKind::DollarParen => {
                children.push(self.parse_resolve_ref());
            }
            TokenKind::At => children.push(self.parse_static_ref()),
            _ => {
                children
                    .push(self.error_node("as-faction-bad-target", "Expected disposition target"));
            }
        }
        self.expect(
            TokenKind::Newline,
            "doc-header-id",
            "Expected newline after disposition target",
        );
        if self.at(TokenKind::Indent) {
            self.advance();
            while !self.at(TokenKind::Dedent) && !self.at_eof() {
                if self.at(TokenKind::Newline) {
                    self.advance();
                    continue;
                }
                if self.at_word("reacts") {
                    children.push(self.parse_disposition_react());
                } else {
                    children.push(self.parse_disposition_axis());
                }
            }
            self.consume(TokenKind::Dedent);
        }
        let end = self.peek().range.start;
        make_node(nk::DISPOSITION_BLOCK, range_to(start, end), children)
    }

    fn parse_disposition_axis(&mut self) -> SyntaxNode {
        let start = self.peek().range.start;
        let mut children = Vec::new();
        if matches!(self.peek_kind(), TokenKind::Ident | TokenKind::Speaker) {
            let t = self.advance().clone();
            children.push(leaf(nk::IDENT, t.range, t.text));
        }
        self.expect(
            TokenKind::Eq,
            "doc-header-id",
            "Expected `=` in disposition axis",
        );
        children.push(self.parse_num_range_or_expression());
        // Optional ", init N" and ", mirror $ref".
        while self.consume(TokenKind::Comma) {
            if self.at_word("init") {
                self.advance();
                if self.at(TokenKind::Number) {
                    let t = self.advance().clone();
                    children.push(leaf("init", t.range, t.text));
                }
            } else if self.at_word("mirror") {
                let mc_start = self.peek().range.start;
                self.advance(); // `mirror`
                let target = match self.peek_kind() {
                    TokenKind::Dollar => self.parse_resolve_ref(),
                    TokenKind::At => self.parse_static_ref(),
                    _ => self.error_node("disposition-mirror-cycle", "Expected mirror target"),
                };
                let mc_end = self.peek().range.start;
                children.push(make_node(
                    nk::MIRROR_CLAUSE,
                    range_to(mc_start, mc_end),
                    vec![target],
                ));
            } else {
                self.advance();
            }
        }
        if self.at(TokenKind::Newline) {
            self.advance();
        }
        let end = self.peek().range.start;
        make_node(nk::DISPOSITION_AXIS, range_to(start, end), children)
    }

    fn parse_disposition_react(&mut self) -> SyntaxNode {
        let start = self.peek().range.start;
        self.advance(); // `reacts`
        let mut children = vec![self.parse_expression()];
        self.expect(
            TokenKind::Arrow,
            "doc-header-id",
            "Expected `->` in disposition reacts",
        );
        match self.peek_kind() {
            TokenKind::Ident | TokenKind::Speaker => {
                let t = self.advance().clone();
                children.push(leaf(nk::IDENT, t.range, t.text));
            }
            TokenKind::String => {
                let t = self.advance().clone();
                children.push(leaf(nk::STRING, t.range, t.text));
            }
            _ => {
                children.push(self.error_node("unexpected-child", "Expected reacts tag"));
            }
        }
        if self.at(TokenKind::Newline) {
            self.advance();
        }
        let end = self.peek().range.start;
        make_node(nk::DISPOSITION_REACT, range_to(start, end), children)
    }

    fn parse_num_range_or_expression(&mut self) -> SyntaxNode {
        // NumRange `N .. N` or fallback to expression.
        let save = self.pos;
        let start = self.peek().range.start;
        if self.at(TokenKind::Number) {
            let lhs_tok = self.advance().clone();
            if self.at(TokenKind::DotDot) {
                self.advance();
                if self.at(TokenKind::Number) {
                    let rhs_tok = self.advance().clone();
                    return make_node(
                        nk::NUM_RANGE,
                        range_to(start, rhs_tok.range.end),
                        vec![
                            leaf(nk::NUMBER, lhs_tok.range, lhs_tok.text),
                            leaf(nk::NUMBER, rhs_tok.range, rhs_tok.text),
                        ],
                    );
                }
            }
            // Wasn't a range — rewind and parse as expression.
            self.pos = save;
        }
        self.parse_expression()
    }

    fn parse_hook_decl(&mut self) -> SyntaxNode {
        let start = self.peek().range.start;
        self.advance(); // `on`
        let pattern_start = self.peek().range.start;
        let mut pattern_children = Vec::new();

        // Slurp the pattern up to end-of-line; the validator will
        // recognise the canonical shapes (meeting / passes / drops below /
        // == / is tag / cue / event / enters / exits / extension).
        // We capture each token as an opaque atom child so the LSP can
        // still highlight the pattern.
        while !self.at(TokenKind::Newline) && !self.at_eof() {
            let t = self.advance().clone();
            pattern_children.push(leaf(nk::IDENT, t.range, t.text));
        }
        let pattern_end = self.peek().range.start;
        let pattern = make_node(
            nk::HOOK_PATTERN,
            range_to(pattern_start, pattern_end),
            pattern_children,
        );
        let mut children = vec![pattern];

        self.expect(
            TokenKind::Newline,
            "doc-header-id",
            "Expected newline after hook pattern",
        );
        if self.at(TokenKind::Indent) {
            children.push(self.parse_indented_content_block());
        }
        let end = self.peek().range.start;
        make_node(nk::HOOK_DECL, range_to(start, end), children)
    }
}

// ═══════════════════════════════════════════════════════════════════
// §17. Stats + tree archetypes
// ═══════════════════════════════════════════════════════════════════

impl Parser<'_> {
    fn parse_attribute_decl(&mut self) -> SyntaxNode {
        let start = self.peek().range.start;
        self.advance(); // `attribute`
        let mut children = Vec::new();
        if matches!(self.peek_kind(), TokenKind::Ident | TokenKind::Speaker) {
            let t = self.advance().clone();
            children.push(leaf(nk::IDENT, t.range, t.text));
        }
        self.expect(
            TokenKind::Eq,
            "doc-header-id",
            "Expected `=` in attribute decl",
        );
        if self.at(TokenKind::Number) {
            let t = self.advance().clone();
            children.push(leaf(nk::NUMBER, t.range, t.text));
        }
        while self.consume(TokenKind::Comma) {
            children.push(self.parse_attribute_mod());
        }
        if self.at(TokenKind::Newline) {
            self.advance();
        }
        let end = self.peek().range.start;
        make_node(nk::ATTRIBUTE_DECL, range_to(start, end), children)
    }

    fn parse_attribute_mod(&mut self) -> SyntaxNode {
        let start = self.peek().range.start;
        let mut children = Vec::new();
        if matches!(self.peek_kind(), TokenKind::Ident | TokenKind::Speaker) {
            let t = self.advance().clone();
            let kind = t.text.clone();
            children.push(leaf(nk::IDENT, t.range, t.text));
            match kind.as_str() {
                "range" => {
                    children.push(self.parse_num_range_or_expression());
                }
                "min" | "max" => {
                    if self.at(TokenKind::Number) {
                        let t = self.advance().clone();
                        children.push(leaf(nk::NUMBER, t.range, t.text));
                    }
                }
                _ => {}
            }
        }
        let end = self.peek().range.start;
        make_node(nk::ATTRIBUTE_MOD, range_to(start, end), children)
    }

    fn parse_axis_decl(&mut self) -> SyntaxNode {
        let start = self.peek().range.start;
        self.advance(); // `axis`
        let mut children = Vec::new();
        if matches!(self.peek_kind(), TokenKind::Ident | TokenKind::Speaker) {
            let t = self.advance().clone();
            children.push(leaf(nk::IDENT, t.range, t.text));
        }
        self.expect(
            TokenKind::Newline,
            "doc-header-id",
            "Expected newline after axis name",
        );
        if self.at(TokenKind::Indent) {
            self.advance();
            while !self.at(TokenKind::Dedent) && !self.at_eof() {
                if self.at(TokenKind::Newline) {
                    self.advance();
                    continue;
                }
                children.push(self.parse_axis_property());
            }
            self.consume(TokenKind::Dedent);
        }
        let end = self.peek().range.start;
        make_node(nk::AXIS_DECL, range_to(start, end), children)
    }

    fn parse_axis_property(&mut self) -> SyntaxNode {
        let start = self.peek().range.start;
        let mut children = Vec::new();
        let key = if matches!(self.peek_kind(), TokenKind::Ident | TokenKind::Speaker) {
            let t = self.advance().clone();
            let k = t.text.clone();
            children.push(leaf(nk::IDENT, t.range, t.text));
            k
        } else {
            String::new()
        };
        match key.as_str() {
            "mode" => {
                if matches!(self.peek_kind(), TokenKind::Ident | TokenKind::Speaker) {
                    let t = self.advance().clone();
                    children.push(leaf(nk::IDENT, t.range, t.text));
                }
            }
            "curve" => {
                children.push(self.parse_expression());
            }
            "milestones" => {
                self.expect(
                    TokenKind::Newline,
                    "doc-header-id",
                    "Expected newline after `milestones`",
                );
                if self.at(TokenKind::Indent) {
                    self.advance();
                    while !self.at(TokenKind::Dedent) && !self.at_eof() {
                        if self.at(TokenKind::Newline) {
                            self.advance();
                            continue;
                        }
                        children.push(self.parse_milestone_entry());
                    }
                    self.consume(TokenKind::Dedent);
                }
            }
            "on" => {
                // `on use @ref` or `on advance <action chain>`
                if matches!(self.peek_kind(), TokenKind::Ident | TokenKind::Speaker) {
                    let t = self.advance().clone();
                    let sub = t.text.clone();
                    children.push(leaf(nk::IDENT, t.range, t.text));
                    if sub == "use" && self.at(TokenKind::At) {
                        children.push(self.parse_static_ref());
                    } else {
                        // action chain — slurp to newline.
                        let chain_start = self.peek().range.start;
                        let mut raw = String::new();
                        while !self.at(TokenKind::Newline) && !self.at_eof() {
                            let t = self.advance();
                            if !raw.is_empty() {
                                raw.push(' ');
                            }
                            raw.push_str(&t.text);
                        }
                        if !raw.is_empty() {
                            let chain_end = self.peek().range.start;
                            children.push(leaf(
                                nk::ACTION_CHAIN,
                                range_to(chain_start, chain_end),
                                raw,
                            ));
                        }
                    }
                }
            }
            "buy" => {
                if self.peek().is_word("from") {
                    let t = self.advance().clone();
                    children.push(leaf(nk::IDENT, t.range, t.text));
                }
                if matches!(self.peek_kind(), TokenKind::Ident | TokenKind::Speaker) {
                    let t = self.advance().clone();
                    children.push(leaf(nk::IDENT, t.range, t.text));
                }
            }
            "advance" => {
                // `advance on event IDENT`
                while !self.at(TokenKind::Newline) && !self.at_eof() {
                    let t = self.advance().clone();
                    children.push(leaf(nk::IDENT, t.range, t.text));
                }
            }
            "handler" => {
                if self.at(TokenKind::At) {
                    children.push(self.parse_static_ref());
                }
            }
            _ => {
                // Unknown — slurp the rest of the line as a payload.
                let val_start = self.peek().range.start;
                let mut raw = String::new();
                while !self.at(TokenKind::Newline) && !self.at_eof() {
                    let t = self.advance();
                    if !raw.is_empty() {
                        raw.push(' ');
                    }
                    raw.push_str(&t.text);
                }
                if !raw.is_empty() {
                    let val_end = self.peek().range.start;
                    children.push(leaf(nk::PROPERTY_VALUE, range_to(val_start, val_end), raw));
                }
            }
        }
        if self.at(TokenKind::Newline) {
            self.advance();
        }
        let end = self.peek().range.start;
        make_node(nk::AXIS_PROPERTY, range_to(start, end), children)
    }

    fn parse_milestone_entry(&mut self) -> SyntaxNode {
        let start = self.peek().range.start;
        let mut children = Vec::new();
        if self.at(TokenKind::Number) {
            let t = self.advance().clone();
            children.push(leaf(nk::NUMBER, t.range, t.text));
        }
        self.expect(
            TokenKind::Colon,
            "doc-header-id",
            "Expected `:` in milestone entry",
        );
        children.push(self.parse_expression());
        if self.at(TokenKind::Newline) {
            self.advance();
        }
        let end = self.peek().range.start;
        make_node(nk::MILESTONE_ENTRY, range_to(start, end), children)
    }

    fn parse_pool_decl(&mut self) -> SyntaxNode {
        let start = self.peek().range.start;
        self.advance(); // `pool`
        let mut children = Vec::new();
        if matches!(self.peek_kind(), TokenKind::Ident | TokenKind::Speaker) {
            let t = self.advance().clone();
            children.push(leaf(nk::IDENT, t.range, t.text));
        }
        self.expect(
            TokenKind::Newline,
            "doc-header-id",
            "Expected newline after pool name",
        );
        if self.at(TokenKind::Indent) {
            self.advance();
            while !self.at(TokenKind::Dedent) && !self.at_eof() {
                if self.at(TokenKind::Newline) {
                    self.advance();
                    continue;
                }
                children.push(self.parse_pool_property());
            }
            self.consume(TokenKind::Dedent);
        }
        let end = self.peek().range.start;
        make_node(nk::POOL_DECL, range_to(start, end), children)
    }

    fn parse_pool_property(&mut self) -> SyntaxNode {
        let start = self.peek().range.start;
        let mut children = Vec::new();
        if matches!(self.peek_kind(), TokenKind::Ident | TokenKind::Speaker) {
            let t = self.advance().clone();
            children.push(leaf(nk::IDENT, t.range, t.text));
        }
        self.consume(TokenKind::Eq); // optional
        children.push(self.parse_expression());
        // Optional `/ Duration` (rate denominator) and `when expr`.
        if self.consume(TokenKind::Slash) {
            children.push(self.parse_expression());
        }
        if self.at_word("when") {
            self.advance();
            children.push(self.parse_expression());
        }
        if self.at(TokenKind::Newline) {
            self.advance();
        }
        let end = self.peek().range.start;
        make_node(nk::POOL_PROPERTY, range_to(start, end), children)
    }

    fn parse_stat_decl(&mut self) -> SyntaxNode {
        let start = self.peek().range.start;
        self.advance(); // `stat`
        let mut children = Vec::new();
        if matches!(self.peek_kind(), TokenKind::Ident | TokenKind::Speaker) {
            let t = self.advance().clone();
            children.push(leaf(nk::IDENT, t.range, t.text));
        }
        // Four shapes (grammar §17.4):
        //   stat NAME = expr           (expression)
        //   stat NAME\n  lookup ...    (lookup)
        //   stat NAME pool max=N ...   (pool sugar)
        //   stat NAME derived from REFs\n  formula ...  (derived)
        if self.consume(TokenKind::Eq) {
            children.push(self.parse_expression());
        } else if self.peek().is_word("pool") {
            self.advance();
            // Slurp the rest of the line as pool sugar — the validator
            // splits it.
            let payload_start = self.peek().range.start;
            let mut raw = String::new();
            while !self.at(TokenKind::Newline) && !self.at_eof() {
                let t = self.advance();
                if !raw.is_empty() {
                    raw.push(' ');
                }
                raw.push_str(&t.text);
            }
            if !raw.is_empty() {
                let payload_end = self.peek().range.start;
                children.push(leaf(
                    nk::PROPERTY_VALUE,
                    range_to(payload_start, payload_end),
                    raw,
                ));
            }
        } else if self.peek().is_word("derived") {
            self.advance();
            if self.peek().is_word("from") {
                self.advance();
            }
            while !self.at(TokenKind::Newline) && !self.at_eof() {
                match self.peek_kind() {
                    TokenKind::Dollar | TokenKind::DollarBrace | TokenKind::DollarParen => {
                        children.push(self.parse_resolve_ref());
                    }
                    TokenKind::At => {
                        children.push(self.parse_static_ref());
                    }
                    TokenKind::Comma => {
                        self.advance();
                    }
                    _ => {
                        self.advance();
                    }
                }
            }
            if self.at(TokenKind::Newline) {
                self.advance();
            }
            if self.at(TokenKind::Indent) {
                self.advance();
                if self.peek().is_word("formula") {
                    let f_start = self.peek().range.start;
                    self.advance();
                    let expr = self.parse_expression();
                    let f_end = self.peek().range.start;
                    children.push(make_node(
                        nk::STAT_DERIVED,
                        range_to(f_start, f_end),
                        vec![expr],
                    ));
                }
                if self.at(TokenKind::Newline) {
                    self.advance();
                }
                self.consume(TokenKind::Dedent);
            }
        } else {
            // Lookup form — `stat NAME\n  lookup ...`.
            self.expect(
                TokenKind::Newline,
                "stat-form-ambiguous",
                "Expected newline after stat name",
            );
            if self.at(TokenKind::Indent) {
                self.advance();
                while !self.at(TokenKind::Dedent) && !self.at_eof() {
                    if self.at(TokenKind::Newline) {
                        self.advance();
                        continue;
                    }
                    children.push(self.parse_lookup_property());
                }
                self.consume(TokenKind::Dedent);
            }
        }
        if self.at(TokenKind::Newline) {
            self.advance();
        }
        let end = self.peek().range.start;
        make_node(nk::STAT_DECL, range_to(start, end), children)
    }

    fn parse_lookup_property(&mut self) -> SyntaxNode {
        let start = self.peek().range.start;
        let mut children = Vec::new();
        if matches!(self.peek_kind(), TokenKind::Ident | TokenKind::Speaker) {
            let t = self.advance().clone();
            let k = t.text.clone();
            children.push(leaf(nk::IDENT, t.range, t.text));
            match k.as_str() {
                "lookup" => {
                    children.push(self.parse_expression());
                }
                "table" => {
                    if self.at(TokenKind::LBrace) {
                        children.push(self.parse_dict_literal());
                    }
                }
                "interpolate" => {
                    if matches!(self.peek_kind(), TokenKind::Ident | TokenKind::Speaker) {
                        let t = self.advance().clone();
                        children.push(leaf(nk::IDENT, t.range, t.text));
                    }
                }
                _ => {}
            }
        }
        if self.at(TokenKind::Newline) {
            self.advance();
        }
        let end = self.peek().range.start;
        make_node(nk::LOOKUP_PROPERTY, range_to(start, end), children)
    }

    fn parse_dict_literal(&mut self) -> SyntaxNode {
        let start = self.peek().range.start;
        self.advance(); // `{`
        let mut children = Vec::new();
        while !self.at(TokenKind::RBrace) && !self.at_eof() {
            let entry_start = self.peek().range.start;
            let key = match self.peek_kind() {
                TokenKind::Number => {
                    let t = self.advance().clone();
                    leaf(nk::NUMBER, t.range, t.text)
                }
                TokenKind::String => {
                    let t = self.advance().clone();
                    leaf(nk::STRING, t.range, t.text)
                }
                TokenKind::Ident | TokenKind::Speaker => {
                    let t = self.advance().clone();
                    leaf(nk::IDENT, t.range, t.text)
                }
                _ => self.error_node("unexpected-child", "Expected dict key"),
            };
            self.expect(
                TokenKind::Colon,
                "doc-header-id",
                "Expected `:` in dict entry",
            );
            let value = self.parse_expression();
            let entry_end = self.peek().range.start;
            children.push(make_node(
                nk::DICT_ENTRY,
                range_to(entry_start, entry_end),
                vec![key, value],
            ));
            if !self.consume(TokenKind::Comma) {
                break;
            }
        }
        self.expect(TokenKind::RBrace, "bracket-unbalanced", "Expected `}`");
        let end = self.peek().range.start;
        make_node(nk::DICT_LITERAL, range_to(start, end), children)
    }

    fn parse_tree_node_decl(&mut self) -> SyntaxNode {
        let start = self.peek().range.start;
        self.advance(); // `node`
        let mut children = Vec::new();
        if matches!(self.peek_kind(), TokenKind::Ident | TokenKind::Speaker) {
            let t = self.advance().clone();
            children.push(leaf(nk::IDENT, t.range, t.text));
        }
        self.expect(
            TokenKind::Newline,
            "doc-header-id",
            "Expected newline after node name",
        );
        if self.at(TokenKind::Indent) {
            self.advance();
            while !self.at(TokenKind::Dedent) && !self.at_eof() {
                if self.at(TokenKind::Newline) {
                    self.advance();
                    continue;
                }
                children.push(self.parse_tree_node_property());
            }
            self.consume(TokenKind::Dedent);
        }
        let end = self.peek().range.start;
        make_node(nk::TREE_NODE_DECL, range_to(start, end), children)
    }

    fn parse_tree_node_property(&mut self) -> SyntaxNode {
        let start = self.peek().range.start;
        let mut children = Vec::new();
        if matches!(self.peek_kind(), TokenKind::Ident | TokenKind::Speaker) {
            let t = self.advance().clone();
            let k = t.text.clone();
            children.push(leaf(nk::IDENT, t.range, t.text));
            match k.as_str() {
                "cost" => {
                    if self.at(TokenKind::LBrace) {
                        children.push(self.parse_dict_literal());
                    }
                }
                "requires" => {
                    children.push(self.parse_expression());
                }
                "effect" => {
                    children.push(self.parse_tree_effect());
                }
                "rank" => {
                    if self.at(TokenKind::Number) {
                        let t = self.advance().clone();
                        children.push(leaf(nk::NUMBER, t.range, t.text));
                    }
                }
                _ => {}
            }
        }
        if self.at(TokenKind::Newline) {
            self.advance();
        }
        let end = self.peek().range.start;
        make_node(nk::TREE_NODE_PROPERTY, range_to(start, end), children)
    }

    fn parse_tree_effect(&mut self) -> SyntaxNode {
        let start = self.peek().range.start;
        let mut children = Vec::new();
        if matches!(self.peek_kind(), TokenKind::Ident | TokenKind::Speaker) {
            let t = self.advance().clone();
            let k = t.text.clone();
            children.push(leaf(nk::IDENT, t.range, t.text));
            match k.as_str() {
                "stat" | "attribute" | "var" | "pool" => {
                    if self.consume(TokenKind::LParen) {
                        if matches!(self.peek_kind(), TokenKind::Ident | TokenKind::Speaker) {
                            let t = self.advance().clone();
                            children.push(leaf(nk::IDENT, t.range, t.text));
                        }
                        self.expect(TokenKind::RParen, "bracket-unbalanced", "Expected `)`");
                    }
                    // op + value (e.g. `+ 5` or `:= expr` or `grant`).
                    while !self.at(TokenKind::Newline) && !self.at_eof() {
                        let t = self.advance().clone();
                        children.push(leaf(nk::IDENT, t.range, t.text));
                    }
                }
                "ability" | "luau" => {
                    if self.at(TokenKind::At) {
                        children.push(self.parse_static_ref());
                    }
                }
                _ => {}
            }
        }
        let end = self.peek().range.start;
        make_node(nk::TREE_EFFECT, range_to(start, end), children)
    }
}

// ═══════════════════════════════════════════════════════════════════
// §18. Reactivity — generator / scene / compose
// ═══════════════════════════════════════════════════════════════════

impl Parser<'_> {
    fn parse_generator_decl(&mut self) -> SyntaxNode {
        let start = self.peek().range.start;
        self.advance(); // `generator`
        let mut children = Vec::new();
        if matches!(self.peek_kind(), TokenKind::Ident | TokenKind::Speaker) {
            let t = self.advance().clone();
            children.push(leaf(nk::IDENT, t.range, t.text));
        }
        if self.at(TokenKind::Pipe) {
            children.push(self.parse_param_list());
        }
        self.expect(
            TokenKind::Newline,
            "doc-header-id",
            "Expected newline after generator header",
        );
        if self.at(TokenKind::Indent) {
            self.advance();
            // Optional scheduler properties first.
            while self.at(TokenKind::Dot) {
                children.push(self.parse_scheduler_property());
            }
            // Generator body items.
            while !self.at(TokenKind::Dedent) && !self.at_eof() {
                if self.at(TokenKind::Newline) {
                    self.advance();
                    continue;
                }
                children.push(self.parse_generator_item());
            }
            self.consume(TokenKind::Dedent);
        }
        let end = self.peek().range.start;
        make_node(nk::GENERATOR_DECL, range_to(start, end), children)
    }

    fn parse_scene_decl(&mut self) -> SyntaxNode {
        let start = self.peek().range.start;
        self.advance(); // `scene`
        let mut children = Vec::new();
        if matches!(self.peek_kind(), TokenKind::Ident | TokenKind::Speaker) {
            let t = self.advance().clone();
            children.push(leaf(nk::IDENT, t.range, t.text));
        }
        if self.at(TokenKind::Pipe) {
            children.push(self.parse_param_list());
        }
        self.expect(
            TokenKind::Newline,
            "doc-header-id",
            "Expected newline after scene header",
        );
        if self.at(TokenKind::Indent) {
            self.advance();
            // Optional scheduler properties.
            while self.at(TokenKind::Dot) {
                children.push(self.parse_scheduler_property());
            }
            // Scene states: IDENT NL INDENT body DEDENT
            while !self.at(TokenKind::Dedent) && !self.at_eof() {
                if self.at(TokenKind::Newline) {
                    self.advance();
                    continue;
                }
                children.push(self.parse_scene_state());
            }
            self.consume(TokenKind::Dedent);
        }
        let end = self.peek().range.start;
        make_node(nk::SCENE_DECL, range_to(start, end), children)
    }

    fn parse_scene_state(&mut self) -> SyntaxNode {
        let start = self.peek().range.start;
        let mut children = Vec::new();
        if matches!(self.peek_kind(), TokenKind::Ident | TokenKind::Speaker) {
            let t = self.advance().clone();
            children.push(leaf(nk::IDENT, t.range, t.text));
        }
        self.expect(
            TokenKind::Newline,
            "doc-header-id",
            "Expected newline after scene state name",
        );
        if self.at(TokenKind::Indent) {
            self.advance();
            while !self.at(TokenKind::Dedent) && !self.at_eof() {
                if self.at(TokenKind::Newline) {
                    self.advance();
                    continue;
                }
                if self.at_word("return") {
                    children.push(self.parse_return_stmt());
                } else {
                    children.push(self.parse_generator_item());
                }
            }
            self.consume(TokenKind::Dedent);
        }
        let end = self.peek().range.start;
        make_node(nk::SCENE_STATE, range_to(start, end), children)
    }

    fn parse_return_stmt(&mut self) -> SyntaxNode {
        let start = self.peek().range.start;
        self.advance(); // `return`
        let mut children = Vec::new();
        if !self.at(TokenKind::Newline) && !self.at_eof() {
            children.push(self.parse_expression());
        }
        if self.at(TokenKind::Newline) {
            self.advance();
        }
        let end = self.peek().range.start;
        make_node(nk::RETURN_STMT, range_to(start, end), children)
    }

    fn parse_scheduler_property(&mut self) -> SyntaxNode {
        let start = self.peek().range.start;
        self.advance(); // `.`
        let mut children = Vec::new();
        if matches!(self.peek_kind(), TokenKind::Ident | TokenKind::Speaker) {
            let t = self.advance().clone();
            children.push(leaf(nk::IDENT, t.range, t.text));
        }
        // Value — single token or expression.
        if !self.at(TokenKind::Newline) && !self.at_eof() {
            let val_start = self.peek().range.start;
            let mut raw = String::new();
            while !self.at(TokenKind::Newline) && !self.at_eof() {
                let t = self.advance();
                if !raw.is_empty() {
                    raw.push(' ');
                }
                raw.push_str(&t.text);
            }
            if !raw.is_empty() {
                let val_end = self.peek().range.start;
                children.push(leaf(nk::PROPERTY_VALUE, range_to(val_start, val_end), raw));
            }
        }
        if self.at(TokenKind::Newline) {
            self.advance();
        }
        let end = self.peek().range.start;
        make_node(nk::SCHEDULER_PROPERTY, range_to(start, end), children)
    }

    /// One item inside a generator or scene-state body. Generator
    /// items reuse the conversation vocabulary plus the coroutine
    /// primitives (loop / wait / yield / time).
    fn parse_generator_item(&mut self) -> SyntaxNode {
        if self.at_word("loop") {
            return self.parse_loop_block();
        }
        if self.at_word("wait") {
            return self.parse_wait_stmt();
        }
        if self.at_word("yield") {
            return self.parse_yield_stmt();
        }
        if self.at_word("at") {
            // `at TIMESPEC` (e.g. `at 6am`, `at 14:30`)
            return self.parse_time_stmt_at();
        }
        if self.at_word("every") {
            return self.parse_time_stmt_every();
        }
        if self.at_word("spawn") {
            return self.parse_action_line();
        }
        if self.at_word("cancel") {
            return self.parse_action_line();
        }
        // Otherwise fall through to a plain content line.
        self.parse_content_line()
    }

    fn parse_loop_block(&mut self) -> SyntaxNode {
        let start = self.peek().range.start;
        self.advance(); // `loop`
        let mut children = Vec::new();
        if self.at(TokenKind::Number) {
            let t = self.advance().clone();
            children.push(leaf(nk::NUMBER, t.range, t.text));
        } else if self.peek().is_word("forever") {
            let t = self.advance().clone();
            children.push(leaf(nk::IDENT, t.range, t.text));
        }
        self.expect(
            TokenKind::Newline,
            "doc-header-id",
            "Expected newline after `loop`",
        );
        if self.at(TokenKind::Indent) {
            self.advance();
            while !self.at(TokenKind::Dedent) && !self.at_eof() {
                if self.at(TokenKind::Newline) {
                    self.advance();
                    continue;
                }
                children.push(self.parse_generator_item());
            }
            self.consume(TokenKind::Dedent);
        }
        let end = self.peek().range.start;
        make_node(nk::LOOP_BLOCK, range_to(start, end), children)
    }

    fn parse_wait_stmt(&mut self) -> SyntaxNode {
        let start = self.peek().range.start;
        self.advance(); // `wait`
        let mut children = Vec::new();
        if self.peek().is_word("until") {
            self.advance();
            children.push(self.parse_expression());
        } else {
            children.push(self.parse_expression());
        }
        if self.at(TokenKind::Newline) {
            self.advance();
        }
        let end = self.peek().range.start;
        make_node(nk::WAIT_STMT, range_to(start, end), children)
    }

    fn parse_yield_stmt(&mut self) -> SyntaxNode {
        let start = self.peek().range.start;
        self.advance(); // `yield`
        let mut children = Vec::new();
        if !self.at(TokenKind::Newline) && !self.at_eof() {
            // Either `bark from @REF`, `with_chance(N)`, or free content.
            if self.peek().is_word("bark") {
                let yb_start = self.peek().range.start;
                self.advance();
                if self.peek().is_word("from") {
                    self.advance();
                }
                let r = match self.peek_kind() {
                    TokenKind::At => self.parse_static_ref(),
                    _ => self.error_node("unknown-ref", "Expected @ref after `bark from`"),
                };
                let yb_end = self.peek().range.start;
                children.push(make_node(
                    nk::YIELD_BODY,
                    range_to(yb_start, yb_end),
                    vec![r],
                ));
            } else if self.peek().is_word("with_chance") {
                let yb_start = self.peek().range.start;
                self.advance();
                if self.consume(TokenKind::LParen) {
                    if self.at(TokenKind::Number) {
                        let t = self.advance().clone();
                        let yb_end = t.range.end;
                        let body_children = vec![leaf(nk::NUMBER, t.range, t.text)];
                        children.push(make_node(
                            nk::YIELD_BODY,
                            range_to(yb_start, yb_end),
                            body_children,
                        ));
                    }
                    self.expect(TokenKind::RParen, "bracket-unbalanced", "Expected `)`");
                }
            } else {
                // Free-form text.
                let body_start = self.peek().range.start;
                let body = self.collect_inline_text_until_newline();
                let body_end = self.peek().range.start;
                children.push(make_node(
                    nk::YIELD_BODY,
                    range_to(body_start, body_end),
                    vec![body],
                ));
            }
        }
        if self.at(TokenKind::Newline) {
            self.advance();
        }
        // Optional indented body.
        if self.at(TokenKind::Indent) {
            children.push(self.parse_indented_content_block());
        }
        let end = self.peek().range.start;
        make_node(nk::YIELD_STMT, range_to(start, end), children)
    }

    fn parse_time_stmt_at(&mut self) -> SyntaxNode {
        let start = self.peek().range.start;
        self.advance(); // `at`
        let mut children = Vec::new();
        // Time spec: `Nam`/`Npm`/`HH:MM`. The lexer already glued `6am`
        // as a single Number? Actually `6am` has `am` which is not a
        // duration suffix — so `am` becomes a separate Ident token.
        // We collect tokens until newline as a single time_spec leaf.
        let ts_start = self.peek().range.start;
        let mut raw = String::new();
        while !self.at(TokenKind::Newline) && !self.at_eof() {
            let t = self.advance();
            if !raw.is_empty() {
                raw.push(' ');
            }
            raw.push_str(&t.text);
        }
        if !raw.is_empty() {
            let ts_end = self.peek().range.start;
            children.push(leaf(nk::TIME_SPEC, range_to(ts_start, ts_end), raw));
        }
        if self.at(TokenKind::Newline) {
            self.advance();
        }
        let end = self.peek().range.start;
        make_node(nk::TIME_STMT, range_to(start, end), children)
    }

    fn parse_time_stmt_every(&mut self) -> SyntaxNode {
        let start = self.peek().range.start;
        self.advance(); // `every`
        let mut children = Vec::new();
        // `every random(a, b)` or `every duration`
        if self.peek().is_word("random") {
            let t = self.advance().clone();
            children.push(leaf(nk::IDENT, t.range, t.text));
            self.expect(TokenKind::LParen, "doc-header-id", "Expected `(`");
            children.push(self.parse_expression());
            self.expect(TokenKind::Comma, "doc-header-id", "Expected `,`");
            children.push(self.parse_expression());
            self.expect(TokenKind::RParen, "bracket-unbalanced", "Expected `)`");
        } else {
            children.push(self.parse_expression());
        }
        if self.at(TokenKind::Newline) {
            self.advance();
        }
        let end = self.peek().range.start;
        make_node(nk::TIME_STMT, range_to(start, end), children)
    }

    fn parse_compose_decl(&mut self) -> SyntaxNode {
        let start = self.peek().range.start;
        self.advance(); // `compose`
        let mut children = Vec::new();
        if matches!(self.peek_kind(), TokenKind::Ident | TokenKind::Speaker) {
            let t = self.advance().clone();
            children.push(leaf(nk::IDENT, t.range, t.text));
        }
        self.expect(
            TokenKind::Newline,
            "doc-header-id",
            "Expected newline after compose name",
        );
        if self.at(TokenKind::Indent) {
            self.advance();
            while !self.at(TokenKind::Dedent) && !self.at_eof() {
                if self.at(TokenKind::Newline) {
                    self.advance();
                    continue;
                }
                children.push(self.parse_pattern_block());
            }
            self.consume(TokenKind::Dedent);
        }
        let end = self.peek().range.start;
        make_node(nk::COMPOSE_DECL, range_to(start, end), children)
    }

    fn parse_pattern_block(&mut self) -> SyntaxNode {
        let start = self.peek().range.start;
        if self.at_word("pattern") {
            self.advance();
        }
        let mut children = Vec::new();
        // Optional resolve selector.
        if self.at(TokenKind::Dollar) {
            children.push(self.parse_resolve_ref());
        }
        self.expect(
            TokenKind::Newline,
            "doc-header-id",
            "Expected newline after `pattern`",
        );
        if self.at(TokenKind::Indent) {
            self.advance();
            while !self.at(TokenKind::Dedent) && !self.at_eof() {
                if self.at(TokenKind::Newline) {
                    self.advance();
                    continue;
                }
                children.push(self.parse_pattern_arm());
            }
            self.consume(TokenKind::Dedent);
        }
        let end = self.peek().range.start;
        make_node(nk::PATTERN_BLOCK, range_to(start, end), children)
    }

    fn parse_pattern_arm(&mut self) -> SyntaxNode {
        let start = self.peek().range.start;
        let mut children = Vec::new();
        // ArmDiscriminant: ident | number | num_range | `_`
        match self.peek_kind() {
            TokenKind::Number => {
                children.push(self.parse_num_range_or_expression());
            }
            TokenKind::Ident | TokenKind::Speaker => {
                let t = self.advance().clone();
                children.push(leaf(nk::IDENT, t.range, t.text));
            }
            _ => {
                let t = self.advance().clone();
                children.push(leaf(nk::IDENT, t.range, t.text));
            }
        }
        self.expect(
            TokenKind::Colon,
            "doc-header-id",
            "Expected `:` in pattern arm",
        );
        if self.at(TokenKind::String) {
            let t = self.advance().clone();
            children.push(leaf(nk::STRING, t.range, t.text));
        }
        // Optional `, weight: N`
        if self.consume(TokenKind::Comma) {
            if matches!(self.peek_kind(), TokenKind::Ident | TokenKind::Speaker) {
                let t = self.advance().clone();
                children.push(leaf(nk::IDENT, t.range, t.text));
            }
            self.consume(TokenKind::Colon);
            if self.at(TokenKind::Number) {
                let t = self.advance().clone();
                children.push(leaf(nk::NUMBER, t.range, t.text));
            }
        }
        if self.at(TokenKind::Newline) {
            self.advance();
        }
        let end = self.peek().range.start;
        make_node(nk::PATTERN_ARM, range_to(start, end), children)
    }
}

// ═══════════════════════════════════════════════════════════════════
// §19. Faction archetype
// ═══════════════════════════════════════════════════════════════════

impl Parser<'_> {
    fn parse_inline_faction_decl(&mut self) -> SyntaxNode {
        let start = self.peek().range.start;
        self.advance(); // `faction`
        let mut children = Vec::new();
        if matches!(self.peek_kind(), TokenKind::Ident | TokenKind::Speaker) {
            let t = self.advance().clone();
            children.push(leaf(nk::IDENT, t.range, t.text));
        }
        self.expect(
            TokenKind::Newline,
            "doc-header-id",
            "Expected newline after faction name",
        );
        if self.at(TokenKind::Indent) {
            self.advance();
            while !self.at(TokenKind::Dedent) && !self.at_eof() {
                if self.at(TokenKind::Newline) {
                    self.advance();
                    continue;
                }
                children.push(self.parse_faction_body_item());
            }
            self.consume(TokenKind::Dedent);
        }
        let end = self.peek().range.start;
        make_node(nk::INLINE_FACTION_DECL, range_to(start, end), children)
    }

    fn parse_faction_body_item(&mut self) -> SyntaxNode {
        // Inside a faction's body, the same set of items may appear at
        // the top-level of a `:faction` document. We delegate.
        if self.at_word("members") {
            return self.parse_members_block();
        }
        if self.at_word("state") {
            return self.parse_state_block();
        }
        if self.at_word("stance") {
            return self.parse_stance_block();
        }
        if self.at_word("goal") {
            return self.parse_goal_decl();
        }
        if self.at_word("generator") {
            return self.parse_generator_decl();
        }
        if self.at_word("scene") {
            return self.parse_scene_decl();
        }
        if self.at_word("on") {
            return self.parse_hook_decl();
        }
        if self.at(TokenKind::Dot) {
            return self.parse_property();
        }
        if self.at_word("let") {
            return self.parse_let_binding();
        }
        // Bail to a content-line-ish fallback.
        self.parse_content_line()
    }

    fn parse_members_block(&mut self) -> SyntaxNode {
        let start = self.peek().range.start;
        self.advance(); // `members`
        self.expect(
            TokenKind::Newline,
            "doc-header-id",
            "Expected newline after `members`",
        );
        let mut children = Vec::new();
        if self.at(TokenKind::Indent) {
            self.advance();
            while !self.at(TokenKind::Dedent) && !self.at_eof() {
                if self.at(TokenKind::Newline) {
                    self.advance();
                    continue;
                }
                children.push(self.parse_member_entry());
            }
            self.consume(TokenKind::Dedent);
        }
        let end = self.peek().range.start;
        make_node(nk::MEMBERS_BLOCK, range_to(start, end), children)
    }

    fn parse_member_entry(&mut self) -> SyntaxNode {
        let start = self.peek().range.start;
        let mut children = Vec::new();

        // `cohort IDENT` form.
        if self.peek().is_word("cohort") {
            let t = self.advance().clone();
            children.push(leaf(nk::IDENT, t.range, t.text));
            if matches!(self.peek_kind(), TokenKind::Ident | TokenKind::Speaker) {
                let t = self.advance().clone();
                children.push(leaf(nk::IDENT, t.range, t.text));
            }
        }
        // `match expr` form.
        else if self.peek().is_word("match") {
            let t = self.advance().clone();
            children.push(leaf(nk::IDENT, t.range, t.text));
            children.push(self.parse_expression());
        }
        // Explicit StaticRef list.
        else {
            while self.at(TokenKind::At) {
                children.push(self.parse_static_ref());
                if !self.consume(TokenKind::Comma) {
                    break;
                }
            }
        }

        // Optional `visibility: <level>`
        if self.at_word("visibility") {
            children.push(self.parse_visibility_clause());
        }

        if self.at(TokenKind::Newline) {
            self.advance();
        }
        let end = self.peek().range.start;
        make_node(nk::MEMBER_ENTRY, range_to(start, end), children)
    }

    fn parse_visibility_clause(&mut self) -> SyntaxNode {
        let start = self.peek().range.start;
        self.advance(); // `visibility`
        self.expect(
            TokenKind::Colon,
            "doc-header-id",
            "Expected `:` in visibility clause",
        );
        let mut children = Vec::new();
        let lvl_start = self.peek().range.start;
        let mut lvl_children = Vec::new();
        if matches!(self.peek_kind(), TokenKind::Ident | TokenKind::Speaker) {
            let t = self.advance().clone();
            lvl_children.push(leaf(nk::IDENT, t.range, t.text));
            // Optional `(arg)` for cohort / participant / faction forms.
            if self.at(TokenKind::LParen) {
                self.advance();
                while !self.at(TokenKind::RParen) && !self.at_eof() {
                    lvl_children.push(self.parse_expression());
                    if !self.consume(TokenKind::Comma) {
                        break;
                    }
                }
                self.expect(TokenKind::RParen, "bracket-unbalanced", "Expected `)`");
            }
        }
        let lvl_end = self.peek().range.start;
        children.push(make_node(
            nk::VISIBILITY_LEVEL,
            range_to(lvl_start, lvl_end),
            lvl_children,
        ));
        let end = self.peek().range.start;
        make_node(nk::VISIBILITY_CLAUSE, range_to(start, end), children)
    }

    fn parse_state_block(&mut self) -> SyntaxNode {
        let start = self.peek().range.start;
        self.advance(); // `state`
        self.expect(
            TokenKind::Newline,
            "doc-header-id",
            "Expected newline after `state`",
        );
        let mut children = Vec::new();
        if self.at(TokenKind::Indent) {
            self.advance();
            while !self.at(TokenKind::Dedent) && !self.at_eof() {
                if self.at(TokenKind::Newline) {
                    self.advance();
                    continue;
                }
                children.push(self.parse_state_axis());
            }
            self.consume(TokenKind::Dedent);
        }
        let end = self.peek().range.start;
        make_node(nk::STATE_BLOCK, range_to(start, end), children)
    }

    fn parse_state_axis(&mut self) -> SyntaxNode {
        let start = self.peek().range.start;
        let mut children = Vec::new();
        if matches!(self.peek_kind(), TokenKind::Ident | TokenKind::Speaker) {
            let t = self.advance().clone();
            children.push(leaf(nk::IDENT, t.range, t.text));
        }
        self.expect(TokenKind::Eq, "doc-header-id", "Expected `=` in state axis");
        children.push(self.parse_num_range_or_expression());
        while self.consume(TokenKind::Comma) {
            if self.at_word("init") {
                self.advance();
                if self.at(TokenKind::Number) {
                    let t = self.advance().clone();
                    children.push(leaf("init", t.range, t.text));
                }
            } else if self.at_word("mirror") {
                let mc_start = self.peek().range.start;
                self.advance();
                let target = match self.peek_kind() {
                    TokenKind::Dollar => self.parse_resolve_ref(),
                    TokenKind::At => self.parse_static_ref(),
                    _ => self.error_node("disposition-mirror-cycle", "Expected mirror target"),
                };
                let mc_end = self.peek().range.start;
                children.push(make_node(
                    nk::MIRROR_CLAUSE,
                    range_to(mc_start, mc_end),
                    vec![target],
                ));
            } else {
                self.advance();
            }
        }
        if self.at(TokenKind::Newline) {
            self.advance();
        }
        let end = self.peek().range.start;
        make_node(nk::STATE_AXIS, range_to(start, end), children)
    }

    fn parse_stance_block(&mut self) -> SyntaxNode {
        let start = self.peek().range.start;
        self.advance(); // `stance`
        self.expect(
            TokenKind::Newline,
            "doc-header-id",
            "Expected newline after `stance`",
        );
        let mut children = Vec::new();
        if self.at(TokenKind::Indent) {
            self.advance();
            while !self.at(TokenKind::Dedent) && !self.at_eof() {
                if self.at(TokenKind::Newline) {
                    self.advance();
                    continue;
                }
                children.push(self.parse_stance_entry());
            }
            self.consume(TokenKind::Dedent);
        }
        let end = self.peek().range.start;
        make_node(nk::STANCE_BLOCK, range_to(start, end), children)
    }

    fn parse_stance_entry(&mut self) -> SyntaxNode {
        let start = self.peek().range.start;
        let mut children = Vec::new();
        // StanceTarget: @ref or `default`
        match self.peek_kind() {
            TokenKind::At => children.push(self.parse_static_ref()),
            _ => {
                if matches!(self.peek_kind(), TokenKind::Ident | TokenKind::Speaker) {
                    let t = self.advance().clone();
                    children.push(leaf(nk::IDENT, t.range, t.text));
                }
            }
        }
        self.expect(
            TokenKind::Eq,
            "doc-header-id",
            "Expected `=` in stance entry",
        );
        // Stance level (open word).
        if matches!(self.peek_kind(), TokenKind::Ident | TokenKind::Speaker) {
            let t = self.advance().clone();
            children.push(leaf(nk::STANCE_LEVEL, t.range, t.text));
        }
        // Optional `asymmetric`.
        if self.peek().is_word("asymmetric") {
            let t = self.advance().clone();
            children.push(leaf("asymmetric", t.range, t.text));
        }
        if self.at(TokenKind::Newline) {
            self.advance();
        }
        let end = self.peek().range.start;
        make_node(nk::STANCE_ENTRY, range_to(start, end), children)
    }
}

// ═══════════════════════════════════════════════════════════════════
// §8 / §9. S-expressions + let-binding + import/export
// ═══════════════════════════════════════════════════════════════════

impl Parser<'_> {
    fn parse_let_binding(&mut self) -> SyntaxNode {
        let start = self.peek().range.start;
        self.advance(); // `let`
        let mut children = Vec::new();
        if matches!(self.peek_kind(), TokenKind::Ident | TokenKind::Speaker) {
            let t = self.advance().clone();
            children.push(leaf(nk::IDENT, t.range, t.text));
        }
        self.expect(
            TokenKind::Eq,
            "assign-op-mismatch",
            "Expected `=` in let binding",
        );
        children.push(self.parse_expression());
        if self.at(TokenKind::Newline) {
            self.advance();
        }
        let end = self.peek().range.start;
        make_node(nk::LET_BINDING, range_to(start, end), children)
    }

    /// Parse a parenthesised top-level form: `(list ...)`,
    /// `(relation ...)`, `(entity ...)`, `(define ...)`, `(defn ...)`,
    /// `(defmacro ...)`, `(import ...)`, `(export ...)`, or a generic
    /// s-expression.
    fn parse_sexp_form_or_decl(&mut self) -> SyntaxNode {
        let _start = self.peek().range.start;
        // Look at the head word inside the parens to pick the
        // production. We don't consume the `(` yet.
        let save = self.pos;
        self.advance(); // `(`
                        // Skip whitespace tokens — none expected inside, but defensive.
        let head_text = if matches!(self.peek_kind(), TokenKind::Ident | TokenKind::Speaker) {
            self.peek().text.clone()
        } else {
            String::new()
        };
        self.pos = save;

        match head_text.as_str() {
            "list" => self.parse_list_decl(),
            "relation" => self.parse_relation_decl(),
            "entity" => self.parse_entity_decl(),
            "define" => self.parse_define_decl(),
            "defn" => self.parse_function_def(),
            "defmacro" => self.parse_macro_def(),
            "import" => self.parse_import_decl(),
            "export" => self.parse_export_decl(),
            _ => self.parse_sexp(),
        }
    }

    fn parse_list_decl(&mut self) -> SyntaxNode {
        let start = self.peek().range.start;
        self.advance(); // `(`
        self.advance(); // `list`
        let mut children = Vec::new();
        if matches!(self.peek_kind(), TokenKind::Ident | TokenKind::Speaker) {
            let t = self.advance().clone();
            children.push(leaf(nk::IDENT, t.range, t.text));
        }
        self.expect(
            TokenKind::Colon,
            "doc-header-id",
            "Expected `:` in list decl",
        );
        while !self.at(TokenKind::RParen) && !self.at_eof() {
            if matches!(self.peek_kind(), TokenKind::Ident | TokenKind::Speaker) {
                let t = self.advance().clone();
                children.push(leaf(nk::IDENT, t.range, t.text));
            } else if self.at(TokenKind::LParen) {
                children.push(self.parse_sexp());
            } else {
                self.advance();
            }
            if !self.consume(TokenKind::Comma) && !self.at(TokenKind::RParen) {
                // Recover.
                break;
            }
        }
        self.expect(TokenKind::RParen, "bracket-unbalanced", "Expected `)`");
        let end = self.peek().range.start;
        make_node(nk::LIST_DECL, range_to(start, end), children)
    }

    fn parse_relation_decl(&mut self) -> SyntaxNode {
        let start = self.peek().range.start;
        self.advance(); // `(`
        self.advance(); // `relation`
        let mut children = Vec::new();
        if matches!(self.peek_kind(), TokenKind::Ident | TokenKind::Speaker) {
            let t = self.advance().clone();
            children.push(leaf(nk::IDENT, t.range, t.text));
        }
        self.expect(TokenKind::Colon, "doc-header-id", "Expected `:`");
        // Cardinality word.
        if matches!(self.peek_kind(), TokenKind::Ident | TokenKind::Speaker) {
            let t = self.advance().clone();
            children.push(leaf(nk::IDENT, t.range, t.text));
        }
        self.expect(TokenKind::RParen, "bracket-unbalanced", "Expected `)`");
        let end = self.peek().range.start;
        make_node(nk::RELATION_DECL, range_to(start, end), children)
    }

    fn parse_entity_decl(&mut self) -> SyntaxNode {
        let start = self.peek().range.start;
        self.advance(); // `(`
        self.advance(); // `entity`
        let mut children = Vec::new();
        if matches!(self.peek_kind(), TokenKind::Ident | TokenKind::Speaker) {
            let t = self.advance().clone();
            children.push(leaf(nk::IDENT, t.range, t.text));
        }
        // Optional spec — we accept any tokens up to `)`.
        while !self.at(TokenKind::RParen) && !self.at_eof() {
            children.push(self.parse_sexp_atom());
        }
        self.expect(TokenKind::RParen, "bracket-unbalanced", "Expected `)`");
        let end = self.peek().range.start;
        make_node(nk::ENTITY_DECL, range_to(start, end), children)
    }

    fn parse_define_decl(&mut self) -> SyntaxNode {
        let start = self.peek().range.start;
        self.advance(); // `(`
        self.advance(); // `define`
        let mut children = Vec::new();
        if matches!(self.peek_kind(), TokenKind::Ident | TokenKind::Speaker) {
            let t = self.advance().clone();
            children.push(leaf(nk::IDENT, t.range, t.text));
        }
        if self.consume(TokenKind::Eq) {
            children.push(self.parse_expression());
        } else {
            // Plain `(define name expr)` body form.
            while !self.at(TokenKind::RParen) && !self.at_eof() {
                children.push(self.parse_sexp_atom());
            }
        }
        self.expect(TokenKind::RParen, "bracket-unbalanced", "Expected `)`");
        let end = self.peek().range.start;
        make_node(nk::CONSTANT_DEF, range_to(start, end), children)
    }

    fn parse_function_def(&mut self) -> SyntaxNode {
        let start = self.peek().range.start;
        self.advance(); // `(`
        self.advance(); // `defn`
        let mut children = Vec::new();
        if matches!(self.peek_kind(), TokenKind::Ident | TokenKind::Speaker) {
            let t = self.advance().clone();
            children.push(leaf(nk::IDENT, t.range, t.text));
        }
        if self.at(TokenKind::Pipe) {
            children.push(self.parse_param_list());
        }
        // Body atoms until `)`.
        while !self.at(TokenKind::RParen) && !self.at_eof() {
            children.push(self.parse_sexp_atom());
        }
        self.expect(TokenKind::RParen, "bracket-unbalanced", "Expected `)`");
        let end = self.peek().range.start;
        make_node(nk::FUNCTION_DEF, range_to(start, end), children)
    }

    fn parse_macro_def(&mut self) -> SyntaxNode {
        let start = self.peek().range.start;
        self.advance(); // `(`
        self.advance(); // `defmacro`
        let mut children = Vec::new();
        if matches!(self.peek_kind(), TokenKind::Ident | TokenKind::Speaker) {
            let t = self.advance().clone();
            children.push(leaf(nk::IDENT, t.range, t.text));
        }
        if self.at(TokenKind::Pipe) {
            children.push(self.parse_param_list());
        }
        while !self.at(TokenKind::RParen) && !self.at_eof() {
            children.push(self.parse_sexp_atom());
        }
        self.expect(TokenKind::RParen, "bracket-unbalanced", "Expected `)`");
        let end = self.peek().range.start;
        make_node(nk::MACRO_DEF, range_to(start, end), children)
    }

    fn parse_param_list(&mut self) -> SyntaxNode {
        let start = self.peek().range.start;
        self.advance(); // `|`
        let mut children = Vec::new();
        while !self.at(TokenKind::Pipe) && !self.at_eof() {
            if matches!(self.peek_kind(), TokenKind::Ident | TokenKind::Speaker) {
                let t = self.advance().clone();
                children.push(leaf(nk::IDENT, t.range, t.text));
            }
            if !self.consume(TokenKind::Comma) {
                break;
            }
        }
        self.expect(
            TokenKind::Pipe,
            "bracket-unbalanced",
            "Expected `|` to close param list",
        );
        let end = self.peek().range.start;
        make_node(nk::PARAM_LIST, range_to(start, end), children)
    }

    fn parse_import_decl(&mut self) -> SyntaxNode {
        let start = self.peek().range.start;
        self.advance(); // `(`
        self.advance(); // `import`
        let mut children = Vec::new();
        if self.at(TokenKind::String) {
            let t = self.advance().clone();
            children.push(leaf(nk::STRING, t.range, t.text));
        }
        if self.consume(TokenKind::Colon) {
            // ImportSpec: `*` | IDENT (',' IDENT)*
            if self.at(TokenKind::Star) {
                let t = self.advance().clone();
                children.push(leaf(nk::IDENT, t.range, t.text));
            } else {
                while matches!(self.peek_kind(), TokenKind::Ident | TokenKind::Speaker) {
                    let t = self.advance().clone();
                    children.push(leaf(nk::IDENT, t.range, t.text));
                    if !self.consume(TokenKind::Comma) {
                        break;
                    }
                }
            }
        }
        self.expect(TokenKind::RParen, "bracket-unbalanced", "Expected `)`");
        let end = self.peek().range.start;
        make_node(nk::IMPORT_DECL, range_to(start, end), children)
    }

    fn parse_export_decl(&mut self) -> SyntaxNode {
        let start = self.peek().range.start;
        self.advance(); // `(`
        self.advance(); // `export`
        let mut children = Vec::new();
        while matches!(self.peek_kind(), TokenKind::Ident | TokenKind::Speaker) {
            let t = self.advance().clone();
            children.push(leaf(nk::IDENT, t.range, t.text));
            if !self.consume(TokenKind::Comma) {
                break;
            }
        }
        self.expect(TokenKind::RParen, "bracket-unbalanced", "Expected `)`");
        let end = self.peek().range.start;
        make_node(nk::EXPORT_DECL, range_to(start, end), children)
    }

    fn parse_sexp(&mut self) -> SyntaxNode {
        let start = self.peek().range.start;
        self.expect(TokenKind::LParen, "bracket-unbalanced", "Expected `(`");
        let mut children = Vec::new();
        while !self.at(TokenKind::RParen) && !self.at_eof() {
            children.push(self.parse_sexp_atom());
        }
        self.expect(TokenKind::RParen, "bracket-unbalanced", "Expected `)`");
        let end = self.peek().range.start;
        make_node(nk::SEXP, range_to(start, end), children)
    }

    fn parse_sexp_atom(&mut self) -> SyntaxNode {
        match self.peek_kind() {
            TokenKind::LParen => self.parse_sexp(),
            TokenKind::String => {
                let t = self.advance().clone();
                leaf(nk::STRING, t.range, t.text)
            }
            TokenKind::Number => {
                let t = self.advance().clone();
                leaf(nk::NUMBER, t.range, t.text)
            }
            TokenKind::Dollar => self.parse_resolve_ref(),
            TokenKind::At => self.parse_static_ref(),
            TokenKind::Ident | TokenKind::Speaker => {
                let t = self.advance().clone();
                leaf(nk::IDENT, t.range, t.text)
            }
            _ => {
                // Punctuation / operator inside an sexp body — capture as
                // an opaque atom.
                let t = self.advance().clone();
                leaf(nk::SEXP_ATOM, t.range, t.text)
            }
        }
    }
}

// ═══════════════════════════════════════════════════════════════════
// §10. Expression grammar (Pratt)
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Copy)]
struct BinOp {
    text: &'static str,
    precedence: u8,
    right_assoc: bool,
}

fn binop_for(token: &Token) -> Option<BinOp> {
    let p = match token.kind {
        TokenKind::Ident => match token.text.as_str() {
            "or" => Some(BinOp {
                text: "or",
                precedence: 1,
                right_assoc: false,
            }),
            "and" => Some(BinOp {
                text: "and",
                precedence: 2,
                right_assoc: false,
            }),
            "is" => Some(BinOp {
                text: "is",
                precedence: 4,
                right_assoc: false,
            }),
            "has" => Some(BinOp {
                text: "has",
                precedence: 6,
                right_assoc: false,
            }),
            "in" => Some(BinOp {
                text: "in",
                precedence: 6,
                right_assoc: false,
            }),
            _ => None,
        },
        TokenKind::IsNot => Some(BinOp {
            text: "is not",
            precedence: 4,
            right_assoc: false,
        }),
        TokenKind::HasNot => Some(BinOp {
            text: "has not",
            precedence: 6,
            right_assoc: false,
        }),
        TokenKind::EqEq => Some(BinOp {
            text: "==",
            precedence: 4,
            right_assoc: false,
        }),
        TokenKind::Neq => Some(BinOp {
            text: "!=",
            precedence: 4,
            right_assoc: false,
        }),
        TokenKind::Lt => Some(BinOp {
            text: "<",
            precedence: 5,
            right_assoc: false,
        }),
        TokenKind::Lte => Some(BinOp {
            text: "<=",
            precedence: 5,
            right_assoc: false,
        }),
        TokenKind::Gt => Some(BinOp {
            text: ">",
            precedence: 5,
            right_assoc: false,
        }),
        TokenKind::Gte => Some(BinOp {
            text: ">=",
            precedence: 5,
            right_assoc: false,
        }),
        TokenKind::Plus => Some(BinOp {
            text: "+",
            precedence: 7,
            right_assoc: false,
        }),
        TokenKind::Minus => Some(BinOp {
            text: "-",
            precedence: 7,
            right_assoc: false,
        }),
        TokenKind::Star => Some(BinOp {
            text: "*",
            precedence: 8,
            right_assoc: false,
        }),
        TokenKind::Slash => Some(BinOp {
            text: "/",
            precedence: 8,
            right_assoc: false,
        }),
        TokenKind::Percent => Some(BinOp {
            text: "%",
            precedence: 8,
            right_assoc: false,
        }),
        _ => None,
    };
    p
}

impl Parser<'_> {
    /// Top-level expression entry point. Returns one expression; calls
    /// to this function should be followed by something that
    /// terminates the expression (a newline, comma, `)`, etc.).
    fn parse_expression(&mut self) -> SyntaxNode {
        self.parse_binary_expression(0)
    }

    fn parse_binary_expression(&mut self, min_prec: u8) -> SyntaxNode {
        let mut lhs = self.parse_unary_expression();
        loop {
            let op = match binop_for(self.peek()) {
                Some(op) if op.precedence >= min_prec => op,
                _ => break,
            };
            let op_tok = self.advance().clone();
            let next_min = if op.right_assoc {
                op.precedence
            } else {
                op.precedence + 1
            };
            let rhs = self.parse_binary_expression(next_min);
            let start = lhs.position.map(|r| r.start).unwrap_or(op_tok.range.start);
            let end = rhs.position.map(|r| r.end).unwrap_or(op_tok.range.end);
            lhs = make_node(
                nk::BINARY_EXPR,
                range_to(start, end),
                vec![lhs, leaf("op", op_tok.range, op.text), rhs],
            );
        }
        lhs
    }

    fn parse_unary_expression(&mut self) -> SyntaxNode {
        if self.at_word("not") || self.at(TokenKind::Bang) || self.at(TokenKind::Minus) {
            let op_tok = self.advance().clone();
            let operand = self.parse_unary_expression();
            let start = op_tok.range.start;
            let end = operand.position.map(|r| r.end).unwrap_or(op_tok.range.end);
            return make_node(
                nk::UNARY_EXPR,
                range_to(start, end),
                vec![leaf("op", op_tok.range, op_tok.text), operand],
            );
        }
        self.parse_postfix_expression()
    }

    fn parse_postfix_expression(&mut self) -> SyntaxNode {
        let atom = self.parse_primary_expression();
        let mut tail: Vec<SyntaxNode> = Vec::new();
        loop {
            match self.peek_kind() {
                TokenKind::Dot => {
                    let dot = self.advance().clone();
                    if matches!(self.peek_kind(), TokenKind::Ident | TokenKind::Speaker) {
                        let id = self.advance().clone();
                        let range = range_to(dot.range.start, id.range.end);
                        tail.push(make_node(
                            nk::FIELD_ACCESS,
                            range,
                            vec![leaf(nk::IDENT, id.range, id.text)],
                        ));
                    }
                }
                TokenKind::SafeNav => {
                    let q = self.advance().clone();
                    if matches!(self.peek_kind(), TokenKind::Ident | TokenKind::Speaker) {
                        let id = self.advance().clone();
                        let range = range_to(q.range.start, id.range.end);
                        tail.push(make_node(
                            nk::SAFE_NAV,
                            range,
                            vec![leaf(nk::IDENT, id.range, id.text)],
                        ));
                    }
                }
                TokenKind::LBrack => {
                    let lb = self.advance().clone();
                    let expr = self.parse_expression();
                    let rb_end = if self.at(TokenKind::RBrack) {
                        self.advance().range.end
                    } else {
                        self.diag("bracket-unbalanced", Severity::Error, "Expected `]`");
                        self.peek().range.start
                    };
                    tail.push(make_node(
                        nk::INDEX_ACCESS,
                        range_to(lb.range.start, rb_end),
                        vec![expr],
                    ));
                }
                TokenKind::LParen => {
                    // Call only at end of chain (per grammar §10.1).
                    let lp = self.advance().clone();
                    let mut args = Vec::new();
                    while !self.at(TokenKind::RParen) && !self.at_eof() {
                        args.push(self.parse_expression());
                        if !self.consume(TokenKind::Comma) {
                            break;
                        }
                    }
                    let rp_end = if self.at(TokenKind::RParen) {
                        self.advance().range.end
                    } else {
                        self.diag("bracket-unbalanced", Severity::Error, "Expected `)`");
                        self.peek().range.start
                    };
                    tail.push(make_node(
                        nk::CALL_EXPR,
                        range_to(lp.range.start, rp_end),
                        args,
                    ));
                    // Calls can only appear at the end of the postfix
                    // chain — break the loop.
                    break;
                }
                _ => break,
            }
        }
        if tail.is_empty() {
            atom
        } else {
            let fallback = self.peek().range.start;
            let start = atom.position.map(|r| r.start).unwrap_or(fallback);
            let end = tail
                .last()
                .and_then(|n| n.position)
                .map(|r| r.end)
                .unwrap_or(start);
            let mut children = vec![atom];
            children.extend(tail);
            make_node(nk::POSTFIX_EXPR, range_to(start, end), children)
        }
    }

    fn parse_primary_expression(&mut self) -> SyntaxNode {
        match self.peek_kind() {
            TokenKind::Number => {
                let t = self.advance().clone();
                let mut n = leaf(nk::NUMBER, t.range, t.text);
                if let Some(unit) = t.duration_unit {
                    n.data.insert(
                        "unit".to_string(),
                        JsonValue::String(unit.as_str().to_string()),
                    );
                }
                n
            }
            TokenKind::String => {
                let t = self.advance().clone();
                leaf(nk::STRING, t.range, t.text)
            }
            TokenKind::Dollar | TokenKind::DollarBrace | TokenKind::DollarParen => {
                self.parse_resolve_ref()
            }
            TokenKind::At => self.parse_static_ref(),
            TokenKind::LBrack => {
                // List literal OR list comprehension. Disambiguate by
                // peeking past the first expression for `for`.
                self.parse_list_or_comprehension()
            }
            TokenKind::LParen => {
                let lp = self.advance().clone();
                let inner = self.parse_expression();
                let end = if self.at(TokenKind::RParen) {
                    self.advance().range.end
                } else {
                    self.diag("bracket-unbalanced", Severity::Error, "Expected `)`");
                    self.peek().range.start
                };
                make_node(nk::GROUPED_EXPR, range_to(lp.range.start, end), vec![inner])
            }
            TokenKind::Ident | TokenKind::Speaker => {
                let t = self.advance().clone();
                match t.text.as_str() {
                    "true" | "false" => leaf(nk::BOOLEAN, t.range, t.text),
                    "nil" => leaf(nk::NIL, t.range, t.text),
                    // Ledger predicates: played(...) / visits(...) etc.
                    // `last` and `first` and `count` overlap with the
                    // aggregate builtins; we keep them in this arm and
                    // tag the resulting node `ledger_pred` since both
                    // builtins share the `head(arg)` shape — downstream
                    // resolution (validator / LSP) decides which based
                    // on the argument shape and the calling context.
                    "played" | "visits" | "chose" | "since" => self.parse_ledger_pred_after_head(t),
                    "any" | "all" | "count" | "min" | "max" | "closest" | "first" | "last" => {
                        if self.at(TokenKind::LParen) {
                            self.parse_aggregate_after_head(t)
                        } else {
                            leaf(nk::IDENT, t.range, t.text)
                        }
                    }
                    _ => leaf(nk::IDENT, t.range, t.text),
                }
            }
            _ => {
                let err = self.error_node(
                    "unexpected-child",
                    format!("Unexpected token `{}` in expression", self.peek().text),
                );
                // Advance past the offending token so the caller can't
                // recurse on the same position.
                if !self.at_eof()
                    && !matches!(
                        self.peek_kind(),
                        TokenKind::Newline
                            | TokenKind::Dedent
                            | TokenKind::RParen
                            | TokenKind::RBrack
                            | TokenKind::RBrace
                    )
                {
                    self.advance();
                }
                err
            }
        }
    }

    fn parse_ledger_pred_after_head(&mut self, head: Token) -> SyntaxNode {
        let start = head.range.start;
        let mut children = vec![leaf(nk::IDENT, head.range, head.text)];
        if self.consume(TokenKind::LParen) {
            if matches!(self.peek_kind(), TokenKind::Ident | TokenKind::Speaker) {
                let t = self.advance().clone();
                children.push(leaf(nk::IDENT, t.range, t.text));
            }
            self.expect(TokenKind::RParen, "bracket-unbalanced", "Expected `)`");
        }
        let end = self.peek().range.start;
        make_node(nk::LEDGER_PRED, range_to(start, end), children)
    }

    fn parse_aggregate_after_head(&mut self, head: Token) -> SyntaxNode {
        let start = head.range.start;
        let mut children = vec![leaf(nk::IDENT, head.range, head.text)];
        self.advance(); // `(`
        children.push(self.parse_expression());
        self.expect(TokenKind::RParen, "bracket-unbalanced", "Expected `)`");
        let end = self.peek().range.start;
        make_node(nk::AGGREGATE_CALL, range_to(start, end), children)
    }

    fn parse_list_or_comprehension(&mut self) -> SyntaxNode {
        let lb = self.advance().clone(); // `[`
                                         // Try parsing an expression; if followed by `for`, it's a
                                         // comprehension.
        if self.at(TokenKind::RBrack) {
            let rb = self.advance().clone();
            return make_node(
                "list_literal",
                range_to(lb.range.start, rb.range.end),
                Vec::new(),
            );
        }
        let first = self.parse_expression();
        if self.at_word("for") {
            self.advance();
            let var_tok = self.advance().clone();
            let var = leaf(nk::IDENT, var_tok.range, var_tok.text);
            self.consume_word("in");
            let iter = self.parse_expression();
            let mut children = vec![first, var, iter];
            if self.at_word("where") {
                self.advance();
                children.push(self.parse_expression());
            }
            let rb_end = if self.at(TokenKind::RBrack) {
                self.advance().range.end
            } else {
                self.diag("bracket-unbalanced", Severity::Error, "Expected `]`");
                self.peek().range.start
            };
            return make_node(
                nk::LIST_COMPREHENSION,
                range_to(lb.range.start, rb_end),
                children,
            );
        }

        let mut children = vec![first];
        while self.consume(TokenKind::Comma) {
            children.push(self.parse_expression());
        }
        let rb_end = if self.at(TokenKind::RBrack) {
            self.advance().range.end
        } else {
            self.diag("bracket-unbalanced", Severity::Error, "Expected `]`");
            self.peek().range.start
        };
        make_node("list_literal", range_to(lb.range.start, rb_end), children)
    }
}

// ═══════════════════════════════════════════════════════════════════
// §11. Sigils — resolve / static / inline assign / backlink
// ═══════════════════════════════════════════════════════════════════

impl Parser<'_> {
    /// `$name`, `$name.field`, `$name?`, `${expr}`, `$(expr ...)`,
    /// `$var:foo`, `$entity:foo`, `$role:SPEAKER`, `$cohort:singers`.
    fn parse_resolve_ref(&mut self) -> SyntaxNode {
        let start = self.peek().range.start;
        match self.peek_kind() {
            TokenKind::DollarBrace => {
                self.advance(); // `${`
                let expr = self.parse_expression();
                let end = if self.at(TokenKind::RBrace) {
                    self.advance().range.end
                } else {
                    self.diag("bracket-unbalanced", Severity::Error, "Expected `}`");
                    self.peek().range.start
                };
                make_node(nk::INLINE_EVAL, range_to(start, end), vec![expr])
            }
            TokenKind::DollarParen => {
                self.advance(); // `$(`
                self.parse_sexp_body_until_rparen(start)
            }
            TokenKind::Dollar => {
                self.advance(); // `$`
                let mut children = Vec::new();
                if matches!(self.peek_kind(), TokenKind::Ident | TokenKind::Speaker) {
                    let t = self.advance().clone();
                    children.push(leaf(nk::IDENT, t.range, t.text));
                } else {
                    return self.error_node("unknown-var", "Expected identifier after `$`");
                }
                // Qualifier form: `$var:foo`, `$entity:foo`, ...
                if self.consume(TokenKind::Colon) {
                    if matches!(self.peek_kind(), TokenKind::Ident | TokenKind::Speaker) {
                        let t = self.advance().clone();
                        children.push(leaf(nk::IDENT, t.range, t.text));
                    }
                    let end = self.peek().range.start;
                    return make_node(nk::QUALIFIED_REF, range_to(start, end), children);
                }
                // Field chain.
                let mut chain_children = Vec::new();
                loop {
                    match self.peek_kind() {
                        TokenKind::Dot => {
                            let dot = self.advance().clone();
                            if matches!(self.peek_kind(), TokenKind::Ident | TokenKind::Speaker) {
                                let id = self.advance().clone();
                                let range = range_to(dot.range.start, id.range.end);
                                chain_children.push(make_node(
                                    nk::FIELD_ACCESS,
                                    range,
                                    vec![leaf(nk::IDENT, id.range, id.text)],
                                ));
                            }
                        }
                        TokenKind::SafeNav => {
                            let q = self.advance().clone();
                            if matches!(self.peek_kind(), TokenKind::Ident | TokenKind::Speaker) {
                                let id = self.advance().clone();
                                let range = range_to(q.range.start, id.range.end);
                                chain_children.push(make_node(
                                    nk::SAFE_NAV,
                                    range,
                                    vec![leaf(nk::IDENT, id.range, id.text)],
                                ));
                            }
                        }
                        TokenKind::LBrack => {
                            let lb = self.advance().clone();
                            let expr = self.parse_expression();
                            let rb_end = if self.at(TokenKind::RBrack) {
                                self.advance().range.end
                            } else {
                                self.diag("bracket-unbalanced", Severity::Error, "Expected `]`");
                                self.peek().range.start
                            };
                            chain_children.push(make_node(
                                nk::INDEX_ACCESS,
                                range_to(lb.range.start, rb_end),
                                vec![expr],
                            ));
                        }
                        _ => break,
                    }
                }
                if !chain_children.is_empty() {
                    let chain_start = chain_children
                        .first()
                        .and_then(|n| n.position)
                        .map(|r| r.start)
                        .unwrap_or(start);
                    let chain_end = chain_children
                        .last()
                        .and_then(|n| n.position)
                        .map(|r| r.end)
                        .unwrap_or(start);
                    children.push(make_node(
                        nk::FIELD_CHAIN,
                        range_to(chain_start, chain_end),
                        chain_children,
                    ));
                }
                // Presence check trailing `?`.
                if self.at(TokenKind::QMark) {
                    let q = self.advance().clone();
                    children.push(leaf(nk::PRESENCE_CHECK, q.range, "?"));
                }
                let end = self.peek().range.start;
                make_node(nk::RESOLVE_REF, range_to(start, end), children)
            }
            _ => self.error_node("unknown-var", "Expected `$` resolve reference"),
        }
    }

    fn parse_sexp_body_until_rparen(&mut self, start: Position) -> SyntaxNode {
        let mut children = Vec::new();
        while !self.at(TokenKind::RParen) && !self.at_eof() {
            children.push(self.parse_sexp_atom());
        }
        let end = if self.at(TokenKind::RParen) {
            self.advance().range.end
        } else {
            self.diag("bracket-unbalanced", Severity::Error, "Expected `)`");
            self.peek().range.start
        };
        make_node(nk::INLINE_EVAL, range_to(start, end), children)
    }

    fn parse_static_ref(&mut self) -> SyntaxNode {
        let start = self.peek().range.start;
        self.expect(
            TokenKind::At,
            "unknown-ref",
            "Expected `@` static reference",
        );
        let mut children = Vec::new();
        if matches!(self.peek_kind(), TokenKind::Ident | TokenKind::Speaker) {
            let t = self.advance().clone();
            children.push(leaf(nk::IDENT, t.range, t.text));
            while self.consume(TokenKind::Dot) {
                if matches!(self.peek_kind(), TokenKind::Ident | TokenKind::Speaker) {
                    let t = self.advance().clone();
                    children.push(leaf(nk::IDENT, t.range, t.text));
                } else {
                    break;
                }
            }
        } else {
            return self.error_node("unknown-ref", "Expected identifier after `@`");
        }
        let end = self.peek().range.start;
        make_node(nk::STATIC_REF, range_to(start, end), children)
    }
}

// ═══════════════════════════════════════════════════════════════════
// §12. Inline-text grammar
// ═══════════════════════════════════════════════════════════════════
//
// The inline-text parser kicks in inside dialogue lines, flavor lines,
// choice labels, parentheticals, and the body of text variations. It
// recognises the §12 atom set — backlinks, inline triggers (point,
// chain, conditional, ranged), range closers, inline assigns, inline
// eval, resolve / static refs, text variations — and falls back to a
// `LITERAL_RUN` for everything else (the source text is preserved
// verbatim modulo single-space normalisation).

impl Parser<'_> {
    /// Collect inline text up to the next `Newline`. Used for
    /// dialogue lines, flavor lines, and stage directions.
    fn collect_inline_text_until_newline(&mut self) -> SyntaxNode {
        self.parse_text_content(InlineStop::Newline)
    }

    /// Collect inline text for a choice label — stops at the line
    /// end, an inline `-> divert`, or an `if` guard.
    fn collect_inline_text_until_guard_or_newline(&mut self) -> SyntaxNode {
        self.parse_text_content(InlineStop::ChoiceLabel)
    }

    /// Collect inline text up to a closing `)`. Used for the body of
    /// a parenthetical.
    fn collect_inline_text_until_close_paren(&mut self) -> SyntaxNode {
        self.parse_text_content(InlineStop::CloseParen)
    }

    fn parse_text_content(&mut self, stop: InlineStop) -> SyntaxNode {
        let start = self.peek().range.start;
        let mut children: Vec<SyntaxNode> = Vec::new();
        let mut literal_acc = String::new();
        let mut literal_start: Option<Position> = None;
        let mut literal_end: Position = start;

        let flush_literal = |children: &mut Vec<SyntaxNode>,
                             literal_acc: &mut String,
                             literal_start: &mut Option<Position>,
                             literal_end: Position| {
            if let Some(s) = literal_start.take() {
                let text = std::mem::take(literal_acc);
                if !text.is_empty() {
                    children.push(leaf(nk::LITERAL_RUN, range_to(s, literal_end), text));
                }
            } else {
                literal_acc.clear();
            }
        };

        while !self.at_eof() && !self.should_stop_inline(&stop) {
            // Backlink — `[[ ... ]]`.
            if self.at(TokenKind::LBrack2) {
                flush_literal(
                    &mut children,
                    &mut literal_acc,
                    &mut literal_start,
                    literal_end,
                );
                children.push(self.parse_backlink());
                continue;
            }
            // Range closer — `</>` or `</%name>`.
            if self.at(TokenKind::LtSlashGt) {
                flush_literal(
                    &mut children,
                    &mut literal_acc,
                    &mut literal_start,
                    literal_end,
                );
                let t = self.advance().clone();
                children.push(leaf(nk::RANGE_CLOSER, t.range, t.text));
                continue;
            }
            if self.at(TokenKind::LtSlashPct) {
                flush_literal(
                    &mut children,
                    &mut literal_acc,
                    &mut literal_start,
                    literal_end,
                );
                children.push(self.parse_named_range_closer());
                continue;
            }
            // Conditional trigger — `<?expr><trig>`.
            if self.at(TokenKind::LtQMark) {
                flush_literal(
                    &mut children,
                    &mut literal_acc,
                    &mut literal_start,
                    literal_end,
                );
                children.push(self.parse_cond_trigger());
                continue;
            }
            // Inline assign — `<$x := expr>` (or `<$x.field += rhs>` etc.).
            if self.at(TokenKind::Lt) && self.peek_n(1).kind == TokenKind::Dollar {
                flush_literal(
                    &mut children,
                    &mut literal_acc,
                    &mut literal_start,
                    literal_end,
                );
                children.push(self.parse_inline_assign());
                continue;
            }
            // Inline trigger — `<type:args>` and variants (chain
            // triggers via `+`, range spec via ` for:N`, etc.). The
            // disambiguator: `<` followed by an identifier and then
            // either `:` or `+` is a trigger; `<` followed by anything
            // else is a literal less-than.
            if self.at(TokenKind::Lt)
                && matches!(self.peek_n(1).kind, TokenKind::Ident | TokenKind::Speaker)
                && matches!(self.peek_n(2).kind, TokenKind::Colon | TokenKind::Plus)
            {
                flush_literal(
                    &mut children,
                    &mut literal_acc,
                    &mut literal_start,
                    literal_end,
                );
                children.push(self.parse_inline_trigger());
                continue;
            }
            // Inline eval — `${...}` or `$(...)`.
            if self.at(TokenKind::DollarBrace) || self.at(TokenKind::DollarParen) {
                flush_literal(
                    &mut children,
                    &mut literal_acc,
                    &mut literal_start,
                    literal_end,
                );
                children.push(self.parse_resolve_ref());
                continue;
            }
            // Resolve ref — `$name(.field)*`.
            if self.at(TokenKind::Dollar) {
                flush_literal(
                    &mut children,
                    &mut literal_acc,
                    &mut literal_start,
                    literal_end,
                );
                children.push(self.parse_resolve_ref());
                continue;
            }
            // Static ref — `@name(.sub)*`.
            if self.at(TokenKind::At) {
                flush_literal(
                    &mut children,
                    &mut literal_acc,
                    &mut literal_start,
                    literal_end,
                );
                children.push(self.parse_static_ref());
                continue;
            }
            // Text variation — `[a / b / c].mode`. Distinguished from a
            // bare `[` literal by looking for the `/` separator inside.
            if self.at(TokenKind::LBrack) && self.looks_like_text_variation() {
                flush_literal(
                    &mut children,
                    &mut literal_acc,
                    &mut literal_start,
                    literal_end,
                );
                children.push(self.parse_text_variation());
                continue;
            }

            // Literal token: accumulate the text. We separate tokens
            // with a single space to match the lexer's normalisation
            // expectations (multiple whitespace runs collapse to one).
            // Apostrophes attach to the preceding word with no space,
            // so contractions ("I'll") survive readably.
            let t = self.advance().clone();
            if literal_start.is_none() {
                literal_start = Some(t.range.start);
            }
            let is_apostrophe = matches!(t.kind, TokenKind::Apostrophe);
            let attaches_back = is_apostrophe;
            let attaches_forward = is_apostrophe;
            if !literal_acc.is_empty() && !attaches_back && !ends_attaching(&literal_acc) {
                literal_acc.push(' ');
            }
            literal_acc.push_str(&t.text);
            if attaches_forward {
                // Don't pre-emit a space before the next token.
                // (Handled by `ends_attaching` in next iteration.)
            }
            literal_end = t.range.end;
        }

        flush_literal(
            &mut children,
            &mut literal_acc,
            &mut literal_start,
            literal_end,
        );

        let end = self.peek().range.start;
        make_node(nk::TEXT_CONTENT, range_to(start, end), children)
    }

    fn should_stop_inline(&self, stop: &InlineStop) -> bool {
        match stop {
            InlineStop::Newline => self.at(TokenKind::Newline),
            InlineStop::CloseParen => self.at(TokenKind::RParen),
            InlineStop::SlashOrCloseBrack => {
                self.at(TokenKind::Slash) || self.at(TokenKind::RBrack)
            }
            InlineStop::ChoiceLabel => {
                self.at(TokenKind::Newline)
                    || self.at(TokenKind::Arrow)
                    || self.peek().is_word("if")
            }
        }
    }

    fn parse_backlink(&mut self) -> SyntaxNode {
        let start = self.peek().range.start;
        self.advance(); // `[[`
        let mut children = Vec::new();

        // Optional type prefix: `type:`
        if matches!(self.peek_kind(), TokenKind::Ident | TokenKind::Speaker)
            && self.peek_n(1).kind == TokenKind::Colon
        {
            let t = self.advance().clone();
            children.push(leaf(nk::IDENT, t.range, t.text));
            self.advance(); // `:`
        }

        // Target — accumulate raw text up to `|` or `]]`.
        let target_start = self.peek().range.start;
        let mut target_text = String::new();
        let mut target_end = target_start;
        while !self.at(TokenKind::Pipe)
            && !self.at(TokenKind::RBrack2)
            && !self.at(TokenKind::Newline)
            && !self.at_eof()
        {
            let t = self.advance();
            if !target_text.is_empty() {
                target_text.push(' ');
            }
            target_text.push_str(&t.text);
            target_end = t.range.end;
        }
        if !target_text.is_empty() {
            children.push(leaf(
                nk::LITERAL_RUN,
                range_to(target_start, target_end),
                target_text,
            ));
        }

        // Optional display: `| ... `
        if self.consume(TokenKind::Pipe) {
            let disp_start = self.peek().range.start;
            let mut disp_text = String::new();
            let mut disp_end = disp_start;
            while !self.at(TokenKind::RBrack2) && !self.at(TokenKind::Newline) && !self.at_eof() {
                let t = self.advance();
                if !disp_text.is_empty() {
                    disp_text.push(' ');
                }
                disp_text.push_str(&t.text);
                disp_end = t.range.end;
            }
            if !disp_text.is_empty() {
                children.push(leaf(
                    nk::LITERAL_RUN,
                    range_to(disp_start, disp_end),
                    disp_text,
                ));
            }
        }

        self.expect(TokenKind::RBrack2, "bracket-unbalanced", "Expected `]]`");
        let end = self.peek().range.start;
        make_node(nk::BACKLINK, range_to(start, end), children)
    }

    fn parse_named_range_closer(&mut self) -> SyntaxNode {
        let start = self.peek().range.start;
        self.advance(); // `</%`
        let mut children = Vec::new();
        if matches!(self.peek_kind(), TokenKind::Ident | TokenKind::Speaker) {
            let t = self.advance().clone();
            children.push(leaf(nk::IDENT, t.range, t.text));
        }
        self.expect(
            TokenKind::Gt,
            "range-unclosed",
            "Expected `>` after named anchor",
        );
        let end = self.peek().range.start;
        make_node(nk::RANGE_CLOSER, range_to(start, end), children)
    }

    fn parse_cond_trigger(&mut self) -> SyntaxNode {
        let start = self.peek().range.start;
        self.advance(); // `<?`
        let mut children = vec![self.parse_expression()];
        self.expect(
            TokenKind::Gt,
            "range-unclosed",
            "Expected `>` after conditional expression",
        );
        // The following trigger.
        if self.at(TokenKind::Lt) {
            children.push(self.parse_inline_trigger());
        } else {
            children
                .push(self.error_node("unknown-trigger-kw", "Expected trigger after conditional"));
        }
        let end = self.peek().range.start;
        make_node(nk::COND_TRIGGER, range_to(start, end), children)
    }

    fn parse_inline_assign(&mut self) -> SyntaxNode {
        let start = self.peek().range.start;
        self.advance(); // `<`
        let lvalue = self.parse_resolve_ref();
        let mut children = vec![lvalue];
        // Assign operator.
        let op_tok = if matches!(
            self.peek_kind(),
            TokenKind::Walrus | TokenKind::PlusEq | TokenKind::MinusEq | TokenKind::PlusPlus
        ) {
            self.advance().clone()
        } else {
            // Defensive: emit an error but keep going.
            self.diag(
                "assign-op-mismatch",
                Severity::Error,
                "Expected assignment operator in inline assign",
            );
            self.advance().clone()
        };
        children.push(leaf("assign_op", op_tok.range, op_tok.text.clone()));
        if !matches!(op_tok.kind, TokenKind::PlusPlus) {
            children.push(self.parse_expression());
        }
        // Tolerate either `>` (proper close) or end-of-line.
        if self.at(TokenKind::Gt) {
            self.advance();
        }
        let end = self.peek().range.start;
        make_node(nk::INLINE_ASSIGN, range_to(start, end), children)
    }

    fn parse_inline_trigger(&mut self) -> SyntaxNode {
        let start = self.peek().range.start;
        self.advance(); // `<`
        let mut children = Vec::new();

        // First head: trigger type.
        let head = self.parse_trigger_head();
        children.push(head);

        // Chain triggers: `+ another:arg + …`
        let mut is_chain = false;
        while self.at(TokenKind::Plus) {
            is_chain = true;
            self.advance();
            children.push(self.parse_trigger_head());
        }

        // Named attrs + range spec — only for non-chain triggers.
        if !is_chain {
            while !self.at(TokenKind::Gt) && !self.at(TokenKind::Newline) && !self.at_eof() {
                // ` for:N` range spec
                if self.peek().is_word("for") && self.peek_n(1).kind == TokenKind::Colon {
                    let rs_start = self.peek().range.start;
                    self.advance(); // `for`
                    self.advance(); // `:`
                    let val = self.parse_expression();
                    let rs_end = self.peek().range.start;
                    children.push(make_node(
                        nk::TRIGGER_RANGE_SPEC,
                        range_to(rs_start, rs_end),
                        vec![val],
                    ));
                    continue;
                }
                // `%name` anchor spec
                if self.at(TokenKind::Percent) {
                    let rs_start = self.peek().range.start;
                    self.advance();
                    if matches!(self.peek_kind(), TokenKind::Ident | TokenKind::Speaker) {
                        let t = self.advance().clone();
                        children.push(make_node(
                            nk::TRIGGER_RANGE_SPEC,
                            range_to(rs_start, t.range.end),
                            vec![leaf(nk::IDENT, t.range, t.text)],
                        ));
                    }
                    continue;
                }
                // `attr:value` named attribute
                if matches!(self.peek_kind(), TokenKind::Ident | TokenKind::Speaker)
                    && self.peek_n(1).kind == TokenKind::Colon
                {
                    children.push(self.parse_trigger_attr());
                    continue;
                }
                // Fall through — unknown tail token. Eat to avoid an
                // infinite loop, but flag.
                self.diag(
                    "unknown-trigger-kw",
                    Severity::Warning,
                    format!(
                        "Unexpected token `{}` inside inline trigger",
                        self.peek().text
                    ),
                );
                self.advance();
            }
        }

        self.expect(
            TokenKind::Gt,
            "range-unclosed",
            "Expected `>` to close inline trigger",
        );
        let end = self.peek().range.start;
        let kind = if is_chain {
            nk::CHAIN_TRIGGER
        } else {
            nk::INLINE_TRIGGER
        };
        make_node(kind, range_to(start, end), children)
    }

    /// Parse a single `type:args` head used inside an inline trigger
    /// or a chain-trigger segment. Stops at `+`, `>`, or end-of-line.
    fn parse_trigger_head(&mut self) -> SyntaxNode {
        let start = self.peek().range.start;
        let mut children = Vec::new();
        if matches!(self.peek_kind(), TokenKind::Ident | TokenKind::Speaker) {
            let t = self.advance().clone();
            children.push(leaf(nk::IDENT, t.range, t.text));
        }
        self.expect(
            TokenKind::Colon,
            "doc-header-id",
            "Expected `:` in trigger head",
        );
        // Args — accumulate text up to `+` / `>` / `for:` / `%name` /
        // named-attr / newline.
        let args_start = self.peek().range.start;
        let mut args_text = String::new();
        let mut args_end = args_start;
        while !matches!(
            self.peek_kind(),
            TokenKind::Plus
                | TokenKind::Gt
                | TokenKind::Percent
                | TokenKind::Newline
                | TokenKind::Eof
        ) {
            if self.peek().is_word("for") && self.peek_n(1).kind == TokenKind::Colon {
                break;
            }
            if matches!(self.peek_kind(), TokenKind::Ident | TokenKind::Speaker)
                && self.peek_n(1).kind == TokenKind::Colon
            {
                // Lookahead: `name:` looks like a named attr starting
                // — stop the args here.
                break;
            }
            let t = self.advance();
            if !args_text.is_empty() {
                args_text.push(' ');
            }
            args_text.push_str(&t.text);
            args_end = t.range.end;
        }
        if !args_text.is_empty() {
            children.push(leaf(
                nk::LITERAL_RUN,
                range_to(args_start, args_end),
                args_text,
            ));
        }
        let end = self.peek().range.start;
        make_node("trigger_head", range_to(start, end), children)
    }

    fn parse_trigger_attr(&mut self) -> SyntaxNode {
        let start = self.peek().range.start;
        let key = self.advance().clone();
        self.advance(); // `:`
        let value = match self.peek_kind() {
            TokenKind::String => {
                let t = self.advance().clone();
                leaf(nk::STRING, t.range, t.text)
            }
            TokenKind::Number => {
                let t = self.advance().clone();
                leaf(nk::NUMBER, t.range, t.text)
            }
            TokenKind::Dollar | TokenKind::DollarBrace | TokenKind::DollarParen => {
                self.parse_resolve_ref()
            }
            TokenKind::At => self.parse_static_ref(),
            _ => {
                let t = self.advance().clone();
                leaf(nk::IDENT, t.range, t.text)
            }
        };
        let end = self.peek().range.start;
        make_node(
            nk::TRIGGER_ATTR,
            range_to(start, end),
            vec![leaf(nk::IDENT, key.range, key.text), value],
        )
    }

    /// Disambiguator for `[ a / b / c ].mode` vs. a stray `[`. We
    /// scan ahead within reasonable bounds — if we find a `/` before a
    /// `]` at the same bracket depth, it's a variation.
    fn looks_like_text_variation(&self) -> bool {
        let mut depth: i32 = 0;
        let mut i = self.pos;
        let max = self.tokens.len();
        let mut found_slash = false;
        while i < max {
            match self.tokens[i].kind {
                TokenKind::LBrack => depth += 1,
                TokenKind::RBrack => {
                    depth -= 1;
                    if depth == 0 {
                        return found_slash;
                    }
                }
                TokenKind::Slash if depth == 1 => found_slash = true,
                TokenKind::Newline | TokenKind::Eof => return false,
                _ => {}
            }
            i += 1;
        }
        false
    }

    fn parse_text_variation(&mut self) -> SyntaxNode {
        let start = self.peek().range.start;
        self.advance(); // `[`
        let mut children = Vec::new();
        loop {
            let variant_start = self.peek().range.start;
            let body = self.parse_text_content(InlineStop::SlashOrCloseBrack);
            let variant_end = self.peek().range.start;
            children.push(make_node(
                nk::VARIATION_VARIANT,
                range_to(variant_start, variant_end),
                vec![body],
            ));
            if self.at(TokenKind::Slash) {
                self.advance();
                continue;
            }
            break;
        }
        self.expect(
            TokenKind::RBrack,
            "bracket-unbalanced",
            "Expected `]` to close text variation",
        );

        // Optional mode: `.cycle`, `.shuffle`, `.weighted(0.7, 0.3)`,
        // etc.
        if self.at(TokenKind::Dot) {
            children.push(self.parse_variation_mode());
        }

        let end = self.peek().range.start;
        make_node(nk::TEXT_VARIATION, range_to(start, end), children)
    }

    fn parse_variation_mode(&mut self) -> SyntaxNode {
        let start = self.peek().range.start;
        self.advance(); // `.`
        let mut children = Vec::new();
        if matches!(self.peek_kind(), TokenKind::Ident | TokenKind::Speaker) {
            let t = self.advance().clone();
            children.push(leaf(nk::IDENT, t.range, t.text));
        }
        if self.at(TokenKind::LParen) {
            self.advance();
            while !self.at(TokenKind::RParen) && !self.at_eof() {
                children.push(self.parse_expression());
                if !self.consume(TokenKind::Comma) {
                    break;
                }
            }
            self.expect(TokenKind::RParen, "bracket-unbalanced", "Expected `)`");
        }
        let end = self.peek().range.start;
        make_node(nk::VARIATION_MODE, range_to(start, end), children)
    }
}

/// Stop predicate for the inline-text parser.
enum InlineStop {
    Newline,
    CloseParen,
    SlashOrCloseBrack,
    ChoiceLabel,
}

/// Whether the accumulated literal ends in a character that should
/// "attach" to the next token without a space (e.g. an apostrophe so
/// "I'll" reads correctly).
fn ends_attaching(acc: &str) -> bool {
    acc.ends_with('\'')
}

// ═══════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_ok(src: &str) -> ParseResult {
        let r = parse(src);
        for d in &r.diagnostics {
            eprintln!("[{}] {}: {}", d.severity.as_str(), d.id, d.message);
        }
        r
    }

    fn find_kind<'a>(node: &'a SyntaxNode, kind: &str) -> Option<&'a SyntaxNode> {
        if node.kind == kind {
            return Some(node);
        }
        for c in &node.children {
            if let Some(hit) = find_kind(c, kind) {
                return Some(hit);
            }
        }
        None
    }

    fn count_kind(node: &SyntaxNode, kind: &str) -> usize {
        let mut n = if node.kind == kind { 1 } else { 0 };
        for c in &node.children {
            n += count_kind(c, kind);
        }
        n
    }

    fn root_to_node(root: &RootNode) -> SyntaxNode {
        SyntaxNode {
            kind: "root".to_string(),
            position: root.position,
            children: root.children.clone(),
            value: None,
            data: IndexMap::new(),
        }
    }

    #[test]
    fn parses_empty_source_to_empty_root() {
        let r = parse_ok("");
        assert!(r.root.children.is_empty());
        assert!(r.diagnostics.is_empty());
    }

    #[test]
    fn parses_minimal_header() {
        let r = parse_ok("# tiny \"Hello\"\n");
        assert!(r.diagnostics.is_empty());
        let doc = &r.root.children[0];
        assert_eq!(doc.kind, nk::DOCUMENT);
        let header = find_kind(doc, nk::HEADER).expect("header");
        assert_eq!(header.children[0].kind, nk::IDENT);
        assert_eq!(header.children[0].value.as_deref(), Some("tiny"));
        assert_eq!(header.children[1].kind, nk::STRING);
    }

    #[test]
    fn parses_header_with_tags() {
        let r = parse_ok("# story \"Title\" :conversation :immersive\n");
        let header = find_kind(&r.root.children[0], nk::HEADER).expect("header");
        let tags: Vec<_> = header
            .children
            .iter()
            .filter(|c| c.kind == nk::DOC_TAG)
            .collect();
        assert_eq!(tags.len(), 2);
    }

    #[test]
    fn parses_properties_under_header() {
        let r = parse_ok("# story\n  .actors wren, hale\n  .tags chapter-1\n");
        let root = root_to_node(&r.root);
        assert_eq!(count_kind(&root, nk::PROPERTY), 2);
    }

    #[test]
    fn parses_docstring_after_properties() {
        let r = parse_ok("# story\n  .actors wren\n\n'''\nOpening.\n'''\n\n");
        let doc = &r.root.children[0];
        let ds = find_kind(doc, nk::DOCSTRING).expect("docstring");
        assert!(ds.value.as_deref().unwrap().contains("Opening"));
    }

    #[test]
    fn parses_cast_decl_with_properties() {
        let r = parse_ok("# story\n\ncast WREN\n  .label \"Wren\"\n  .voice female_mezzo\n");
        let cast = find_kind(&r.root.children[0], nk::CAST_DECL).expect("cast");
        assert_eq!(cast.children[0].kind, nk::SPEAKER);
        assert_eq!(cast.children[0].value.as_deref(), Some("WREN"));
        let props = count_kind(cast, nk::PROPERTY);
        assert_eq!(props, 2);
    }

    #[test]
    fn parses_cue_location_cohort_decls() {
        let src = "# story\ncue bell\n  .target sound_console\nlocation BELL_TOWER\n  .label \"Bell Tower\"\ncohort singers\n  .capacity 6\n";
        let r = parse_ok(src);
        let doc = &r.root.children[0];
        assert!(find_kind(doc, nk::CUE_DECL).is_some());
        assert!(find_kind(doc, nk::LOCATION_DECL).is_some());
        assert!(find_kind(doc, nk::COHORT_DECL).is_some());
    }

    #[test]
    fn parses_section_with_dialogue_and_choice() {
        let src = "# story\n\n-- start\n\nWREN { worried }\n  The bell went silent.\n\n  * I'll help. -> investigate\n";
        let r = parse_ok(src);
        let doc = &r.root.children[0];
        assert!(find_kind(doc, nk::SECTION).is_some());
        assert!(find_kind(doc, nk::DIALOGUE).is_some());
        assert!(find_kind(doc, nk::CHAR_BLOCK).is_some());
        assert!(find_kind(doc, nk::CHOICE).is_some());
        assert!(find_kind(doc, nk::DIVERT).is_some());
    }

    #[test]
    fn parses_resolve_ref_with_field_chain() {
        let r = parse_ok("# d\n-- s\nlet t = $alice.trust > 50\n");
        let resolve = find_kind(&r.root.children[0], nk::RESOLVE_REF).expect("resolve");
        assert!(find_kind(resolve, nk::FIELD_CHAIN).is_some());
    }

    #[test]
    fn parses_static_ref_chain() {
        let r = parse_ok("# d\n-- s\n-> @harbor.entry\n");
        let div = find_kind(&r.root.children[0], nk::DIVERT).expect("divert");
        assert!(find_kind(div, nk::STATIC_REF).is_some());
    }

    #[test]
    fn parses_after_otherwise_block_pair() {
        let src = "# d\n-- s\nafter $trusted\n  WREN\n    warm\notherwise\n  WREN\n    cold\n";
        let r = parse_ok(src);
        assert!(find_kind(&r.root.children[0], nk::AFTER_BLOCK).is_some());
        assert!(find_kind(&r.root.children[0], nk::OTHERWISE_BLOCK).is_some());
    }

    #[test]
    fn parses_each_visit() {
        let src =
            "# d\n-- s\neach visit\n  first\n    WREN\n      hi\n  then\n    WREN\n      again\n";
        let r = parse_ok(src);
        assert!(find_kind(&r.root.children[0], nk::EACH_VISIT_BLOCK).is_some());
    }

    #[test]
    fn parses_match_block() {
        let src = "# d\n-- s\nmatch $disposition\n  \"high\"\n    WREN\n      trusting\n  _\n    WREN\n      wary\n";
        let r = parse_ok(src);
        assert!(find_kind(&r.root.children[0], nk::MATCH_BLOCK).is_some());
        assert_eq!(count_kind(&root_to_node(&r.root), nk::MATCH_ARM), 2);
    }

    #[test]
    fn parses_let_binding() {
        let r = parse_ok("# d\nlet trusted = $wren_trust > 30 and $met_wren\n");
        assert!(find_kind(&r.root.children[0], nk::LET_BINDING).is_some());
    }

    #[test]
    fn parses_sexp_declarations() {
        let r = parse_ok("# d\n(list factions: rebels, loyalists)\n(import \"./shared.loom\")\n");
        let root = root_to_node(&r.root);
        assert!(find_kind(&root, nk::LIST_DECL).is_some());
        assert!(find_kind(&root, nk::IMPORT_DECL).is_some());
    }

    #[test]
    fn parses_when_participant_lifecycle() {
        let src = "# d :immersive\nwhen participant joins\n  -> orientation\n";
        let r = parse_ok(src);
        assert!(find_kind(&r.root.children[0], nk::PARTICIPANT_LIFECYCLE).is_some());
    }

    #[test]
    fn parses_when_location_event() {
        let src = "# d :immersive\nwhen participant enters @BELL_TOWER\n  -> bell_first_visit\n";
        let r = parse_ok(src);
        assert!(find_kind(&r.root.children[0], nk::LOCATION_EVENT).is_some());
    }

    #[test]
    fn parses_broadcast_with_scope() {
        let src = "# d :immersive\nbroadcast :participant($P)\n  NARRATOR\n    you alone\n";
        let r = parse_ok(src);
        assert!(find_kind(&r.root.children[0], nk::BROADCAST_BLOCK).is_some());
        assert!(find_kind(&r.root.children[0], nk::BROADCAST_SCOPE).is_some());
    }

    #[test]
    fn parses_slugline_scene() {
        let src = "# story :script\n## act_2.scene_3 \"INT. HARBOR\"\n  WREN\n    line\n";
        let r = parse_ok(src);
        assert!(find_kind(&r.root.children[0], nk::SLUGLINE_SCENE).is_some());
        assert!(find_kind(&r.root.children[0], nk::SCENE_SLUG).is_some());
    }

    #[test]
    fn parses_action_line_with_mutation() {
        let src = "# d\n-- s\n~ var $met_wren := true\n";
        let r = parse_ok(src);
        assert!(find_kind(&r.root.children[0], nk::ACTION_LINE).is_some());
    }

    #[test]
    fn parses_expression_with_precedence() {
        let r = parse_ok("# d\nlet x = $a + $b * $c\n");
        let bin = find_kind(&r.root.children[0], nk::BINARY_EXPR).expect("binary");
        // Outer op should be `+`, with `*` nested on the right.
        let op = &bin.children[1];
        assert_eq!(op.value.as_deref(), Some("+"));
    }

    #[test]
    fn parses_unary_not_and_comparison() {
        let r = parse_ok("# d\n-- s\n-> elsewhere if not $trusted\n");
        let div = find_kind(&r.root.children[0], nk::DIVERT).expect("divert");
        assert!(find_kind(div, nk::UNARY_EXPR).is_some());
    }

    #[test]
    fn parses_ledger_predicate() {
        let r = parse_ok("# d\n-- s\nafter visits(intro) >= 3\n  WREN\n    hi\n");
        assert!(find_kind(&r.root.children[0], nk::LEDGER_PRED).is_some());
    }

    #[test]
    fn parses_list_comprehension() {
        let r = parse_ok("# d\nlet xs = [x for x in $things where $x > 0]\n");
        assert!(find_kind(&r.root.children[0], nk::LIST_COMPREHENSION).is_some());
    }

    #[test]
    fn parses_return_line() {
        let r = parse_ok("# d\n-- s\n<- finale\n");
        let ret = find_kind(&r.root.children[0], nk::RETURN_LINE).expect("return");
        assert_eq!(ret.children[0].value.as_deref(), Some("finale"));
    }

    #[test]
    fn parses_flavor_line() {
        let r = parse_ok("# d\n-- s\n> The water laps quietly.\n");
        assert!(find_kind(&r.root.children[0], nk::FLAVOR_LINE).is_some());
    }

    #[test]
    fn produces_diagnostic_on_lex_error() {
        let r = parse(""); // empty is fine
        assert!(r.diagnostics.is_empty());

        // Unterminated string is a lex error.
        let r = parse("# d\nlet s = \"oops\n");
        assert!(
            !r.diagnostics.is_empty(),
            "unterminated string should surface a diagnostic"
        );
        assert!(r.diagnostics.iter().any(|d| d.id == "lex-error"));
    }

    #[test]
    fn produces_diagnostic_on_unbalanced_bracket() {
        let r = parse("# d\n-- s\nlet x = (1 + 2\n");
        // The expression's `(` opens a group that's never closed before
        // the newline; the parser should flag it.
        assert!(
            r.diagnostics.iter().any(|d| d.id == "bracket-unbalanced"),
            "expected bracket-unbalanced diagnostic, got {:?}",
            r.diagnostics
        );
    }

    #[test]
    fn recovers_after_bad_line_and_keeps_parsing() {
        let src = "# d\n!!! bad\n\n-- good\n  WREN\n    hi\n";
        let r = parse(src);
        // Good content should still parse despite the bad line.
        assert!(find_kind(&r.root.children[0], nk::SECTION).is_some());
        assert!(find_kind(&r.root.children[0], nk::DIALOGUE).is_some());
    }

    #[test]
    fn parses_multiple_documents_in_one_file() {
        let src = "# first\n-- a\n  ALICE\n    hi\n\n# second\n-- b\n  BOB\n    hi\n";
        let r = parse(src);
        assert_eq!(r.root.children.len(), 2);
    }

    // ── §12 inline-text grammar ─────────────────────────────────────

    #[test]
    fn parses_backlink_in_dialogue_text() {
        let src = "# d\n-- s\nWREN\n  [[object:maren|Maren]] taught me to listen.\n";
        let r = parse_ok(src);
        assert!(find_kind(&r.root.children[0], nk::BACKLINK).is_some());
    }

    #[test]
    fn parses_inline_trigger_in_text() {
        let src = "# d\n-- s\nWREN\n  You will?<pause:300>\n";
        let r = parse_ok(src);
        assert!(find_kind(&r.root.children[0], nk::INLINE_TRIGGER).is_some());
    }

    #[test]
    fn parses_chain_trigger() {
        let src = "# d\n-- s\nWREN\n  Now! <sfx:bell+camera:shake>\n";
        let r = parse_ok(src);
        assert!(find_kind(&r.root.children[0], nk::CHAIN_TRIGGER).is_some());
    }

    #[test]
    fn parses_ranged_trigger_with_close() {
        let src = "# d\n-- s\nWREN\n  listen to <speed:0.7>the water</>.\n";
        let r = parse_ok(src);
        assert!(find_kind(&r.root.children[0], nk::INLINE_TRIGGER).is_some());
        assert!(find_kind(&r.root.children[0], nk::RANGE_CLOSER).is_some());
    }

    #[test]
    fn parses_inline_eval_in_text() {
        let src = "# d\n-- s\nWREN\n  trust is ${$wren_trust}.\n";
        let r = parse_ok(src);
        assert!(find_kind(&r.root.children[0], nk::INLINE_EVAL).is_some());
    }

    #[test]
    fn parses_inline_assign_in_text() {
        let src = "# d\n-- s\nWREN\n  silence<$wren_trust := 100>.\n";
        let r = parse_ok(src);
        assert!(find_kind(&r.root.children[0], nk::INLINE_ASSIGN).is_some());
    }

    #[test]
    fn parses_resolve_ref_in_text() {
        let src = "# d\n-- s\nWREN\n  Hello, $name.\n";
        let r = parse_ok(src);
        let dialogue = find_kind(&r.root.children[0], nk::DIALOGUE).expect("dialogue");
        assert!(find_kind(dialogue, nk::RESOLVE_REF).is_some());
    }

    #[test]
    fn parses_static_ref_in_text() {
        let src = "# d\n-- s\nWREN\n  See @lighthouse on the map.\n";
        let r = parse_ok(src);
        let dialogue = find_kind(&r.root.children[0], nk::DIALOGUE).expect("dialogue");
        assert!(find_kind(dialogue, nk::STATIC_REF).is_some());
    }

    #[test]
    fn parses_text_variation() {
        let src = "# d\n-- s\nWREN\n  [Hi / Hey / Greetings].cycle\n";
        let r = parse_ok(src);
        assert!(find_kind(&r.root.children[0], nk::TEXT_VARIATION).is_some());
        assert!(find_kind(&r.root.children[0], nk::VARIATION_MODE).is_some());
    }

    #[test]
    fn parses_conditional_trigger() {
        let src = "# d\n-- s\nWREN\n  And<?$alert><sfx:siren>!\n";
        let r = parse_ok(src);
        assert!(find_kind(&r.root.children[0], nk::COND_TRIGGER).is_some());
    }

    #[test]
    fn literal_run_preserves_contraction() {
        let src = "# d\n-- s\nWREN\n  I'll help.\n";
        let r = parse_ok(src);
        let lit = find_kind(&r.root.children[0], nk::LITERAL_RUN).expect("literal");
        let v = lit.value.as_deref().unwrap_or("");
        assert!(
            v.contains("I'll"),
            "expected `I'll` to survive intact, got `{v}`"
        );
    }

    #[test]
    fn named_range_closer_carries_anchor_name() {
        let src = "# d\n-- s\nWREN\n  Open <speed:0.5 %slow>here</%slow>.\n";
        let r = parse_ok(src);
        let closer = find_kind(&r.root.children[0], nk::RANGE_CLOSER).expect("closer");
        // The named form has an identifier child carrying the anchor name.
        assert!(closer
            .children
            .iter()
            .any(|c| c.value.as_deref() == Some("slow")));
    }

    // ── §16 character archetype ─────────────────────────────────────

    #[test]
    fn parses_knowledge_block() {
        let src = "# elena :character\nknowledge\n  met_player: bool = false\n  rumor_count: int = 0\n  factions_known: list<string>\n";
        let r = parse_ok(src);
        let kb = find_kind(&r.root.children[0], nk::KNOWLEDGE_BLOCK).expect("knowledge block");
        let fields = kb
            .children
            .iter()
            .filter(|c| c.kind == nk::KNOWLEDGE_FIELD)
            .count();
        assert_eq!(fields, 3);
    }

    #[test]
    fn parses_knowledge_field_with_closed_enum_type() {
        let src = "# c :character\nknowledge\n  mood: { calm, agitated, focused } = calm\n";
        let r = parse_ok(src);
        assert!(find_kind(&r.root.children[0], nk::KNOWLEDGE_TYPE).is_some());
    }

    #[test]
    fn parses_goal_with_required_priority() {
        let src = "# c :character\ngoal investigate_bell\n  priority = 0.9\n  active_when = $met_wren\n  on_complete fire bell_solved\n";
        let r = parse_ok(src);
        let goal = find_kind(&r.root.children[0], nk::GOAL_DECL).expect("goal");
        let knobs = goal
            .children
            .iter()
            .filter(|c| c.kind == nk::GOAL_KNOB)
            .count();
        assert_eq!(knobs, 3);
    }

    #[test]
    fn parses_goal_drives_generator_form() {
        let src = "# c\ngoal wander\n  priority = 0.4\n  drives generator wander_with_purpose\n";
        let r = parse_ok(src);
        let goal = find_kind(&r.root.children[0], nk::GOAL_DECL).expect("goal");
        let drives = goal
            .children
            .iter()
            .find(|c| {
                c.kind == nk::GOAL_KNOB
                    && c.children.first().and_then(|n| n.value.as_deref()) == Some("drives")
            })
            .expect("drives knob");
        // Should reference the generator name.
        assert!(drives
            .children
            .iter()
            .any(|c| c.value.as_deref() == Some("wander_with_purpose")));
    }

    #[test]
    fn parses_disposition_block_with_axes() {
        let src = "# c :character\ndisposition $PLAYER\n  trust = -1..1, init 0.0\n  fear = 0..1, init 0.0, mirror $PLAYER.disposition.dread\n";
        let r = parse_ok(src);
        let disp = find_kind(&r.root.children[0], nk::DISPOSITION_BLOCK).expect("disposition");
        let axes = disp
            .children
            .iter()
            .filter(|c| c.kind == nk::DISPOSITION_AXIS)
            .count();
        assert_eq!(axes, 2);
        // Mirror clause should appear on the second axis.
        assert!(find_kind(disp, nk::MIRROR_CLAUSE).is_some());
    }

    #[test]
    fn parses_disposition_react_emits_tag() {
        let src = "# c :character\ndisposition $PLAYER\n  reacts $PLAYER.disposition.trust > 0.8 -> ally\n";
        let r = parse_ok(src);
        assert!(find_kind(&r.root.children[0], nk::DISPOSITION_REACT).is_some());
    }

    #[test]
    fn parses_hook_decl_with_body() {
        let src = "# c :character\non $wren_trust passes 50\n  ~ fire trust_threshold_hit\n";
        let r = parse_ok(src);
        let hook = find_kind(&r.root.children[0], nk::HOOK_DECL).expect("hook");
        assert!(find_kind(hook, nk::HOOK_PATTERN).is_some());
    }

    // ── §17 stats + tree archetype ─────────────────────────────────

    #[test]
    fn parses_attribute_decl_with_range() {
        let src = "# s :stats\nattribute strength = 10, range 0..20\n";
        let r = parse_ok(src);
        let a = find_kind(&r.root.children[0], nk::ATTRIBUTE_DECL).expect("attribute");
        let mods = a
            .children
            .iter()
            .filter(|c| c.kind == nk::ATTRIBUTE_MOD)
            .count();
        assert_eq!(mods, 1);
        assert!(find_kind(a, nk::NUM_RANGE).is_some());
    }

    #[test]
    fn parses_axis_with_properties() {
        let src = "# s :stats\naxis combat\n  mode use_tracking\n  curve $combat * 100\n";
        let r = parse_ok(src);
        let axis = find_kind(&r.root.children[0], nk::AXIS_DECL).expect("axis");
        let props = axis
            .children
            .iter()
            .filter(|c| c.kind == nk::AXIS_PROPERTY)
            .count();
        assert_eq!(props, 2);
    }

    #[test]
    fn parses_pool_with_properties() {
        let src = "# s :stats\npool stamina\n  max = 100\n  regen 5 / 1s when not in_combat\n  init = 100\n";
        let r = parse_ok(src);
        let pool = find_kind(&r.root.children[0], nk::POOL_DECL).expect("pool");
        let props = pool
            .children
            .iter()
            .filter(|c| c.kind == nk::POOL_PROPERTY)
            .count();
        assert_eq!(props, 3);
    }

    #[test]
    fn parses_stat_expression_form() {
        let src = "# s :stats\nstat damage = $strength + $weapon.damage\n";
        let r = parse_ok(src);
        assert!(find_kind(&r.root.children[0], nk::STAT_DECL).is_some());
    }

    #[test]
    fn parses_stat_lookup_form() {
        let src = "# s :stats\nstat dodge\n  lookup $agility\n  table {0: 0.05, 10: 0.15, 20: 0.30}\n  interpolate linear\n";
        let r = parse_ok(src);
        let stat = find_kind(&r.root.children[0], nk::STAT_DECL).expect("stat");
        let props = count_kind(stat, nk::LOOKUP_PROPERTY);
        assert_eq!(props, 3);
        assert!(find_kind(stat, nk::DICT_LITERAL).is_some());
    }

    #[test]
    fn parses_tree_node_decl() {
        let src = "# t :tree\nnode dual_wield\n  cost {points: 3}\n  requires basic_combat\n  effect ability @swift_strike\n  rank 1\n";
        let r = parse_ok(src);
        let n = find_kind(&r.root.children[0], nk::TREE_NODE_DECL).expect("tree node");
        let props = count_kind(n, nk::TREE_NODE_PROPERTY);
        assert_eq!(props, 4);
        assert!(find_kind(n, nk::TREE_EFFECT).is_some());
    }

    // ── §18 reactivity ─────────────────────────────────────────────

    #[test]
    fn parses_generator_with_scheduler_properties() {
        let src = "# d\ngenerator wander\n  .tier ambient\n  .priority 0.4\n  .budget_ms 8\n  loop forever\n    wait 30s\n    yield bark from @observations\n";
        let r = parse_ok(src);
        let gen = find_kind(&r.root.children[0], nk::GENERATOR_DECL).expect("generator");
        assert_eq!(count_kind(gen, nk::SCHEDULER_PROPERTY), 3);
        assert!(find_kind(gen, nk::LOOP_BLOCK).is_some());
        assert!(find_kind(gen, nk::WAIT_STMT).is_some());
        assert!(find_kind(gen, nk::YIELD_STMT).is_some());
    }

    #[test]
    fn parses_scene_with_states_and_return() {
        let src = "# d\nscene greeting |p, npc|\n  open\n    wait until $p.distance($npc) < 2\n    NPC\n      Hello\n    return true\n";
        let r = parse_ok(src);
        let scene = find_kind(&r.root.children[0], nk::SCENE_DECL).expect("scene");
        assert!(find_kind(scene, nk::SCENE_STATE).is_some());
        assert!(find_kind(scene, nk::RETURN_STMT).is_some());
    }

    #[test]
    fn parses_compose_with_patterns() {
        let src = "# d\ncompose ship_name\n  pattern $size\n    1..3: \"skiff\"\n    4..7: \"sloop\"\n    _: \"frigate\"\n";
        let r = parse_ok(src);
        let comp = find_kind(&r.root.children[0], nk::COMPOSE_DECL).expect("compose");
        let arms = count_kind(comp, nk::PATTERN_ARM);
        assert_eq!(arms, 3);
    }

    #[test]
    fn parses_wait_until_and_time_stmt() {
        let src = "# d\ngenerator g\n  wait until $bell_rung\n  at 6am\n  every 15m\n";
        let r = parse_ok(src);
        let gen = find_kind(&r.root.children[0], nk::GENERATOR_DECL).expect("generator");
        let waits = count_kind(gen, nk::WAIT_STMT);
        let times = count_kind(gen, nk::TIME_STMT);
        assert_eq!(waits, 1);
        assert_eq!(times, 2);
    }

    // ── §19 faction archetype ──────────────────────────────────────

    #[test]
    fn parses_inline_faction_decl() {
        let src = "# d :immersive\nfaction rebels\n  .label \"Tideturners\"\n  members\n    @wren, @hale\n  state\n    morale = 0..100, init 50\n  stance\n    @loyalists = hostile asymmetric\n    default = neutral\n";
        let r = parse_ok(src);
        let fac = find_kind(&r.root.children[0], nk::INLINE_FACTION_DECL).expect("inline_faction");
        assert!(find_kind(fac, nk::MEMBERS_BLOCK).is_some());
        assert!(find_kind(fac, nk::STATE_BLOCK).is_some());
        assert!(find_kind(fac, nk::STANCE_BLOCK).is_some());
    }

    #[test]
    fn parses_members_block_with_visibility() {
        let src = "# d :immersive\nfaction rebels\n  members\n    @wren visibility: public\n    cohort singers visibility: cohort(initiate)\n    match $is_aligned visibility: private\n";
        let r = parse_ok(src);
        let members = find_kind(&r.root.children[0], nk::MEMBERS_BLOCK).expect("members");
        let entries = count_kind(members, nk::MEMBER_ENTRY);
        assert_eq!(entries, 3);
        let viz = count_kind(members, nk::VISIBILITY_CLAUSE);
        assert_eq!(viz, 3);
    }

    #[test]
    fn parses_stance_block_with_asymmetric() {
        let src = "# d :immersive\nfaction rebels\n  stance\n    @loyalists = hostile asymmetric\n    default = neutral\n";
        let r = parse_ok(src);
        let stance = find_kind(&r.root.children[0], nk::STANCE_BLOCK).expect("stance");
        let entries: Vec<_> = stance
            .children
            .iter()
            .filter(|c| c.kind == nk::STANCE_ENTRY)
            .collect();
        assert_eq!(entries.len(), 2);
        // First entry should carry the asymmetric marker.
        let first = entries[0];
        assert!(first.children.iter().any(|c| c.kind == "asymmetric"));
    }

    #[test]
    fn parses_when_faction_emerges() {
        let src = "# d :immersive\nwhen faction emerges from template @grassroots\n  ~ fire new_faction_formed\n";
        let r = parse_ok(src);
        assert!(find_kind(&r.root.children[0], nk::FACTION_EVENT).is_some());
    }

    #[test]
    fn parses_when_participant_proposes_faction() {
        let src = "# d :immersive\nwhen participant proposes faction\n  ~ fire faction_proposed\n";
        let r = parse_ok(src);
        assert!(find_kind(&r.root.children[0], nk::PARTICIPANT_FACTION_LIFECYCLE).is_some());
    }

    #[test]
    fn parses_when_stance_revealed() {
        let src = "# d :immersive\nwhen stance(@a, @b) revealed to $OBSERVER\n  ~ fire revealed\n";
        let r = parse_ok(src);
        assert!(find_kind(&r.root.children[0], nk::DISCOVERY_EVENT).is_some());
    }
}
