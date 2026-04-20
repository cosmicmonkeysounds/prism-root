//! Code / text editor core — ropey-backed buffer, cursor, selection,
//! indent guides, code folding, and editor state. Framework-agnostic
//! primitives that `prism-shell` renders through Slint and syncs to
//! Loro for collaborative editing.

mod buffer;
mod cursor;
pub mod fold;
pub mod highlight;
pub mod indent;
mod selection;
mod state;

pub use buffer::Buffer;
pub use cursor::{Cursor, Position};
pub use fold::{is_foldable, FoldState, FoldedRange};
pub use highlight::{highlight_line, Token, TokenKind};
pub use indent::{active_indent_depth, compute_line_indent_guides, IndentGuide};
pub use selection::Selection;
pub use state::EditorState;
