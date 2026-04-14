//! AST types — Unist-compatible base node types, position
//! tracking, and source-mapping helpers.
//!
//! Port of `language/syntax/ast-types.ts`. Used by scanners,
//! parsers, and codegen throughout Prism.

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Position {
    pub offset: usize,
    pub line: usize,
    pub column: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SourceRange {
    pub start: Position,
    pub end: Position,
}

/// Compute a `Position` for a byte offset into a UTF-8 source.
///
/// Matches the legacy JS semantics: the line counter advances on
/// `\n`, and the column is the length (in chars) of the current
/// line up to `offset`. Clamped so callers can pass end-of-file.
pub fn pos_at(source: &str, offset: usize) -> Position {
    let clamped = offset.min(source.len());
    let prefix = &source[..clamped];
    let line = prefix.matches('\n').count() + 1;
    let column = match prefix.rfind('\n') {
        Some(nl) => prefix[nl + 1..].chars().count(),
        None => prefix.chars().count(),
    };
    Position {
        offset,
        line,
        column,
    }
}

/// Build a `SourceRange` from `start..end` byte offsets.
pub fn range(source: &str, start: usize, end: usize) -> SourceRange {
    SourceRange {
        start: pos_at(source, start),
        end: pos_at(source, end),
    }
}

/// Unist-compatible base AST node. Every language parser that
/// participates in the shared syntax pipeline emits a tree of
/// `SyntaxNode`s rooted in a [`RootNode`].
///
/// `data` mirrors the unist `data` slot — an open dictionary for
/// language-specific decorations that downstream tooling can read
/// without knowing the concrete node type ahead of time.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SyntaxNode {
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub position: Option<SourceRange>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<SyntaxNode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub data: IndexMap<String, JsonValue>,
}

/// Root of a parsed source file. Always has `kind == "root"`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct RootNode {
    #[serde(rename = "type")]
    pub kind: RootKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub position: Option<SourceRange>,
    #[serde(default)]
    pub children: Vec<SyntaxNode>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum RootKind {
    #[default]
    #[serde(rename = "root")]
    Root,
}

impl RootNode {
    pub fn new() -> Self {
        Self::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pos_at_counts_lines_and_columns() {
        let src = "abc\ndef\nghi";
        assert_eq!(
            pos_at(src, 0),
            Position {
                offset: 0,
                line: 1,
                column: 0
            }
        );
        assert_eq!(
            pos_at(src, 4),
            Position {
                offset: 4,
                line: 2,
                column: 0
            }
        );
        assert_eq!(
            pos_at(src, 6),
            Position {
                offset: 6,
                line: 2,
                column: 2
            }
        );
    }

    #[test]
    fn range_spans_two_points() {
        let src = "ab\ncd";
        let r = range(src, 0, 5);
        assert_eq!(r.start.line, 1);
        assert_eq!(r.end.line, 2);
        assert_eq!(r.end.column, 2);
    }
}
