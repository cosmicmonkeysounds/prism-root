//! Stitch a [`Vec<ScannedLine>`] into a [`LoomFile`] AST.
//!
//! Phase-2 scope:
//! * Header (`# Title` + `key: value` properties)
//! * Declarations (raw body — sub-grammar deferred)
//! * Top-level `let` bindings
//! * Beats with contract + body
//! * Body items: scene heading, action paragraphs, dialogue blocks
//!   with parentheticals, choices with indented continuation,
//!   diverts, raw directives, raw metadata fences
//!
//! Out of scope here (later phases):
//! * Resolving directive verbs against the runtime registry
//! * Lowering declaration bodies into Simulacra / Meridian / Scene /
//!   Generator structures
//! * Lowering `let` expressions into the reactive graph
//! * Parsing inline `<…>` / `{…}` / `[…]` substitutions inside
//!   action / dialogue text (kept as raw text for now)

use indexmap::IndexMap;

use crate::ast::*;
use crate::diagnostics::{Code, Diagnostic};
use crate::lexer::{scan, LineKind, ScannedLine};
use crate::source::{Position, Span};

/// Parse a single `.loom` source string into a [`LoomFile`] plus a
/// diagnostic stream.
pub fn parse(source: &str) -> (LoomFile, Vec<Diagnostic>) {
    let (lines, mut diagnostics) = scan(source);
    let mut parser = Parser {
        lines,
        cursor: 0,
        diagnostics: &mut diagnostics,
    };
    let file = parser.parse_file();
    (file, diagnostics)
}

struct Parser<'d> {
    lines: Vec<ScannedLine>,
    cursor: usize,
    /// Reserved for Phase-3 structural diagnostics. Currently unused;
    /// the line-classifier in `lexer` is responsible for every
    /// diagnostic surfaced today.
    #[allow(dead_code)]
    diagnostics: &'d mut Vec<Diagnostic>,
}

impl<'d> Parser<'d> {
    fn parse_file(&mut self) -> LoomFile {
        let header = self.parse_header();
        let mut items = Vec::new();
        while let Some(line) = self.peek() {
            match &line.kind {
                LineKind::DeclarationOpener { .. } => {
                    items.push(Item::Declaration(self.parse_declaration()));
                }
                LineKind::LetBinding { .. } => {
                    items.push(Item::LetBinding(self.parse_let_binding()));
                }
                LineKind::KnotMarker(_) => {
                    items.push(Item::Beat(self.parse_beat()));
                }
                // Anything else outside a beat is silently skipped
                // for now — Phase 2 doesn't model free-floating
                // top-level prose. A diagnostic for genuinely
                // unexpected content lands in Phase 3.
                _ => {
                    self.cursor += 1;
                }
            }
        }
        LoomFile { header, items }
    }

    fn parse_header(&mut self) -> Header {
        let start = self.peek().map(|l| l.span().start).unwrap_or_default();
        let mut title = None;
        let mut properties: IndexMap<String, PropertyValue> = IndexMap::new();
        let mut end = start;

        while let Some(line) = self.peek() {
            match &line.kind {
                LineKind::Heading(text) => {
                    if title.is_none() {
                        title = Some(text.clone());
                    }
                    end = line.span().end;
                    self.cursor += 1;
                }
                LineKind::Property { key, value } => {
                    let span = line.span();
                    properties.insert(
                        key.clone(),
                        PropertyValue {
                            value: value.clone(),
                            span,
                        },
                    );
                    end = span.end;
                    self.cursor += 1;
                }
                _ => break,
            }
        }

        Header {
            title,
            properties,
            span: Span::new(start, end),
        }
    }

    fn parse_declaration(&mut self) -> Declaration {
        let opener = self.lines[self.cursor].clone();
        let (kind_word, name, mixin) = match &opener.kind {
            LineKind::DeclarationOpener {
                kind_word,
                name,
                mixin,
            } => (kind_word.clone(), name.clone(), mixin.clone()),
            _ => unreachable!("parse_declaration entered without an opener"),
        };
        let kind = DeclarationKind::from_keyword(&kind_word)
            .expect("classify() already validated the keyword");
        self.cursor += 1;

        let body_indent_floor = opener.indent + 1;
        let mut body = Vec::new();
        let mut end = opener.span().end;
        while let Some(line) = self.peek() {
            if line.indent < body_indent_floor {
                break;
            }
            body.push(RawLine {
                indent: line.indent,
                text: line.text.clone(),
                span: line.span(),
            });
            end = line.span().end;
            self.cursor += 1;
        }

        Declaration {
            kind,
            name,
            mixin,
            body,
            span: Span::new(opener.span().start, end),
        }
    }

    fn parse_let_binding(&mut self) -> LetBinding {
        let line = self.lines[self.cursor].clone();
        self.cursor += 1;
        let span = line.span();
        match line.kind {
            LineKind::LetBinding { name, expression } => LetBinding {
                name,
                expression,
                span,
            },
            _ => unreachable!(),
        }
    }

    fn parse_beat(&mut self) -> Beat {
        let opener = self.lines[self.cursor].clone();
        let name = match &opener.kind {
            LineKind::KnotMarker(name) => name.clone(),
            _ => unreachable!(),
        };
        self.cursor += 1;

        let contract = self.parse_contract(opener.indent);
        let body_indent_floor = opener.indent;
        let mut body = Vec::new();
        let mut end = opener.span().end;
        while let Some(line) = self.peek() {
            match &line.kind {
                LineKind::KnotMarker(_) => break,
                LineKind::DeclarationOpener { .. } if line.indent <= body_indent_floor => break,
                _ => {}
            }
            if let Some(item) = self.parse_body_item(body_indent_floor) {
                end = body_item_end(&item).unwrap_or(end);
                body.push(item);
            } else {
                self.cursor += 1;
            }
        }

        Beat {
            name,
            contract,
            body,
            span: Span::new(opener.span().start, end),
        }
    }

    /// Consume property lines that are indented strictly more than the
    /// beat / declaration opener — these form the contract zone
    /// (`cast:`, `setting:`, `with topic:`).
    fn parse_contract(&mut self, opener_indent: u32) -> IndexMap<String, PropertyValue> {
        let mut out: IndexMap<String, PropertyValue> = IndexMap::new();
        while let Some(line) = self.peek() {
            if line.indent <= opener_indent {
                break;
            }
            match &line.kind {
                LineKind::Property { key, value } => {
                    out.insert(
                        key.clone(),
                        PropertyValue {
                            value: value.clone(),
                            span: line.span(),
                        },
                    );
                    self.cursor += 1;
                }
                _ => break,
            }
        }
        out
    }

    fn parse_body_item(&mut self, beat_indent: u32) -> Option<BodyItem> {
        let line = self.peek()?.clone();
        match &line.kind {
            LineKind::SceneHeading(text) => {
                self.cursor += 1;
                Some(BodyItem::SceneHeading(Located {
                    value: text.clone(),
                    span: line.span(),
                }))
            }
            LineKind::Speaker(speaker) => Some(BodyItem::Dialogue(
                self.parse_dialogue_block(speaker.clone(), &line),
            )),
            LineKind::Choice { sticky, text } => Some(BodyItem::Choice(self.parse_choice(
                *sticky,
                text.clone(),
                &line,
                beat_indent,
            ))),
            LineKind::DivertLine(target) => {
                self.cursor += 1;
                Some(BodyItem::Divert(parse_divert_text(target, line.span())))
            }
            LineKind::TunnelReturn => {
                self.cursor += 1;
                Some(BodyItem::Divert(Divert::Return { span: line.span() }))
            }
            LineKind::Fence { tail, inline_close } => Some(BodyItem::Metadata(self.collect_fence(
                tail.clone(),
                *inline_close,
                &line,
            ))),
            LineKind::Directive(raw) => {
                // Only `<if: cond>` opens a NEW conditional chain.
                // `<else if:>` / `<else>` are consumed as siblings
                // inside `parse_conditional`. A bare `<else>` with no
                // preceding `<if:>` falls through to the plain
                // Directive path.
                if raw.trim_start().starts_with("if:") {
                    Some(BodyItem::Conditional(self.parse_conditional(&line)))
                } else if self.directive_has_body(&line) {
                    Some(BodyItem::DirectiveBlock(
                        self.parse_directive_block(raw.clone(), &line),
                    ))
                } else {
                    self.cursor += 1;
                    Some(BodyItem::Directive(crate::ast::Directive {
                        raw: raw.clone(),
                        span: line.span(),
                    }))
                }
            }
            LineKind::Prose(text) => {
                // Action paragraph — accumulate consecutive prose
                // lines at the same indent.
                let span = self.collect_action(&line);
                Some(BodyItem::Action(Located {
                    value: text.clone(),
                    span,
                }))
            }
            LineKind::Parenthetical(_) => {
                // A bare parenthetical with no preceding speaker is
                // an action stage direction. Treated as a Located
                // string so the director sees it.
                self.cursor += 1;
                let value = match &line.kind {
                    LineKind::Parenthetical(p) => format!("({p})"),
                    _ => unreachable!(),
                };
                Some(BodyItem::Action(Located {
                    value,
                    span: line.span(),
                }))
            }
            // A property line inside a beat body is part of a
            // continued contract or an inline divert parameter; the
            // contract zone is captured up-front by `parse_contract`
            // so leftover ones are skipped.
            LineKind::Property { .. } => {
                self.cursor += 1;
                None
            }
            LineKind::LetBinding { .. }
            | LineKind::Heading(_)
            | LineKind::DeclarationOpener { .. }
            | LineKind::KnotMarker(_) => None,
        }
    }

    fn parse_dialogue_block(&mut self, speaker: String, opener: &ScannedLine) -> DialogueBlock {
        self.cursor += 1;
        let body_indent_floor = opener.indent + 1;
        let mut parenthetical = None;
        let mut lines = Vec::new();
        let mut end = opener.span().end;

        while let Some(line) = self.peek() {
            if line.indent < body_indent_floor {
                break;
            }
            match &line.kind {
                LineKind::Parenthetical(text) => {
                    let span = line.span();
                    if parenthetical.is_none() && lines.is_empty() {
                        parenthetical = Some(text.clone());
                    } else {
                        lines.push(DialogueLine::Parenthetical(Located {
                            value: text.clone(),
                            span,
                        }));
                    }
                    end = span.end;
                    self.cursor += 1;
                }
                LineKind::DivertLine(target) => {
                    let span = line.span();
                    lines.push(DialogueLine::Divert(parse_divert_text(target, span)));
                    end = span.end;
                    self.cursor += 1;
                }
                LineKind::TunnelReturn => {
                    let span = line.span();
                    lines.push(DialogueLine::Divert(Divert::Return { span }));
                    end = span.end;
                    self.cursor += 1;
                }
                LineKind::Prose(text) | LineKind::SceneHeading(text) => {
                    let span = line.span();
                    lines.push(DialogueLine::Text(Located {
                        value: text.clone(),
                        span,
                    }));
                    end = span.end;
                    self.cursor += 1;
                }
                LineKind::Directive(raw) => {
                    let span = line.span();
                    lines.push(DialogueLine::Directive(crate::ast::Directive {
                        raw: raw.clone(),
                        span,
                    }));
                    end = span.end;
                    self.cursor += 1;
                }
                _ => break,
            }
        }

        DialogueBlock {
            speaker,
            parenthetical,
            lines,
            span: Span::new(opener.span().start, end),
        }
    }

    fn parse_choice(
        &mut self,
        sticky: bool,
        raw_text: String,
        opener: &ScannedLine,
        _beat_indent: u32,
    ) -> Choice {
        self.cursor += 1;
        let (text, suppressed) = split_choice_suppression(&raw_text);
        let body_indent_floor = opener.indent + 1;
        let mut body = Vec::new();
        let mut end = opener.span().end;
        while let Some(line) = self.peek() {
            if line.indent < body_indent_floor {
                break;
            }
            if let Some(item) = self.parse_body_item(body_indent_floor) {
                end = body_item_end(&item).unwrap_or(end);
                body.push(item);
            } else {
                self.cursor += 1;
            }
        }
        Choice {
            sticky,
            text,
            suppressed,
            body,
            span: Span::new(opener.span().start, end),
        }
    }

    /// Accumulate an action paragraph — successive prose lines at
    /// the same indent are concatenated with a single newline, the
    /// way a reader (or director) sees them on the page.
    fn collect_action(&mut self, opener: &ScannedLine) -> Span {
        let start = opener.span().start;
        let mut end = opener.span().end;
        self.cursor += 1;
        while let Some(line) = self.peek() {
            if line.indent != opener.indent {
                break;
            }
            if let LineKind::Prose(_) = &line.kind {
                end = line.span().end;
                self.cursor += 1;
            } else {
                break;
            }
        }
        Span::new(start, end)
    }

    fn collect_fence(
        &mut self,
        tail: String,
        inline_close: bool,
        opener: &ScannedLine,
    ) -> Located<String> {
        self.cursor += 1;
        if inline_close {
            return Located {
                value: tail,
                span: opener.span(),
            };
        }
        let mut buf = tail;
        let mut end = opener.span().end;
        while let Some(line) = self.peek().cloned() {
            if let LineKind::Fence { tail, inline_close } = &line.kind {
                end = line.span().end;
                self.cursor += 1;
                if tail.is_empty() && !inline_close {
                    break;
                }
            } else {
                if !buf.is_empty() {
                    buf.push('\n');
                }
                buf.push_str(&line.text);
                end = line.span().end;
                self.cursor += 1;
            }
        }
        Located {
            value: buf,
            span: Span::new(opener.span().start, end),
        }
    }

    fn peek(&self) -> Option<&ScannedLine> {
        self.lines.get(self.cursor)
    }

    fn peek_ahead(&self, offset: usize) -> Option<&ScannedLine> {
        self.lines.get(self.cursor + offset)
    }

    /// True if the next line after the current directive is indented
    /// strictly further. Used to decide whether to consume body items
    /// into a `DirectiveBlock` vs leaving the directive standalone.
    fn directive_has_body(&self, opener: &ScannedLine) -> bool {
        match self.peek_ahead(1) {
            Some(next) => next.indent > opener.indent,
            None => false,
        }
    }

    fn parse_conditional(&mut self, opener: &ScannedLine) -> Conditional {
        let opener_indent = opener.indent;
        let start = opener.span().start;
        let mut end = opener.span().end;
        let mut arms = Vec::new();
        let mut saw_opener = false;
        while let Some(line) = self.peek().cloned() {
            if line.indent != opener_indent {
                break;
            }
            let raw = match &line.kind {
                LineKind::Directive(r) => r.clone(),
                _ => break,
            };
            let trimmed = raw.trim_start();
            let cond = if !saw_opener && trimmed.starts_with("if:") {
                // Leading `if:` — open the chain.
                Some(trimmed.trim_start_matches("if:").trim().to_string())
            } else if saw_opener && trimmed.starts_with("else if:") {
                Some(trimmed.trim_start_matches("else if:").trim().to_string())
            } else if saw_opener && trimmed.trim() == "else" {
                None
            } else {
                // A second top-level `<if:>` is a NEW chain — not a
                // continuation. Bail out and let the outer loop pick
                // it up as a sibling.
                break;
            };
            saw_opener = true;
            self.cursor += 1;
            let body_indent_floor = opener_indent + 1;
            let mut body = Vec::new();
            let mut arm_end = line.span().end;
            while let Some(child) = self.peek() {
                if child.indent < body_indent_floor {
                    break;
                }
                if let Some(item) = self.parse_body_item(body_indent_floor) {
                    arm_end = body_item_end(&item).unwrap_or(arm_end);
                    body.push(item);
                } else {
                    self.cursor += 1;
                }
            }
            end = arm_end;
            arms.push(ConditionalArm {
                condition: cond,
                body,
                span: Span::new(line.span().start, arm_end),
            });
        }
        Conditional {
            arms,
            span: Span::new(start, end),
        }
    }

    fn parse_directive_block(&mut self, raw: String, opener: &ScannedLine) -> DirectiveBlock {
        self.cursor += 1;
        let body_indent_floor = opener.indent + 1;
        let mut body = Vec::new();
        let mut end = opener.span().end;
        while let Some(child) = self.peek() {
            if child.indent < body_indent_floor {
                break;
            }
            if let Some(item) = self.parse_body_item(body_indent_floor) {
                end = body_item_end(&item).unwrap_or(end);
                body.push(item);
            } else {
                self.cursor += 1;
            }
        }
        DirectiveBlock {
            directive: crate::ast::Directive {
                raw,
                span: opener.span(),
            },
            body,
            span: Span::new(opener.span().start, end),
        }
    }
}

fn body_item_end(item: &BodyItem) -> Option<Position> {
    Some(match item {
        BodyItem::SceneHeading(l) => l.span.end,
        BodyItem::Action(l) => l.span.end,
        BodyItem::Dialogue(b) => b.span.end,
        BodyItem::Choice(c) => c.span.end,
        BodyItem::Divert(d) => divert_span(d).end,
        BodyItem::Directive(d) => d.span.end,
        BodyItem::Metadata(l) => l.span.end,
        BodyItem::Conditional(c) => c.span.end,
        BodyItem::DirectiveBlock(d) => d.span.end,
    })
}

fn divert_span(d: &Divert) -> Span {
    match d {
        Divert::To { span, .. }
        | Divert::Tunnel { span, .. }
        | Divert::Return { span }
        | Divert::End { span } => *span,
    }
}

fn split_choice_suppression(raw: &str) -> (String, Option<String>) {
    if let Some(open) = raw.find('[') {
        if let Some(close_rel) = raw[open + 1..].find(']') {
            let close = open + 1 + close_rel;
            let visible = format!("{}{}", &raw[..open], &raw[close + 1..]);
            let suppressed = raw[open + 1..close].to_string();
            return (visible.trim().to_string(), Some(suppressed));
        }
    }
    (raw.trim().to_string(), None)
}

fn parse_divert_text(text: &str, span: Span) -> Divert {
    let text = text.trim();
    if text == "END" {
        return Divert::End { span };
    }
    // Tunnel call form: `(name) ->` or `(name with k: v) ->`.
    if let Some(rest) = text.strip_prefix('(') {
        if let Some(end_paren) = rest.find(')') {
            let inner = rest[..end_paren].trim();
            let target = parse_divert_target(inner);
            return Divert::Tunnel { target, span };
        }
    }
    let (head, params_text) = match text.find(" with ") {
        Some(idx) => (&text[..idx], Some(&text[idx + 6..])),
        None => (text, None),
    };
    let target = parse_divert_target(head.trim());
    let params = parse_divert_params(params_text.unwrap_or(""));
    Divert::To {
        target,
        params,
        span,
    }
}

fn parse_divert_target(text: &str) -> DivertTarget {
    let (file_part, knot) = match text.find('#') {
        Some(idx) => (&text[..idx], Some(text[idx + 1..].trim().to_string())),
        None => (text, None),
    };
    if let Some(slash) = file_part.rfind('/') {
        DivertTarget {
            qualifier: Some(file_part[..slash].to_string()),
            name: file_part[slash + 1..].to_string(),
            knot,
        }
    } else {
        DivertTarget {
            qualifier: None,
            name: file_part.to_string(),
            knot,
        }
    }
}

fn parse_divert_params(text: &str) -> IndexMap<String, String> {
    let mut out = IndexMap::new();
    if text.trim().is_empty() {
        return out;
    }
    for chunk in text.split(',') {
        let chunk = chunk.trim();
        if let Some((k, v)) = chunk.split_once(':') {
            out.insert(k.trim().to_string(), v.trim().to_string());
        }
    }
    out
}

// `Code` is used in tests below; pull the warning silencer where applicable.
#[allow(dead_code)]
fn _code_used_in_diagnostics(_: Code) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_collects_title_and_properties() {
        let (file, diags) = parse("# Saltmere\nentry: opening\n");
        assert!(diags.is_empty(), "{diags:?}");
        assert_eq!(file.header.title.as_deref(), Some("Saltmere"));
        assert_eq!(
            file.header
                .properties
                .get("entry")
                .map(|v| v.value.as_str()),
            Some("opening")
        );
    }

    #[test]
    fn beat_contract_then_body() {
        let src = "\
== opening
  cast: Wren, Player
  setting: Lighthouse

A bell rope swings in the gloom.

WREN
  (quietly)
  It hasn't rung in three days.

* Ring the bell.
  -> ringing
* Leave quietly.
  -> END
";
        let (file, diags) = parse(src);
        assert!(diags.is_empty(), "{diags:?}");
        assert_eq!(file.items.len(), 1);
        let beat = match &file.items[0] {
            Item::Beat(b) => b,
            other => panic!("expected beat, got {other:?}"),
        };
        assert_eq!(beat.name, "opening");
        assert_eq!(beat.contract.get("cast").unwrap().value, "Wren, Player");
        assert_eq!(beat.contract.get("setting").unwrap().value, "Lighthouse");
        // Expect: action, dialogue, choice, choice.
        assert_eq!(beat.body.len(), 4);
        assert!(matches!(beat.body[0], BodyItem::Action(_)));
        let dialogue = match &beat.body[1] {
            BodyItem::Dialogue(d) => d,
            _ => panic!("expected dialogue"),
        };
        assert_eq!(dialogue.speaker, "WREN");
        assert_eq!(dialogue.parenthetical.as_deref(), Some("quietly"));
        assert_eq!(dialogue.lines.len(), 1);
        let choice = match &beat.body[2] {
            BodyItem::Choice(c) => c,
            _ => panic!("expected choice"),
        };
        assert_eq!(choice.text, "Ring the bell.");
        assert_eq!(choice.body.len(), 1);
        match &choice.body[0] {
            BodyItem::Divert(Divert::To { target, .. }) => {
                assert_eq!(target.name, "ringing");
                assert!(target.qualifier.is_none());
                assert!(target.knot.is_none());
            }
            other => panic!("expected divert, got {other:?}"),
        }
    }

    #[test]
    fn ink_style_suppression() {
        let (file, _) = parse("== opening\n* \"Yes.\"[ I said firmly.]\n  -> END\n");
        let beat = match &file.items[0] {
            Item::Beat(b) => b,
            _ => panic!(),
        };
        let choice = match &beat.body[0] {
            BodyItem::Choice(c) => c,
            _ => panic!(),
        };
        assert_eq!(choice.text, "\"Yes.\"");
        assert_eq!(choice.suppressed.as_deref(), Some(" I said firmly."));
    }

    #[test]
    fn divert_with_parameters_and_qualifier() {
        let (file, _) =
            parse("== opening\n* Ask.\n  -> Lighthouse/ringing with topic: bell, NPC: Wren\n");
        let beat = match &file.items[0] {
            Item::Beat(b) => b,
            _ => panic!(),
        };
        let choice = match &beat.body[0] {
            BodyItem::Choice(c) => c,
            _ => panic!(),
        };
        match &choice.body[0] {
            BodyItem::Divert(Divert::To { target, params, .. }) => {
                assert_eq!(target.qualifier.as_deref(), Some("Lighthouse"));
                assert_eq!(target.name, "ringing");
                assert_eq!(params.get("topic").map(String::as_str), Some("bell"));
                assert_eq!(params.get("NPC").map(String::as_str), Some("Wren"));
            }
            other => panic!("expected parameterised divert, got {other:?}"),
        }
    }

    #[test]
    fn declaration_keeps_body_raw() {
        let src = "\
CHARACTER Wren is Keeper, Combatant
  voice: female_alto
  hp: 80
  on meeting Player
    -> introduce
";
        let (file, diags) = parse(src);
        assert!(diags.is_empty(), "{diags:?}");
        let decl = match &file.items[0] {
            Item::Declaration(d) => d,
            _ => panic!(),
        };
        assert_eq!(decl.kind, DeclarationKind::Character);
        assert_eq!(decl.name, "Wren");
        assert_eq!(decl.mixin, vec!["Keeper", "Combatant"]);
        assert!(decl.body.len() >= 4);
    }

    #[test]
    fn let_binding_is_top_level_item() {
        let (file, _) = parse("let trusted = Wren.trusts.Player > 50\n");
        match &file.items[0] {
            Item::LetBinding(l) => {
                assert_eq!(l.name, "trusted");
                assert_eq!(l.expression, "Wren.trusts.Player > 50");
            }
            other => panic!("expected let binding, got {other:?}"),
        }
    }

    #[test]
    fn conditional_arms_are_grouped() {
        let src = "\
== opening

<if: trust > 50>
  WREN
    You may pass.
<else if: trust > 20>
  WREN
    Maybe later.
<else>
  WREN
    Leave. Now.
  -> END
";
        let (file, diags) = parse(src);
        assert!(diags.is_empty(), "{diags:?}");
        let beat = match &file.items[0] {
            Item::Beat(b) => b,
            other => panic!("expected beat, got {other:?}"),
        };
        let cond = match &beat.body[0] {
            BodyItem::Conditional(c) => c,
            other => panic!("expected conditional, got {other:?}"),
        };
        assert_eq!(cond.arms.len(), 3);
        assert_eq!(cond.arms[0].condition.as_deref(), Some("trust > 50"));
        assert_eq!(cond.arms[1].condition.as_deref(), Some("trust > 20"));
        assert!(cond.arms[2].condition.is_none());
        // The else arm carries the dialogue + the divert.
        assert_eq!(cond.arms[2].body.len(), 2);
    }

    #[test]
    fn directive_block_collects_indented_body() {
        let (file, _) =
            parse("== opening\n\n<broadcast: location(BellTower)>\n  WREN\n    Listen.\n");
        let beat = match &file.items[0] {
            Item::Beat(b) => b,
            _ => panic!(),
        };
        let block = match &beat.body[0] {
            BodyItem::DirectiveBlock(d) => d,
            other => panic!("expected directive block, got {other:?}"),
        };
        assert!(block.directive.raw.starts_with("broadcast"));
        assert_eq!(block.body.len(), 1);
    }

    #[test]
    fn metadata_fence_is_collected() {
        let (file, _) = parse("== opening\n```note\nThis felt long.\n```\n");
        let beat = match &file.items[0] {
            Item::Beat(b) => b,
            _ => panic!(),
        };
        let meta = match &beat.body[0] {
            BodyItem::Metadata(m) => m,
            _ => panic!("expected metadata"),
        };
        assert!(meta.value.contains("note"));
        assert!(meta.value.contains("This felt long."));
    }
}
