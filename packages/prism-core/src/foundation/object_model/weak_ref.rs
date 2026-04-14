//! Weak-reference system — content-derived cross-object edges.
//! Port of `foundation/object-model/weak-ref.ts`.
//!
//! Lenses register a [`WeakRefProvider`] that knows how to extract
//! foreign references from a [`GraphObject`]. The engine materialises
//! those references as regular [`ObjectEdge`]s tagged with
//! `__weakRef: true` in their `data` payload. Weak-ref edges are
//! engine-owned: if the source content changes, the edges churn.
//!
//! Auto-subscription to a [`TreeModel`]'s event stream (as the
//! legacy TS engine did) is not part of the Rust port — the shell
//! that owns both models is expected to call [`WeakRefEngine::recompute`]
//! / [`WeakRefEngine::remove_for`] from its own event pipeline.
//! Rust's borrow rules make a self-subscribing engine awkward
//! without interior mutability, and the manual-drive API is what
//! every current consumer needs anyway.

// Listener slots are `Vec<Box<dyn FnMut(...)>>`; a fresh type alias
// wouldn't improve readability, so allow the complexity locally.
#![allow(clippy::type_complexity)]

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::edge_model::EdgeModel;
use super::registry::{TreeNode, WeakRefChildNode};
use super::tree_model::TreeModel;
use super::types::{GraphObject, ObjectEdge, ObjectId};

// ── Types ──────────────────────────────────────────────────────────

/// A single reference extracted from a source object's content.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WeakRefExtraction {
    #[serde(rename = "targetId")]
    pub target_id: String,
    pub relation: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub location: Option<WeakRefLocation>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub scope: Option<WeakRefScope>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct WeakRefLocation {
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub field: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub offset: Option<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WeakRefScope {
    Local,
    Federated,
}

/// Declarative extractor registered by a lens. Implementors must be
/// fast + synchronous (no I/O, no network).
pub trait WeakRefProvider {
    fn id(&self) -> &str;
    fn label(&self) -> Option<&str> {
        None
    }
    /// Empty list = "match any source type".
    fn source_types(&self) -> &[String];
    fn extract_refs(&self, object: &GraphObject) -> Vec<WeakRefExtraction>;
}

/// A weak-ref child as seen from the **target** object's side.
#[derive(Debug, Clone, PartialEq)]
pub struct WeakRefChild {
    pub object: GraphObject,
    pub relation: String,
    pub edge_id: String,
    pub provider_id: String,
    pub provider_label: String,
    pub location: Option<WeakRefLocation>,
}

// ── Edge tagging helpers ───────────────────────────────────────────

const WEAK_REF_TAG: &str = "__weakRef";

fn is_weak_ref_edge(edge: &ObjectEdge) -> bool {
    edge.data
        .get(WEAK_REF_TAG)
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

fn weak_ref_data(
    provider_id: &str,
    provider_label: &str,
    location: Option<&WeakRefLocation>,
    federated: bool,
) -> std::collections::BTreeMap<String, serde_json::Value> {
    let mut data = std::collections::BTreeMap::new();
    data.insert(WEAK_REF_TAG.into(), serde_json::Value::Bool(true));
    data.insert(
        "providerId".into(),
        serde_json::Value::String(provider_id.to_string()),
    );
    data.insert(
        "providerLabel".into(),
        serde_json::Value::String(provider_label.to_string()),
    );
    if let Some(loc) = location {
        if let Ok(v) = serde_json::to_value(loc) {
            data.insert("location".into(), v);
        }
    }
    if federated {
        data.insert("federated".into(), serde_json::Value::Bool(true));
    }
    data
}

fn get_provider_id(edge: &ObjectEdge) -> Option<&str> {
    edge.data.get("providerId").and_then(|v| v.as_str())
}

fn get_provider_label(edge: &ObjectEdge) -> Option<&str> {
    edge.data.get("providerLabel").and_then(|v| v.as_str())
}

fn get_location(edge: &ObjectEdge) -> Option<WeakRefLocation> {
    edge.data
        .get("location")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
}

// ── Events ─────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum WeakRefEngineEvent {
    Recomputed {
        object_id: ObjectId,
        added: usize,
        removed: usize,
    },
    Rebuilt {
        total_edges: usize,
    },
    Change,
}

// ── Engine ─────────────────────────────────────────────────────────

pub struct WeakRefEngine {
    providers: HashMap<String, Box<dyn WeakRefProvider>>,
    listeners: Vec<Box<dyn FnMut(&WeakRefEngineEvent)>>,
}

impl Default for WeakRefEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl WeakRefEngine {
    pub fn new() -> Self {
        Self {
            providers: HashMap::new(),
            listeners: Vec::new(),
        }
    }

    // ── Provider registration ────────────────────────────────────

    pub fn register_provider(&mut self, provider: Box<dyn WeakRefProvider>) {
        self.providers.insert(provider.id().to_string(), provider);
    }

    pub fn unregister_provider(&mut self, id: &str, edges: &mut EdgeModel) {
        if self.providers.remove(id).is_none() {
            return;
        }
        let doomed: Vec<String> = edges
            .all()
            .into_iter()
            .filter(|e| is_weak_ref_edge(e) && get_provider_id(e) == Some(id))
            .map(|e| e.id.as_str().to_string())
            .collect();
        for eid in doomed {
            edges.remove(&eid);
        }
    }

    pub fn has_provider(&self, id: &str) -> bool {
        self.providers.contains_key(id)
    }

    pub fn provider_ids(&self) -> Vec<&str> {
        self.providers.keys().map(String::as_str).collect()
    }

    // ── Core operations ──────────────────────────────────────────

    pub fn recompute(&mut self, object_id: &ObjectId, tree: &TreeModel, edges: &mut EdgeModel) {
        let Some(obj) = tree.get(object_id.as_str()).cloned() else {
            return;
        };

        struct Desired {
            extraction: WeakRefExtraction,
            provider_id: String,
            provider_label: String,
        }

        let mut desired: Vec<Desired> = Vec::new();
        for provider in self.providers.values() {
            if !matches_provider(provider.as_ref(), &obj) {
                continue;
            }
            for ext in provider.extract_refs(&obj) {
                desired.push(Desired {
                    extraction: ext,
                    provider_id: provider.id().to_string(),
                    provider_label: provider
                        .label()
                        .unwrap_or_else(|| provider.id())
                        .to_string(),
                });
            }
        }

        let existing: Vec<ObjectEdge> = edges
            .get_from(object_id, None)
            .into_iter()
            .filter(is_weak_ref_edge)
            .collect();

        let key = |target: &str, relation: &str, provider: &str| {
            format!("{target}::{relation}::{provider}")
        };

        let mut desired_keys: HashMap<String, usize> = HashMap::new();
        for (idx, d) in desired.iter().enumerate() {
            desired_keys.insert(
                key(
                    &d.extraction.target_id,
                    &d.extraction.relation,
                    &d.provider_id,
                ),
                idx,
            );
        }

        let mut existing_keys: HashMap<String, String> = HashMap::new();
        for edge in &existing {
            let provider = get_provider_id(edge).unwrap_or("").to_string();
            existing_keys.insert(
                key(edge.target_id.as_str(), &edge.relation, &provider),
                edge.id.as_str().to_string(),
            );
        }

        let mut added = 0usize;
        let mut removed = 0usize;

        for (k, edge_id) in &existing_keys {
            if !desired_keys.contains_key(k) {
                edges.remove(edge_id);
                removed += 1;
            }
        }

        for (k, idx) in &desired_keys {
            if existing_keys.contains_key(k) {
                continue;
            }
            let d = &desired[*idx];
            let federated = d.extraction.scope == Some(WeakRefScope::Federated);
            if !federated && !tree.has(&d.extraction.target_id) {
                continue;
            }
            let data = weak_ref_data(
                &d.provider_id,
                &d.provider_label,
                d.extraction.location.as_ref(),
                federated,
            );
            let draft = super::edge_model::EdgeDraft {
                id: None,
                source_id: obj.id.clone(),
                target_id: ObjectId::new(d.extraction.target_id.clone()),
                relation: d.extraction.relation.clone(),
                position: None,
                data,
            };
            if edges.add(draft).is_ok() {
                added += 1;
            }
        }

        if added > 0 || removed > 0 {
            self.emit(WeakRefEngineEvent::Recomputed {
                object_id: object_id.clone(),
                added,
                removed,
            });
            self.emit(WeakRefEngineEvent::Change);
        }
    }

    pub fn rebuild_all(&mut self, tree: &TreeModel, edges: &mut EdgeModel) {
        let doomed: Vec<String> = edges
            .all()
            .into_iter()
            .filter(is_weak_ref_edge)
            .map(|e| e.id.as_str().to_string())
            .collect();
        for id in doomed {
            edges.remove(&id);
        }

        let ids: Vec<ObjectId> = tree.to_vec().into_iter().map(|o| o.id).collect();
        for id in &ids {
            self.recompute(id, tree, edges);
        }

        let total = edges.all().into_iter().filter(is_weak_ref_edge).count();
        self.emit(WeakRefEngineEvent::Rebuilt { total_edges: total });
        self.emit(WeakRefEngineEvent::Change);
    }

    pub fn remove_for(&mut self, object_id: &ObjectId, edges: &mut EdgeModel) {
        let doomed: Vec<String> = edges
            .get_from(object_id, None)
            .into_iter()
            .chain(edges.get_to(object_id, None))
            .filter(is_weak_ref_edge)
            .map(|e| e.id.as_str().to_string())
            .collect();
        for id in doomed {
            edges.remove(&id);
        }
    }

    // ── Queries ──────────────────────────────────────────────────

    pub fn get_weak_ref_children(
        &self,
        target_id: &ObjectId,
        tree: &TreeModel,
        edges: &EdgeModel,
    ) -> Vec<WeakRefChild> {
        edges
            .get_to(target_id, None)
            .into_iter()
            .filter(is_weak_ref_edge)
            .filter_map(|edge| {
                let source = tree.get(edge.source_id.as_str())?.clone();
                Some(WeakRefChild {
                    object: source,
                    relation: edge.relation.clone(),
                    edge_id: edge.id.as_str().to_string(),
                    provider_id: get_provider_id(&edge).unwrap_or("").to_string(),
                    provider_label: get_provider_label(&edge).unwrap_or("").to_string(),
                    location: get_location(&edge),
                })
            })
            .collect()
    }

    pub fn get_weak_ref_parents(
        &self,
        source_id: &ObjectId,
        tree: &TreeModel,
        edges: &EdgeModel,
    ) -> Vec<WeakRefChild> {
        edges
            .get_from(source_id, None)
            .into_iter()
            .filter(is_weak_ref_edge)
            .filter_map(|edge| {
                let target = tree.get(edge.target_id.as_str())?.clone();
                Some(WeakRefChild {
                    object: target,
                    relation: edge.relation.clone(),
                    edge_id: edge.id.as_str().to_string(),
                    provider_id: get_provider_id(&edge).unwrap_or("").to_string(),
                    provider_label: get_provider_label(&edge).unwrap_or("").to_string(),
                    location: get_location(&edge),
                })
            })
            .collect()
    }

    pub fn augment_tree(&self, roots: &mut [TreeNode], tree: &TreeModel, edges: &EdgeModel) {
        walk(roots, self, tree, edges);

        fn walk(
            nodes: &mut [TreeNode],
            engine: &WeakRefEngine,
            tree: &TreeModel,
            edges: &EdgeModel,
        ) {
            for node in nodes.iter_mut() {
                let children = engine.get_weak_ref_children(&node.object.id, tree, edges);
                if !children.is_empty() {
                    node.weak_ref_children = Some(
                        children
                            .into_iter()
                            .map(|c| WeakRefChildNode {
                                object: c.object,
                                relation: c.relation,
                                edge_id: c.edge_id,
                                provider_id: c.provider_id,
                                provider_label: c.provider_label,
                            })
                            .collect(),
                    );
                }
                walk(&mut node.children, engine, tree, edges);
            }
        }
    }

    pub fn is_weak_ref_edge(&self, edge: &ObjectEdge) -> bool {
        is_weak_ref_edge(edge)
    }

    pub fn weak_ref_count(&self, edges: &EdgeModel) -> usize {
        edges.all().into_iter().filter(is_weak_ref_edge).count()
    }

    // ── Events ───────────────────────────────────────────────────

    pub fn on(&mut self, listener: Box<dyn FnMut(&WeakRefEngineEvent)>) {
        self.listeners.push(listener);
    }

    fn emit(&mut self, event: WeakRefEngineEvent) {
        for listener in self.listeners.iter_mut() {
            listener(&event);
        }
    }
}

fn matches_provider(provider: &dyn WeakRefProvider, obj: &GraphObject) -> bool {
    let types = provider.source_types();
    types.is_empty() || types.iter().any(|t| t == &obj.type_name)
}

// ── Tests ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::foundation::object_model::tree_model::{GraphObjectDraft, TreeModel};

    struct NameLinkProvider;

    impl WeakRefProvider for NameLinkProvider {
        fn id(&self) -> &str {
            "name-link"
        }
        fn label(&self) -> Option<&str> {
            Some("Name Link")
        }
        fn source_types(&self) -> &[String] {
            &[]
        }
        fn extract_refs(&self, object: &GraphObject) -> Vec<WeakRefExtraction> {
            // Very dumb extractor: the description holds target ids
            // separated by commas.
            if object.description.is_empty() {
                return Vec::new();
            }
            object
                .description
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .map(|target| WeakRefExtraction {
                    target_id: target,
                    relation: "mentions".into(),
                    location: None,
                    scope: None,
                })
                .collect()
        }
    }

    fn add(tree: &mut TreeModel, id: &str, desc: &str) -> ObjectId {
        let draft = GraphObjectDraft {
            id: Some(id.into()),
            type_name: "note".into(),
            name: id.into(),
            status: None,
            tags: None,
            date: None,
            end_date: None,
            description: if desc.is_empty() {
                None
            } else {
                Some(desc.into())
            },
            color: None,
            image: None,
            pinned: None,
            data: None,
        };
        let obj = tree.add(draft, Default::default()).unwrap();
        obj.id
    }

    #[test]
    fn recompute_creates_weak_edges_for_existing_targets() {
        let mut tree = TreeModel::new();
        let mut edges = EdgeModel::new();
        let mut engine = WeakRefEngine::new();
        engine.register_provider(Box::new(NameLinkProvider));

        let a = add(&mut tree, "a", "");
        let _b = add(&mut tree, "b", "");
        let src = add(&mut tree, "src", "a, b");

        engine.recompute(&src, &tree, &mut edges);
        assert_eq!(engine.weak_ref_count(&edges), 2);

        // Updating the description to drop `b` should churn the edge.
        tree.update(
            src.as_str(),
            crate::foundation::object_model::tree_model::GraphObjectPatch {
                description: Some("a".into()),
                ..Default::default()
            },
        )
        .unwrap();
        engine.recompute(&src, &tree, &mut edges);
        assert_eq!(engine.weak_ref_count(&edges), 1);

        // The single remaining edge should point at `a`.
        let children = engine.get_weak_ref_parents(&src, &tree, &edges);
        assert_eq!(children.len(), 1);
        assert_eq!(children[0].object.id, a);
    }

    #[test]
    fn remove_for_drops_inbound_and_outbound() {
        let mut tree = TreeModel::new();
        let mut edges = EdgeModel::new();
        let mut engine = WeakRefEngine::new();
        engine.register_provider(Box::new(NameLinkProvider));

        let _a = add(&mut tree, "a", "");
        let src = add(&mut tree, "src", "a");
        engine.recompute(&src, &tree, &mut edges);
        assert_eq!(engine.weak_ref_count(&edges), 1);

        engine.remove_for(&src, &mut edges);
        assert_eq!(engine.weak_ref_count(&edges), 0);
    }

    #[test]
    fn unregister_provider_drops_owned_edges() {
        let mut tree = TreeModel::new();
        let mut edges = EdgeModel::new();
        let mut engine = WeakRefEngine::new();
        engine.register_provider(Box::new(NameLinkProvider));

        let _a = add(&mut tree, "a", "");
        let src = add(&mut tree, "src", "a");
        engine.recompute(&src, &tree, &mut edges);
        assert_eq!(engine.weak_ref_count(&edges), 1);

        engine.unregister_provider("name-link", &mut edges);
        assert_eq!(engine.weak_ref_count(&edges), 0);
        assert!(!engine.has_provider("name-link"));
    }
}
