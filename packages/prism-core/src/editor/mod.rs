//! Code / text editor core — ropey-backed buffer, cursor, selection,
//! and editor state. Framework-agnostic primitives that `prism-shell`
//! renders through Slint and syncs to Loro for collaborative editing.

mod buffer;
mod cursor;
pub mod highlight;
mod selection;
mod state;

pub use buffer::Buffer;
pub use cursor::{Cursor, Position};
pub use highlight::{highlight_line, Token, TokenKind};
pub use selection::Selection;
pub use state::EditorState;
