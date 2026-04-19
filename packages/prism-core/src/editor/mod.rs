//! Code / text editor core — ropey-backed buffer, cursor, selection,
//! and editor state. Framework-agnostic primitives that `prism-shell`
//! renders through Slint and syncs to Loro for collaborative editing.

mod buffer;
mod cursor;
mod selection;
mod state;

pub use buffer::Buffer;
pub use cursor::{Cursor, Position};
pub use selection::Selection;
pub use state::EditorState;
