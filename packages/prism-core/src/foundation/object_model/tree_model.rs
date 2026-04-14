//! `TreeModel` — stateful, event-driven in-memory tree of
//! [`GraphObject`]s. Port of `foundation/object-model/tree-model.ts`.
//!
//! The legacy TS version is a plain class with callback
//! subscribers. The Rust port keeps the same shape but:
//!
//! * uses `BTreeMap` for deterministic iteration,
//! * stores listeners as `Rc<RefCell<...>>`-free `Box<dyn Fn>`
//!   slots because the model is single-threaded,
//! * returns [`ObjectModelError`] instead of throwing exceptions.

// Lifecycle hooks are all `Option<Box<dyn FnMut(...)>>` slots.
// Wrapping each in a named alias would add noise without improving
// readability — allow the complexity locally.
#![allow(clippy::type_complexity)]

use std::collections::HashMap;

use chrono::Utc;
use indexmap::IndexMap;

use super::error::ObjectModelError;
#[cfg(test)]
use super::error::ObjectModelErrorCode;
use super::registry::{ObjectRegistry, TreeNode};
use super::types::{object_id, GraphObject, ObjectId};

// ── Event types ────────────────────────────────────────────────────

/// Events emitted by [`TreeModel`]. `Update` boxes its `previous`
/// snapshot so the whole enum stays small (Clippy
/// `large_enum_variant`).
#[derive(Debug, Clone)]
pub enum TreeModelEvent {
    Add {
        object: GraphObject,
    },
    Remove {
        object: GraphObject,
        descendants: Vec<GraphObject>,
    },
    Move {
        object: GraphObject,
        from: Placement,
        to: Placement,
    },
    Reorder {
        parent_id: Option<ObjectId>,
        children: Vec<GraphObject>,
    },
    Duplicate {
        original: GraphObject,
        copies: Vec<GraphObject>,
    },
    Update {
        object: GraphObject,
        previous: Box<GraphObject>,
    },
    Change,
}

#[derive(Debug, Clone)]
pub struct Placement {
    pub parent_id: Option<ObjectId>,
    pub position: f64,
}

pub type TreeModelEventListener = Box<dyn FnMut(&TreeModelEvent)>;

// ── Lifecycle hooks ────────────────────────────────────────────────

#[derive(Default)]
pub struct TreeModelHooks {
    pub before_add: Option<Box<dyn FnMut(&GraphObjectDraft, Option<&ObjectId>)>>,
    pub after_add: Option<Box<dyn FnMut(&GraphObject)>>,
    pub before_remove: Option<Box<dyn FnMut(&GraphObject)>>,
    pub after_remove: Option<Box<dyn FnMut(&GraphObject, &[GraphObject])>>,
    pub before_move: Option<Box<dyn FnMut(&GraphObject, Option<&ObjectId>, f64)>>,
    pub after_move: Option<Box<dyn FnMut(&GraphObject)>>,
    pub before_duplicate: Option<Box<dyn FnMut(&GraphObject)>>,
    pub after_duplicate: Option<Box<dyn FnMut(&GraphObject, &[GraphObject])>>,
    pub before_update: Option<Box<dyn FnMut(&GraphObject, &GraphObjectPatch)>>,
    pub after_update: Option<Box<dyn FnMut(&GraphObject, &GraphObject)>>,
}

// ── Drafts & patches ───────────────────────────────────────────────

/// Minimum fields required to create a [`GraphObject`] via
/// [`TreeModel::add`]. The rest of the shell is filled with
/// defaults identical to the legacy TS `add()`.
#[derive(Debug, Clone)]
pub struct GraphObjectDraft {
    pub id: Option<String>,
    pub type_name: String,
    pub name: String,
    pub status: Option<String>,
    pub tags: Option<Vec<String>>,
    pub date: Option<String>,
    pub end_date: Option<String>,
    pub description: Option<String>,
    pub color: Option<String>,
    pub image: Option<String>,
    pub pinned: Option<bool>,
    pub data: Option<std::collections::BTreeMap<String, serde_json::Value>>,
}

impl GraphObjectDraft {
    pub fn new(type_name: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: None,
            type_name: type_name.into(),
            name: name.into(),
            status: None,
            tags: None,
            date: None,
            end_date: None,
            description: None,
            color: None,
            image: None,
            pinned: None,
            data: None,
        }
    }
}

/// Subset of [`GraphObject`] fields allowed in
/// [`TreeModel::update`]. `id`, `type_name`, and `created_at` are
/// intentionally not present so they can't be mutated.
#[derive(Debug, Clone, Default)]
pub struct GraphObjectPatch {
    pub name: Option<String>,
    pub parent_id: Option<Option<ObjectId>>,
    pub position: Option<f64>,
    pub status: Option<Option<String>>,
    pub tags: Option<Vec<String>>,
    pub date: Option<Option<String>>,
    pub end_date: Option<Option<String>>,
    pub description: Option<String>,
    pub color: Option<Option<String>>,
    pub image: Option<Option<String>>,
    pub pinned: Option<bool>,
    pub data: Option<std::collections::BTreeMap<String, serde_json::Value>>,
}

// ── Options ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default)]
pub struct AddOptions {
    pub parent_id: Option<ObjectId>,
    pub position: Option<f64>,
}

#[derive(Debug, Clone, Default)]
pub struct DuplicateOptions {
    pub deep: bool,
    /// `Some(None)` duplicates to root, `Some(Some(_))` duplicates
    /// under a specific parent, `None` falls back to the source's
    /// current parent (matches the legacy TS `in` check).
    pub target_parent_id: Option<Option<ObjectId>>,
    pub position: Option<f64>,
}

// ── Implementation ─────────────────────────────────────────────────

pub struct TreeModel {
    map: IndexMap<String, GraphObject>,
    listeners: Vec<TreeModelEventListener>,
    hooks: TreeModelHooks,
    registry: Option<ObjectRegistry>,
    id_gen: Box<dyn FnMut() -> String>,
}

impl Default for TreeModel {
    fn default() -> Self {
        Self {
            map: IndexMap::new(),
            listeners: Vec::new(),
            hooks: TreeModelHooks::default(),
            registry: None,
            id_gen: Box::new(default_id_generator),
        }
    }
}

#[derive(Default)]
pub struct TreeModelOptions {
    pub registry: Option<ObjectRegistry>,
    pub objects: Vec<GraphObject>,
    pub hooks: TreeModelHooks,
    pub id_gen: Option<Box<dyn FnMut() -> String>>,
}

impl TreeModel {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_options(options: TreeModelOptions) -> Self {
        let mut map = IndexMap::new();
        for obj in options.objects {
            map.insert(obj.id.as_str().to_string(), obj);
        }
        Self {
            map,
            listeners: Vec::new(),
            hooks: options.hooks,
            registry: options.registry,
            id_gen: options
                .id_gen
                .unwrap_or_else(|| Box::new(default_id_generator)),
        }
    }

    pub fn set_registry(&mut self, registry: ObjectRegistry) {
        self.registry = Some(registry);
    }

    // ── Mutations ─────────────────────────────────────────────────

    pub fn add(
        &mut self,
        draft: GraphObjectDraft,
        options: AddOptions,
    ) -> Result<GraphObject, ObjectModelError> {
        let parent_id = options.parent_id;
        if let Some(parent) = parent_id.as_ref() {
            if !self.map.contains_key(parent.as_str()) {
                return Err(ObjectModelError::not_found(format!(
                    "Parent '{}' not found",
                    parent
                )));
            }
        }

        if let (Some(reg), Some(parent_ref)) = (self.registry.as_ref(), parent_id.as_ref()) {
            if let Some(parent_obj) = self.map.get(parent_ref.as_str()) {
                if !reg.can_be_child_of(&draft.type_name, &parent_obj.type_name) {
                    return Err(ObjectModelError::containment_violation(format!(
                        "'{}' cannot be a child of '{}'",
                        draft.type_name, parent_obj.type_name
                    )));
                }
            }
        }

        if let Some(hook) = self.hooks.before_add.as_mut() {
            hook(&draft, parent_id.as_ref());
        }

        let siblings = self.children_ordered(parent_id.as_ref());
        let pos_target = options
            .position
            .map(|p| p.max(0.0).min(siblings.len() as f64))
            .unwrap_or(siblings.len() as f64);

        for sib in siblings.iter() {
            if sib.position >= pos_target {
                let mut bumped = sib.clone();
                bumped.position += 1.0;
                self.map.insert(sib.id.as_str().to_string(), bumped);
            }
        }

        let now = Utc::now();
        let id_string = draft.id.clone().unwrap_or_else(|| (self.id_gen)());
        let obj = GraphObject {
            id: object_id(id_string.clone()),
            type_name: draft.type_name.clone(),
            name: draft.name.clone(),
            parent_id: parent_id.clone(),
            position: pos_target,
            status: draft.status.clone(),
            tags: draft.tags.clone().unwrap_or_default(),
            date: draft.date.clone(),
            end_date: draft.end_date.clone(),
            description: draft.description.clone().unwrap_or_default(),
            color: draft.color.clone(),
            image: draft.image.clone(),
            pinned: draft.pinned.unwrap_or(false),
            data: draft.data.clone().unwrap_or_default(),
            created_at: now,
            updated_at: now,
            deleted_at: None,
        };

        self.map.insert(id_string.clone(), obj.clone());

        if let Some(hook) = self.hooks.after_add.as_mut() {
            hook(&obj);
        }
        self.emit(TreeModelEvent::Add {
            object: obj.clone(),
        });
        self.emit(TreeModelEvent::Change);
        Ok(obj)
    }

    pub fn remove(&mut self, id: &str) -> Option<RemovedBundle> {
        let obj = self.map.get(id)?.clone();
        if let Some(hook) = self.hooks.before_remove.as_mut() {
            hook(&obj);
        }

        let descendants = self.collect_descendants(id);
        self.map.shift_remove(id);
        for desc in descendants.iter() {
            self.map.shift_remove(desc.id.as_str());
        }
        self.compact_positions(obj.parent_id.as_ref());

        if let Some(hook) = self.hooks.after_remove.as_mut() {
            hook(&obj, &descendants);
        }

        let result = RemovedBundle {
            removed: obj.clone(),
            descendants: descendants.clone(),
        };
        self.emit(TreeModelEvent::Remove {
            object: obj,
            descendants,
        });
        self.emit(TreeModelEvent::Change);
        Some(result)
    }

    pub fn move_to(
        &mut self,
        id: &str,
        to_parent_id: Option<ObjectId>,
        to_position: Option<f64>,
    ) -> Result<GraphObject, ObjectModelError> {
        let obj = self
            .map
            .get(id)
            .ok_or_else(|| ObjectModelError::not_found(format!("Object '{id}' not found")))?
            .clone();

        if let Some(target_parent) = to_parent_id.as_ref() {
            if target_parent.as_str() == id {
                return Err(ObjectModelError::circular_ref(format!(
                    "Cannot move '{id}' inside itself"
                )));
            }
            let descendant_ids: std::collections::HashSet<String> = self
                .collect_descendants(id)
                .into_iter()
                .map(|d| d.id.as_str().to_string())
                .collect();
            if descendant_ids.contains(target_parent.as_str()) {
                return Err(ObjectModelError::circular_ref(format!(
                    "Cannot move '{id}' inside its own descendant"
                )));
            }
            if !self.map.contains_key(target_parent.as_str()) {
                return Err(ObjectModelError::not_found(format!(
                    "Target parent '{target_parent}' not found"
                )));
            }
        }

        if let (Some(reg), Some(target_parent)) = (self.registry.as_ref(), to_parent_id.as_ref()) {
            if let Some(parent_obj) = self.map.get(target_parent.as_str()) {
                if !reg.can_be_child_of(&obj.type_name, &parent_obj.type_name) {
                    return Err(ObjectModelError::containment_violation(format!(
                        "'{}' cannot be a child of '{}'",
                        obj.type_name, parent_obj.type_name
                    )));
                }
            }
        }

        let new_siblings: Vec<GraphObject> = self
            .children_ordered(to_parent_id.as_ref())
            .into_iter()
            .filter(|s| s.id.as_str() != id)
            .collect();
        let pos_target = to_position
            .map(|p| p.max(0.0).min(new_siblings.len() as f64))
            .unwrap_or(new_siblings.len() as f64);

        if let Some(hook) = self.hooks.before_move.as_mut() {
            hook(&obj, to_parent_id.as_ref(), pos_target);
        }

        let from = Placement {
            parent_id: obj.parent_id.clone(),
            position: obj.position,
        };
        let changing_parent = obj.parent_id != to_parent_id;

        let mut shifted = obj.clone();
        shifted.parent_id = to_parent_id.clone();
        shifted.position = -1.0;
        self.map.insert(id.to_string(), shifted);

        if changing_parent {
            self.compact_positions(obj.parent_id.as_ref());
        }

        let fresh_siblings: Vec<GraphObject> = self
            .children_ordered(to_parent_id.as_ref())
            .into_iter()
            .filter(|s| s.id.as_str() != id)
            .collect();
        for sib in fresh_siblings.iter() {
            if sib.position >= pos_target {
                let mut bumped = sib.clone();
                bumped.position += 1.0;
                self.map.insert(sib.id.as_str().to_string(), bumped);
            }
        }

        let mut updated = self
            .map
            .get(id)
            .ok_or_else(|| ObjectModelError::not_found(format!("Object '{id}' not found")))?
            .clone();
        updated.parent_id = to_parent_id.clone();
        updated.position = pos_target;
        updated.updated_at = Utc::now();
        self.map.insert(id.to_string(), updated.clone());

        if let Some(hook) = self.hooks.after_move.as_mut() {
            hook(&updated);
        }
        self.emit(TreeModelEvent::Move {
            object: updated.clone(),
            from,
            to: Placement {
                parent_id: to_parent_id,
                position: pos_target,
            },
        });
        self.emit(TreeModelEvent::Change);
        Ok(updated)
    }

    pub fn reparent(
        &mut self,
        id: &str,
        to_parent_id: Option<ObjectId>,
    ) -> Result<GraphObject, ObjectModelError> {
        self.move_to(id, to_parent_id, None)
    }

    pub fn reorder(
        &mut self,
        id: &str,
        to_position: f64,
    ) -> Result<Vec<GraphObject>, ObjectModelError> {
        let obj = self
            .map
            .get(id)
            .ok_or_else(|| ObjectModelError::not_found(format!("Object '{id}' not found")))?
            .clone();
        let siblings = self.children_ordered(obj.parent_id.as_ref());
        let from_pos = siblings
            .iter()
            .position(|s| s.id.as_str() == id)
            .ok_or_else(|| {
                ObjectModelError::not_found(format!("Object '{id}' not in sibling list"))
            })?;

        let clamped = to_position.max(0.0).min(siblings.len() as f64) as usize;
        if clamped == from_pos {
            return Ok(siblings);
        }

        let mut reordered: Vec<GraphObject> = siblings;
        let moving = reordered.remove(from_pos);
        let insert_at = clamped.min(reordered.len());
        reordered.insert(insert_at, moving);

        let now = Utc::now();
        let updated: Vec<GraphObject> = reordered
            .into_iter()
            .enumerate()
            .map(|(i, mut s)| {
                s.position = i as f64;
                s.updated_at = now;
                self.map.insert(s.id.as_str().to_string(), s.clone());
                s
            })
            .collect();

        self.emit(TreeModelEvent::Reorder {
            parent_id: obj.parent_id.clone(),
            children: updated.clone(),
        });
        self.emit(TreeModelEvent::Change);
        Ok(updated)
    }

    pub fn duplicate(
        &mut self,
        id: &str,
        options: DuplicateOptions,
    ) -> Result<Vec<GraphObject>, ObjectModelError> {
        let obj = self
            .map
            .get(id)
            .ok_or_else(|| ObjectModelError::not_found(format!("Object '{id}' not found")))?
            .clone();

        if let Some(hook) = self.hooks.before_duplicate.as_mut() {
            hook(&obj);
        }

        let deep = options.deep;
        let target_parent = match options.target_parent_id {
            Some(p) => p,
            None => obj.parent_id.clone(),
        };

        let mut copies: Vec<GraphObject> = Vec::new();
        let now = Utc::now();

        self.copy_subtree(id, target_parent.as_ref(), deep, true, &mut copies, now)?;

        if let Some(hook) = self.hooks.after_duplicate.as_mut() {
            hook(&obj, &copies);
        }
        self.emit(TreeModelEvent::Duplicate {
            original: obj,
            copies: copies.clone(),
        });
        self.emit(TreeModelEvent::Change);
        Ok(copies)
    }

    fn copy_subtree(
        &mut self,
        source_id: &str,
        copy_parent_id: Option<&ObjectId>,
        deep: bool,
        is_root_copy: bool,
        copies: &mut Vec<GraphObject>,
        now: chrono::DateTime<chrono::Utc>,
    ) -> Result<(), ObjectModelError> {
        let source = self
            .map
            .get(source_id)
            .ok_or_else(|| ObjectModelError::not_found(format!("Object '{source_id}' not found")))?
            .clone();
        let new_id = (self.id_gen)();
        let ref_pos = if is_root_copy {
            source.position + 1.0
        } else {
            source.position
        };

        let siblings = self.children_ordered(copy_parent_id);
        for sib in siblings.iter() {
            if sib.position >= ref_pos {
                let mut bumped = sib.clone();
                bumped.position += 1.0;
                self.map.insert(sib.id.as_str().to_string(), bumped);
            }
        }

        let mut copy = source.clone();
        copy.id = object_id(new_id.clone());
        copy.parent_id = copy_parent_id.cloned();
        copy.position = ref_pos;
        copy.created_at = now;
        copy.updated_at = now;
        self.map.insert(new_id.clone(), copy.clone());
        copies.push(copy);

        if deep {
            let child_ids: Vec<String> = self
                .children_ordered(Some(&object_id(source_id.to_string())))
                .into_iter()
                .map(|c| c.id.as_str().to_string())
                .collect();
            for child_id in child_ids {
                self.copy_subtree(
                    &child_id,
                    Some(&object_id(new_id.clone())),
                    deep,
                    false,
                    copies,
                    now,
                )?;
            }
        }
        Ok(())
    }

    pub fn update(
        &mut self,
        id: &str,
        patch: GraphObjectPatch,
    ) -> Result<GraphObject, ObjectModelError> {
        let previous = self
            .map
            .get(id)
            .ok_or_else(|| ObjectModelError::not_found(format!("Object '{id}' not found")))?
            .clone();

        if let Some(hook) = self.hooks.before_update.as_mut() {
            hook(&previous, &patch);
        }

        let mut updated = previous.clone();
        if let Some(v) = patch.name {
            updated.name = v;
        }
        if let Some(v) = patch.parent_id {
            updated.parent_id = v;
        }
        if let Some(v) = patch.position {
            updated.position = v;
        }
        if let Some(v) = patch.status {
            updated.status = v;
        }
        if let Some(v) = patch.tags {
            updated.tags = v;
        }
        if let Some(v) = patch.date {
            updated.date = v;
        }
        if let Some(v) = patch.end_date {
            updated.end_date = v;
        }
        if let Some(v) = patch.description {
            updated.description = v;
        }
        if let Some(v) = patch.color {
            updated.color = v;
        }
        if let Some(v) = patch.image {
            updated.image = v;
        }
        if let Some(v) = patch.pinned {
            updated.pinned = v;
        }
        if let Some(v) = patch.data {
            updated.data = v;
        }
        updated.updated_at = Utc::now();
        self.map.insert(id.to_string(), updated.clone());

        if let Some(hook) = self.hooks.after_update.as_mut() {
            hook(&updated, &previous);
        }
        self.emit(TreeModelEvent::Update {
            object: updated.clone(),
            previous: Box::new(previous),
        });
        self.emit(TreeModelEvent::Change);
        Ok(updated)
    }

    // ── Query ─────────────────────────────────────────────────────

    pub fn get(&self, id: &str) -> Option<&GraphObject> {
        self.map.get(id)
    }

    pub fn has(&self, id: &str) -> bool {
        self.map.contains_key(id)
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    pub fn get_children(&self, parent_id: Option<&ObjectId>) -> Vec<GraphObject> {
        self.children_ordered(parent_id)
    }

    pub fn get_descendants(&self, id: &str) -> Vec<GraphObject> {
        self.collect_descendants(id)
    }

    pub fn get_ancestors(&self, id: &str) -> Vec<GraphObject> {
        let mut ancestors = Vec::new();
        let mut current = self.map.get(id);
        while let Some(obj) = current {
            if let Some(parent_id) = obj.parent_id.as_ref() {
                if let Some(parent) = self.map.get(parent_id.as_str()) {
                    ancestors.push(parent.clone());
                    current = Some(parent);
                    continue;
                }
            }
            break;
        }
        ancestors
    }

    pub fn build_tree(&self) -> Vec<TreeNode> {
        let registry = ObjectRegistry::new();
        let objects: Vec<GraphObject> = self.map.values().cloned().collect();
        registry.build_tree(&objects)
    }

    pub fn to_vec(&self) -> Vec<GraphObject> {
        self.map.values().cloned().collect()
    }

    pub fn to_json(&self) -> Vec<GraphObject> {
        self.to_vec()
    }

    // ── Events ────────────────────────────────────────────────────

    pub fn on(&mut self, listener: TreeModelEventListener) {
        self.listeners.push(listener);
    }

    fn emit(&mut self, event: TreeModelEvent) {
        for listener in self.listeners.iter_mut() {
            listener(&event);
        }
    }

    // ── Internal ──────────────────────────────────────────────────

    fn children_ordered(&self, parent_id: Option<&ObjectId>) -> Vec<GraphObject> {
        let mut children: Vec<GraphObject> = self
            .map
            .values()
            .filter(|o| o.parent_id.as_ref() == parent_id)
            .cloned()
            .collect();
        children.sort_by(|a, b| {
            a.position
                .partial_cmp(&b.position)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        children
    }

    fn collect_descendants(&self, id: &str) -> Vec<GraphObject> {
        let mut result = Vec::new();
        let mut stack: Vec<String> = vec![id.to_string()];
        while let Some(current) = stack.pop() {
            for child in self.children_ordered(Some(&object_id(current.clone()))) {
                result.push(child.clone());
                stack.push(child.id.as_str().to_string());
            }
        }
        result
    }

    fn compact_positions(&mut self, parent_id: Option<&ObjectId>) {
        let mut children = self.children_ordered(parent_id);
        for (i, child) in children.iter_mut().enumerate() {
            if child.position != i as f64 {
                child.position = i as f64;
                self.map
                    .insert(child.id.as_str().to_string(), child.clone());
            }
        }
    }
}

pub struct RemovedBundle {
    pub removed: GraphObject,
    pub descendants: Vec<GraphObject>,
}

fn default_id_generator() -> String {
    uuid::Uuid::new_v4().to_string()
}

// Small no-alloc helpers to satisfy the compiler when asserting
// map-style equality in tests.
#[allow(dead_code)]
fn _assert_hashmap<K, V>(_: &HashMap<K, V>) {}

#[cfg(test)]
mod tests {
    use super::*;

    fn draft(t: &str, name: &str) -> GraphObjectDraft {
        GraphObjectDraft::new(t, name)
    }

    #[test]
    fn add_and_get_round_trip() {
        let mut tree = TreeModel::new();
        let added = tree
            .add(draft("task", "Buy milk"), AddOptions::default())
            .unwrap();
        assert_eq!(tree.len(), 1);
        assert_eq!(tree.get(added.id.as_str()).unwrap().name, "Buy milk");
    }

    #[test]
    fn add_assigns_monotonic_positions() {
        let mut tree = TreeModel::new();
        let a = tree.add(draft("task", "A"), AddOptions::default()).unwrap();
        let b = tree.add(draft("task", "B"), AddOptions::default()).unwrap();
        let c = tree.add(draft("task", "C"), AddOptions::default()).unwrap();
        assert_eq!(a.position, 0.0);
        assert_eq!(b.position, 1.0);
        assert_eq!(c.position, 2.0);
    }

    #[test]
    fn remove_collects_descendants() {
        let mut tree = TreeModel::new();
        let parent = tree
            .add(draft("project", "Proj"), AddOptions::default())
            .unwrap();
        tree.add(
            draft("task", "child-1"),
            AddOptions {
                parent_id: Some(parent.id.clone()),
                position: None,
            },
        )
        .unwrap();
        tree.add(
            draft("task", "child-2"),
            AddOptions {
                parent_id: Some(parent.id.clone()),
                position: None,
            },
        )
        .unwrap();
        let bundle = tree.remove(parent.id.as_str()).unwrap();
        assert_eq!(bundle.descendants.len(), 2);
        assert_eq!(tree.len(), 0);
    }

    #[test]
    fn move_rejects_cycles() {
        let mut tree = TreeModel::new();
        let parent = tree
            .add(draft("project", "Proj"), AddOptions::default())
            .unwrap();
        let child = tree
            .add(
                draft("project", "Child"),
                AddOptions {
                    parent_id: Some(parent.id.clone()),
                    position: None,
                },
            )
            .unwrap();
        let err = tree
            .move_to(parent.id.as_str(), Some(child.id.clone()), None)
            .unwrap_err();
        assert!(matches!(err.code, ObjectModelErrorCode::CircularRef));
    }

    #[test]
    fn reorder_renumbers_siblings() {
        let mut tree = TreeModel::new();
        let a = tree.add(draft("task", "A"), AddOptions::default()).unwrap();
        let b = tree.add(draft("task", "B"), AddOptions::default()).unwrap();
        let c = tree.add(draft("task", "C"), AddOptions::default()).unwrap();
        tree.reorder(c.id.as_str(), 0.0).unwrap();
        let ordered = tree.get_children(None);
        assert_eq!(ordered[0].id.as_str(), c.id.as_str());
        assert_eq!(ordered[1].id.as_str(), a.id.as_str());
        assert_eq!(ordered[2].id.as_str(), b.id.as_str());
    }
}
