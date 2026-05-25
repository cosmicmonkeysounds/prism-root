//! Completion suggestions for the Loom v3 surface.
//!
//! Context detection is line-local: we look at the prefix of the
//! current line up to the cursor and decide whether the user is in a
//! divert tail, a `<…>` directive head, an `is …` mixin list, or none
//! of the above. Spec §14 + §17 drive the candidate sets.

use lsp_types::{CompletionItem, CompletionItemKind, Position, Url};

use crate::workspace::{line_at, Workspace};

/// Canonical directive verbs (spec §14). Hard-coded here because the
/// runtime registry isn't reachable from the LSP crate (and shouldn't
/// be — the LSP must stay parser-only).
pub const DIRECTIVES: &[&str] = &[
    "sfx",
    "cue",
    "set",
    "fire",
    "pause",
    "shuffle",
    "cycle",
    "spawn",
    "cancel",
    "goal",
    "broadcast",
    "enroll",
    "goto",
    "compose",
    "flash",
    "heal",
    "if",
    "else",
    "else if",
    "match",
    "for",
    "each visit",
    "after",
    "otherwise",
    "anchor",
    "let",
];

pub fn completion_at(ws: &Workspace, uri: &Url, pos: Position) -> Vec<CompletionItem> {
    let Some(doc) = ws.docs.get(uri) else {
        return Vec::new();
    };
    let line = line_at(&doc.text, pos.line);
    let col = pos.character as usize;
    let prefix = if col <= line.len() {
        &line[..col]
    } else {
        line
    };

    if is_divert_position(prefix) {
        return ws
            .beats
            .keys()
            .map(|name| simple_item(name, CompletionItemKind::FUNCTION))
            .collect();
    }

    if is_directive_position(prefix) {
        return DIRECTIVES
            .iter()
            .map(|d| simple_item(d, CompletionItemKind::KEYWORD))
            .collect();
    }

    if is_is_position(prefix) {
        let mut out: Vec<CompletionItem> = ws
            .characters
            .keys()
            .map(|n| simple_item(n, CompletionItemKind::CLASS))
            .collect();
        out.extend(
            ws.traits
                .keys()
                .map(|n| simple_item(n, CompletionItemKind::INTERFACE)),
        );
        return out;
    }

    Vec::new()
}

fn simple_item(label: &str, kind: CompletionItemKind) -> CompletionItem {
    CompletionItem {
        label: label.to_string(),
        kind: Some(kind),
        ..Default::default()
    }
}

/// True when the prefix ends with `->` (optionally followed by an
/// identifier-in-progress and arbitrary whitespace).
fn is_divert_position(prefix: &str) -> bool {
    // Find the rightmost `->` and confirm what follows is only an
    // identifier-in-progress (no extra tokens).
    let Some(idx) = prefix.rfind("->") else {
        return false;
    };
    let tail = &prefix[idx + 2..];
    tail.chars()
        .all(|c| c.is_whitespace() || c.is_ascii_alphanumeric() || c == '_' || c == '/' || c == '#')
}

/// True when the cursor sits inside an unterminated `<…>` directive
/// head: the last `<` on the line has no matching `>` after it.
fn is_directive_position(prefix: &str) -> bool {
    let Some(open) = prefix.rfind('<') else {
        return false;
    };
    !prefix[open..].contains('>')
}

/// True when the line is an `is ` mixin clause and the cursor is past
/// the `is ` keyword.
fn is_is_position(prefix: &str) -> bool {
    // Look for the last ` is ` or leading `is ` token.
    let trimmed = prefix.trim_start();
    if let Some(rest) = trimmed.strip_prefix("is ") {
        return rest.chars().all(|c| c != '<' && c != '>');
    }
    if let Some(idx) = prefix.rfind(" is ") {
        let tail = &prefix[idx + 4..];
        return tail.chars().all(|c| c != '<' && c != '>');
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_divert_position() {
        assert!(is_divert_position("    -> rin"));
        assert!(is_divert_position("-> "));
        assert!(!is_divert_position("ringing"));
    }

    #[test]
    fn detects_directive_position() {
        assert!(is_directive_position("    <sf"));
        assert!(!is_directive_position("<sfx: bell>"));
    }

    #[test]
    fn detects_is_position() {
        assert!(is_is_position("is Wren"));
        assert!(is_is_position("    is "));
        assert!(!is_is_position("    is<sfx>"));
    }
}
