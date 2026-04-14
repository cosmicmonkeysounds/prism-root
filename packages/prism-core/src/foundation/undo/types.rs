//! Snapshot types for the undo/redo engine.
//!
//! Port of `foundation/undo/undo-types.ts`. Every mutation reduces
//! to a before/after snapshot; batched mutations are bundled into
//! a single [`UndoEntry`] that gets applied atomically.

use chrono::{DateTime, Utc};

use crate::foundation::object_model::types::{GraphObject, ObjectEdge};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UndoDirection {
    Undo,
    Redo,
}

#[derive(Debug, Clone, PartialEq)]
#[allow(clippy::large_enum_variant)]
pub enum ObjectSnapshot {
    Object {
        before: Option<GraphObject>,
        after: Option<GraphObject>,
    },
    Edge {
        before: Option<ObjectEdge>,
        after: Option<ObjectEdge>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct UndoEntry {
    pub description: String,
    pub snapshots: Vec<ObjectSnapshot>,
    pub timestamp: DateTime<Utc>,
}
