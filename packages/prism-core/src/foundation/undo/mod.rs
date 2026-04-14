//! `undo` — snapshot-based undo/redo stack.
//!
//! Port of `foundation/undo/*`. [`UndoRedoManager`] is the
//! framework-agnostic core; [`create_undo_bridge`] hooks it into
//! `TreeModel` / `EdgeModel` mutation lifecycles.

pub mod bridge;
pub mod manager;
pub mod types;

pub use bridge::{create_undo_bridge, SharedUndoManager, UndoBridge};
pub use manager::{
    UndoApplier, UndoListener, UndoRedoManager, UndoRedoManagerOptions, UndoSubscription,
};
pub use types::{ObjectSnapshot, UndoDirection, UndoEntry};
