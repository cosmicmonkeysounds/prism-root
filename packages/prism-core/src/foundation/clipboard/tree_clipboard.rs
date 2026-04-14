//! Cut/copy/paste for [`GraphObject`] subtrees with edge preservation.
//!
//! Port of `foundation/clipboard/tree-clipboard.ts`. Objects are
//! deep-cloned on copy; IDs are remapped on paste to avoid
//! conflicts. Cut = copy + delete sources on paste, consumed in a
//! single undo entry.

use std::collections::HashMap;

use chrono::Utc;

use crate::foundation::object_model::edge_model::{EdgeDraft, EdgeModel};
use crate::foundation::object_model::tree_model::{AddOptions, GraphObjectDraft, TreeModel};
use crate::foundation::object_model::types::{GraphObject, ObjectEdge, ObjectId};
use crate::foundation::undo::manager::UndoRedoManager;
use crate::foundation::undo::types::ObjectSnapshot;

use super::types::{ClipboardEntry, ClipboardMode, PasteOptions, PasteResult, SerializedSubtree};

pub type ClipboardIdGen = Box<dyn FnMut() -> String>;

pub struct TreeClipboard {
    current: Option<ClipboardEntry>,
    id_gen: ClipboardIdGen,
}

fn default_id_generator() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{}-{n:x}", Utc::now().timestamp_millis())
}

impl Default for TreeClipboard {
    fn default() -> Self {
        Self::new()
    }
}

impl TreeClipboard {
    pub fn new() -> Self {
        Self {
            current: None,
            id_gen: Box::new(default_id_generator),
        }
    }

    pub fn with_id_gen(id_gen: ClipboardIdGen) -> Self {
        Self {
            current: None,
            id_gen,
        }
    }

    pub fn has_content(&self) -> bool {
        self.current
            .as_ref()
            .is_some_and(|e| !e.subtrees.is_empty())
    }

    pub fn entry(&self) -> Option<&ClipboardEntry> {
        self.current.as_ref()
    }

    pub fn clear(&mut self) {
        self.current = None;
    }

    pub fn copy(&mut self, tree: &TreeModel, edges: Option<&EdgeModel>, ids: &[&str]) {
        self.set_clipboard(ClipboardMode::Copy, tree, edges, ids);
    }

    pub fn cut(&mut self, tree: &TreeModel, edges: Option<&EdgeModel>, ids: &[&str]) {
        self.set_clipboard(ClipboardMode::Cut, tree, edges, ids);
    }

    fn set_clipboard(
        &mut self,
        mode: ClipboardMode,
        tree: &TreeModel,
        edges: Option<&EdgeModel>,
        ids: &[&str],
    ) {
        let mut subtrees = Vec::new();
        for id in ids {
            if let Some(s) = serialize_subtree(id, tree, edges) {
                subtrees.push(s);
            }
        }
        if subtrees.is_empty() {
            return;
        }
        let source_ids = ids
            .iter()
            .filter(|id| tree.has(id))
            .map(|id| (*id).to_string())
            .collect();
        self.current = Some(ClipboardEntry {
            mode,
            subtrees,
            source_ids,
            timestamp: Utc::now().timestamp_millis(),
        });
    }

    pub fn paste(
        &mut self,
        tree: &mut TreeModel,
        edges: Option<&mut EdgeModel>,
        undo: Option<&mut UndoRedoManager>,
        options: PasteOptions,
    ) -> Option<PasteResult> {
        let entry = self.current.as_ref()?;
        if entry.subtrees.is_empty() {
            return None;
        }

        let parent_id_str = options.parent_id.clone();
        let parent_object_id = parent_id_str.as_deref().map(ObjectId::new);
        let mut insert_pos = options.position;

        let mut all_created: Vec<GraphObject> = Vec::new();
        let mut all_created_edges: Vec<ObjectEdge> = Vec::new();
        let mut global_id_map: HashMap<String, String> = HashMap::new();
        let mut snapshots: Vec<ObjectSnapshot> = Vec::new();

        let subtrees = entry.subtrees.clone();
        let mode = entry.mode;
        let source_ids = entry.source_ids.clone();

        let mut edges_opt = edges;

        for subtree in &subtrees {
            let mut id_map: HashMap<String, String> = HashMap::new();
            let all_objects: Vec<&GraphObject> = std::iter::once(&subtree.root)
                .chain(subtree.descendants.iter())
                .collect();
            for obj in &all_objects {
                id_map.insert(obj.id.as_str().to_string(), (self.id_gen)());
            }

            let root_new_id = id_map
                .get(subtree.root.id.as_str())
                .cloned()
                .unwrap_or_else(|| (self.id_gen)());

            let root_obj = tree
                .add(
                    draft_from_object(&subtree.root, root_new_id.clone()),
                    AddOptions {
                        parent_id: parent_object_id.clone(),
                        position: insert_pos,
                    },
                )
                .ok()?;
            all_created.push(root_obj.clone());
            snapshots.push(ObjectSnapshot::Object {
                before: None,
                after: Some(root_obj.clone()),
            });

            if let Some(p) = insert_pos.as_mut() {
                *p += 1.0;
            }

            add_descendants(
                subtree.root.id.as_str(),
                &root_new_id,
                &subtree.descendants,
                &id_map,
                tree,
                &mut all_created,
                &mut snapshots,
            );

            if let Some(edges_ref) = edges_opt.as_deref_mut() {
                for edge in &subtree.internal_edges {
                    let new_source = id_map.get(edge.source_id.as_str());
                    let new_target = id_map.get(edge.target_id.as_str());
                    if let (Some(ns), Some(nt)) = (new_source, new_target) {
                        if let Ok(new_edge) = edges_ref.add(EdgeDraft {
                            id: None,
                            source_id: ObjectId::new(ns.clone()),
                            target_id: ObjectId::new(nt.clone()),
                            relation: edge.relation.clone(),
                            position: edge.position,
                            data: edge.data.clone(),
                        }) {
                            all_created_edges.push(new_edge.clone());
                            snapshots.push(ObjectSnapshot::Edge {
                                before: None,
                                after: Some(new_edge),
                            });
                        }
                    }
                }
            }

            for (old, new_) in id_map.into_iter() {
                global_id_map.insert(old, new_);
            }
        }

        if mode == ClipboardMode::Cut {
            for source_id in &source_ids {
                if let Some(source) = tree.get(source_id).cloned() {
                    let descendants = tree.get_descendants(source_id);
                    snapshots.push(ObjectSnapshot::Object {
                        before: Some(source),
                        after: None,
                    });
                    for d in descendants {
                        snapshots.push(ObjectSnapshot::Object {
                            before: Some(d),
                            after: None,
                        });
                    }
                    tree.remove(source_id);
                }
            }
            self.current = None;
        }

        if let Some(mgr) = undo {
            if !snapshots.is_empty() {
                mgr.push("Paste", snapshots);
            }
        }

        Some(PasteResult {
            created: all_created,
            created_edges: all_created_edges,
            id_map: global_id_map,
        })
    }
}

fn serialize_subtree(
    root_id: &str,
    tree: &TreeModel,
    edges: Option<&EdgeModel>,
) -> Option<SerializedSubtree> {
    let root = tree.get(root_id)?.clone();
    let descendants = tree.get_descendants(root_id);
    let mut all_ids: std::collections::HashSet<String> = std::collections::HashSet::new();
    all_ids.insert(root_id.to_string());
    for d in &descendants {
        all_ids.insert(d.id.as_str().to_string());
    }

    let mut internal_edges: Vec<ObjectEdge> = Vec::new();
    if let Some(edges) = edges {
        for id in &all_ids {
            let from = edges.get_from(&ObjectId::new(id.clone()), None);
            for edge in from {
                if all_ids.contains(edge.target_id.as_str()) {
                    internal_edges.push(edge);
                }
            }
        }
    }

    Some(SerializedSubtree {
        root,
        descendants,
        internal_edges,
    })
}

fn add_descendants(
    original_parent_id: &str,
    new_parent_id: &str,
    descendants: &[GraphObject],
    id_map: &HashMap<String, String>,
    tree: &mut TreeModel,
    created: &mut Vec<GraphObject>,
    snapshots: &mut Vec<ObjectSnapshot>,
) {
    let mut children: Vec<&GraphObject> = descendants
        .iter()
        .filter(|d| {
            d.parent_id
                .as_ref()
                .is_some_and(|p| p.as_str() == original_parent_id)
        })
        .collect();
    children.sort_by(|a, b| a.position.partial_cmp(&b.position).unwrap());

    for child in children {
        let child_new_id = id_map
            .get(child.id.as_str())
            .cloned()
            .unwrap_or_else(|| child.id.as_str().to_string());
        let draft = draft_from_object(child, child_new_id.clone());
        if let Ok(child_obj) = tree.add(
            draft,
            AddOptions {
                parent_id: Some(ObjectId::new(new_parent_id.to_string())),
                position: None,
            },
        ) {
            created.push(child_obj.clone());
            snapshots.push(ObjectSnapshot::Object {
                before: None,
                after: Some(child_obj),
            });
            add_descendants(
                child.id.as_str(),
                &child_new_id,
                descendants,
                id_map,
                tree,
                created,
                snapshots,
            );
        }
    }
}

fn draft_from_object(obj: &GraphObject, new_id: String) -> GraphObjectDraft {
    GraphObjectDraft {
        id: Some(new_id),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::foundation::object_model::tree_model::TreeModelOptions;
    use crate::foundation::undo::manager::{UndoApplier, UndoRedoManager};

    fn seq_id_gen() -> ClipboardIdGen {
        use std::sync::atomic::{AtomicU64, Ordering};
        let counter = std::sync::Arc::new(AtomicU64::new(0));
        Box::new(move || {
            let n = counter.fetch_add(1, Ordering::Relaxed);
            format!("paste-{n}")
        })
    }

    fn tree_id_gen() -> Box<dyn FnMut() -> String> {
        use std::sync::atomic::{AtomicU64, Ordering};
        let counter = std::sync::Arc::new(AtomicU64::new(0));
        Box::new(move || {
            let n = counter.fetch_add(1, Ordering::Relaxed);
            format!("id-{}", n + 1)
        })
    }

    fn make_tree() -> TreeModel {
        TreeModel::with_options(TreeModelOptions {
            id_gen: Some(tree_id_gen()),
            ..Default::default()
        })
    }

    fn make_edges() -> EdgeModel {
        EdgeModel::with_options(
            crate::foundation::object_model::edge_model::EdgeModelOptions {
                id_gen: Some(tree_id_gen()),
                ..Default::default()
            },
        )
    }

    fn make_undo() -> UndoRedoManager {
        let applier: UndoApplier = Box::new(|_s, _d| {});
        UndoRedoManager::new(applier)
    }

    fn make_clipboard() -> TreeClipboard {
        TreeClipboard::with_id_gen(seq_id_gen())
    }

    #[test]
    fn empty_state_paste_returns_none() {
        let mut tree = make_tree();
        let mut cb = make_clipboard();
        assert!(!cb.has_content());
        assert!(cb.entry().is_none());
        assert!(cb
            .paste(&mut tree, None, None, PasteOptions::default())
            .is_none());
    }

    #[test]
    fn copies_a_single_object() {
        let mut tree = make_tree();
        tree.add(GraphObjectDraft::new("task", "A"), AddOptions::default())
            .unwrap();
        let mut cb = make_clipboard();
        cb.copy(&tree, None, &["id-1"]);
        assert!(cb.has_content());
        let entry = cb.entry().unwrap();
        assert_eq!(entry.mode, ClipboardMode::Copy);
        assert_eq!(entry.subtrees.len(), 1);
    }

    #[test]
    fn copies_multiple_objects() {
        let mut tree = make_tree();
        tree.add(GraphObjectDraft::new("task", "A"), AddOptions::default())
            .unwrap();
        tree.add(GraphObjectDraft::new("task", "B"), AddOptions::default())
            .unwrap();
        let mut cb = make_clipboard();
        cb.copy(&tree, None, &["id-1", "id-2"]);
        assert_eq!(cb.entry().unwrap().subtrees.len(), 2);
    }

    #[test]
    fn copies_object_with_descendants() {
        let mut tree = make_tree();
        let parent = tree
            .add(GraphObjectDraft::new("folder", "F"), AddOptions::default())
            .unwrap();
        tree.add(
            GraphObjectDraft::new("task", "A"),
            AddOptions {
                parent_id: Some(parent.id.clone()),
                position: None,
            },
        )
        .unwrap();
        tree.add(
            GraphObjectDraft::new("task", "B"),
            AddOptions {
                parent_id: Some(parent.id.clone()),
                position: None,
            },
        )
        .unwrap();
        let mut cb = make_clipboard();
        cb.copy(&tree, None, &[parent.id.as_str()]);
        assert_eq!(cb.entry().unwrap().subtrees[0].descendants.len(), 2);
    }

    #[test]
    fn skips_nonexistent_objects() {
        let mut tree = make_tree();
        tree.add(GraphObjectDraft::new("task", "A"), AddOptions::default())
            .unwrap();
        let mut cb = make_clipboard();
        cb.copy(&tree, None, &["id-1", "nonexistent"]);
        assert_eq!(cb.entry().unwrap().subtrees.len(), 1);
    }

    #[test]
    fn captures_internal_edges() {
        let mut tree = make_tree();
        let mut edges = make_edges();
        let a = tree
            .add(GraphObjectDraft::new("task", "A"), AddOptions::default())
            .unwrap();
        let b = tree
            .add(GraphObjectDraft::new("task", "B"), AddOptions::default())
            .unwrap();
        let parent = tree
            .add(GraphObjectDraft::new("folder", "F"), AddOptions::default())
            .unwrap();
        tree.move_to(a.id.as_str(), Some(parent.id.clone()), None)
            .unwrap();
        tree.move_to(b.id.as_str(), Some(parent.id.clone()), None)
            .unwrap();
        edges
            .add(EdgeDraft {
                id: None,
                source_id: a.id.clone(),
                target_id: b.id.clone(),
                relation: "depends-on".into(),
                position: None,
                data: Default::default(),
            })
            .unwrap();
        let mut cb = make_clipboard();
        cb.copy(&tree, Some(&edges), &[parent.id.as_str()]);
        assert_eq!(cb.entry().unwrap().subtrees[0].internal_edges.len(), 1);
    }

    #[test]
    fn paste_creates_new_objects_with_fresh_ids() {
        let mut tree = make_tree();
        tree.add(GraphObjectDraft::new("task", "A"), AddOptions::default())
            .unwrap();
        let mut cb = make_clipboard();
        cb.copy(&tree, None, &["id-1"]);
        let result = cb
            .paste(&mut tree, None, None, PasteOptions::default())
            .unwrap();
        assert_eq!(result.created.len(), 1);
        assert_ne!(result.created[0].id.as_str(), "id-1");
        assert_eq!(result.created[0].name, "A");
        assert_eq!(tree.len(), 2);
    }

    #[test]
    fn paste_under_specified_parent() {
        let mut tree = make_tree();
        let folder = tree
            .add(GraphObjectDraft::new("folder", "F"), AddOptions::default())
            .unwrap();
        tree.add(GraphObjectDraft::new("task", "A"), AddOptions::default())
            .unwrap();
        let mut cb = make_clipboard();
        cb.copy(&tree, None, &["id-2"]);
        let result = cb
            .paste(
                &mut tree,
                None,
                None,
                PasteOptions {
                    parent_id: Some(folder.id.as_str().to_string()),
                    position: None,
                },
            )
            .unwrap();
        assert_eq!(
            result.created[0].parent_id.as_ref().map(|p| p.as_str()),
            Some(folder.id.as_str())
        );
    }

    #[test]
    fn paste_deep_copies_preserving_hierarchy() {
        let mut tree = make_tree();
        let parent = tree
            .add(GraphObjectDraft::new("folder", "F"), AddOptions::default())
            .unwrap();
        tree.add(
            GraphObjectDraft::new("task", "A"),
            AddOptions {
                parent_id: Some(parent.id.clone()),
                position: None,
            },
        )
        .unwrap();
        tree.add(
            GraphObjectDraft::new("task", "B"),
            AddOptions {
                parent_id: Some(parent.id.clone()),
                position: None,
            },
        )
        .unwrap();
        let mut cb = make_clipboard();
        cb.copy(&tree, None, &[parent.id.as_str()]);
        let result = cb
            .paste(&mut tree, None, None, PasteOptions::default())
            .unwrap();
        assert_eq!(result.created.len(), 3);
        let new_folder = &result.created[0];
        let new_children = tree.get_children(Some(&new_folder.id));
        assert_eq!(new_children.len(), 2);
    }

    #[test]
    fn paste_remaps_internal_edges() {
        let mut tree = make_tree();
        let mut edges = make_edges();
        let a = tree
            .add(GraphObjectDraft::new("task", "A"), AddOptions::default())
            .unwrap();
        let b = tree
            .add(GraphObjectDraft::new("task", "B"), AddOptions::default())
            .unwrap();
        let parent = tree
            .add(GraphObjectDraft::new("folder", "F"), AddOptions::default())
            .unwrap();
        tree.move_to(a.id.as_str(), Some(parent.id.clone()), None)
            .unwrap();
        tree.move_to(b.id.as_str(), Some(parent.id.clone()), None)
            .unwrap();
        edges
            .add(EdgeDraft {
                id: None,
                source_id: a.id.clone(),
                target_id: b.id.clone(),
                relation: "dep".into(),
                position: None,
                data: Default::default(),
            })
            .unwrap();
        let mut cb = make_clipboard();
        cb.copy(&tree, Some(&edges), &[parent.id.as_str()]);
        let result = cb
            .paste(&mut tree, Some(&mut edges), None, PasteOptions::default())
            .unwrap();
        assert_eq!(result.created_edges.len(), 1);
        let new_edge = &result.created_edges[0];
        assert_ne!(new_edge.source_id.as_str(), a.id.as_str());
        assert_ne!(new_edge.target_id.as_str(), b.id.as_str());
        let new_ids: std::collections::HashSet<String> = result
            .created
            .iter()
            .map(|o| o.id.as_str().to_string())
            .collect();
        assert!(new_ids.contains(new_edge.source_id.as_str()));
        assert!(new_ids.contains(new_edge.target_id.as_str()));
    }

    #[test]
    fn paste_provides_id_map() {
        let mut tree = make_tree();
        tree.add(GraphObjectDraft::new("task", "A"), AddOptions::default())
            .unwrap();
        let mut cb = make_clipboard();
        cb.copy(&tree, None, &["id-1"]);
        let result = cb
            .paste(&mut tree, None, None, PasteOptions::default())
            .unwrap();
        assert_eq!(result.id_map.len(), 1);
        assert_eq!(
            result.id_map.get("id-1").map(|s| s.as_str()),
            Some(result.created[0].id.as_str())
        );
    }

    #[test]
    fn allows_multiple_pastes_from_same_copy() {
        let mut tree = make_tree();
        tree.add(GraphObjectDraft::new("task", "A"), AddOptions::default())
            .unwrap();
        let mut cb = make_clipboard();
        cb.copy(&tree, None, &["id-1"]);
        cb.paste(&mut tree, None, None, PasteOptions::default());
        cb.paste(&mut tree, None, None, PasteOptions::default());
        assert_eq!(tree.len(), 3);
    }

    #[test]
    fn pastes_at_specified_position() {
        let mut tree = make_tree();
        tree.add(
            GraphObjectDraft::new("task", "Existing"),
            AddOptions::default(),
        )
        .unwrap();
        tree.add(
            GraphObjectDraft::new("task", "Source"),
            AddOptions::default(),
        )
        .unwrap();
        let mut cb = make_clipboard();
        cb.copy(&tree, None, &["id-2"]);
        cb.paste(
            &mut tree,
            None,
            None,
            PasteOptions {
                parent_id: None,
                position: Some(0.0),
            },
        );
        let roots = tree.get_children(None);
        assert_eq!(roots[0].name, "Source");
    }

    #[test]
    fn cut_mode_sets_cut() {
        let mut tree = make_tree();
        tree.add(GraphObjectDraft::new("task", "A"), AddOptions::default())
            .unwrap();
        let mut cb = make_clipboard();
        cb.cut(&tree, None, &["id-1"]);
        assert_eq!(cb.entry().unwrap().mode, ClipboardMode::Cut);
    }

    #[test]
    fn cut_deletes_source_objects_on_paste() {
        let mut tree = make_tree();
        tree.add(GraphObjectDraft::new("task", "A"), AddOptions::default())
            .unwrap();
        let mut cb = make_clipboard();
        cb.cut(&tree, None, &["id-1"]);
        let result = cb
            .paste(&mut tree, None, None, PasteOptions::default())
            .unwrap();
        assert_eq!(result.created.len(), 1);
        assert!(!tree.has("id-1"));
        assert_eq!(tree.len(), 1);
    }

    #[test]
    fn cut_clears_clipboard_after_paste() {
        let mut tree = make_tree();
        tree.add(GraphObjectDraft::new("task", "A"), AddOptions::default())
            .unwrap();
        let mut cb = make_clipboard();
        cb.cut(&tree, None, &["id-1"]);
        cb.paste(&mut tree, None, None, PasteOptions::default());
        assert!(!cb.has_content());
        assert!(cb
            .paste(&mut tree, None, None, PasteOptions::default())
            .is_none());
    }

    #[test]
    fn undo_integration_pushes_single_entry_for_paste() {
        let mut tree = make_tree();
        tree.add(GraphObjectDraft::new("task", "A"), AddOptions::default())
            .unwrap();
        let mut cb = make_clipboard();
        let mut undo = make_undo();
        cb.copy(&tree, None, &["id-1"]);
        cb.paste(&mut tree, None, Some(&mut undo), PasteOptions::default());
        assert_eq!(undo.history().len(), 1);
        assert_eq!(undo.history()[0].description, "Paste");
    }

    #[test]
    fn cut_includes_deletions_in_same_undo_entry() {
        let mut tree = make_tree();
        tree.add(GraphObjectDraft::new("task", "A"), AddOptions::default())
            .unwrap();
        let mut cb = make_clipboard();
        let mut undo = make_undo();
        cb.cut(&tree, None, &["id-1"]);
        cb.paste(&mut tree, None, Some(&mut undo), PasteOptions::default());
        assert_eq!(undo.history().len(), 1);
        assert!(undo.history()[0].snapshots.len() >= 2);
    }

    #[test]
    fn clear_empties_clipboard() {
        let mut tree = make_tree();
        tree.add(GraphObjectDraft::new("task", "A"), AddOptions::default())
            .unwrap();
        let mut cb = make_clipboard();
        cb.copy(&tree, None, &["id-1"]);
        cb.clear();
        assert!(!cb.has_content());
        assert!(cb
            .paste(&mut tree, None, None, PasteOptions::default())
            .is_none());
    }

    #[test]
    fn works_without_edges() {
        let mut tree = make_tree();
        tree.add(GraphObjectDraft::new("task", "A"), AddOptions::default())
            .unwrap();
        let mut cb = make_clipboard();
        cb.copy(&tree, None, &["id-1"]);
        let result = cb
            .paste(&mut tree, None, None, PasteOptions::default())
            .unwrap();
        assert_eq!(result.created.len(), 1);
        assert_eq!(result.created_edges.len(), 0);
    }
}
