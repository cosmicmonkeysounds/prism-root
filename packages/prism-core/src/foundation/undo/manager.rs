//! Framework-agnostic undo/redo stack.
//!
//! Port of `foundation/undo/undo-manager.ts`. Applier is passed
//! in as a `FnMut` closure so callers can wire the stack to their
//! own mutation plumbing without this crate knowing anything about
//! it.

use chrono::Utc;

use super::types::{ObjectSnapshot, UndoDirection, UndoEntry};

pub type UndoApplier = Box<dyn FnMut(&[ObjectSnapshot], UndoDirection)>;
pub type UndoListener = Box<dyn FnMut()>;

pub struct UndoRedoManagerOptions {
    pub max_history: usize,
}

impl Default for UndoRedoManagerOptions {
    fn default() -> Self {
        Self { max_history: 100 }
    }
}

pub struct UndoRedoManager {
    applier: UndoApplier,
    past: Vec<UndoEntry>,
    future: Vec<UndoEntry>,
    listeners: Vec<(u64, UndoListener)>,
    next_listener_id: u64,
    max_history: usize,
}

pub struct UndoSubscription {
    id: u64,
}

impl UndoSubscription {
    pub fn id(&self) -> u64 {
        self.id
    }
}

impl UndoRedoManager {
    pub fn new(applier: UndoApplier) -> Self {
        Self::with_options(applier, UndoRedoManagerOptions::default())
    }

    pub fn with_options(applier: UndoApplier, options: UndoRedoManagerOptions) -> Self {
        Self {
            applier,
            past: Vec::new(),
            future: Vec::new(),
            listeners: Vec::new(),
            next_listener_id: 0,
            max_history: options.max_history,
        }
    }

    // ── Record ───────────────────────────────────────────────────

    pub fn push(&mut self, description: impl Into<String>, snapshots: Vec<ObjectSnapshot>) {
        if snapshots.is_empty() {
            return;
        }
        self.past.push(UndoEntry {
            description: description.into(),
            snapshots,
            timestamp: Utc::now(),
        });
        if self.past.len() > self.max_history {
            self.past.remove(0);
        }
        self.future.clear();
        self.notify();
    }

    pub fn merge(&mut self, snapshots: Vec<ObjectSnapshot>) {
        let Some(last) = self.past.last_mut() else {
            return;
        };
        last.snapshots.extend(snapshots);
        self.notify();
    }

    // ── Undo / Redo ──────────────────────────────────────────────

    pub fn can_undo(&self) -> bool {
        !self.past.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.future.is_empty()
    }

    pub fn undo_label(&self) -> Option<&str> {
        self.past.last().map(|e| e.description.as_str())
    }

    pub fn redo_label(&self) -> Option<&str> {
        self.future.last().map(|e| e.description.as_str())
    }

    pub fn undo(&mut self) {
        let Some(entry) = self.past.pop() else {
            return;
        };
        (self.applier)(&entry.snapshots, UndoDirection::Undo);
        self.future.push(entry);
        self.notify();
    }

    pub fn redo(&mut self) {
        let Some(entry) = self.future.pop() else {
            return;
        };
        (self.applier)(&entry.snapshots, UndoDirection::Redo);
        self.past.push(entry);
        self.notify();
    }

    pub fn clear(&mut self) {
        self.past.clear();
        self.future.clear();
        self.notify();
    }

    pub fn history(&self) -> &[UndoEntry] {
        &self.past
    }

    pub fn history_size(&self) -> usize {
        self.past.len()
    }

    pub fn future_size(&self) -> usize {
        self.future.len()
    }

    // ── Subscriptions ────────────────────────────────────────────

    pub fn subscribe(&mut self, listener: UndoListener) -> UndoSubscription {
        let id = self.next_listener_id;
        self.next_listener_id += 1;
        self.listeners.push((id, listener));
        UndoSubscription { id }
    }

    pub fn unsubscribe(&mut self, subscription: UndoSubscription) {
        self.listeners.retain(|(id, _)| *id != subscription.id);
    }

    fn notify(&mut self) {
        for (_, cb) in self.listeners.iter_mut() {
            cb();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::foundation::object_model::types::{GraphObject, ObjectEdge};
    use chrono::Utc;
    use std::cell::RefCell;
    use std::rc::Rc;

    fn make_object() -> GraphObject {
        GraphObject {
            id: crate::foundation::object_model::types::ObjectId::new("obj-1"),
            type_name: "task".into(),
            name: "Test Task".into(),
            parent_id: None,
            position: 0.0,
            status: None,
            tags: Vec::new(),
            date: None,
            end_date: None,
            description: String::new(),
            color: None,
            image: None,
            pinned: false,
            data: Default::default(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            deleted_at: None,
        }
    }

    fn make_edge() -> ObjectEdge {
        ObjectEdge {
            id: crate::foundation::object_model::types::EdgeId::new("edge-1"),
            source_id: crate::foundation::object_model::types::ObjectId::new("obj-1"),
            target_id: crate::foundation::object_model::types::ObjectId::new("obj-2"),
            relation: "depends-on".into(),
            position: None,
            created_at: Utc::now(),
            data: Default::default(),
        }
    }

    type Applied = Rc<RefCell<Vec<(Vec<ObjectSnapshot>, UndoDirection)>>>;

    fn make_manager() -> (UndoRedoManager, Applied) {
        let applied: Applied = Rc::new(RefCell::new(Vec::new()));
        let applied_clone = applied.clone();
        let applier: UndoApplier = Box::new(move |snapshots, direction| {
            applied_clone
                .borrow_mut()
                .push((snapshots.to_vec(), direction));
        });
        (UndoRedoManager::new(applier), applied)
    }

    #[test]
    fn push_adds_to_undo_stack() {
        let (mut m, _) = make_manager();
        let obj = make_object();
        m.push(
            "Create task",
            vec![ObjectSnapshot::Object {
                before: None,
                after: Some(obj),
            }],
        );
        assert!(m.can_undo());
        assert_eq!(m.undo_label(), Some("Create task"));
        assert_eq!(m.history_size(), 1);
    }

    #[test]
    fn push_ignores_empty_snapshots() {
        let (mut m, _) = make_manager();
        m.push("Empty", Vec::new());
        assert!(!m.can_undo());
    }

    #[test]
    fn push_clears_redo_stack() {
        let (mut m, _) = make_manager();
        let obj = make_object();
        m.push(
            "A",
            vec![ObjectSnapshot::Object {
                before: None,
                after: Some(obj.clone()),
            }],
        );
        m.undo();
        assert!(m.can_redo());
        m.push(
            "B",
            vec![ObjectSnapshot::Object {
                before: None,
                after: Some(obj),
            }],
        );
        assert!(!m.can_redo());
    }

    #[test]
    fn undo_calls_applier_and_moves_to_future() {
        let (mut m, applied) = make_manager();
        let obj = make_object();
        m.push(
            "Create",
            vec![ObjectSnapshot::Object {
                before: None,
                after: Some(obj),
            }],
        );
        m.undo();
        let calls = applied.borrow();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].1, UndoDirection::Undo);
        drop(calls);
        assert!(!m.can_undo());
        assert!(m.can_redo());
    }

    #[test]
    fn redo_calls_applier_and_moves_to_past() {
        let (mut m, applied) = make_manager();
        let obj = make_object();
        m.push(
            "Create",
            vec![ObjectSnapshot::Object {
                before: None,
                after: Some(obj),
            }],
        );
        m.undo();
        m.redo();
        let calls = applied.borrow();
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[1].1, UndoDirection::Redo);
        drop(calls);
        assert!(m.can_undo());
        assert!(!m.can_redo());
    }

    #[test]
    fn undo_and_redo_on_empty_are_noops() {
        let (mut m, applied) = make_manager();
        m.undo();
        m.redo();
        assert_eq!(applied.borrow().len(), 0);
    }

    #[test]
    fn multiple_undo_redo_preserves_order() {
        let (mut m, _) = make_manager();
        let obj = make_object();
        m.push(
            "Create 1",
            vec![ObjectSnapshot::Object {
                before: None,
                after: Some(obj.clone()),
            }],
        );
        m.push(
            "Create 2",
            vec![ObjectSnapshot::Object {
                before: None,
                after: Some(obj),
            }],
        );
        assert_eq!(m.undo_label(), Some("Create 2"));
        m.undo();
        assert_eq!(m.undo_label(), Some("Create 1"));
        m.undo();
        assert!(!m.can_undo());
        m.redo();
        assert_eq!(m.undo_label(), Some("Create 1"));
        m.redo();
        assert_eq!(m.undo_label(), Some("Create 2"));
    }

    #[test]
    fn merge_appends_to_last_entry() {
        let (mut m, _) = make_manager();
        let obj = make_object();
        let edge = make_edge();
        m.push(
            "Create",
            vec![ObjectSnapshot::Object {
                before: None,
                after: Some(obj),
            }],
        );
        m.merge(vec![ObjectSnapshot::Edge {
            before: None,
            after: Some(edge),
        }]);
        assert_eq!(m.history_size(), 1);
        assert_eq!(m.history()[0].snapshots.len(), 2);
    }

    #[test]
    fn merge_on_empty_is_noop() {
        let (mut m, _) = make_manager();
        m.merge(vec![ObjectSnapshot::Object {
            before: None,
            after: Some(make_object()),
        }]);
        assert_eq!(m.history_size(), 0);
    }

    #[test]
    fn clear_empties_both_stacks() {
        let (mut m, _) = make_manager();
        let obj = make_object();
        m.push(
            "A",
            vec![ObjectSnapshot::Object {
                before: None,
                after: Some(obj.clone()),
            }],
        );
        m.push(
            "B",
            vec![ObjectSnapshot::Object {
                before: None,
                after: Some(obj),
            }],
        );
        m.undo();
        m.clear();
        assert!(!m.can_undo());
        assert!(!m.can_redo());
    }

    #[test]
    fn respects_max_history_limit() {
        let applied: Applied = Rc::new(RefCell::new(Vec::new()));
        let ac = applied.clone();
        let applier: UndoApplier = Box::new(move |s, d| ac.borrow_mut().push((s.to_vec(), d)));
        let mut m =
            UndoRedoManager::with_options(applier, UndoRedoManagerOptions { max_history: 3 });
        let obj = make_object();
        for i in 0..5 {
            m.push(
                format!("Action {i}"),
                vec![ObjectSnapshot::Object {
                    before: None,
                    after: Some(obj.clone()),
                }],
            );
        }
        assert_eq!(m.history_size(), 3);
        assert_eq!(m.undo_label(), Some("Action 4"));
    }

    #[test]
    fn empty_stacks_return_none_labels() {
        let (m, _) = make_manager();
        assert!(m.undo_label().is_none());
        assert!(m.redo_label().is_none());
    }

    #[test]
    fn subscribe_notifies_on_push_undo_redo_clear_merge() {
        let (mut m, _) = make_manager();
        let calls = Rc::new(RefCell::new(0usize));
        let cc = calls.clone();
        m.subscribe(Box::new(move || *cc.borrow_mut() += 1));
        let obj = make_object();
        m.push(
            "A",
            vec![ObjectSnapshot::Object {
                before: None,
                after: Some(obj.clone()),
            }],
        );
        m.undo();
        m.redo();
        m.merge(vec![ObjectSnapshot::Edge {
            before: None,
            after: Some(make_edge()),
        }]);
        m.clear();
        assert_eq!(*calls.borrow(), 5);
    }

    #[test]
    fn unsubscribe_stops_notifications() {
        let (mut m, _) = make_manager();
        let calls = Rc::new(RefCell::new(0usize));
        let cc = calls.clone();
        let sub = m.subscribe(Box::new(move || *cc.borrow_mut() += 1));
        m.unsubscribe(sub);
        m.push(
            "A",
            vec![ObjectSnapshot::Object {
                before: None,
                after: Some(make_object()),
            }],
        );
        assert_eq!(*calls.borrow(), 0);
    }

    #[test]
    fn handles_edge_snapshots() {
        let (mut m, applied) = make_manager();
        let edge = make_edge();
        m.push(
            "Create edge",
            vec![ObjectSnapshot::Edge {
                before: None,
                after: Some(edge),
            }],
        );
        m.undo();
        let calls = applied.borrow();
        assert!(matches!(calls[0].0[0], ObjectSnapshot::Edge { .. }));
    }

    #[test]
    fn batch_entry_undoes_all_snapshots_together() {
        let (mut m, applied) = make_manager();
        let obj = make_object();
        let edge = make_edge();
        m.push(
            "Move to folder",
            vec![
                ObjectSnapshot::Object {
                    before: Some(obj.clone()),
                    after: Some(obj),
                },
                ObjectSnapshot::Edge {
                    before: None,
                    after: Some(edge),
                },
            ],
        );
        m.undo();
        assert_eq!(applied.borrow()[0].0.len(), 2);
    }
}
