//! Line-oriented scanner for the Loom v3 surface.
//!
//! Walks source bytes line-by-line and emits a [`ScannedLine`] for
//! every non-blank line. Each line is classified by its leading
//! tokens — speaker / choice / divert / declaration opener / heading
//! / property / fence / fallback — so the parser can stitch them
//! into the AST without re-scanning characters.
//!
//! Position-as-syntax (spec §2) is implemented here: indentation is
//! a structural signal, ALL-CAPS lines are speakers, lines starting
//! with `*` / `+` / `->` / `==` / `#` are syntactic.

use crate::diagnostics::{Code, Diagnostic};
use crate::source::{Position, Span};

/// One classified non-blank source line.
#[derive(Clone, Debug)]
pub struct ScannedLine {
    /// 0-based line index in the source file.
    pub line: u32,
    /// Column (in bytes) where the line's content begins, i.e. the
    /// indent. Trailing whitespace is not counted toward the next
    /// line's indent.
    pub indent: u32,
    /// The trimmed content (no leading indent, no trailing newline).
    pub text: String,
    /// Byte offset in source where the line's content begins.
    pub start_byte: u32,
    /// Byte offset in source where the line's content ends.
    pub end_byte: u32,
    pub kind: LineKind,
}

impl ScannedLine {
    pub fn span(&self) -> Span {
        Span::new(
            Position::new(self.line, self.indent, self.start_byte),
            Position::new(
                self.line,
                self.indent + (self.text.len() as u32),
                self.end_byte,
            ),
        )
    }
}

/// Surface classification of a single line. Carries no body parse —
/// the parser splits inline `<…>` / `[…]` / `{…}` content out of
/// `text` when it lowers to the AST.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LineKind {
    /// `# Title` — first-class header heading. Stores the title text
    /// (everything after `#` and surrounding whitespace).
    Heading(String),
    /// `key: value` — a property line in the header *or* the
    /// contract zone under a `==` opener.
    Property { key: String, value: String },
    /// `== knot_name` (with optional trailing whitespace).
    KnotMarker(String),
    /// `INT.` / `EXT.` Fountain-style scene heading.
    SceneHeading(String),
    /// `let name = expr`.
    LetBinding { name: String, expression: String },
    /// `CHARACTER Name [is Mixin, ...]` and friends.
    DeclarationOpener {
        kind_word: String,
        name: String,
        mixin: Vec<String>,
    },
    /// `* …` or `+ …` choice. `sticky` is `true` for `+`.
    Choice { sticky: bool, text: String },
    /// `-> target [with k: v, …]`, `-> END`, or
    /// `-> (name) ->` (tunnel — phase-2 emits a single `DivertLine`
    /// and the parser disambiguates).
    DivertLine(String),
    /// `<-` tunnel-return marker on its own line.
    TunnelReturn,
    /// ALL CAPS speaker cue — `WREN`, `BELLKEEPER`, `WREN | FISHER`.
    Speaker(String),
    /// `(…)` parenthetical on its own line.
    Parenthetical(String),
    /// Triple-backtick fence opener / closer / inline-pair. The
    /// parser groups multi-line fences.
    Fence { tail: String, inline_close: bool },
    /// `<kind: args>` runtime directive on its own line. Stores the
    /// inner text (without the `<`/`>`). Inline directives embedded in
    /// dialogue / action text are split out by the parser in a later
    /// phase.
    Directive(String),
    /// Fallback: action prose or dialogue continuation. Disambiguated
    /// by the parser based on whether a speaker is currently active.
    Prose(String),
}

/// Scan `source` into a list of non-blank classified lines plus
/// diagnostics. Blank lines (whitespace-only) are dropped here —
/// they're only meaningful for separating paragraphs, which the
/// parser handles via `terminates_paragraph` on the *next* non-blank
/// line.
pub fn scan(source: &str) -> (Vec<ScannedLine>, Vec<Diagnostic>) {
    let (stripped, mut diagnostics) = crate::comments::strip(source);
    let source = stripped.as_str();
    let mut lines = Vec::new();
    let mut byte: u32 = 0;
    for (line_idx, raw) in source.split_inclusive('\n').enumerate() {
        let line_start_byte = byte;
        let line_len = raw.len() as u32;
        byte = byte.saturating_add(line_len);

        let stripped = raw.strip_suffix('\n').unwrap_or(raw);
        let stripped = stripped.strip_suffix('\r').unwrap_or(stripped);

        let (indent, content_offset) = leading_indent(stripped);
        let trimmed = &stripped[content_offset..];
        let trimmed = trimmed.trim_end();
        if trimmed.is_empty() {
            continue;
        }

        if stripped[..content_offset].contains('\t') {
            diagnostics.push(Diagnostic::error(
                Code::L1001TabIndent,
                Span::new(
                    Position::new(line_idx as u32, 0, line_start_byte),
                    Position::new(
                        line_idx as u32,
                        content_offset as u32,
                        line_start_byte + content_offset as u32,
                    ),
                ),
                "indentation uses tabs; Loom v3 indents with spaces only",
            ));
        }

        let start_byte = line_start_byte + content_offset as u32;
        let end_byte = start_byte + trimmed.len() as u32;
        let kind = classify(trimmed, line_idx as u32, start_byte, &mut diagnostics);
        lines.push(ScannedLine {
            line: line_idx as u32,
            indent: indent as u32,
            text: trimmed.to_string(),
            start_byte,
            end_byte,
            kind,
        });
    }
    (lines, diagnostics)
}

/// Count leading-whitespace columns + the byte offset where content
/// begins. A tab counts as one column for classification purposes;
/// the tab-warning is raised separately so the column number still
/// reflects what the writer sees in their editor.
fn leading_indent(line: &str) -> (usize, usize) {
    let mut cols = 0usize;
    let mut byte = 0usize;
    for ch in line.chars() {
        match ch {
            ' ' | '\t' => {
                cols += 1;
                byte += ch.len_utf8();
            }
            _ => break,
        }
    }
    (cols, byte)
}

fn classify(text: &str, line: u32, start_byte: u32, diagnostics: &mut Vec<Diagnostic>) -> LineKind {
    // Header heading: `# Title` (not `##` — that's reserved for
    // future use). We accept `#` followed by whitespace + text.
    if let Some(rest) = text.strip_prefix('#') {
        if !rest.starts_with('#') {
            let title = rest.trim().to_string();
            return LineKind::Heading(title);
        }
    }

    // Knot marker: `== name`.
    if let Some(rest) = text.strip_prefix("==") {
        let name = rest.trim().to_string();
        if name.is_empty() {
            diagnostics.push(Diagnostic::error(
                Code::L1004UnnamedKnot,
                line_span(line, start_byte, text),
                "`==` knot marker is missing a name",
            ));
        }
        return LineKind::KnotMarker(name);
    }

    // Scene heading: `INT.`, `EXT.`, `INT./EXT.`, `I/E`.
    if is_scene_heading(text) {
        return LineKind::SceneHeading(text.to_string());
    }

    // `let name = expr` reactive binding.
    if let Some(rest) = text.strip_prefix("let ") {
        if let Some((name, expr)) = rest.split_once('=') {
            return LineKind::LetBinding {
                name: name.trim().to_string(),
                expression: expr.trim().to_string(),
            };
        }
    }

    // Divert / tunnel-return.
    if let Some(rest) = text.strip_prefix("->") {
        return LineKind::DivertLine(rest.trim().to_string());
    }
    if text == "<-" {
        return LineKind::TunnelReturn;
    }

    // Choice — `*` or `+` followed by required whitespace + text.
    if let Some(rest) = text.strip_prefix('*') {
        if let Some(body) = rest.strip_prefix(' ') {
            return LineKind::Choice {
                sticky: false,
                text: body.trim_start().to_string(),
            };
        }
        if rest.is_empty() {
            diagnostics.push(Diagnostic::error(
                Code::L1003EmptyChoice,
                line_span(line, start_byte, text),
                "`*` choice has no body text",
            ));
            return LineKind::Choice {
                sticky: false,
                text: String::new(),
            };
        }
    }
    if let Some(rest) = text.strip_prefix('+') {
        if let Some(body) = rest.strip_prefix(' ') {
            return LineKind::Choice {
                sticky: true,
                text: body.trim_start().to_string(),
            };
        }
    }

    // Whole-line angle-bracket directive — `<kind: args>` or `<kind>`.
    // Multi-line block-opening directives are handled by the parser via
    // indent.
    if text.starts_with('<') && text.ends_with('>') && text.len() >= 2 && text != "<-" {
        let inner = &text[1..text.len() - 1];
        return LineKind::Directive(inner.to_string());
    }

    // Triple-backtick fence.
    if let Some(rest) = text.strip_prefix("```") {
        let inline_close = rest.ends_with("```") && rest.len() >= 3;
        let tail = if inline_close {
            rest[..rest.len() - 3].to_string()
        } else {
            rest.to_string()
        };
        return LineKind::Fence { tail, inline_close };
    }

    // Declaration opener — `KEYWORD Name [is X, Y]`.
    if let Some(opener) = parse_declaration_opener(text) {
        if opener.name.is_empty() {
            diagnostics.push(Diagnostic::error(
                Code::L1006UnnamedDeclaration,
                line_span(line, start_byte, text),
                format!("`{}` declaration is missing a name", opener.kind_word),
            ));
        }
        return LineKind::DeclarationOpener {
            kind_word: opener.kind_word,
            name: opener.name,
            mixin: opener.mixin,
        };
    }

    // Parenthetical line — the whole line is wrapped in `(…)`.
    if let Some(inner) = strip_parens(text) {
        return LineKind::Parenthetical(inner.to_string());
    }

    // Property line — `key: value` (and `:` precedes everything else).
    if let Some((key, value)) = property_split(text) {
        return LineKind::Property {
            key: key.to_string(),
            value: value.to_string(),
        };
    }

    // Speaker cue — pure ALL CAPS (allowing digits, underscores,
    // spaces, and `|` for cohort-of-speakers `WREN | FISHER`).
    if is_speaker_line(text) {
        return LineKind::Speaker(text.to_string());
    }

    LineKind::Prose(text.to_string())
}

fn line_span(line: u32, start_byte: u32, text: &str) -> Span {
    Span::new(
        Position::new(line, 0, start_byte),
        Position::new(line, text.len() as u32, start_byte + text.len() as u32),
    )
}

fn is_scene_heading(text: &str) -> bool {
    let upper = text.trim_start();
    upper.starts_with("INT.")
        || upper.starts_with("EXT.")
        || upper.starts_with("INT/EXT")
        || upper.starts_with("INT./EXT.")
        || upper.starts_with("I/E ")
}

fn strip_parens(text: &str) -> Option<&str> {
    let inner = text.strip_prefix('(')?.strip_suffix(')')?;
    // Reject `(x)y(z)` patterns where parens aren't balanced as the
    // outermost wrapper. A scan suffices because parens nest only
    // shallowly in performer cues.
    let mut depth = 0i32;
    for (idx, ch) in inner.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => {
                if depth == 0 && idx + 1 != inner.len() {
                    return None;
                }
                depth -= 1;
            }
            _ => {}
        }
    }
    Some(inner)
}

fn property_split(text: &str) -> Option<(&str, &str)> {
    let colon = text.find(':')?;
    let key = &text[..colon];
    // Property keys are simple identifiers (letters, digits,
    // underscore, hyphen).
    if key.is_empty() || !key.chars().all(is_property_key_char) {
        return None;
    }
    // Require whitespace (or end of line) after the colon — this
    // rejects URL-shaped lines like `https://example.com` from being
    // mis-read as properties.
    let after = &text[colon + 1..];
    if !after.is_empty() && !after.starts_with(char::is_whitespace) {
        return None;
    }
    let value = after.trim();
    Some((key.trim(), value))
}

fn is_property_key_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_' || ch == '-'
}

fn is_speaker_line(text: &str) -> bool {
    let mut saw_letter = false;
    for ch in text.chars() {
        match ch {
            'A'..='Z' => saw_letter = true,
            '0'..='9' | '_' | ' ' | '|' => {}
            _ => return false,
        }
    }
    saw_letter
}

struct DeclarationOpener {
    kind_word: String,
    name: String,
    mixin: Vec<String>,
}

fn parse_declaration_opener(text: &str) -> Option<DeclarationOpener> {
    let mut parts = text.splitn(2, char::is_whitespace);
    let kind_word = parts.next()?.to_string();
    crate::ast::DeclarationKind::from_keyword(&kind_word)?;
    let rest = parts.next().unwrap_or("").trim();

    let (name, mixin_clause) = match rest.find(" is ") {
        Some(idx) => (rest[..idx].trim(), Some(rest[idx + 4..].trim())),
        None => (rest, None),
    };

    let mixin = mixin_clause
        .map(|c| c.split(',').map(|s| s.trim().to_string()).collect())
        .unwrap_or_default();

    Some(DeclarationOpener {
        kind_word,
        name: name.to_string(),
        mixin,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn first(text: &str) -> LineKind {
        let (lines, _) = scan(text);
        lines.into_iter().next().unwrap().kind
    }

    #[test]
    fn heading_is_parsed() {
        assert_eq!(first("# Saltmere\n"), LineKind::Heading("Saltmere".into()));
    }

    #[test]
    fn property_requires_identifier_key() {
        match first("entry: opening\n") {
            LineKind::Property { key, value } => {
                assert_eq!(key, "entry");
                assert_eq!(value, "opening");
            }
            other => panic!("expected property, got {other:?}"),
        }
        // URL-shaped lines should fall through to prose.
        assert!(matches!(first("https://example.com\n"), LineKind::Prose(_)));
    }

    #[test]
    fn knot_marker() {
        assert_eq!(
            first("== opening\n"),
            LineKind::KnotMarker("opening".into())
        );
    }

    #[test]
    fn declaration_opener_with_mixin() {
        match first("CHARACTER Wren is Keeper, Combatant\n") {
            LineKind::DeclarationOpener {
                kind_word,
                name,
                mixin,
            } => {
                assert_eq!(kind_word, "CHARACTER");
                assert_eq!(name, "Wren");
                assert_eq!(mixin, vec!["Keeper", "Combatant"]);
            }
            other => panic!("expected declaration opener, got {other:?}"),
        }
    }

    #[test]
    fn speaker_vs_action() {
        assert!(matches!(first("WREN\n"), LineKind::Speaker(_)));
        assert!(matches!(first("Wren turns away.\n"), LineKind::Prose(_)));
    }

    #[test]
    fn choice_kinds() {
        assert!(matches!(
            first("* Ring the bell.\n"),
            LineKind::Choice { sticky: false, .. }
        ));
        assert!(matches!(
            first("+ Keep talking.\n"),
            LineKind::Choice { sticky: true, .. }
        ));
    }

    #[test]
    fn divert_and_tunnel_return() {
        assert_eq!(
            first("-> ringing\n"),
            LineKind::DivertLine("ringing".into())
        );
        assert_eq!(first("<-\n"), LineKind::TunnelReturn);
    }

    #[test]
    fn parenthetical_line() {
        assert_eq!(
            first("(quietly)\n"),
            LineKind::Parenthetical("quietly".into())
        );
    }

    #[test]
    fn let_binding() {
        match first("let trusted = Wren.trusts.Player > 50\n") {
            LineKind::LetBinding { name, expression } => {
                assert_eq!(name, "trusted");
                assert_eq!(expression, "Wren.trusts.Player > 50");
            }
            other => panic!("expected let binding, got {other:?}"),
        }
    }

    #[test]
    fn scene_heading() {
        assert_eq!(
            first("INT. LIGHTHOUSE - DAWN\n"),
            LineKind::SceneHeading("INT. LIGHTHOUSE - DAWN".into())
        );
    }

    #[test]
    fn fence_inline_vs_block() {
        match first("```warn lx14```\n") {
            LineKind::Fence { tail, inline_close } => {
                assert_eq!(tail, "warn lx14");
                assert!(inline_close);
            }
            other => panic!("expected inline fence, got {other:?}"),
        }
        match first("```note\n") {
            LineKind::Fence { tail, inline_close } => {
                assert_eq!(tail, "note");
                assert!(!inline_close);
            }
            other => panic!("expected block fence opener, got {other:?}"),
        }
    }

    #[test]
    fn tab_indent_warns() {
        let (_lines, diags) = scan("\tWREN\n");
        assert!(diags.iter().any(|d| d.code == Code::L1001TabIndent));
    }

    #[test]
    fn line_comment_is_invisible_to_classifier() {
        // The bare `//` line strips to whitespace, becomes blank, gets dropped.
        let (lines, diags) = scan("// rough order: bell, beat\nWREN\n");
        assert!(diags.is_empty());
        assert_eq!(lines.len(), 1);
        assert!(matches!(lines[0].kind, LineKind::Speaker(_)));
    }

    #[test]
    fn trailing_line_comment_is_stripped_from_prose() {
        let (lines, _) = scan("It rang. // pickup pace here\n");
        assert_eq!(lines.len(), 1);
        match &lines[0].kind {
            LineKind::Prose(text) => assert_eq!(text, "It rang."),
            other => panic!("expected prose, got {other:?}"),
        }
    }

    #[test]
    fn block_comment_does_not_eat_following_speaker() {
        let (lines, diags) = scan("/* blocking sketch\nlives across lines */\nWREN\n");
        assert!(diags.is_empty());
        assert_eq!(lines.len(), 1);
        assert!(matches!(lines[0].kind, LineKind::Speaker(_)));
    }

    #[test]
    fn unterminated_block_comment_diagnoses() {
        let (_lines, diags) = scan("/* never closed\nstill open\n");
        assert!(diags
            .iter()
            .any(|d| d.code == Code::L1007UnterminatedBlockComment));
    }

    #[test]
    fn url_in_prose_is_not_mistaken_for_comment() {
        let (lines, _) = scan("See https://example.com/path for details.\n");
        assert_eq!(lines.len(), 1);
        match &lines[0].kind {
            LineKind::Prose(text) => {
                assert!(text.contains("https://example.com/path"));
            }
            other => panic!("expected prose, got {other:?}"),
        }
    }

    #[test]
    fn comment_inside_fence_is_preserved() {
        let (lines, _) = scan("```note\n// stage manager: lights low\n```\n");
        // Three fence lines (opener, content, closer) survive.
        assert_eq!(lines.len(), 3);
        // Middle line is a prose line carrying the literal `//`.
        match &lines[1].kind {
            LineKind::Prose(text) => assert!(text.starts_with("//")),
            other => panic!("expected prose inside fence, got {other:?}"),
        }
    }

    #[test]
    fn blank_lines_are_dropped() {
        let (lines, _) = scan("# T\n\n\nentry: x\n");
        assert_eq!(lines.len(), 2);
        assert!(matches!(lines[0].kind, LineKind::Heading(_)));
        assert!(matches!(lines[1].kind, LineKind::Property { .. }));
    }
}
