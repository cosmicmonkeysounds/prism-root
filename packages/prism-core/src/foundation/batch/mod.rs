//! `batch` — transactional mutation queue over [`TreeModel`] +
//! [`EdgeModel`]. Port of `foundation/batch/` from the legacy TS
//! tree.
//!
//! Batching collects multiple tree/edge mutations and applies them
//! atomically: on success the caller receives a [`BatchResult`]
//! bundling per-op snapshots, created objects, and created edges;
//! on failure every already-applied op is rolled back (best-effort)
//! before the originating error is returned.
//!
//! See [`transaction`] for the manual-drive API shape and
//! rollback strategy.
//!
//! [`TreeModel`]: super::object_model::TreeModel
//! [`EdgeModel`]: super::object_model::EdgeModel

pub mod transaction;
pub mod types;

pub use transaction::{BatchExecuteOptions, BatchTransaction, BatchTransactionOptions};
pub use types::{
    BatchOp, BatchProgress, BatchResult, BatchSnapshot, BatchValidationError,
    BatchValidationResult, CreateEdgeOp, CreateObjectOp, DeleteEdgeOp, DeleteObjectOp,
    MoveObjectOp, UpdateEdgeOp, UpdateObjectOp,
};
