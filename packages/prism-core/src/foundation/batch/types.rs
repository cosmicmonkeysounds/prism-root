//! Batch operation descriptors. Port of
//! `foundation/batch/batch-types.ts`.
//!
//! Each [`BatchOp`] variant describes a single mutation that will be
//! applied to a [`TreeModel`] or [`EdgeModel`] as part of a
//! transaction. The port keeps the same surface but trades TS's
//! `Partial<GraphObject>` shape for the strongly-typed
//! [`GraphObjectDraft`] / [`GraphObjectPatch`] / [`EdgeDraft`] /
//! [`EdgePatch`] structs already defined by the object model layer.

use super::super::object_model::edge_model::{EdgeDraft, EdgePatch};
use super::super::object_model::tree_model::{GraphObjectDraft, GraphObjectPatch};
use super::super::object_model::types::{GraphObject, ObjectEdge, ObjectId};

// ── Object operations ─────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct CreateObjectOp {
    pub draft: GraphObjectDraft,
    pub parent_id: Option<ObjectId>,
    pub position: Option<f64>,
}

#[derive(Debug, Clone)]
pub struct UpdateObjectOp {
    pub id: String,
    pub changes: GraphObjectPatch,
}

#[derive(Debug, Clone)]
pub struct DeleteObjectOp {
    pub id: String,
}

#[derive(Debug, Clone)]
pub struct MoveObjectOp {
    pub id: String,
    pub to_parent_id: Option<ObjectId>,
    pub to_position: Option<f64>,
}

// ── Edge operations ───────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct CreateEdgeOp {
    pub draft: EdgeDraft,
}

#[derive(Debug, Clone)]
pub struct UpdateEdgeOp {
    pub id: String,
    pub changes: EdgePatch,
}

#[derive(Debug, Clone)]
pub struct DeleteEdgeOp {
    pub id: String,
}

// ── Union ─────────────────────────────────────────────────────────

/// A single mutation enqueued in a [`BatchTransaction`].
#[derive(Debug, Clone)]
pub enum BatchOp {
    CreateObject(CreateObjectOp),
    UpdateObject(UpdateObjectOp),
    DeleteObject(DeleteObjectOp),
    MoveObject(MoveObjectOp),
    CreateEdge(CreateEdgeOp),
    UpdateEdge(UpdateEdgeOp),
    DeleteEdge(DeleteEdgeOp),
}

impl BatchOp {
    /// Short, stable kind tag used by validation errors and logs.
    pub fn kind(&self) -> &'static str {
        match self {
            BatchOp::CreateObject(_) => "create-object",
            BatchOp::UpdateObject(_) => "update-object",
            BatchOp::DeleteObject(_) => "delete-object",
            BatchOp::MoveObject(_) => "move-object",
            BatchOp::CreateEdge(_) => "create-edge",
            BatchOp::UpdateEdge(_) => "update-edge",
            BatchOp::DeleteEdge(_) => "delete-edge",
        }
    }
}

// ── Snapshots ─────────────────────────────────────────────────────

/// Undo-friendly snapshot of a single mutation. `before = None`
/// means the target did not exist prior to the op (create);
/// `after = None` means the target no longer exists (delete).
/// Matches the shape the legacy TS `UndoRedoManager` expects so
/// callers can feed these straight into a future Rust port.
///
/// Both variants box their payloads to keep the enum small;
/// `GraphObject` is ~672 bytes and `BatchSnapshot`s land in
/// per-op vecs that may be cloned around the undo pipeline.
#[derive(Debug, Clone)]
pub enum BatchSnapshot {
    Object {
        before: Option<Box<GraphObject>>,
        after: Option<Box<GraphObject>>,
    },
    Edge {
        before: Option<Box<ObjectEdge>>,
        after: Option<Box<ObjectEdge>>,
    },
}

// ── Result ────────────────────────────────────────────────────────

/// Summary returned by [`crate::foundation::batch::BatchTransaction::execute`]
/// on success.
#[derive(Debug, Clone, Default)]
pub struct BatchResult {
    /// Total number of operations executed.
    pub executed: usize,
    /// Objects created by `CreateObject` ops, in order.
    pub created: Vec<GraphObject>,
    /// Edges created by `CreateEdge` ops, in order.
    pub created_edges: Vec<ObjectEdge>,
    /// Per-op snapshots for feeding into an undo manager. One
    /// entry per mutation (a `DeleteObject` that removes N
    /// descendants contributes N+1 entries).
    pub snapshots: Vec<BatchSnapshot>,
    /// Human-readable description for undo grouping, mirroring
    /// the legacy TS `BatchExecuteOptions.description`.
    pub description: String,
}

// ── Progress ──────────────────────────────────────────────────────

/// Progress callback payload, emitted once per queued op just
/// before it executes.
#[derive(Debug, Clone)]
pub struct BatchProgress<'a> {
    pub current: usize,
    pub total: usize,
    pub op: &'a BatchOp,
}

// ── Validation ────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct BatchValidationError {
    /// Index of the failing op in the queue.
    pub index: usize,
    /// Short kind tag (same as [`BatchOp::kind`]).
    pub kind: &'static str,
    /// Human-readable reason.
    pub reason: String,
}

#[derive(Debug, Clone, Default)]
pub struct BatchValidationResult {
    pub valid: bool,
    pub errors: Vec<BatchValidationError>,
}
