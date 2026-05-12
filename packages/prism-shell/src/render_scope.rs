//! `render_scope` — Phase 3 of the Dioxus-inspired reactive overhaul
//! (`docs/dev/dioxus-inspiration.md`).
//!
//! The plan: the shell's per-frame render walk runs inside an
//! `Owner` + outermost `ReactiveContext`, and per-block lowering
//! runs inside per-block contexts whose dirty callback marks a
//! `NodeId` into a `DirtyQueue<NodeId>`. The femtovg backend's
//! redraw scheduling reads the queue: empty → no `request_redraw`;
//! non-empty → redraw.
//!
//! This module lands the primitive that bridges signal-driven
//! invalidation to the shell's existing event-driven redraw path.
//! Today the shell rebuilds the whole runtime tree on every event
//! that `dispatch_event` reports as handled; a [`RenderScope`] adds
//! a second source of "needs redraw" — fine-grained signal changes
//! pushed by services or bindings without needing an event arm.
//!
//! ## Shape
//!
//! ```ignore
//! let scope = RenderScope::new();
//! // Anywhere a service holds a reactive signal:
//! scope.invalidate_on("node-id", move || {
//!     let _ = some_signal.read(|_| {});
//! });
//! // … later, when `some_signal` changes, the dirty queue gets
//! // `"node-id"` pushed.
//!
//! // In the per-frame redraw loop:
//! if scope.needs_redraw() {
//!     let dirty: Vec<String> = scope.drain_dirty();
//!     // … re-lower the dirty subtrees (Phase 3-final) OR
//!     // re-render the whole tree (Phase 3 today). The full
//!     // per-block lowering pass lands once `lower_ui` is reactive.
//! }
//! ```
//!
//! ## Why it lives in `prism-shell`
//!
//! The reactive primitives (`Owner`, `ReactiveContext`, `Effect`,
//! `DirtyQueue`) all live in `prism-core::reactive`. The shell is
//! the host that owns the per-frame render walk and the femtovg
//! redraw schedule — the natural place for the per-host `Owner` +
//! `DirtyQueue<NodeId>` pair. The relay's SSR-scope (Phase 8) will
//! get its own analogous wrapper later.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use prism_core::reactive::{DirtyQueue, Owner};

/// Shared dirty-queue handle so effects on `RenderScope::owner` can
/// push from inside their closure.
type SharedDirty = Rc<RefCell<DirtyQueue<String>>>;

/// Per-shell reactive render scope: an `Owner` for effects + a
/// `DirtyQueue<NodeId>` they fan into.
///
/// `RenderScope` is `Clone`-cheap (shared inner via two `Rc`s). The
/// owner and queue both share its lifetime; dropping the last clone
/// disposes every effect inserted through [`RenderScope::invalidate_on`].
#[derive(Clone)]
pub struct RenderScope {
    owner: Rc<Owner>,
    dirty: SharedDirty,
}

impl RenderScope {
    /// Construct a fresh scope with an empty dirty queue.
    pub fn new() -> Self {
        Self {
            owner: Rc::new(Owner::new()),
            dirty: Rc::new(RefCell::new(DirtyQueue::new())),
        }
    }

    /// The per-shell reactive `Owner`. Effects allocated through
    /// this owner are torn down when the scope drops; for the
    /// common "mark this node dirty on signal change" pattern, use
    /// [`RenderScope::invalidate_on`] instead.
    pub fn owner(&self) -> &Owner {
        &self.owner
    }

    /// True if any reactive invalidation has marked a node dirty
    /// since the last [`RenderScope::drain_dirty`]. The femtovg
    /// frame handler reads this to decide whether to redraw.
    pub fn needs_redraw(&self) -> bool {
        !self.dirty.borrow().is_empty()
    }

    /// Number of distinct node IDs currently in the dirty queue.
    pub fn pending(&self) -> usize {
        self.dirty.borrow().len()
    }

    /// Drain the dirty queue. Returns node IDs in insertion order
    /// (FIFO with O(1) deduplication). The queue is empty after
    /// this call.
    pub fn drain_dirty(&self) -> Vec<String> {
        self.dirty.borrow_mut().drain()
    }

    /// Directly mark a node id dirty. Useful when imperative code
    /// (a service handler, an IPC bridge) needs to force a redraw
    /// of a specific node without authoring an effect.
    pub fn mark_dirty(&self, node_id: impl Into<String>) {
        self.dirty.borrow_mut().mark(node_id.into());
    }

    /// Build an effect that fires once initially (to subscribe its
    /// dependency set) and on every subsequent dirty signal it
    /// observes, marking `node_id` into the dirty queue on every
    /// re-fire *after* the initial subscription pass.
    ///
    /// The body `probe` should perform the signal reads whose
    /// changes the caller wants to track. The effect's tracked
    /// dependency set is the union of all signals `probe` touches;
    /// any subsequent write to one of them re-fires the effect and
    /// queues `node_id`.
    ///
    /// The initial-run skip matters: an `Effect` body always runs
    /// once on construction (to record its dependency set), and we
    /// don't want that bootstrap fire to leak into the dirty queue
    /// as a phantom invalidation before the first frame.
    pub fn invalidate_on<F>(&self, node_id: impl Into<String>, mut probe: F)
    where
        F: FnMut() + 'static,
    {
        let id = node_id.into();
        let dirty = Rc::clone(&self.dirty);
        let first = Rc::new(Cell::new(true));
        self.owner.insert_effect(move || {
            probe();
            if first.replace(false) {
                return;
            }
            dirty.borrow_mut().mark(id.clone());
        });
    }
}

impl Default for RenderScope {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use prism_core::reactive::Owner as CoreOwner;

    #[test]
    fn fresh_scope_is_clean() {
        let s = RenderScope::new();
        assert!(!s.needs_redraw());
        assert_eq!(s.pending(), 0);
        assert!(s.drain_dirty().is_empty());
    }

    #[test]
    fn mark_dirty_dedups_per_node() {
        let s = RenderScope::new();
        s.mark_dirty("a");
        s.mark_dirty("b");
        s.mark_dirty("a"); // dup
        assert!(s.needs_redraw());
        assert_eq!(s.pending(), 2);
        let drained = s.drain_dirty();
        assert_eq!(drained, vec!["a".to_string(), "b".to_string()]);
        assert!(!s.needs_redraw());
    }

    #[test]
    fn invalidate_on_skips_initial_run() {
        // Subscribing a probe must not immediately mark the node
        // dirty — only subsequent signal writes should. Otherwise
        // every service wiring up its bindings would mass-flood
        // the dirty queue before the first frame.
        let outer_owner = CoreOwner::new();
        let sig = outer_owner.insert(0_i32);
        let scope = RenderScope::new();
        scope.invalidate_on("node-1", move || {
            let _ = sig.get(); // subscribes
        });
        assert!(!scope.needs_redraw(), "initial subscribe didn't mark");
        sig.set(1);
        assert_eq!(scope.drain_dirty(), vec!["node-1".to_string()]);
    }

    #[test]
    fn invalidate_on_coalesces_multiple_writes() {
        let outer_owner = CoreOwner::new();
        let sig = outer_owner.insert(0_i32);
        let scope = RenderScope::new();
        scope.invalidate_on("node-2", move || {
            let _ = sig.get();
        });
        sig.set(1);
        sig.set(2);
        sig.set(3);
        // Three writes coalesce into one dirty mark — the DirtyQueue
        // dedups by id between drains.
        assert_eq!(scope.drain_dirty(), vec!["node-2".to_string()]);
    }

    #[test]
    fn invalidate_on_multiple_nodes_record_order() {
        let outer_owner = CoreOwner::new();
        let sig_a = outer_owner.insert(0);
        let sig_b = outer_owner.insert(0);
        let scope = RenderScope::new();
        scope.invalidate_on("node-a", move || {
            let _ = sig_a.get();
        });
        scope.invalidate_on("node-b", move || {
            let _ = sig_b.get();
        });
        sig_b.set(1);
        sig_a.set(1);
        // FIFO of marks: b then a (drained in that order).
        assert_eq!(
            scope.drain_dirty(),
            vec!["node-b".to_string(), "node-a".to_string()]
        );
    }

    #[test]
    fn scope_drop_tears_down_effects() {
        // After the scope drops, mutations to the source signal must
        // not panic and must not feed into a stale queue. The
        // foreign owner outlives the scope so the signal stays live.
        let foreign_owner = CoreOwner::new();
        let sig = foreign_owner.insert(0_i32);

        {
            let scope = RenderScope::new();
            scope.invalidate_on("node-x", move || {
                let _ = sig.get();
            });
            sig.set(1);
            assert!(scope.needs_redraw());
        } // scope drops — effect disposed via Owner-scoped lifetime

        // No panic, no observable side effect.
        sig.set(2);
        sig.set(3);
    }

    #[test]
    fn clone_shares_state() {
        let scope = RenderScope::new();
        let copy = scope.clone();
        scope.mark_dirty("shared");
        assert!(copy.needs_redraw());
        assert_eq!(copy.drain_dirty(), vec!["shared".to_string()]);
        assert!(!scope.needs_redraw(), "drain on clone empties shared queue");
    }
}
