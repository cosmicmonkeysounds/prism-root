//! Source positions and spans.
//!
//! Loom files are UTF-8 text. Positions carry zero-based line + UTF-8
//! byte offsets — column is the 0-based UTF-8 *byte* column within
//! the line, not a grapheme column. The LSP layer converts to UTF-16
//! at the protocol boundary; everything inside the parser stays in
//! bytes for cheap slicing.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct Position {
    pub line: u32,
    pub column: u32,
    pub byte: u32,
}

impl Position {
    pub const fn new(line: u32, column: u32, byte: u32) -> Self {
        Self { line, column, byte }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct Span {
    pub start: Position,
    pub end: Position,
}

impl Span {
    pub const fn new(start: Position, end: Position) -> Self {
        Self { start, end }
    }

    /// Span covering a single line from `start` to its end column.
    pub const fn line(start: Position, end_byte: u32, end_column: u32) -> Self {
        Self {
            start,
            end: Position {
                line: start.line,
                column: end_column,
                byte: end_byte,
            },
        }
    }
}
