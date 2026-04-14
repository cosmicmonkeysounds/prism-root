//! `EdgeModel` — stateful, event-driven in-memory store of
//! [`ObjectEdge`]s. Port of `foundation/object-model/edge-model.ts`.

// Lifecycle hooks are `Option<Box<dyn FnMut(...)>>` slots. Wrapping
// each of those in a fresh type alias would add noise without
// improving readability, and the entire shape only appears inside
// `EdgeModelHooks`. Allow the complexity locally.
#![allow(clippy::type_complexity)]

use chrono::Utc;
use indexmap::IndexMap;

use super::error::ObjectModelError;
use super::registry::ObjectRegistry;
// Re-export the edge type up to the module root alongside the model.
pub use super::types::ObjectEdge as Edge;
use super::types::{edge_id, ObjectEdge, ObjectId};

#[derive(Debug, Clone)]
pub enum EdgeModelEvent {
    Add {
        edge: ObjectEdge,
    },
    Remove {
        edge: ObjectEdge,
    },
    Update {
        edge: ObjectEdge,
        previous: ObjectEdge,
    },
    Change,
}

pub type EdgeModelEventListener = Box<dyn FnMut(&EdgeModelEvent)>;

#[derive(Default)]
pub struct EdgeModelHooks {
    pub before_add: Option<Box<dyn FnMut(&EdgeDraft)>>,
    pub after_add: Option<Box<dyn FnMut(&ObjectEdge)>>,
    pub before_remove: Option<Box<dyn FnMut(&ObjectEdge)>>,
    pub after_remove: Option<Box<dyn FnMut(&ObjectEdge)>>,
    pub before_update: Option<Box<dyn FnMut(&ObjectEdge, &EdgePatch)>>,
    pub after_update: Option<Box<dyn FnMut(&ObjectEdge, &ObjectEdge)>>,
}

#[derive(Debug, Clone)]
pub struct EdgeDraft {
    pub id: Option<String>,
    pub source_id: ObjectId,
    pub target_id: ObjectId,
    pub relation: String,
    pub position: Option<f64>,
    pub data: std::collections::BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Default)]
pub struct EdgePatch {
    pub source_id: Option<ObjectId>,
    pub target_id: Option<ObjectId>,
    pub relation: Option<String>,
    pub position: Option<Option<f64>>,
    pub data: Option<std::collections::BTreeMap<String, serde_json::Value>>,
}

pub struct EdgeModel {
    map: IndexMap<String, ObjectEdge>,
    listeners: Vec<EdgeModelEventListener>,
    hooks: EdgeModelHooks,
    registry: Option<ObjectRegistry>,
    id_gen: Box<dyn FnMut() -> String>,
}

#[derive(Default)]
pub struct EdgeModelOptions {
    pub registry: Option<ObjectRegistry>,
    pub edges: Vec<ObjectEdge>,
    pub hooks: EdgeModelHooks,
    pub id_gen: Option<Box<dyn FnMut() -> String>>,
}

impl Default for EdgeModel {
    fn default() -> Self {
        Self {
            map: IndexMap::new(),
            listeners: Vec::new(),
            hooks: EdgeModelHooks::default(),
            registry: None,
            id_gen: Box::new(default_id_generator),
        }
    }
}

impl EdgeModel {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_options(options: EdgeModelOptions) -> Self {
        let mut map = IndexMap::new();
        for e in options.edges {
            map.insert(e.id.as_str().to_string(), e);
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

    pub fn add(&mut self, draft: EdgeDraft) -> Result<ObjectEdge, ObjectModelError> {
        if draft.source_id.as_str().is_empty()
            || draft.target_id.as_str().is_empty()
            || draft.relation.is_empty()
        {
            return Err(ObjectModelError::not_found(
                "Invalid edge: sourceId, targetId, and relation are required".to_string(),
            ));
        }

        if let Some(reg) = self.registry.as_ref() {
            if let Some(edge_def) = reg.get_edge_type(&draft.relation) {
                if !edge_def.allow_multiple() {
                    let existing =
                        self.get_between(&draft.source_id, &draft.target_id, Some(&draft.relation));
                    if !existing.is_empty() {
                        return Err(ObjectModelError::containment_violation(format!(
                            "Relation '{}' does not allow multiple edges between the same objects",
                            draft.relation
                        )));
                    }
                    if edge_def.is_undirected() {
                        let reverse = self.get_between(
                            &draft.target_id,
                            &draft.source_id,
                            Some(&draft.relation),
                        );
                        if !reverse.is_empty() {
                            return Err(ObjectModelError::containment_violation(format!(
                                "Undirected relation '{}' already exists between these objects",
                                draft.relation
                            )));
                        }
                    }
                }
            }
        }

        if let Some(hook) = self.hooks.before_add.as_mut() {
            hook(&draft);
        }

        let id_string = draft.id.clone().unwrap_or_else(|| (self.id_gen)());
        let edge = ObjectEdge {
            id: edge_id(id_string.clone()),
            source_id: draft.source_id,
            target_id: draft.target_id,
            relation: draft.relation,
            position: draft.position,
            created_at: Utc::now(),
            data: draft.data,
        };

        self.map.insert(id_string, edge.clone());
        if let Some(hook) = self.hooks.after_add.as_mut() {
            hook(&edge);
        }
        self.emit(EdgeModelEvent::Add { edge: edge.clone() });
        self.emit(EdgeModelEvent::Change);
        Ok(edge)
    }

    pub fn remove(&mut self, id: &str) -> Option<ObjectEdge> {
        let edge = self.map.get(id)?.clone();
        if let Some(hook) = self.hooks.before_remove.as_mut() {
            hook(&edge);
        }
        self.map.shift_remove(id);
        if let Some(hook) = self.hooks.after_remove.as_mut() {
            hook(&edge);
        }
        self.emit(EdgeModelEvent::Remove { edge: edge.clone() });
        self.emit(EdgeModelEvent::Change);
        Some(edge)
    }

    pub fn update(&mut self, id: &str, patch: EdgePatch) -> Result<ObjectEdge, ObjectModelError> {
        let previous = self
            .map
            .get(id)
            .ok_or_else(|| ObjectModelError::not_found(format!("Edge '{id}' not found")))?
            .clone();
        if let Some(hook) = self.hooks.before_update.as_mut() {
            hook(&previous, &patch);
        }

        let mut updated = previous.clone();
        if let Some(v) = patch.source_id {
            updated.source_id = v;
        }
        if let Some(v) = patch.target_id {
            updated.target_id = v;
        }
        if let Some(v) = patch.relation {
            updated.relation = v;
        }
        if let Some(v) = patch.position {
            updated.position = v;
        }
        if let Some(v) = patch.data {
            updated.data = v;
        }
        self.map.insert(id.to_string(), updated.clone());

        if let Some(hook) = self.hooks.after_update.as_mut() {
            hook(&updated, &previous);
        }
        self.emit(EdgeModelEvent::Update {
            edge: updated.clone(),
            previous,
        });
        self.emit(EdgeModelEvent::Change);
        Ok(updated)
    }

    // ── Query ─────────────────────────────────────────────────────

    pub fn get(&self, id: &str) -> Option<&ObjectEdge> {
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

    pub fn all(&self) -> Vec<ObjectEdge> {
        self.map.values().cloned().collect()
    }

    pub fn get_from(&self, source_id: &ObjectId, relation: Option<&str>) -> Vec<ObjectEdge> {
        self.map
            .values()
            .filter(|e| {
                e.source_id == *source_id && relation.map(|r| e.relation == r).unwrap_or(true)
            })
            .cloned()
            .collect()
    }

    pub fn get_to(&self, target_id: &ObjectId, relation: Option<&str>) -> Vec<ObjectEdge> {
        self.map
            .values()
            .filter(|e| {
                e.target_id == *target_id && relation.map(|r| e.relation == r).unwrap_or(true)
            })
            .cloned()
            .collect()
    }

    pub fn get_between(
        &self,
        source_id: &ObjectId,
        target_id: &ObjectId,
        relation: Option<&str>,
    ) -> Vec<ObjectEdge> {
        self.map
            .values()
            .filter(|e| {
                e.source_id == *source_id
                    && e.target_id == *target_id
                    && relation.map(|r| e.relation == r).unwrap_or(true)
            })
            .cloned()
            .collect()
    }

    pub fn get_connected(&self, object_id: &ObjectId, relation: Option<&str>) -> Vec<ObjectEdge> {
        self.map
            .values()
            .filter(|e| {
                (e.source_id == *object_id || e.target_id == *object_id)
                    && relation.map(|r| e.relation == r).unwrap_or(true)
            })
            .cloned()
            .collect()
    }

    pub fn to_json(&self) -> Vec<ObjectEdge> {
        self.all()
    }

    // ── Events ────────────────────────────────────────────────────

    pub fn on(&mut self, listener: EdgeModelEventListener) {
        self.listeners.push(listener);
    }

    fn emit(&mut self, event: EdgeModelEvent) {
        for listener in self.listeners.iter_mut() {
            listener(&event);
        }
    }
}

fn default_id_generator() -> String {
    uuid::Uuid::new_v4().to_string()
}

// ── Tests ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::foundation::object_model::types::object_id;

    fn draft_between(source: &str, target: &str, relation: &str) -> EdgeDraft {
        EdgeDraft {
            id: None,
            source_id: object_id(source),
            target_id: object_id(target),
            relation: relation.to_string(),
            position: None,
            data: Default::default(),
        }
    }

    #[test]
    fn add_and_get_from() {
        let mut edges = EdgeModel::new();
        edges.add(draft_between("a", "b", "blocks")).unwrap();
        let from = edges.get_from(&object_id("a"), None);
        assert_eq!(from.len(), 1);
        assert_eq!(from[0].target_id.as_str(), "b");
    }

    #[test]
    fn remove_removes() {
        let mut edges = EdgeModel::new();
        let e = edges.add(draft_between("a", "b", "blocks")).unwrap();
        let _ = edges.remove(e.id.as_str());
        assert_eq!(edges.len(), 0);
    }

    #[test]
    fn rejects_invalid_drafts() {
        let mut edges = EdgeModel::new();
        let err = edges
            .add(EdgeDraft {
                id: None,
                source_id: object_id(""),
                target_id: object_id(""),
                relation: "".into(),
                position: None,
                data: Default::default(),
            })
            .unwrap_err();
        assert!(err.message.contains("Invalid edge"));
    }
}
