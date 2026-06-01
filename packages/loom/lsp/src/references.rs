//! Find-references: every occurrence of the identifier under the
//! cursor across every open document in the workspace.
//!
//! v1 is identifier-shaped: it does not distinguish a `WREN`
//! dialogue cue from a CHARACTER declaration named `WREN` — both
//! tokens land in the result set. The editor renders the matches
//! grouped by URI, which is enough signal for navigation; semantic
//! filtering (only references to *this* declaration kind) can layer
//! on top once the workspace index grows per-kind tables.

use lsp_types::{Location, Position, Range, Url};

use crate::hover::token_at;
use crate::workspace::{line_at, Workspace};

pub fn references_at(ws: &Workspace, uri: &Url, pos: Position) -> Vec<Location> {
    let doc = match ws.docs.get(uri) {
        Some(d) => d,
        None => return Vec::new(),
    };
    let line = line_at(&doc.text, pos.line);
    let (token, _, _) = match token_at(line, pos.character as usize) {
        Some(t) => t,
        None => return Vec::new(),
    };
    let needle = token.to_string();
    let mut out = Vec::new();
    for (doc_uri, doc) in ws.docs.iter() {
        for (line_no, line_text) in doc.text.lines().enumerate() {
            for (start, end) in find_token_spans(line_text, &needle) {
                out.push(Location {
                    uri: doc_uri.clone(),
                    range: Range {
                        start: Position {
                            line: line_no as u32,
                            character: start as u32,
                        },
                        end: Position {
                            line: line_no as u32,
                            character: end as u32,
                        },
                    },
                });
            }
        }
    }
    out
}

/// Return every (start, end) byte span on `line` where `needle`
/// appears bounded by non-identifier characters.
pub fn find_token_spans(line: &str, needle: &str) -> Vec<(usize, usize)> {
    if needle.is_empty() || needle.len() > line.len() {
        return Vec::new();
    }
    let bytes = line.as_bytes();
    let needle_bytes = needle.as_bytes();
    let is_word = |b: u8| b.is_ascii_alphanumeric() || b == b'_';
    let mut out = Vec::new();
    let mut i = 0;
    while i + needle_bytes.len() <= bytes.len() {
        if &bytes[i..i + needle_bytes.len()] == needle_bytes {
            let before_ok = i == 0 || !is_word(bytes[i - 1]);
            let after_ok =
                i + needle_bytes.len() == bytes.len() || !is_word(bytes[i + needle_bytes.len()]);
            if before_ok && after_ok {
                out.push((i, i + needle_bytes.len()));
                i += needle_bytes.len();
                continue;
            }
        }
        i += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn word_boundaries_respected() {
        let spans = find_token_spans("WREN bell Wrenched WREN", "WREN");
        assert_eq!(spans, vec![(0, 4), (19, 23)]);
    }

    #[test]
    fn no_match() {
        assert!(find_token_spans("nothing here", "WREN").is_empty());
    }
}
