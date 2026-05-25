//! Hover information for directive verbs, CHARACTER refs, and beat
//! diverts. Resolution is purely lexical: we identify the token under
//! the cursor on the source line and look it up in the workspace
//! index.

use lsp_types::{Hover, HoverContents, MarkupContent, MarkupKind, Position, Range, Url};

use crate::completion::DIRECTIVES;
use crate::workspace::{line_at, Workspace};

pub fn hover_at(ws: &Workspace, uri: &Url, pos: Position) -> Option<Hover> {
    let doc = ws.docs.get(uri)?;
    let line = line_at(&doc.text, pos.line);
    let (token, start, end) = token_at(line, pos.character as usize)?;
    let range = Range {
        start: Position {
            line: pos.line,
            character: start as u32,
        },
        end: Position {
            line: pos.line,
            character: end as u32,
        },
    };

    // Directive head: `<name…` token immediately follows `<`.
    if let Some(open) = line[..start].rfind('<') {
        if line[open + 1..start].trim().is_empty()
            && DIRECTIVES.contains(&token)
        {
            return Some(markdown(
                format!("`<{token}: …>` — directive"),
                Some(range),
            ));
        }
    }

    // Divert tail: `-> token` form.
    if let Some(arrow) = line[..start].rfind("->") {
        if line[arrow + 2..start].trim().is_empty() {
            if let Some(entries) = ws.beats.get(token) {
                if let Some(beat) = entries.first() {
                    let mut body = format!("**beat** `{token}`");
                    if let Some(cast) = &beat.cast {
                        body.push_str(&format!("\n\ncast: {cast}"));
                    }
                    if let Some(setting) = &beat.setting {
                        body.push_str(&format!("\n\nsetting: {setting}"));
                    }
                    return Some(markdown(body, Some(range)));
                }
            }
        }
    }

    // CHARACTER reference: ALL-CAPS token that matches a declared name.
    if token.chars().all(|c| c.is_ascii_uppercase() || c == '_') {
        if let Some(info) = ws.characters.get(token) {
            let mut body = format!("**CHARACTER** `{token}`");
            if !info.mixins.is_empty() {
                body.push_str(&format!("\n\nis {}", info.mixins.join(", ")));
            }
            if !info.body.is_empty() {
                body.push_str("\n\n```loom\n");
                for line in &info.body {
                    body.push_str(line);
                    body.push('\n');
                }
                body.push_str("```");
            }
            return Some(markdown(body, Some(range)));
        }
    }

    None
}

fn markdown(value: String, range: Option<Range>) -> Hover {
    Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value,
        }),
        range,
    }
}

/// Identify the alphanumeric / `_` token under the byte offset `col`.
pub fn token_at(line: &str, col: usize) -> Option<(&str, usize, usize)> {
    if col > line.len() {
        return None;
    }
    let bytes = line.as_bytes();
    let is_word = |b: u8| b.is_ascii_alphanumeric() || b == b'_';
    let mut start = col;
    while start > 0 && is_word(bytes[start - 1]) {
        start -= 1;
    }
    let mut end = col;
    while end < bytes.len() && is_word(bytes[end]) {
        end += 1;
    }
    if start == end {
        return None;
    }
    Some((&line[start..end], start, end))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_in_middle() {
        let (tok, s, e) = token_at("    -> ringing", 10).unwrap();
        assert_eq!(tok, "ringing");
        assert_eq!((s, e), (7, 14));
    }
}
