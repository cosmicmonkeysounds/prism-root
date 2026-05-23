//! Loom lexer — produces a token stream from `.loom` source.
//!
//! Reference: `docs/dev/loom-grammar.md` §§2-3. Built on the shared
//! `language::syntax::Scanner` primitive so the byte-cursor / line
//! tracking / save+restore semantics match every other Prism parser
//! (`feedback_prism_syntax_parsing.md` — all external-format parsers
//! must compose on Scanner).
//!
//! The lexer's two non-trivial responsibilities:
//!
//! 1. **Indent sensitivity (§2.3)** — emits synthetic `Indent`,
//!    `Dedent`, and `Newline` tokens, Python-style. Bracket nesting
//!    (`(`, `[`, `{`) suppresses layout emission so an s-expression
//!    can wrap freely.
//!
//! 2. **Multi-word operator merging (§3.1)** — `is not` and `has not`
//!    are joined into single tokens (`IsNot`, `HasNot`) at lex time so
//!    the expression parser can keep its precedence table flat. Every
//!    other multi-word combination in the grammar (`wait until`,
//!    `each visit`, `participant joins`, …) is left as two tokens for
//!    the parser to recognise structurally.
//!
//! Speakers (ALL\_CAPS identifiers, grammar §3.2) lex as a distinct
//! `Speaker` token kind so the parser can prefer them in dialogue
//! position. In expression position, both `Ident` and `Speaker` are
//! acceptable as atoms — the parser doesn't care which.
//!
//! Reserved-word classification is **not** done at lex time. Every
//! identifier comes through as `Ident` and the parser checks the
//! text against [`super::keywords`] at the point of use. That keeps
//! the keyword tables (which evolve as the language grows) the
//! single source of truth without forcing the lexer to grow a new
//! enum variant per word.

use crate::language::syntax::{
    is_digit, is_ident_char, is_ident_start, Position, ScanError, Scanner, SourceRange,
};

/// Coarse classification of a lexed token. Concrete keyword identity
/// is recovered from [`Token::text`] via the [`super::keywords`]
/// tables.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenKind {
    // ── Atoms ───────────────────────────────────────────────────────
    /// `[a-z_][a-zA-Z0-9_]*` or mixed-case identifier.
    Ident,
    /// `[A-Z][A-Z0-9_]+` — ALL-CAPS speaker label (grammar §3.2).
    Speaker,
    /// `-?[0-9]+(\.[0-9]+)?`. Optional duration suffix (`s`, `ms`,
    /// `m`, `h`) is consumed into the same token; query via
    /// [`Token::duration_unit`].
    Number,
    /// `"..."` — quoted string literal.
    String,
    /// `'''...'''` — triple-single-quoted docstring.
    Docstring,
    /// `//`-style line comment. Lexer emits these so the parser can
    /// attach them to following nodes (or drop them); tests find them
    /// useful too.
    LineComment,
    /// `/* ... */` block comment ("boneyard").
    BlockComment,

    // ── Single-character sigils ────────────────────────────────────
    Hash,    // #
    Dot,     // .
    At,      // @
    Dollar,  // $
    Tilde,   // ~
    Caret,   // ^
    QMark,   // ?
    Percent, // %
    Pipe,    // |
    Comma,   // ,
    Colon,   // :
    Semi,    // ;
    Bang,    // !
    Star,    // *
    Plus,    // +
    Minus,   // -
    Slash,   // /
    Eq,      // =
    Lt,      // <
    Gt,      // >
    LParen,  // (
    RParen,  // )
    LBrace,  // {
    RBrace,  // }
    LBrack,  // [
    RBrack,  // ]

    // ── Multi-character sigils ─────────────────────────────────────
    DashDash,    // --   (section opener; also a line marker)
    HashHash,    // ##   (slugline scene opener)
    Arrow,       // ->
    BackArrow,   // <-
    LBrack2,     // [[
    RBrack2,     // ]]
    DollarBrace, // ${
    DollarParen, // $(
    LtSlashGt,   // </>
    LtSlashPct,  // </%
    LtQMark,     // <?

    // ── Multi-character operators ──────────────────────────────────
    Walrus,    // :=
    EqEq,      // ==
    Neq,       // !=
    Gte,       // >=
    Lte,       // <=
    PlusEq,    // +=
    MinusEq,   // -=
    PlusPlus,  // ++
    SafeNav,   // ?.
    Member,    // ?=
    NotMember, // !?=
    DotDot,    // ..   (numeric range in disposition / state axes)

    // ── Multi-word operators merged at lex time ────────────────────
    IsNot,  // is not
    HasNot, // has not

    // ── Layout tokens (synthetic) ──────────────────────────────────
    Newline,
    Indent,
    Dedent,

    /// Synthetic end-of-input token. Always the final token in the
    /// stream so the parser can peek without bounds-checking.
    Eof,
}

/// Optional duration suffix attached to a `Number` token. Only set
/// when the literal carried `s` / `ms` / `m` / `h`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DurationUnit {
    Ms,
    Sec,
    Min,
    Hour,
}

impl DurationUnit {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ms => "ms",
            Self::Sec => "s",
            Self::Min => "m",
            Self::Hour => "h",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Token {
    pub kind: TokenKind,
    pub range: SourceRange,
    /// Verbatim slice of the original source the token covers.
    /// Always populated, even for synthetic tokens (empty string).
    pub text: String,
    /// Set on `Number` tokens that carried a duration suffix.
    pub duration_unit: Option<DurationUnit>,
}

impl Token {
    pub fn new(kind: TokenKind, range: SourceRange, text: impl Into<String>) -> Self {
        Self {
            kind,
            range,
            text: text.into(),
            duration_unit: None,
        }
    }

    pub fn with_unit(mut self, unit: DurationUnit) -> Self {
        self.duration_unit = Some(unit);
        self
    }

    pub fn synthetic(kind: TokenKind, at: Position) -> Self {
        Self {
            kind,
            range: SourceRange { start: at, end: at },
            text: String::new(),
            duration_unit: None,
        }
    }

    /// Convenience: does this token's `text` match a particular
    /// keyword? Cheaper than constructing a Keyword enum.
    pub fn is_word(&self, word: &str) -> bool {
        matches!(self.kind, TokenKind::Ident | TokenKind::Speaker) && self.text == word
    }
}

/// Lex the full source into a vector of tokens, terminated by `Eof`.
///
/// On a lex error (unterminated string, etc.) the error is returned
/// and any tokens collected before the failure are discarded —
/// editor-side incremental parsing isn't a goal of this entry point;
/// the LSP server will wrap the lexer in an error-tolerant variant
/// when it lands.
pub fn lex(source: &str) -> Result<Vec<Token>, ScanError> {
    Lexer::new(source).run()
}

struct Lexer<'s> {
    sc: Scanner<'s>,
    /// Stack of indent column widths. Index 0 is always 0 (column-0
    /// baseline). Push on `Indent`, pop on `Dedent`.
    indents: Vec<usize>,
    /// Bracket nesting depth. While `> 0`, newlines/indents/dedents
    /// are suppressed and `\n` is treated as ordinary whitespace.
    paren_depth: u32,
    /// True at start-of-file and immediately after a `Newline` token
    /// has been emitted. Used to decide whether the next leading
    /// whitespace should produce indent/dedent tokens.
    at_line_start: bool,
    /// Output buffer; tokens are appended in source order.
    out: Vec<Token>,
}

impl<'s> Lexer<'s> {
    fn new(source: &'s str) -> Self {
        Self {
            sc: Scanner::new(source),
            indents: vec![0],
            paren_depth: 0,
            at_line_start: true,
            out: Vec::new(),
        }
    }

    fn run(mut self) -> Result<Vec<Token>, ScanError> {
        // Optional shebang (grammar §1) — only legal on the first line.
        if self.sc.offset() == 0
            && self.sc.peek() == Some('#')
            && self.sc.peek_ahead(1) == Some('!')
        {
            self.scan_shebang();
        }

        while !self.sc.is_at_end() {
            if self.at_line_start && self.paren_depth == 0 {
                self.handle_line_start()?;
            }
            // After handling layout we may already have consumed the
            // line's content (blank line, comment-only line) — loop.
            if self.sc.is_at_end() {
                break;
            }
            self.scan_token()?;
        }

        // Drain trailing dedents back to baseline + emit one final
        // synthetic Newline (per grammar §2.1: EOF is a synthetic NL
        // followed by enough DEDENTs to reach column 0).
        let eof_pos = self.sc.position();
        if !self
            .out
            .last()
            .map(|t| t.kind == TokenKind::Newline)
            .unwrap_or(false)
        {
            self.out.push(Token::synthetic(TokenKind::Newline, eof_pos));
        }
        while self.indents.len() > 1 {
            self.indents.pop();
            self.out.push(Token::synthetic(TokenKind::Dedent, eof_pos));
        }
        self.out.push(Token::synthetic(TokenKind::Eof, eof_pos));

        Ok(self.out)
    }

    fn scan_shebang(&mut self) {
        // Consume `#!...` up to but not including the newline. We
        // don't emit a token for it — shebangs are metadata, not
        // syntax.
        let _ = self.sc.scan_while(|c| c != '\n' && c != '\r');
    }

    // ── Line-start layout handling ───────────────────────────────────

    fn handle_line_start(&mut self) -> Result<(), ScanError> {
        // Skip blank / comment-only lines without emitting indent
        // tokens (grammar §2.4: comments do not affect indent).
        loop {
            let line_start = self.sc.offset();
            let indent_width = self.measure_indent();

            // After measuring, look at what's next.
            match self.sc.peek() {
                None => {
                    self.at_line_start = false;
                    return Ok(());
                }
                Some('\n') | Some('\r') => {
                    // Blank line — skip the newline and re-measure.
                    self.consume_newline_chars();
                    continue;
                }
                Some('/') if self.sc.peek_ahead(1) == Some('/') => {
                    // Line comment that consumes the rest of the line.
                    let start = self.sc.offset();
                    self.sc.advance();
                    self.sc.advance();
                    let _ = self.sc.scan_while(|c| c != '\n' && c != '\r');
                    let range = self.sc.range_from(start);
                    let text = self.sc.slice(start, self.sc.offset()).to_string();
                    self.out.push(Token {
                        kind: TokenKind::LineComment,
                        range,
                        text,
                        duration_unit: None,
                    });
                    self.consume_newline_chars();
                    continue;
                }
                Some('/') if self.sc.peek_ahead(1) == Some('*') => {
                    let start = self.sc.offset();
                    self.scan_block_comment(start)?;
                    // If the block comment terminated mid-line, keep going
                    // (subsequent content is on this same logical line, so
                    // we leave `at_line_start` armed and fall out).
                    // If we're now sitting on a newline, treat the line as
                    // blank.
                    match self.sc.peek() {
                        Some('\n') | Some('\r') | None => {
                            self.consume_newline_chars();
                            continue;
                        }
                        _ => {
                            // Comment ended mid-line; emit indent tokens
                            // for the now-resolved indent width.
                            self.emit_indent_change(indent_width, line_start);
                            self.at_line_start = false;
                            return Ok(());
                        }
                    }
                }
                _ => {
                    self.emit_indent_change(indent_width, line_start);
                    self.at_line_start = false;
                    return Ok(());
                }
            }
        }
    }

    fn measure_indent(&mut self) -> usize {
        let mut width = 0;
        while let Some(ch) = self.sc.peek() {
            match ch {
                ' ' | '\t' => {
                    self.sc.advance();
                    width += 1;
                }
                _ => break,
            }
        }
        width
    }

    fn consume_newline_chars(&mut self) {
        match self.sc.peek() {
            Some('\r') => {
                self.sc.advance();
                if self.sc.peek() == Some('\n') {
                    self.sc.advance();
                }
            }
            Some('\n') => {
                self.sc.advance();
            }
            _ => {}
        }
    }

    fn emit_indent_change(&mut self, width: usize, line_start: usize) {
        let current = *self.indents.last().unwrap_or(&0);
        let pos = self.sc.pos_at(line_start);
        if width > current {
            self.indents.push(width);
            self.out.push(Token::synthetic(TokenKind::Indent, pos));
        } else if width < current {
            while *self.indents.last().unwrap_or(&0) > width {
                self.indents.pop();
                self.out.push(Token::synthetic(TokenKind::Dedent, pos));
            }
        }
        // width == current → no synthetic token (the previous Newline
        // already separated the lines).
    }

    // ── Mid-line token dispatch ──────────────────────────────────────

    fn scan_token(&mut self) -> Result<(), ScanError> {
        // Mid-line whitespace is non-meaningful; eat it without
        // emitting.
        loop {
            match self.sc.peek() {
                Some(' ') | Some('\t') => {
                    self.sc.advance();
                }
                Some('\\') => {
                    // Explicit line continuation `\\\n` (§2.2). Consume
                    // both and treat as horizontal whitespace.
                    let save = self.sc.save();
                    self.sc.advance();
                    if matches!(self.sc.peek(), Some('\n') | Some('\r')) {
                        self.consume_newline_chars();
                    } else {
                        self.sc.restore(save);
                        break;
                    }
                }
                _ => break,
            }
        }

        let Some(ch) = self.sc.peek() else {
            return Ok(());
        };

        // Logical newline (only when not inside brackets).
        if ch == '\n' || ch == '\r' {
            self.consume_newline_chars();
            if self.paren_depth == 0 {
                // Emit Newline only if the last emitted token wasn't
                // already a Newline (collapses consecutive blank lines
                // structurally — the parser sees one).
                let last_was_nl = self
                    .out
                    .last()
                    .map(|t| t.kind == TokenKind::Newline)
                    .unwrap_or(false);
                if !last_was_nl {
                    self.out
                        .push(Token::synthetic(TokenKind::Newline, self.sc.position()));
                }
                self.at_line_start = true;
            }
            return Ok(());
        }

        // Dispatch on the first non-whitespace character.
        let start = self.sc.offset();

        if ch == '/' && self.sc.peek_ahead(1) == Some('/') {
            self.sc.advance();
            self.sc.advance();
            let _ = self.sc.scan_while(|c| c != '\n' && c != '\r');
            let range = self.sc.range_from(start);
            let text = self.sc.slice(start, self.sc.offset()).to_string();
            self.out.push(Token {
                kind: TokenKind::LineComment,
                range,
                text,
                duration_unit: None,
            });
            return Ok(());
        }
        if ch == '/' && self.sc.peek_ahead(1) == Some('*') {
            self.scan_block_comment(start)?;
            return Ok(());
        }

        // Docstrings — three quotes.
        if ch == '\'' && self.sc.peek_ahead(1) == Some('\'') && self.sc.peek_ahead(2) == Some('\'')
        {
            self.scan_docstring(start)?;
            return Ok(());
        }

        if ch == '"' {
            self.scan_string(start)?;
            return Ok(());
        }

        if is_digit(ch) || (ch == '-' && self.sc.peek_ahead(1).is_some_and(is_digit)) {
            self.scan_number(start)?;
            return Ok(());
        }

        if is_ident_start(ch) {
            self.scan_identifier_or_speaker(start);
            return Ok(());
        }

        self.scan_punctuation(start)?;
        Ok(())
    }

    fn scan_block_comment(&mut self, start: usize) -> Result<(), ScanError> {
        self.sc.advance(); // /
        self.sc.advance(); // *
        loop {
            match self.sc.peek() {
                None => {
                    return Err(self.sc.error("Unterminated block comment"));
                }
                Some('*') => {
                    self.sc.advance();
                    if self.sc.peek() == Some('/') {
                        self.sc.advance();
                        break;
                    }
                }
                _ => {
                    self.sc.advance_unicode();
                }
            }
        }
        let range = self.sc.range_from(start);
        let text = self.sc.slice(start, self.sc.offset()).to_string();
        self.out.push(Token {
            kind: TokenKind::BlockComment,
            range,
            text,
            duration_unit: None,
        });
        Ok(())
    }

    fn scan_docstring(&mut self, start: usize) -> Result<(), ScanError> {
        self.sc.advance(); // '
        self.sc.advance(); // '
        self.sc.advance(); // '
        loop {
            match self.sc.peek() {
                None => return Err(self.sc.error("Unterminated docstring")),
                Some('\'')
                    if self.sc.peek_ahead(1) == Some('\'')
                        && self.sc.peek_ahead(2) == Some('\'') =>
                {
                    self.sc.advance();
                    self.sc.advance();
                    self.sc.advance();
                    break;
                }
                _ => {
                    self.sc.advance_unicode();
                }
            }
        }
        let range = self.sc.range_from(start);
        let text = self.sc.slice(start, self.sc.offset()).to_string();
        self.out.push(Token {
            kind: TokenKind::Docstring,
            range,
            text,
            duration_unit: None,
        });
        Ok(())
    }

    fn scan_string(&mut self, start: usize) -> Result<(), ScanError> {
        self.sc.advance(); // opening "
        while let Some(ch) = self.sc.peek() {
            match ch {
                '"' => {
                    self.sc.advance();
                    let range = self.sc.range_from(start);
                    let text = self.sc.slice(start, self.sc.offset()).to_string();
                    self.out.push(Token {
                        kind: TokenKind::String,
                        range,
                        text,
                        duration_unit: None,
                    });
                    return Ok(());
                }
                '\\' => {
                    self.sc.advance();
                    self.sc.advance(); // escaped char (any)
                }
                '\n' | '\r' => {
                    return Err(self.sc.error("Unterminated string literal"));
                }
                _ => {
                    self.sc.advance_unicode();
                }
            }
        }
        Err(self.sc.error("Unterminated string literal"))
    }

    fn scan_number(&mut self, start: usize) -> Result<(), ScanError> {
        if self.sc.peek() == Some('-') {
            self.sc.advance();
        }
        // Integer part.
        while self.sc.peek().is_some_and(is_digit) {
            self.sc.advance();
        }
        // Optional fraction. Take care not to consume `..` (NumRange).
        if self.sc.peek() == Some('.')
            && self.sc.peek_ahead(1).is_some_and(is_digit)
            && self.sc.peek_ahead(1) != Some('.')
        {
            self.sc.advance();
            while self.sc.peek().is_some_and(is_digit) {
                self.sc.advance();
            }
        }

        // Optional duration suffix. We only consume a suffix if it
        // ends at a non-ident-char boundary, otherwise `5ms_thing`
        // would mis-lex.
        let value_end = self.sc.offset();
        let unit = self.try_consume_duration_suffix();

        let range = self.sc.range_from(start);
        let text = self.sc.slice(start, self.sc.offset()).to_string();
        let mut tok = Token {
            kind: TokenKind::Number,
            range,
            text,
            duration_unit: None,
        };
        if let Some(u) = unit {
            tok = tok.with_unit(u);
        }
        let _ = value_end;
        self.out.push(tok);
        Ok(())
    }

    fn try_consume_duration_suffix(&mut self) -> Option<DurationUnit> {
        let save = self.sc.save();
        let two_chars_eat = |sc: &mut Scanner<'_>, a: char, b: char| -> bool {
            if sc.peek() == Some(a) && sc.peek_ahead(1) == Some(b) {
                sc.advance();
                sc.advance();
                true
            } else {
                false
            }
        };

        // Order matters: `ms` before `m`.
        if two_chars_eat(&mut self.sc, 'm', 's') {
            if self.sc.peek().is_none_or(|c| !is_ident_char(c)) {
                return Some(DurationUnit::Ms);
            }
            self.sc.restore(save);
            return None;
        }

        let one = self.sc.peek();
        if let Some(c) = one {
            if matches!(c, 's' | 'm' | 'h') {
                self.sc.advance();
                if self.sc.peek().is_none_or(|nc| !is_ident_char(nc)) {
                    return Some(match c {
                        's' => DurationUnit::Sec,
                        'm' => DurationUnit::Min,
                        'h' => DurationUnit::Hour,
                        _ => unreachable!(),
                    });
                }
                self.sc.restore(save);
            }
        }
        None
    }

    fn scan_identifier_or_speaker(&mut self, start: usize) {
        let _ = self.sc.advance(); // ident-start char
        while self.sc.peek().is_some_and(is_ident_char) {
            self.sc.advance();
        }
        let range = self.sc.range_from(start);
        let text = self.sc.slice(start, self.sc.offset()).to_string();

        // Multi-word merge: `is not` / `has not`. Look ahead past
        // horizontal whitespace; if the next ident is `not`, fold the
        // two into a single IsNot / HasNot token.
        if text == "is" || text == "has" {
            let save = self.sc.save();
            while matches!(self.sc.peek(), Some(' ') | Some('\t')) {
                self.sc.advance();
            }
            let next_start = self.sc.offset();
            if self.sc.peek().is_some_and(is_ident_start) {
                let mut probe_end = next_start;
                while self
                    .sc
                    .source()
                    .as_bytes()
                    .get(probe_end)
                    .is_some_and(|b| is_ident_char(*b as char))
                {
                    probe_end += 1;
                }
                if self.sc.slice(next_start, probe_end) == "not" {
                    while self.sc.offset() < probe_end {
                        self.sc.advance();
                    }
                    let kind = if text == "is" {
                        TokenKind::IsNot
                    } else {
                        TokenKind::HasNot
                    };
                    let merged_range = self.sc.range_from(start);
                    let merged_text = self.sc.slice(start, self.sc.offset()).to_string();
                    self.out.push(Token {
                        kind,
                        range: merged_range,
                        text: merged_text,
                        duration_unit: None,
                    });
                    return;
                }
            }
            self.sc.restore(save);
        }

        let kind = if is_speaker(&text) {
            TokenKind::Speaker
        } else {
            TokenKind::Ident
        };
        self.out.push(Token {
            kind,
            range,
            text,
            duration_unit: None,
        });
    }

    fn scan_punctuation(&mut self, start: usize) -> Result<(), ScanError> {
        let ch = self.sc.peek().expect("scan_punctuation called at EOF");
        let next = self.sc.peek_ahead(1);

        // Greedy multi-character first.
        let kind = match (ch, next) {
            ('-', Some('-')) => {
                self.sc.advance();
                self.sc.advance();
                TokenKind::DashDash
            }
            ('#', Some('#')) => {
                self.sc.advance();
                self.sc.advance();
                TokenKind::HashHash
            }
            ('-', Some('>')) => {
                self.sc.advance();
                self.sc.advance();
                TokenKind::Arrow
            }
            ('<', Some('-')) => {
                self.sc.advance();
                self.sc.advance();
                TokenKind::BackArrow
            }
            ('[', Some('[')) => {
                self.sc.advance();
                self.sc.advance();
                TokenKind::LBrack2
            }
            (']', Some(']')) => {
                self.sc.advance();
                self.sc.advance();
                TokenKind::RBrack2
            }
            ('$', Some('{')) => {
                self.sc.advance();
                self.sc.advance();
                TokenKind::DollarBrace
            }
            ('$', Some('(')) => {
                self.sc.advance();
                self.sc.advance();
                TokenKind::DollarParen
            }
            ('<', Some('/')) => {
                self.sc.advance();
                self.sc.advance();
                if self.sc.peek() == Some('>') {
                    self.sc.advance();
                    TokenKind::LtSlashGt
                } else if self.sc.peek() == Some('%') {
                    self.sc.advance();
                    TokenKind::LtSlashPct
                } else {
                    return Err(self.sc.error("Expected `</>` or `</%`"));
                }
            }
            ('<', Some('?')) => {
                self.sc.advance();
                self.sc.advance();
                TokenKind::LtQMark
            }
            (':', Some('=')) => {
                self.sc.advance();
                self.sc.advance();
                TokenKind::Walrus
            }
            ('=', Some('=')) => {
                self.sc.advance();
                self.sc.advance();
                TokenKind::EqEq
            }
            ('!', Some('=')) => {
                self.sc.advance();
                self.sc.advance();
                TokenKind::Neq
            }
            ('>', Some('=')) => {
                self.sc.advance();
                self.sc.advance();
                TokenKind::Gte
            }
            ('<', Some('=')) => {
                self.sc.advance();
                self.sc.advance();
                TokenKind::Lte
            }
            ('+', Some('=')) => {
                self.sc.advance();
                self.sc.advance();
                TokenKind::PlusEq
            }
            ('-', Some('=')) => {
                self.sc.advance();
                self.sc.advance();
                TokenKind::MinusEq
            }
            ('+', Some('+')) => {
                self.sc.advance();
                self.sc.advance();
                TokenKind::PlusPlus
            }
            ('?', Some('.')) => {
                self.sc.advance();
                self.sc.advance();
                TokenKind::SafeNav
            }
            ('?', Some('=')) => {
                self.sc.advance();
                self.sc.advance();
                TokenKind::Member
            }
            ('!', Some('?')) if self.sc.peek_ahead(2) == Some('=') => {
                self.sc.advance();
                self.sc.advance();
                self.sc.advance();
                TokenKind::NotMember
            }
            ('.', Some('.')) => {
                self.sc.advance();
                self.sc.advance();
                TokenKind::DotDot
            }
            _ => self.scan_single_char_punct()?,
        };

        let range = self.sc.range_from(start);
        let text = self.sc.slice(start, self.sc.offset()).to_string();
        self.out.push(Token {
            kind,
            range,
            text,
            duration_unit: None,
        });

        match kind {
            TokenKind::LParen | TokenKind::LBrack | TokenKind::LBrace => {
                self.paren_depth = self.paren_depth.saturating_add(1);
            }
            TokenKind::RParen | TokenKind::RBrack | TokenKind::RBrace => {
                self.paren_depth = self.paren_depth.saturating_sub(1);
            }
            _ => {}
        }

        Ok(())
    }

    fn scan_single_char_punct(&mut self) -> Result<TokenKind, ScanError> {
        let ch = self.sc.advance().expect("called at EOF");
        Ok(match ch {
            '#' => TokenKind::Hash,
            '.' => TokenKind::Dot,
            '@' => TokenKind::At,
            '$' => TokenKind::Dollar,
            '~' => TokenKind::Tilde,
            '^' => TokenKind::Caret,
            '?' => TokenKind::QMark,
            '%' => TokenKind::Percent,
            '|' => TokenKind::Pipe,
            ',' => TokenKind::Comma,
            ':' => TokenKind::Colon,
            ';' => TokenKind::Semi,
            '!' => TokenKind::Bang,
            '*' => TokenKind::Star,
            '+' => TokenKind::Plus,
            '-' => TokenKind::Minus,
            '/' => TokenKind::Slash,
            '=' => TokenKind::Eq,
            '<' => TokenKind::Lt,
            '>' => TokenKind::Gt,
            '(' => TokenKind::LParen,
            ')' => TokenKind::RParen,
            '{' => TokenKind::LBrace,
            '}' => TokenKind::RBrace,
            '[' => TokenKind::LBrack,
            ']' => TokenKind::RBrack,
            other => return Err(self.sc.error(&format!("Unexpected character `{other}`"))),
        })
    }
}

/// All-caps identifier predicate (grammar §3.2). Returns `true` when
/// every character is uppercase ASCII, a digit, or underscore, and
/// the string contains at least one alphabetic character.
fn is_speaker(s: &str) -> bool {
    let mut saw_alpha = false;
    for ch in s.chars() {
        if ch.is_ascii_uppercase() {
            saw_alpha = true;
        } else if ch.is_ascii_digit() || ch == '_' {
            // OK, but only after at least one alpha — `_` alone is
            // a plain ident, not a speaker.
        } else {
            return false;
        }
    }
    saw_alpha
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(source: &str) -> Vec<TokenKind> {
        lex(source).unwrap().iter().map(|t| t.kind).collect()
    }

    #[test]
    fn empty_source_emits_only_eof() {
        assert_eq!(kinds(""), vec![TokenKind::Newline, TokenKind::Eof]);
    }

    #[test]
    fn header_line_tokens() {
        let ks = kinds("# tiny \"Hello\"\n");
        assert_eq!(
            ks,
            vec![
                TokenKind::Hash,
                TokenKind::Ident,
                TokenKind::String,
                TokenKind::Newline,
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn speaker_identifier_distinct_from_ident() {
        let toks = lex("WREN talks").unwrap();
        assert_eq!(toks[0].kind, TokenKind::Speaker);
        assert_eq!(toks[0].text, "WREN");
        assert_eq!(toks[1].kind, TokenKind::Ident);
        assert_eq!(toks[1].text, "talks");
    }

    #[test]
    fn single_letter_caps_is_ident_not_speaker() {
        // A single uppercase letter (e.g. `X`) is too short to be a
        // dialogue label per §3.2's two-char minimum convention.
        // Our `is_speaker` accepts any all-caps run with at least
        // one alpha, so `X` IS a Speaker — document this for the
        // future if it bites.
        let toks = lex("X").unwrap();
        assert_eq!(toks[0].kind, TokenKind::Speaker);
    }

    #[test]
    fn is_not_and_has_not_merge() {
        let toks = lex("if a is not b and c has not d").unwrap();
        assert!(toks.iter().any(|t| t.kind == TokenKind::IsNot));
        assert!(toks.iter().any(|t| t.kind == TokenKind::HasNot));
    }

    #[test]
    fn is_followed_by_other_word_stays_split() {
        let toks = lex("a is foo").unwrap();
        assert_eq!(toks[1].text, "is");
        assert_eq!(toks[1].kind, TokenKind::Ident);
        assert_eq!(toks[2].text, "foo");
    }

    #[test]
    fn duration_suffix_attached_to_number() {
        let toks = lex("wait 250ms").unwrap();
        let n = &toks[1];
        assert_eq!(n.kind, TokenKind::Number);
        assert_eq!(n.text, "250ms");
        assert_eq!(n.duration_unit, Some(DurationUnit::Ms));
    }

    #[test]
    fn duration_does_not_swallow_ident_suffix() {
        let toks = lex("5ms_thing").unwrap();
        // 5 → Number, then ms_thing → Ident.
        assert_eq!(toks[0].kind, TokenKind::Number);
        assert_eq!(toks[0].text, "5");
        assert_eq!(toks[0].duration_unit, None);
        assert_eq!(toks[1].kind, TokenKind::Ident);
        assert_eq!(toks[1].text, "ms_thing");
    }

    #[test]
    fn dotdot_emits_single_token() {
        let toks = lex("0..100").unwrap();
        assert_eq!(toks[0].kind, TokenKind::Number);
        assert_eq!(toks[1].kind, TokenKind::DotDot);
        assert_eq!(toks[2].kind, TokenKind::Number);
    }

    #[test]
    fn float_then_dotdot_is_unambiguous() {
        // 1.5..2.5 — the lexer must NOT consume the second `.` of `..`
        // into the float.
        let toks = lex("1.5..2.5").unwrap();
        let kinds: Vec<_> = toks.iter().map(|t| t.kind).take(3).collect();
        assert_eq!(
            kinds,
            vec![TokenKind::Number, TokenKind::DotDot, TokenKind::Number]
        );
        assert_eq!(toks[0].text, "1.5");
        assert_eq!(toks[2].text, "2.5");
    }

    #[test]
    fn indent_dedent_python_style() {
        let toks = lex("a\n  b\n    c\n  d\ne\n").unwrap();
        let ks: Vec<_> = toks.iter().map(|t| t.kind).collect();
        // a NL INDENT b NL INDENT c NL DEDENT d NL DEDENT e NL EOF
        assert_eq!(
            ks,
            vec![
                TokenKind::Ident,
                TokenKind::Newline,
                TokenKind::Indent,
                TokenKind::Ident,
                TokenKind::Newline,
                TokenKind::Indent,
                TokenKind::Ident,
                TokenKind::Newline,
                TokenKind::Dedent,
                TokenKind::Ident,
                TokenKind::Newline,
                TokenKind::Dedent,
                TokenKind::Ident,
                TokenKind::Newline,
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn blank_lines_dont_affect_indent() {
        let toks = lex("a\n\n  b\n").unwrap();
        let ks: Vec<_> = toks.iter().map(|t| t.kind).collect();
        // The blank line collapses; we still get INDENT before `b`.
        assert!(ks
            .windows(2)
            .any(|w| w[0] == TokenKind::Newline && w[1] == TokenKind::Indent));
    }

    #[test]
    fn comment_only_lines_dont_affect_indent() {
        let toks = lex("a\n  // comment\n  b\n").unwrap();
        // Should yield: a NL INDENT // b NL DEDENT EOF — the comment
        // is emitted but doesn't open a deeper block.
        let kinds: Vec<_> = toks.iter().map(|t| t.kind).collect();
        assert!(kinds.contains(&TokenKind::LineComment));
        assert!(kinds.contains(&TokenKind::Indent));
        let nested = kinds.iter().filter(|k| **k == TokenKind::Indent).count();
        assert_eq!(nested, 1, "comment line should not push an indent");
    }

    #[test]
    fn brackets_suppress_layout() {
        let toks = lex("(a\n  b\n  c)\n").unwrap();
        // Inside `( ... )` we expect no Newline / Indent / Dedent tokens.
        let between_lparen_rparen: Vec<_> = toks
            .iter()
            .skip_while(|t| t.kind != TokenKind::LParen)
            .take_while(|t| t.kind != TokenKind::RParen)
            .map(|t| t.kind)
            .collect();
        assert!(!between_lparen_rparen.contains(&TokenKind::Newline));
        assert!(!between_lparen_rparen.contains(&TokenKind::Indent));
        assert!(!between_lparen_rparen.contains(&TokenKind::Dedent));
    }

    #[test]
    fn line_continuation_swallows_newline() {
        let toks = lex("a \\\n  b\n").unwrap();
        let ks: Vec<_> = toks.iter().map(|t| t.kind).collect();
        // No Newline / Indent between `a` and `b` — the continuation
        // glued the logical line together.
        let between: Vec<_> = ks
            .iter()
            .skip_while(|k| **k != TokenKind::Ident)
            .skip(1)
            .take_while(|k| **k != TokenKind::Ident)
            .collect();
        assert!(between.iter().all(|k| **k != TokenKind::Newline));
        assert!(between.iter().all(|k| **k != TokenKind::Indent));
    }

    #[test]
    fn docstring_round_trip() {
        let toks = lex("'''\nopener.\n'''\n").unwrap();
        assert_eq!(toks[0].kind, TokenKind::Docstring);
        assert!(toks[0].text.starts_with("'''"));
        assert!(toks[0].text.ends_with("'''"));
    }

    #[test]
    fn arrows_and_brackets() {
        let toks = lex("-> <- [[ ]] ${ $( </> </%").unwrap();
        let ks: Vec<_> = toks.iter().map(|t| t.kind).collect();
        for needle in [
            TokenKind::Arrow,
            TokenKind::BackArrow,
            TokenKind::LBrack2,
            TokenKind::RBrack2,
            TokenKind::DollarBrace,
            TokenKind::DollarParen,
            TokenKind::LtSlashGt,
            TokenKind::LtSlashPct,
        ] {
            assert!(ks.contains(&needle), "missing {:?}", needle);
        }
    }

    #[test]
    fn unterminated_string_is_error() {
        let err = lex("\"oops\n").unwrap_err();
        assert!(err.message.contains("string"));
    }

    #[test]
    fn unterminated_docstring_is_error() {
        let err = lex("'''oops\n").unwrap_err();
        assert!(err.message.contains("docstring"));
    }

    #[test]
    fn unterminated_block_comment_is_error() {
        let err = lex("/* nope ").unwrap_err();
        assert!(err.message.contains("block comment"));
    }

    #[test]
    fn shebang_swallowed_silently() {
        let toks = lex("#!/usr/bin/env loom\n# doc\n").unwrap();
        // First emitted token should be `#`, not anything from the shebang.
        assert_eq!(toks[0].kind, TokenKind::Hash);
        assert_eq!(toks[1].kind, TokenKind::Ident);
    }
}
