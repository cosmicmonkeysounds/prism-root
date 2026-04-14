//! `BatchTransaction` — transactional mutation queue over
//! [`TreeModel`] and [`EdgeModel`]. Port of
//! `foundation/batch/batch-transaction.ts`.
//!
//! ## Manual-drive API
//!
//! The legacy TS version captured the tree + edge models by
//! reference inside a closure factory. That shape doesn't survive
//! the Rust borrow checker cleanly: both models require `&mut`
//! access for every mutation, so stashing them behind an
//! `Rc<RefCell<_>>` would leak through the public surface. Instead
//! the Rust port uses the same manual-drive pattern as
//! [`crate::foundation::object_model::weak_ref::WeakRefEngine`]: a
//! [`BatchTransaction`] owns only the queue + progress hook, and
//! the caller hands in `&mut TreeModel` / `Option<&mut EdgeModel>`
//! at [`BatchTransaction::execute`] time.
//!
//! ## Rollback
//!
//! Rollback is **replay-based**, not snapshot-based. As each op
//! runs we push a closure-free [`RollbackAction`] that captures
//! just enough state to invert the mutation through the same
//! model APIs. On the happy path the vec is dropped; on failure
//! it is drained in reverse order and each inverse is applied as
//! best-effort (inner errors are swallowed because the caller
//! already has an outer error to surface).
//!
//! Replay-based rollback is chosen over a full model clone for
//! three reasons:
//!
//! 1. `TreeModel` holds `Box<dyn FnMut>` hook + listener slots
//!    which are not `Clone`, so a whole-model snapshot would
//!    require side-table storage.
//! 2. The legacy TS version also replays inverses, so this is
//!    behaviour-preserving.
//! 3. Per-op inversion plays nicely with the future undo system
//!    we'll eventually wire through [`BatchSnapshot`].

#![allow(clippy::type_complexity)]

use super::super::object_model::edge_model::{EdgeDraft, EdgeModel, EdgePatch};
use super::super::object_model::error::ObjectModelError;
use super::super::object_model::tree_model::{
    AddOptions, GraphObjectDraft, GraphObjectPatch, TreeModel,
};
use super::super::object_model::types::{GraphObject, ObjectEdge, ObjectId};
use super::types::{
    BatchOp, BatchProgress, BatchResult, BatchSnapshot, BatchValidationError,
    BatchValidationResult, CreateEdgeOp, CreateObjectOp, DeleteEdgeOp, DeleteObjectOp,
    MoveObjectOp, UpdateEdgeOp, UpdateObjectOp,
};

/// Options passed at construction time.
#[derive(Default)]
pub struct BatchTransactionOptions {
    /// Whether an [`EdgeModel`] will be provided at execute time.
    /// Used only by [`BatchTransaction::validate`] for pre-flight
    /// detection of "edge op with no edge model" — actual presence
    /// is rechecked at execute time via the `edges` argument.
    pub has_edges: bool,
}

/// Options passed at [`BatchTransaction::execute`] time.
pub struct BatchExecuteOptions<'a> {
    /// Human-readable description used to label the resulting
    /// [`BatchResult::description`] for undo grouping.
    pub description: String,
    /// Optional per-op progress callback.
    pub on_progress: Option<Box<dyn FnMut(BatchProgress<'_>) + 'a>>,
}

impl Default for BatchExecuteOptions<'_> {
    fn default() -> Self {
        Self {
            description: "Batch operation".to_string(),
            on_progress: None,
        }
    }
}

/// A queue of mutations to apply atomically. See the module doc
/// for the manual-drive model and rollback strategy.
pub struct BatchTransaction {
    queue: Vec<BatchOp>,
    has_edges: bool,
}

impl BatchTransaction {
    pub fn new() -> Self {
        Self {
            queue: Vec::new(),
            has_edges: false,
        }
    }

    pub fn with_options(options: BatchTransactionOptions) -> Self {
        Self {
            queue: Vec::new(),
            has_edges: options.has_edges,
        }
    }

    // ── Queue management ──────────────────────────────────────

    pub fn add(&mut self, op: BatchOp) {
        self.queue.push(op);
    }

    pub fn add_all(&mut self, ops: impl IntoIterator<Item = BatchOp>) {
        self.queue.extend(ops);
    }

    pub fn size(&self) -> usize {
        self.queue.len()
    }

    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }

    pub fn ops(&self) -> &[BatchOp] {
        &self.queue
    }

    pub fn clear(&mut self) {
        self.queue.clear();
    }

    // ── Validation ────────────────────────────────────────────

    /// Pre-flight check: walk the queue and return a list of
    /// structural problems without touching either model. Mirrors
    /// the legacy TS `validate()`.
    pub fn validate(&self) -> BatchValidationResult {
        let mut errors: Vec<BatchValidationError> = Vec::new();

        for (i, op) in self.queue.iter().enumerate() {
            match op {
                BatchOp::CreateObject(CreateObjectOp { draft, .. }) => {
                    if draft.type_name.is_empty() {
                        errors.push(BatchValidationError {
                            index: i,
                            kind: op.kind(),
                            reason: "Missing type in draft".to_string(),
                        });
                    }
                    if draft.name.is_empty() {
                        errors.push(BatchValidationError {
                            index: i,
                            kind: op.kind(),
                            reason: "Missing name in draft".to_string(),
                        });
                    }
                }
                BatchOp::UpdateObject(UpdateObjectOp { id, .. })
                | BatchOp::DeleteObject(DeleteObjectOp { id })
                | BatchOp::MoveObject(MoveObjectOp { id, .. }) => {
                    if id.is_empty() {
                        errors.push(BatchValidationError {
                            index: i,
                            kind: op.kind(),
                            reason: "Missing object id".to_string(),
                        });
                    }
                }
                BatchOp::CreateEdge(CreateEdgeOp { draft }) => {
                    if !self.has_edges {
                        errors.push(BatchValidationError {
                            index: i,
                            kind: op.kind(),
                            reason: "No EdgeModel provided".to_string(),
                        });
                    }
                    if draft.source_id.as_str().is_empty() || draft.target_id.as_str().is_empty() {
                        errors.push(BatchValidationError {
                            index: i,
                            kind: op.kind(),
                            reason: "Missing sourceId or targetId in edge draft".to_string(),
                        });
                    }
                }
                BatchOp::UpdateEdge(UpdateEdgeOp { id, .. })
                | BatchOp::DeleteEdge(DeleteEdgeOp { id }) => {
                    if !self.has_edges {
                        errors.push(BatchValidationError {
                            index: i,
                            kind: op.kind(),
                            reason: "No EdgeModel provided".to_string(),
                        });
                    }
                    if id.is_empty() {
                        errors.push(BatchValidationError {
                            index: i,
                            kind: op.kind(),
                            reason: "Missing edge id".to_string(),
                        });
                    }
                }
            }
        }

        BatchValidationResult {
            valid: errors.is_empty(),
            errors,
        }
    }

    // ── Execute ───────────────────────────────────────────────

    /// Apply every queued op in order. On the first failure, every
    /// previously-applied op is rolled back (best-effort) before
    /// the error is returned.
    pub fn execute(
        &mut self,
        tree: &mut TreeModel,
        mut edges: Option<&mut EdgeModel>,
        mut options: BatchExecuteOptions<'_>,
    ) -> Result<BatchResult, ObjectModelError> {
        let total = self.queue.len();
        let queue = std::mem::take(&mut self.queue);

        let mut created: Vec<GraphObject> = Vec::new();
        let mut created_edges: Vec<ObjectEdge> = Vec::new();
        let mut snapshots: Vec<BatchSnapshot> = Vec::new();
        let mut rollbacks: Vec<RollbackAction> = Vec::new();
        let mut executed = 0usize;

        for (i, op) in queue.iter().enumerate() {
            if let Some(cb) = options.on_progress.as_mut() {
                cb(BatchProgress {
                    current: i,
                    total,
                    op,
                });
            }

            let step = apply_op(tree, edges.as_deref_mut(), op);
            match step {
                Ok(StepOutcome {
                    snapshots: step_snapshots,
                    created: step_created,
                    created_edge: step_created_edge,
                    rollback,
                }) => {
                    snapshots.extend(step_snapshots);
                    if let Some(obj) = step_created {
                        created.push(obj);
                    }
                    if let Some(edge) = step_created_edge {
                        created_edges.push(edge);
                    }
                    if let Some(rb) = rollback {
                        rollbacks.push(rb);
                    }
                    executed += 1;
                }
                Err(err) => {
                    // Roll back in reverse order. Swallow inner
                    // errors to mirror the TS behaviour.
                    while let Some(action) = rollbacks.pop() {
                        let _ = run_rollback(tree, edges.as_deref_mut(), action);
                    }
                    return Err(err);
                }
            }
        }

        Ok(BatchResult {
            executed,
            created,
            created_edges,
            snapshots,
            description: options.description,
        })
    }
}

impl Default for BatchTransaction {
    fn default() -> Self {
        Self::new()
    }
}

// ── Step outcome + rollback plumbing ──────────────────────────────

struct StepOutcome {
    snapshots: Vec<BatchSnapshot>,
    created: Option<GraphObject>,
    created_edge: Option<ObjectEdge>,
    rollback: Option<RollbackAction>,
}

/// Closure-free rollback descriptor. Each variant captures the
/// minimum state needed to invert one op through the model API.
enum RollbackAction {
    RemoveObject {
        id: String,
    },
    ReinsertObjects {
        /// Ordered list of (object-to-reinsert, its original
        /// parent, its original position). Reinserted in order.
        objects: Vec<(GraphObject, Option<ObjectId>, f64)>,
    },
    RestoreObject {
        id: String,
        /// A patch whose fields are all `Some(...)` and set back
        /// to the previous values.
        patch: GraphObjectPatch,
    },
    RestoreMove {
        id: String,
        parent_id: Option<ObjectId>,
        position: f64,
    },
    RemoveEdge {
        id: String,
    },
    ReinsertEdge {
        edge: ObjectEdge,
    },
    RestoreEdge {
        id: String,
        patch: EdgePatch,
    },
}

fn apply_op(
    tree: &mut TreeModel,
    edges: Option<&mut EdgeModel>,
    op: &BatchOp,
) -> Result<StepOutcome, ObjectModelError> {
    match op {
        BatchOp::CreateObject(CreateObjectOp {
            draft,
            parent_id,
            position,
        }) => {
            let obj = tree.add(
                draft.clone(),
                AddOptions {
                    parent_id: parent_id.clone(),
                    position: *position,
                },
            )?;
            Ok(StepOutcome {
                snapshots: vec![BatchSnapshot::Object {
                    before: None,
                    after: Some(Box::new(obj.clone())),
                }],
                created: Some(obj.clone()),
                created_edge: None,
                rollback: Some(RollbackAction::RemoveObject {
                    id: obj.id.as_str().to_string(),
                }),
            })
        }

        BatchOp::UpdateObject(UpdateObjectOp { id, changes }) => {
            let before = tree
                .get(id)
                .ok_or_else(|| ObjectModelError::not_found(format!("Object '{id}' not found")))?
                .clone();
            let after = tree.update(id, changes.clone())?;
            let restore_patch = patch_from_object(&before);
            Ok(StepOutcome {
                snapshots: vec![BatchSnapshot::Object {
                    before: Some(Box::new(before)),
                    after: Some(Box::new(after)),
                }],
                created: None,
                created_edge: None,
                rollback: Some(RollbackAction::RestoreObject {
                    id: id.clone(),
                    patch: restore_patch,
                }),
            })
        }

        BatchOp::DeleteObject(DeleteObjectOp { id }) => {
            let target = tree
                .get(id)
                .ok_or_else(|| ObjectModelError::not_found(format!("Object '{id}' not found")))?
                .clone();
            // Collect descendants in breadth/order-independent
            // form for re-insertion. They need to be sorted such
            // that parents come before children — we sort by depth
            // (ancestor chain length) to guarantee that.
            let descendants = tree.get_descendants(id);

            let mut snapshots: Vec<BatchSnapshot> = Vec::new();
            snapshots.push(BatchSnapshot::Object {
                before: Some(Box::new(target.clone())),
                after: None,
            });
            for d in descendants.iter() {
                snapshots.push(BatchSnapshot::Object {
                    before: Some(Box::new(d.clone())),
                    after: None,
                });
            }

            // Snapshot every object with parent/position for
            // faithful restore ordering.
            let mut ordered: Vec<(GraphObject, Option<ObjectId>, f64)> = Vec::new();
            ordered.push((target.clone(), target.parent_id.clone(), target.position));
            for d in descendants.iter() {
                ordered.push((d.clone(), d.parent_id.clone(), d.position));
            }
            // Sort parents-before-children by precomputing a
            // depth key (can't borrow `ordered` inside its own
            // sort closure).
            let depths: Vec<usize> = ordered
                .iter()
                .map(|(obj, _, _)| depth_of(obj, &ordered))
                .collect();
            let mut indexed: Vec<(usize, (GraphObject, Option<ObjectId>, f64))> =
                ordered.into_iter().enumerate().collect();
            indexed.sort_by_key(|(i, _)| depths[*i]);
            let ordered: Vec<(GraphObject, Option<ObjectId>, f64)> =
                indexed.into_iter().map(|(_, v)| v).collect();

            tree.remove(id);

            Ok(StepOutcome {
                snapshots,
                created: None,
                created_edge: None,
                rollback: Some(RollbackAction::ReinsertObjects { objects: ordered }),
            })
        }

        BatchOp::MoveObject(MoveObjectOp {
            id,
            to_parent_id,
            to_position,
        }) => {
            let obj = tree
                .get(id)
                .ok_or_else(|| ObjectModelError::not_found(format!("Object '{id}' not found")))?
                .clone();
            let moved = tree.move_to(id, to_parent_id.clone(), *to_position)?;
            Ok(StepOutcome {
                snapshots: vec![BatchSnapshot::Object {
                    before: Some(Box::new(obj.clone())),
                    after: Some(Box::new(moved)),
                }],
                created: None,
                created_edge: None,
                rollback: Some(RollbackAction::RestoreMove {
                    id: id.clone(),
                    parent_id: obj.parent_id.clone(),
                    position: obj.position,
                }),
            })
        }

        BatchOp::CreateEdge(CreateEdgeOp { draft }) => {
            let edges = edges
                .ok_or_else(|| ObjectModelError::not_found("No EdgeModel provided".to_string()))?;
            let edge = edges.add(draft.clone())?;
            Ok(StepOutcome {
                snapshots: vec![BatchSnapshot::Edge {
                    before: None,
                    after: Some(Box::new(edge.clone())),
                }],
                created: None,
                created_edge: Some(edge.clone()),
                rollback: Some(RollbackAction::RemoveEdge {
                    id: edge.id.as_str().to_string(),
                }),
            })
        }

        BatchOp::UpdateEdge(UpdateEdgeOp { id, changes }) => {
            let edges = edges
                .ok_or_else(|| ObjectModelError::not_found("No EdgeModel provided".to_string()))?;
            let before = edges
                .get(id)
                .ok_or_else(|| ObjectModelError::not_found(format!("Edge '{id}' not found")))?
                .clone();
            let after = edges.update(id, changes.clone())?;
            let restore_patch = patch_from_edge(&before);
            Ok(StepOutcome {
                snapshots: vec![BatchSnapshot::Edge {
                    before: Some(Box::new(before)),
                    after: Some(Box::new(after)),
                }],
                created: None,
                created_edge: None,
                rollback: Some(RollbackAction::RestoreEdge {
                    id: id.clone(),
                    patch: restore_patch,
                }),
            })
        }

        BatchOp::DeleteEdge(DeleteEdgeOp { id }) => {
            let edges = edges
                .ok_or_else(|| ObjectModelError::not_found("No EdgeModel provided".to_string()))?;
            let before = edges
                .get(id)
                .ok_or_else(|| ObjectModelError::not_found(format!("Edge '{id}' not found")))?
                .clone();
            edges.remove(id);
            Ok(StepOutcome {
                snapshots: vec![BatchSnapshot::Edge {
                    before: Some(Box::new(before.clone())),
                    after: None,
                }],
                created: None,
                created_edge: None,
                rollback: Some(RollbackAction::ReinsertEdge { edge: before }),
            })
        }
    }
}

fn run_rollback(
    tree: &mut TreeModel,
    edges: Option<&mut EdgeModel>,
    action: RollbackAction,
) -> Result<(), ObjectModelError> {
    match action {
        RollbackAction::RemoveObject { id } => {
            tree.remove(&id);
            Ok(())
        }
        RollbackAction::ReinsertObjects { objects } => {
            for (obj, parent_id, position) in objects {
                let draft = draft_from_object(&obj);
                tree.add(
                    draft,
                    AddOptions {
                        parent_id,
                        position: Some(position),
                    },
                )?;
            }
            Ok(())
        }
        RollbackAction::RestoreObject { id, patch } => {
            tree.update(&id, patch)?;
            Ok(())
        }
        RollbackAction::RestoreMove {
            id,
            parent_id,
            position,
        } => {
            tree.move_to(&id, parent_id, Some(position))?;
            Ok(())
        }
        RollbackAction::RemoveEdge { id } => {
            if let Some(edges) = edges {
                edges.remove(&id);
            }
            Ok(())
        }
        RollbackAction::ReinsertEdge { edge } => {
            if let Some(edges) = edges {
                edges.add(edge_to_draft(&edge))?;
            }
            Ok(())
        }
        RollbackAction::RestoreEdge { id, patch } => {
            if let Some(edges) = edges {
                edges.update(&id, patch)?;
            }
            Ok(())
        }
    }
}

// ── Snapshot → patch/draft helpers ────────────────────────────────

fn patch_from_object(obj: &GraphObject) -> GraphObjectPatch {
    GraphObjectPatch {
        name: Some(obj.name.clone()),
        parent_id: Some(obj.parent_id.clone()),
        position: Some(obj.position),
        status: Some(obj.status.clone()),
        tags: Some(obj.tags.clone()),
        date: Some(obj.date.clone()),
        end_date: Some(obj.end_date.clone()),
        description: Some(obj.description.clone()),
        color: Some(obj.color.clone()),
        image: Some(obj.image.clone()),
        pinned: Some(obj.pinned),
        data: Some(obj.data.clone()),
    }
}

fn draft_from_object(obj: &GraphObject) -> GraphObjectDraft {
    GraphObjectDraft {
        id: Some(obj.id.as_str().to_string()),
        type_name: obj.type_name.clone(),
        name: obj.name.clone(),
        status: obj.status.clone(),
        tags: Some(obj.tags.clone()),
        date: obj.date.clone(),
        end_date: obj.end_date.clone(),
        description: Some(obj.description.clone()),
        color: obj.color.clone(),
        image: obj.image.clone(),
        pinned: Some(obj.pinned),
        data: Some(obj.data.clone()),
    }
}

fn patch_from_edge(edge: &ObjectEdge) -> EdgePatch {
    EdgePatch {
        source_id: Some(edge.source_id.clone()),
        target_id: Some(edge.target_id.clone()),
        relation: Some(edge.relation.clone()),
        position: Some(edge.position),
        data: Some(edge.data.clone()),
    }
}

fn edge_to_draft(edge: &ObjectEdge) -> EdgeDraft {
    EdgeDraft {
        id: Some(edge.id.as_str().to_string()),
        source_id: edge.source_id.clone(),
        target_id: edge.target_id.clone(),
        relation: edge.relation.clone(),
        position: edge.position,
        data: edge.data.clone(),
    }
}

/// Compute the depth of `obj` inside `pool` by walking
/// `parent_id` links that resolve to other members of `pool`.
/// Used to order a set of objects so that parents come before
/// children during replay-style re-insertion.
fn depth_of(obj: &GraphObject, pool: &[(GraphObject, Option<ObjectId>, f64)]) -> usize {
    let mut depth = 0usize;
    let mut cursor = obj.parent_id.clone();
    while let Some(pid) = cursor {
        let parent = pool.iter().find(|(o, _, _)| o.id.as_str() == pid.as_str());
        match parent {
            Some((p, _, _)) => {
                depth += 1;
                cursor = p.parent_id.clone();
            }
            None => break,
        }
    }
    depth
}

// ── Tests ─────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::foundation::object_model::edge_model::{EdgeDraft, EdgePatch};
    use crate::foundation::object_model::tree_model::{GraphObjectDraft, GraphObjectPatch};
    use crate::foundation::object_model::types::object_id;
    use serde_json::json;

    fn new_tree() -> TreeModel {
        TreeModel::new()
    }

    fn new_edges() -> EdgeModel {
        EdgeModel::new()
    }

    fn draft(t: &str, name: &str) -> GraphObjectDraft {
        GraphObjectDraft::new(t, name)
    }

    fn new_tx() -> BatchTransaction {
        BatchTransaction::with_options(BatchTransactionOptions { has_edges: true })
    }

    // ── queueing ──────────────────────────────────────────────

    #[test]
    fn starts_empty() {
        let tx = new_tx();
        assert_eq!(tx.size(), 0);
        assert!(tx.ops().is_empty());
    }

    #[test]
    fn add_enqueues_one_op() {
        let mut tx = new_tx();
        tx.add(BatchOp::CreateObject(CreateObjectOp {
            draft: draft("task", "A"),
            parent_id: None,
            position: None,
        }));
        assert_eq!(tx.size(), 1);
    }

    #[test]
    fn add_all_enqueues_multiple() {
        let mut tx = new_tx();
        tx.add_all([
            BatchOp::CreateObject(CreateObjectOp {
                draft: draft("task", "A"),
                parent_id: None,
                position: None,
            }),
            BatchOp::CreateObject(CreateObjectOp {
                draft: draft("task", "B"),
                parent_id: None,
                position: None,
            }),
        ]);
        assert_eq!(tx.size(), 2);
    }

    #[test]
    fn clear_resets_queue() {
        let mut tx = new_tx();
        tx.add(BatchOp::CreateObject(CreateObjectOp {
            draft: draft("task", "A"),
            parent_id: None,
            position: None,
        }));
        tx.clear();
        assert_eq!(tx.size(), 0);
    }

    // ── validate ──────────────────────────────────────────────

    #[test]
    fn validate_returns_valid_for_wellformed_ops() {
        let mut tx = new_tx();
        tx.add(BatchOp::CreateObject(CreateObjectOp {
            draft: draft("task", "A"),
            parent_id: None,
            position: None,
        }));
        let result = tx.validate();
        assert!(result.valid);
        assert!(result.errors.is_empty());
    }

    #[test]
    fn validate_catches_missing_type() {
        let mut tx = new_tx();
        tx.add(BatchOp::CreateObject(CreateObjectOp {
            draft: draft("", "A"),
            parent_id: None,
            position: None,
        }));
        let result = tx.validate();
        assert!(!result.valid);
        assert!(result.errors[0].reason.contains("type"));
    }

    #[test]
    fn validate_catches_missing_name() {
        let mut tx = new_tx();
        tx.add(BatchOp::CreateObject(CreateObjectOp {
            draft: draft("task", ""),
            parent_id: None,
            position: None,
        }));
        let result = tx.validate();
        assert!(!result.valid);
    }

    #[test]
    fn validate_catches_missing_update_id() {
        let mut tx = new_tx();
        tx.add(BatchOp::UpdateObject(UpdateObjectOp {
            id: String::new(),
            changes: GraphObjectPatch {
                name: Some("X".into()),
                ..Default::default()
            },
        }));
        let result = tx.validate();
        assert!(!result.valid);
    }

    #[test]
    fn validate_catches_edge_op_without_edge_model() {
        let mut tx = BatchTransaction::with_options(BatchTransactionOptions { has_edges: false });
        tx.add(BatchOp::CreateEdge(CreateEdgeOp {
            draft: EdgeDraft {
                id: None,
                source_id: object_id("a"),
                target_id: object_id("b"),
                relation: "dep".into(),
                position: None,
                data: Default::default(),
            },
        }));
        let result = tx.validate();
        assert!(!result.valid);
        assert!(result.errors[0].reason.contains("EdgeModel"));
    }

    #[test]
    fn validate_catches_missing_source_in_create_edge() {
        let mut tx = new_tx();
        tx.add(BatchOp::CreateEdge(CreateEdgeOp {
            draft: EdgeDraft {
                id: None,
                source_id: object_id(""),
                target_id: object_id("b"),
                relation: "dep".into(),
                position: None,
                data: Default::default(),
            },
        }));
        let result = tx.validate();
        assert!(!result.valid);
    }

    // ── execute: create ───────────────────────────────────────

    #[test]
    fn execute_creates_objects_in_tree() {
        let mut tree = new_tree();
        let mut edges = new_edges();
        let mut tx = new_tx();
        tx.add(BatchOp::CreateObject(CreateObjectOp {
            draft: draft("task", "A"),
            parent_id: None,
            position: None,
        }));
        tx.add(BatchOp::CreateObject(CreateObjectOp {
            draft: draft("task", "B"),
            parent_id: None,
            position: None,
        }));
        let result = tx
            .execute(&mut tree, Some(&mut edges), BatchExecuteOptions::default())
            .unwrap();
        assert_eq!(result.executed, 2);
        assert_eq!(result.created.len(), 2);
        assert_eq!(tree.len(), 2);
    }

    #[test]
    fn execute_creates_objects_with_parent_and_position() {
        let mut tree = new_tree();
        let mut edges = new_edges();
        let parent = tree
            .add(draft("folder", "F"), AddOptions::default())
            .unwrap();
        let mut tx = new_tx();
        tx.add(BatchOp::CreateObject(CreateObjectOp {
            draft: draft("task", "A"),
            parent_id: Some(parent.id.clone()),
            position: Some(0.0),
        }));
        let result = tx
            .execute(&mut tree, Some(&mut edges), BatchExecuteOptions::default())
            .unwrap();
        assert_eq!(result.created[0].parent_id.as_ref(), Some(&parent.id));
    }

    // ── execute: update ───────────────────────────────────────

    #[test]
    fn execute_updates_existing_object() {
        let mut tree = new_tree();
        let mut edges = new_edges();
        let obj = tree.add(draft("task", "A"), AddOptions::default()).unwrap();
        let mut tx = new_tx();
        tx.add(BatchOp::UpdateObject(UpdateObjectOp {
            id: obj.id.as_str().to_string(),
            changes: GraphObjectPatch {
                name: Some("B".into()),
                ..Default::default()
            },
        }));
        tx.execute(&mut tree, Some(&mut edges), BatchExecuteOptions::default())
            .unwrap();
        assert_eq!(tree.get(obj.id.as_str()).unwrap().name, "B");
    }

    #[test]
    fn execute_update_errors_on_missing_object() {
        let mut tree = new_tree();
        let mut edges = new_edges();
        let mut tx = new_tx();
        tx.add(BatchOp::UpdateObject(UpdateObjectOp {
            id: "nonexistent".into(),
            changes: GraphObjectPatch {
                name: Some("X".into()),
                ..Default::default()
            },
        }));
        let err = tx
            .execute(&mut tree, Some(&mut edges), BatchExecuteOptions::default())
            .unwrap_err();
        assert!(err.message.contains("not found"));
    }

    // ── execute: delete ───────────────────────────────────────

    #[test]
    fn execute_deletes_object_and_descendants() {
        let mut tree = new_tree();
        let mut edges = new_edges();
        let parent = tree
            .add(draft("folder", "F"), AddOptions::default())
            .unwrap();
        tree.add(
            draft("task", "A"),
            AddOptions {
                parent_id: Some(parent.id.clone()),
                position: None,
            },
        )
        .unwrap();
        let mut tx = new_tx();
        tx.add(BatchOp::DeleteObject(DeleteObjectOp {
            id: parent.id.as_str().to_string(),
        }));
        tx.execute(&mut tree, Some(&mut edges), BatchExecuteOptions::default())
            .unwrap();
        assert_eq!(tree.len(), 0);
    }

    // ── execute: move ─────────────────────────────────────────

    #[test]
    fn execute_moves_object_to_new_parent() {
        let mut tree = new_tree();
        let mut edges = new_edges();
        let folder = tree
            .add(draft("folder", "F"), AddOptions::default())
            .unwrap();
        let task = tree.add(draft("task", "A"), AddOptions::default()).unwrap();
        let mut tx = new_tx();
        tx.add(BatchOp::MoveObject(MoveObjectOp {
            id: task.id.as_str().to_string(),
            to_parent_id: Some(folder.id.clone()),
            to_position: None,
        }));
        tx.execute(&mut tree, Some(&mut edges), BatchExecuteOptions::default())
            .unwrap();
        assert_eq!(
            tree.get(task.id.as_str()).unwrap().parent_id.as_ref(),
            Some(&folder.id)
        );
    }

    // ── execute: edges ────────────────────────────────────────

    #[test]
    fn execute_creates_edge() {
        let mut tree = new_tree();
        let mut edges = new_edges();
        let a = tree.add(draft("task", "A"), AddOptions::default()).unwrap();
        let b = tree.add(draft("task", "B"), AddOptions::default()).unwrap();
        let mut tx = new_tx();
        tx.add(BatchOp::CreateEdge(CreateEdgeOp {
            draft: EdgeDraft {
                id: None,
                source_id: a.id.clone(),
                target_id: b.id.clone(),
                relation: "depends-on".into(),
                position: None,
                data: Default::default(),
            },
        }));
        let result = tx
            .execute(&mut tree, Some(&mut edges), BatchExecuteOptions::default())
            .unwrap();
        assert_eq!(result.created_edges.len(), 1);
        assert_eq!(edges.all().len(), 1);
    }

    #[test]
    fn execute_updates_edge() {
        let mut tree = new_tree();
        let mut edges = new_edges();
        let a = tree.add(draft("task", "A"), AddOptions::default()).unwrap();
        let b = tree.add(draft("task", "B"), AddOptions::default()).unwrap();
        let edge = edges
            .add(EdgeDraft {
                id: None,
                source_id: a.id.clone(),
                target_id: b.id.clone(),
                relation: "depends-on".into(),
                position: None,
                data: Default::default(),
            })
            .unwrap();

        let mut new_data = std::collections::BTreeMap::new();
        new_data.insert("weight".to_string(), json!(5));

        let mut tx = new_tx();
        tx.add(BatchOp::UpdateEdge(UpdateEdgeOp {
            id: edge.id.as_str().to_string(),
            changes: EdgePatch {
                data: Some(new_data),
                ..Default::default()
            },
        }));
        tx.execute(&mut tree, Some(&mut edges), BatchExecuteOptions::default())
            .unwrap();
        assert_eq!(
            edges.get(edge.id.as_str()).unwrap().data.get("weight"),
            Some(&json!(5))
        );
    }

    #[test]
    fn execute_deletes_edge() {
        let mut tree = new_tree();
        let mut edges = new_edges();
        let a = tree.add(draft("task", "A"), AddOptions::default()).unwrap();
        let b = tree.add(draft("task", "B"), AddOptions::default()).unwrap();
        let edge = edges
            .add(EdgeDraft {
                id: None,
                source_id: a.id.clone(),
                target_id: b.id.clone(),
                relation: "dep".into(),
                position: None,
                data: Default::default(),
            })
            .unwrap();
        let mut tx = new_tx();
        tx.add(BatchOp::DeleteEdge(DeleteEdgeOp {
            id: edge.id.as_str().to_string(),
        }));
        tx.execute(&mut tree, Some(&mut edges), BatchExecuteOptions::default())
            .unwrap();
        assert_eq!(edges.all().len(), 0);
    }

    // ── snapshots ─────────────────────────────────────────────

    #[test]
    fn execute_emits_snapshots_for_undo() {
        let mut tree = new_tree();
        let mut edges = new_edges();
        let mut tx = new_tx();
        tx.add(BatchOp::CreateObject(CreateObjectOp {
            draft: draft("task", "A"),
            parent_id: None,
            position: None,
        }));
        tx.add(BatchOp::CreateObject(CreateObjectOp {
            draft: draft("task", "B"),
            parent_id: None,
            position: None,
        }));
        let result = tx
            .execute(
                &mut tree,
                Some(&mut edges),
                BatchExecuteOptions {
                    description: "Bulk create".into(),
                    on_progress: None,
                },
            )
            .unwrap();
        assert_eq!(result.snapshots.len(), 2);
        assert_eq!(result.description, "Bulk create");
    }

    #[test]
    fn execute_works_without_edge_model() {
        let mut tree = new_tree();
        let mut tx = BatchTransaction::new();
        tx.add(BatchOp::CreateObject(CreateObjectOp {
            draft: draft("task", "A"),
            parent_id: None,
            position: None,
        }));
        tx.execute(&mut tree, None, BatchExecuteOptions::default())
            .unwrap();
        assert_eq!(tree.len(), 1);
    }

    // ── progress callback ─────────────────────────────────────

    #[test]
    fn execute_calls_on_progress_per_op() {
        let mut tree = new_tree();
        let mut edges = new_edges();
        let mut tx = new_tx();
        tx.add(BatchOp::CreateObject(CreateObjectOp {
            draft: draft("task", "A"),
            parent_id: None,
            position: None,
        }));
        tx.add(BatchOp::CreateObject(CreateObjectOp {
            draft: draft("task", "B"),
            parent_id: None,
            position: None,
        }));

        let calls = std::rc::Rc::new(std::cell::RefCell::new(Vec::<usize>::new()));
        let calls_clone = calls.clone();

        tx.execute(
            &mut tree,
            Some(&mut edges),
            BatchExecuteOptions {
                description: "Batch".into(),
                on_progress: Some(Box::new(move |p: BatchProgress<'_>| {
                    calls_clone.borrow_mut().push(p.current);
                })),
            },
        )
        .unwrap();

        assert_eq!(*calls.borrow(), vec![0, 1]);
    }

    // ── rollback ──────────────────────────────────────────────

    #[test]
    fn execute_rolls_back_on_failure() {
        let mut tree = new_tree();
        let mut edges = new_edges();
        let mut tx = new_tx();
        tx.add(BatchOp::CreateObject(CreateObjectOp {
            draft: draft("task", "A"),
            parent_id: None,
            position: None,
        }));
        tx.add(BatchOp::UpdateObject(UpdateObjectOp {
            id: "nonexistent".into(),
            changes: GraphObjectPatch {
                name: Some("X".into()),
                ..Default::default()
            },
        }));
        let err = tx
            .execute(&mut tree, Some(&mut edges), BatchExecuteOptions::default())
            .unwrap_err();
        assert!(err.message.contains("not found"));
        // Created object should have been rolled back.
        assert_eq!(tree.len(), 0);
    }

    // ── mixed ops ─────────────────────────────────────────────

    #[test]
    fn execute_handles_update_and_move_in_one_batch() {
        let mut tree = new_tree();
        let mut edges = new_edges();

        // First batch: create the task.
        let mut tx1 = new_tx();
        tx1.add(BatchOp::CreateObject(CreateObjectOp {
            draft: draft("task", "A"),
            parent_id: None,
            position: None,
        }));
        let result1 = tx1
            .execute(
                &mut tree,
                Some(&mut edges),
                BatchExecuteOptions {
                    description: "Create".into(),
                    on_progress: None,
                },
            )
            .unwrap();
        let task_id = result1.created[0].id.clone();

        // Seed a folder so move has a target.
        let folder = tree
            .add(draft("folder", "F"), AddOptions::default())
            .unwrap();

        // Second batch: update status + move under folder.
        let mut tx2 = new_tx();
        tx2.add(BatchOp::UpdateObject(UpdateObjectOp {
            id: task_id.as_str().to_string(),
            changes: GraphObjectPatch {
                status: Some(Some("done".into())),
                ..Default::default()
            },
        }));
        tx2.add(BatchOp::MoveObject(MoveObjectOp {
            id: task_id.as_str().to_string(),
            to_parent_id: Some(folder.id.clone()),
            to_position: None,
        }));
        tx2.execute(
            &mut tree,
            Some(&mut edges),
            BatchExecuteOptions {
                description: "Update and move".into(),
                on_progress: None,
            },
        )
        .unwrap();

        let final_obj = tree.get(task_id.as_str()).unwrap();
        assert_eq!(final_obj.status.as_deref(), Some("done"));
        assert_eq!(final_obj.parent_id.as_ref(), Some(&folder.id));
    }
}
