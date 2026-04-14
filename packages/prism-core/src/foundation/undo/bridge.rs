//! Wires [`TreeModel`] and [`EdgeModel`] lifecycle hooks to an
//! [`UndoRedoManager`] so every mutation auto-records snapshots.
//!
//! Port of `foundation/undo/undo-bridge.ts`. The TS version returns
//! a hooks object the caller plugs into the tree/edge constructor;
//! the Rust version does the same but the manager is wrapped in
//! `Rc<RefCell<…>>` so the closures can share it.
//!
//! [`TreeModel`]: crate::foundation::object_model::TreeModel
//! [`EdgeModel`]: crate::foundation::object_model::EdgeModel

use std::cell::RefCell;
use std::rc::Rc;

use crate::foundation::object_model::edge_model::EdgeModelHooks;
use crate::foundation::object_model::tree_model::TreeModelHooks;

use super::manager::UndoRedoManager;
use super::types::ObjectSnapshot;

pub type SharedUndoManager = Rc<RefCell<UndoRedoManager>>;

pub struct UndoBridge {
    pub tree_hooks: TreeModelHooks,
    pub edge_hooks: EdgeModelHooks,
}

pub fn create_undo_bridge(manager: SharedUndoManager) -> UndoBridge {
    let tree_hooks = TreeModelHooks {
        after_add: Some({
            let m = manager.clone();
            Box::new(move |object| {
                m.borrow_mut().push(
                    format!("Create {}", object.type_name),
                    vec![ObjectSnapshot::Object {
                        before: None,
                        after: Some(object.clone()),
                    }],
                );
            })
        }),
        after_remove: Some({
            let m = manager.clone();
            Box::new(move |object, descendants| {
                let mut snapshots = Vec::with_capacity(descendants.len() + 1);
                snapshots.push(ObjectSnapshot::Object {
                    before: Some(object.clone()),
                    after: None,
                });
                for d in descendants {
                    snapshots.push(ObjectSnapshot::Object {
                        before: Some(d.clone()),
                        after: None,
                    });
                }
                m.borrow_mut()
                    .push(format!("Delete {}", object.type_name), snapshots);
            })
        }),
        after_move: Some({
            let m = manager.clone();
            Box::new(move |object| {
                m.borrow_mut().push(
                    format!("Move {}", object.type_name),
                    vec![ObjectSnapshot::Object {
                        before: None,
                        after: Some(object.clone()),
                    }],
                );
            })
        }),
        after_duplicate: Some({
            let m = manager.clone();
            Box::new(move |original, copies| {
                let snapshots = copies
                    .iter()
                    .map(|c| ObjectSnapshot::Object {
                        before: None,
                        after: Some(c.clone()),
                    })
                    .collect();
                m.borrow_mut()
                    .push(format!("Duplicate {}", original.type_name), snapshots);
            })
        }),
        after_update: Some({
            let m = manager.clone();
            Box::new(move |object, previous| {
                m.borrow_mut().push(
                    format!("Update {}", object.type_name),
                    vec![ObjectSnapshot::Object {
                        before: Some(previous.clone()),
                        after: Some(object.clone()),
                    }],
                );
            })
        }),
        ..Default::default()
    };

    let edge_hooks = EdgeModelHooks {
        after_add: Some({
            let m = manager.clone();
            Box::new(move |edge| {
                m.borrow_mut().push(
                    format!("Create edge {}", edge.relation),
                    vec![ObjectSnapshot::Edge {
                        before: None,
                        after: Some(edge.clone()),
                    }],
                );
            })
        }),
        after_remove: Some({
            let m = manager.clone();
            Box::new(move |edge| {
                m.borrow_mut().push(
                    format!("Delete edge {}", edge.relation),
                    vec![ObjectSnapshot::Edge {
                        before: Some(edge.clone()),
                        after: None,
                    }],
                );
            })
        }),
        after_update: Some({
            let m = manager.clone();
            Box::new(move |edge, previous| {
                m.borrow_mut().push(
                    format!("Update edge {}", edge.relation),
                    vec![ObjectSnapshot::Edge {
                        before: Some(previous.clone()),
                        after: Some(edge.clone()),
                    }],
                );
            })
        }),
        ..Default::default()
    };

    UndoBridge {
        tree_hooks,
        edge_hooks,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::foundation::object_model::tree_model::{
        AddOptions, GraphObjectDraft, TreeModel, TreeModelOptions,
    };
    use crate::foundation::undo::manager::{UndoApplier, UndoRedoManager};
    use crate::foundation::undo::types::UndoDirection;

    fn make_manager() -> SharedUndoManager {
        let applier: UndoApplier = Box::new(|_snapshots, _direction: UndoDirection| {});
        Rc::new(RefCell::new(UndoRedoManager::new(applier)))
    }

    #[test]
    fn returns_tree_and_edge_hooks() {
        let m = make_manager();
        let bridge = create_undo_bridge(m);
        assert!(bridge.tree_hooks.after_add.is_some());
        assert!(bridge.edge_hooks.after_add.is_some());
    }

    #[test]
    fn tree_hook_records_create() {
        let m = make_manager();
        let bridge = create_undo_bridge(m.clone());
        let mut tree = TreeModel::with_options(TreeModelOptions {
            hooks: bridge.tree_hooks,
            ..Default::default()
        });
        tree.add(GraphObjectDraft::new("task", "Test"), AddOptions::default())
            .unwrap();
        assert_eq!(m.borrow().history_size(), 1);
        assert_eq!(m.borrow().undo_label(), Some("Create task"));
    }

    #[test]
    fn tree_hook_records_remove_with_descendants() {
        let m = make_manager();
        let bridge = create_undo_bridge(m.clone());
        let mut tree = TreeModel::with_options(TreeModelOptions {
            hooks: bridge.tree_hooks,
            ..Default::default()
        });
        let parent = tree
            .add(
                GraphObjectDraft::new("task", "Parent"),
                AddOptions::default(),
            )
            .unwrap();
        let parent_id = parent.id.clone();
        tree.add(
            GraphObjectDraft::new("task", "Child"),
            AddOptions {
                parent_id: Some(parent_id.clone()),
                position: None,
            },
        )
        .unwrap();
        m.borrow_mut().clear();
        tree.remove(parent_id.as_str()).unwrap();
        let mgr = m.borrow();
        assert_eq!(mgr.history_size(), 1);
        assert_eq!(mgr.undo_label(), Some("Delete task"));
        assert_eq!(mgr.history()[0].snapshots.len(), 2);
    }
}
