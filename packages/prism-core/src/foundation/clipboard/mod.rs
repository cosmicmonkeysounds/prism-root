//! `clipboard` — cut/copy/paste for GraphObject subtrees.
//!
//! Port of `foundation/clipboard/*`. Objects are deep-cloned on
//! copy; IDs are remapped on paste. Cut consumes itself on paste
//! and records a single undo entry covering both the new copies
//! and the original deletions.

pub mod tree_clipboard;
pub mod types;

pub use tree_clipboard::{ClipboardIdGen, TreeClipboard};
pub use types::{ClipboardEntry, ClipboardMode, PasteOptions, PasteResult, SerializedSubtree};
