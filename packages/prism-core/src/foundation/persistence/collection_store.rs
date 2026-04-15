//! `collection_store` — Loro-backed storage for graph objects and
//! edges.
//!
//! Each [`CollectionStore`] wraps one `LoroDoc` holding two top-level
//! maps (`objects` and `edges`). Records are stored as JSON strings
//! inside the Loro maps, matching the TS port byte-for-byte so
//! snapshots round-trip freely between the JS and Rust runtimes
//! during the migration.
//!
//! The store is the persistence-side counterpart to `TreeModel` /
//! `EdgeModel` (which are in-memory projection caches): the store is
//! the durable truth, the models are ephemeral views that can be
//! rebuilt from it.

use std::collections::HashMap;

use loro::{ExportMode, LoroDoc, LoroError, LoroValue};

use crate::foundation::object_model::{EdgeId, GraphObject, ObjectEdge, ObjectId};

/// Errors raised by [`CollectionStore`] and [`super::VaultManager`].
#[derive(Debug, thiserror::Error)]
pub enum PersistenceError {
    #[error("unknown collection: {0}")]
    UnknownCollection(String),
    #[error("loro error: {0}")]
    Loro(String),
    #[error("serde error: {0}")]
    Json(#[from] serde_json::Error),
}

impl From<LoroError> for PersistenceError {
    fn from(err: LoroError) -> Self {
        PersistenceError::Loro(err.to_string())
    }
}

impl From<loro::LoroEncodeError> for PersistenceError {
    fn from(err: loro::LoroEncodeError) -> Self {
        PersistenceError::Loro(err.to_string())
    }
}

/// Kind of mutation that produced a [`CollectionChange`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CollectionChangeKind {
    ObjectPut,
    ObjectRemove,
    EdgePut,
    EdgeRemove,
}

/// A single change notification. One of these fires per mutated
/// record; batched mutations emit one change each. Mirrors the TS
/// `CollectionChange` shape.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CollectionChange {
    pub kind: CollectionChangeKind,
    pub id: String,
}

/// Filter passed to [`CollectionStore::list_objects`].
///
/// Defaults (`ObjectFilter::default()`) return every object including
/// deleted ones — same as calling `list_objects(None)`. Use
/// [`ObjectFilter::exclude_deleted`] to match the TS default behaviour
/// when at least one filter field is set.
#[derive(Debug, Clone, Default)]
pub struct ObjectFilter {
    pub types: Option<Vec<String>>,
    pub tags: Option<Vec<String>>,
    pub statuses: Option<Vec<String>>,
    pub parent_id: ParentIdFilter,
    pub exclude_deleted: bool,
}

/// `parent_id` filter trichotomy: ignore the field, match a specific
/// id, or match the root (`null`) parent.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum ParentIdFilter {
    #[default]
    Any,
    Some(ObjectId),
    Root,
}

/// Filter passed to [`CollectionStore::list_edges`].
#[derive(Debug, Clone, Default)]
pub struct EdgeFilter {
    pub source_id: Option<ObjectId>,
    pub target_id: Option<ObjectId>,
    pub relation: Option<String>,
}

/// Options passed to [`CollectionStore::new`].
#[derive(Debug, Clone, Default)]
pub struct CollectionStoreOptions {
    /// Loro peer ID for multi-peer CRDT sync. `None` lets Loro pick
    /// a random id (its default).
    pub peer_id: Option<u64>,
}

/// Handle returned by [`CollectionStore::on_change`]. Feed back to
/// [`CollectionStore::off_change`] to unsubscribe.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Subscription(u64);

impl Subscription {
    pub fn raw(&self) -> u64 {
        self.0
    }
}

type ChangeListener = Box<dyn FnMut(&[CollectionChange])>;

const OBJECTS_MAP: &str = "objects";
const EDGES_MAP: &str = "edges";

/// Loro-backed storage for graph objects and edges. One per
/// logical "collection" in the vault — a tasks store, a contacts
/// store, etc.
pub struct CollectionStore {
    doc: LoroDoc,
    listeners: Vec<(u64, ChangeListener)>,
    next_listener_id: u64,
    dirty: bool,
}

impl CollectionStore {
    /// Create a fresh empty store.
    pub fn new() -> Self {
        Self::with_options(CollectionStoreOptions::default())
    }

    /// Create a fresh store with options (currently just the optional
    /// peer id).
    pub fn with_options(options: CollectionStoreOptions) -> Self {
        let doc = LoroDoc::new();
        if let Some(peer_id) = options.peer_id {
            // Setting a peer id on a fresh doc only fails if the doc
            // already has changes — impossible here. We still surface
            // the error as a panic because it's a programming bug if
            // it ever triggers.
            doc.set_peer_id(peer_id).expect("fresh doc accepts peer id");
        }
        Self {
            doc,
            listeners: Vec::new(),
            next_listener_id: 0,
            dirty: false,
        }
    }

    /// Borrow the underlying `LoroDoc`. Useful for hosts that need to
    /// drive Loro directly (e.g. to hook into its own subscription
    /// path, or to run peer-sync machinery that isn't exposed here
    /// yet).
    pub fn doc(&self) -> &LoroDoc {
        &self.doc
    }

    // ── Object CRUD ───────────────────────────────────────────────

    /// Insert or replace an object. Commits the Loro transaction and
    /// marks the store dirty.
    pub fn put_object(&mut self, obj: &GraphObject) -> Result<(), PersistenceError> {
        let json = serde_json::to_string(obj)?;
        let map = self.doc.get_map(OBJECTS_MAP);
        map.insert(obj.id.0.as_str(), LoroValue::from(json))?;
        self.doc.commit();
        let change = CollectionChange {
            kind: CollectionChangeKind::ObjectPut,
            id: obj.id.0.clone(),
        };
        self.mark_dirty_and_notify(&[change]);
        Ok(())
    }

    /// Retrieve an object by id, or `None` if absent.
    pub fn get_object(&self, id: &ObjectId) -> Option<GraphObject> {
        let map = self.doc.get_map(OBJECTS_MAP);
        let raw = read_string(&map, &id.0)?;
        serde_json::from_str(&raw).ok()
    }

    /// Remove an object by id. Returns `true` if the object existed.
    pub fn remove_object(&mut self, id: &ObjectId) -> Result<bool, PersistenceError> {
        let map = self.doc.get_map(OBJECTS_MAP);
        if read_string(&map, &id.0).is_none() {
            return Ok(false);
        }
        map.delete(&id.0)?;
        self.doc.commit();
        let change = CollectionChange {
            kind: CollectionChangeKind::ObjectRemove,
            id: id.0.clone(),
        };
        self.mark_dirty_and_notify(&[change]);
        Ok(true)
    }

    /// Return every stored object, regardless of deleted state.
    pub fn all_objects(&self) -> Vec<GraphObject> {
        let map = self.doc.get_map(OBJECTS_MAP);
        let mut out = Vec::with_capacity(map.len());
        map.for_each(|_, value| {
            if let Some(raw) = string_from_value(value) {
                if let Ok(obj) = serde_json::from_str::<GraphObject>(&raw) {
                    out.push(obj);
                }
            }
        });
        out
    }

    /// List objects, optionally filtered. Passing `None` returns
    /// every object (including deleted ones). Passing a filter with
    /// `exclude_deleted = true` drops tombstoned objects — the TS
    /// port defaulted to that behaviour whenever any filter field was
    /// specified; Rust callers spell it out explicitly for clarity.
    pub fn list_objects(&self, filter: Option<&ObjectFilter>) -> Vec<GraphObject> {
        let mut objects = self.all_objects();

        let Some(filter) = filter else {
            return objects;
        };

        if filter.exclude_deleted {
            objects.retain(|o| o.deleted_at.is_none());
        }
        if let Some(types) = &filter.types {
            if !types.is_empty() {
                objects.retain(|o| types.iter().any(|t| t == &o.type_name));
            }
        }
        if let Some(tags) = &filter.tags {
            if !tags.is_empty() {
                objects.retain(|o| tags.iter().all(|t| o.tags.contains(t)));
            }
        }
        if let Some(statuses) = &filter.statuses {
            if !statuses.is_empty() {
                objects.retain(|o| {
                    o.status
                        .as_ref()
                        .is_some_and(|s| statuses.iter().any(|f| f == s))
                });
            }
        }
        match &filter.parent_id {
            ParentIdFilter::Any => {}
            ParentIdFilter::Some(parent) => {
                objects.retain(|o| o.parent_id.as_ref() == Some(parent));
            }
            ParentIdFilter::Root => {
                objects.retain(|o| o.parent_id.is_none());
            }
        }

        objects
    }

    /// Number of stored objects, including tombstones.
    pub fn object_count(&self) -> usize {
        self.doc.get_map(OBJECTS_MAP).len()
    }

    // ── Edge CRUD ─────────────────────────────────────────────────

    pub fn put_edge(&mut self, edge: &ObjectEdge) -> Result<(), PersistenceError> {
        let json = serde_json::to_string(edge)?;
        let map = self.doc.get_map(EDGES_MAP);
        map.insert(edge.id.0.as_str(), LoroValue::from(json))?;
        self.doc.commit();
        let change = CollectionChange {
            kind: CollectionChangeKind::EdgePut,
            id: edge.id.0.clone(),
        };
        self.mark_dirty_and_notify(&[change]);
        Ok(())
    }

    pub fn get_edge(&self, id: &EdgeId) -> Option<ObjectEdge> {
        let map = self.doc.get_map(EDGES_MAP);
        let raw = read_string(&map, &id.0)?;
        serde_json::from_str(&raw).ok()
    }

    pub fn remove_edge(&mut self, id: &EdgeId) -> Result<bool, PersistenceError> {
        let map = self.doc.get_map(EDGES_MAP);
        if read_string(&map, &id.0).is_none() {
            return Ok(false);
        }
        map.delete(&id.0)?;
        self.doc.commit();
        let change = CollectionChange {
            kind: CollectionChangeKind::EdgeRemove,
            id: id.0.clone(),
        };
        self.mark_dirty_and_notify(&[change]);
        Ok(true)
    }

    pub fn all_edges(&self) -> Vec<ObjectEdge> {
        let map = self.doc.get_map(EDGES_MAP);
        let mut out = Vec::with_capacity(map.len());
        map.for_each(|_, value| {
            if let Some(raw) = string_from_value(value) {
                if let Ok(edge) = serde_json::from_str::<ObjectEdge>(&raw) {
                    out.push(edge);
                }
            }
        });
        out
    }

    pub fn list_edges(&self, filter: Option<&EdgeFilter>) -> Vec<ObjectEdge> {
        let mut edges = self.all_edges();
        let Some(filter) = filter else {
            return edges;
        };
        if let Some(source_id) = &filter.source_id {
            edges.retain(|e| &e.source_id == source_id);
        }
        if let Some(target_id) = &filter.target_id {
            edges.retain(|e| &e.target_id == target_id);
        }
        if let Some(relation) = &filter.relation {
            edges.retain(|e| &e.relation == relation);
        }
        edges
    }

    pub fn edge_count(&self) -> usize {
        self.doc.get_map(EDGES_MAP).len()
    }

    // ── Snapshot / sync ───────────────────────────────────────────

    /// Export the full document state as a binary Loro snapshot.
    pub fn export_snapshot(&self) -> Result<Vec<u8>, PersistenceError> {
        Ok(self.doc.export(ExportMode::Snapshot)?)
    }

    /// Import a snapshot or update blob from another peer. After an
    /// import the store is considered clean — incoming updates are
    /// already on disk somewhere.
    pub fn import(&mut self, data: &[u8]) -> Result<(), PersistenceError> {
        self.doc.import(data)?;
        Ok(())
    }

    // ── Bulk / debug ──────────────────────────────────────────────

    /// Dump the store as a pair of `id -> record` maps. Primarily
    /// useful for tests and diagnostics.
    pub fn to_json(&self) -> (HashMap<String, GraphObject>, HashMap<String, ObjectEdge>) {
        let mut objects = HashMap::new();
        let mut edges = HashMap::new();
        self.doc.get_map(OBJECTS_MAP).for_each(|key, value| {
            if let Some(raw) = string_from_value(value) {
                if let Ok(obj) = serde_json::from_str::<GraphObject>(&raw) {
                    objects.insert(key.to_string(), obj);
                }
            }
        });
        self.doc.get_map(EDGES_MAP).for_each(|key, value| {
            if let Some(raw) = string_from_value(value) {
                if let Ok(edge) = serde_json::from_str::<ObjectEdge>(&raw) {
                    edges.insert(key.to_string(), edge);
                }
            }
        });
        (objects, edges)
    }

    // ── Dirty tracking ────────────────────────────────────────────

    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    pub fn clear_dirty(&mut self) {
        self.dirty = false;
    }

    // ── Change subscription ───────────────────────────────────────

    /// Register a change listener. Fires synchronously after every
    /// successful mutation via [`put_object`], [`remove_object`],
    /// [`put_edge`], or [`remove_edge`]. Does not fire for imports.
    pub fn on_change<F>(&mut self, listener: F) -> Subscription
    where
        F: FnMut(&[CollectionChange]) + 'static,
    {
        let id = self.next_listener_id;
        self.next_listener_id += 1;
        self.listeners.push((id, Box::new(listener)));
        Subscription(id)
    }

    pub fn off_change(&mut self, sub: Subscription) {
        self.listeners.retain(|(id, _)| *id != sub.0);
    }

    pub fn listener_count(&self) -> usize {
        self.listeners.len()
    }

    fn mark_dirty_and_notify(&mut self, changes: &[CollectionChange]) {
        self.dirty = true;
        for (_, listener) in self.listeners.iter_mut() {
            listener(changes);
        }
    }
}

impl Default for CollectionStore {
    fn default() -> Self {
        Self::new()
    }
}

// ── Loro value helpers ────────────────────────────────────────────

fn read_string(map: &loro::LoroMap, key: &str) -> Option<String> {
    let value = map.get(key)?;
    let value = value.into_value().ok()?;
    string_from_loro_value(&value)
}

fn string_from_value(value: loro::ValueOrContainer) -> Option<String> {
    string_from_loro_value(&value.into_value().ok()?)
}

fn string_from_loro_value(value: &LoroValue) -> Option<String> {
    match value {
        LoroValue::String(s) => Some(s.to_string()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::foundation::object_model::{edge_id, object_id};
    use chrono::{DateTime, Utc};
    use std::collections::BTreeMap;

    fn ts(s: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(s)
            .expect("valid rfc3339 timestamp")
            .with_timezone(&Utc)
    }

    fn make_object(id: &str) -> GraphObject {
        GraphObject {
            id: object_id(id),
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
            data: BTreeMap::new(),
            created_at: ts("2026-01-01T00:00:00Z"),
            updated_at: ts("2026-01-01T00:00:00Z"),
            deleted_at: None,
        }
    }

    fn make_edge(id: &str, source: &str, target: &str) -> ObjectEdge {
        ObjectEdge {
            id: edge_id(id),
            source_id: object_id(source),
            target_id: object_id(target),
            relation: "depends-on".into(),
            position: None,
            created_at: ts("2026-01-01T00:00:00Z"),
            data: BTreeMap::new(),
        }
    }

    // ── Object CRUD ───────────────────────────────────────────────

    #[test]
    fn puts_and_gets_an_object() {
        let mut store = CollectionStore::new();
        store.put_object(&make_object("obj-1")).unwrap();

        let retrieved = store.get_object(&object_id("obj-1")).unwrap();
        assert_eq!(retrieved.name, "Test Task");
        assert_eq!(retrieved.type_name, "task");
    }

    #[test]
    fn returns_none_for_missing_object() {
        let store = CollectionStore::new();
        assert!(store.get_object(&object_id("nope")).is_none());
    }

    #[test]
    fn overwrites_an_existing_object_on_put() {
        let mut store = CollectionStore::new();
        let mut obj = make_object("obj-1");
        obj.name = "Original".into();
        store.put_object(&obj).unwrap();

        obj.name = "Updated".into();
        store.put_object(&obj).unwrap();

        let retrieved = store.get_object(&object_id("obj-1")).unwrap();
        assert_eq!(retrieved.name, "Updated");
        assert_eq!(store.object_count(), 1);
    }

    #[test]
    fn removes_an_object() {
        let mut store = CollectionStore::new();
        store.put_object(&make_object("obj-1")).unwrap();
        assert!(store.remove_object(&object_id("obj-1")).unwrap());
        assert!(store.get_object(&object_id("obj-1")).is_none());
        assert_eq!(store.object_count(), 0);
    }

    #[test]
    fn remove_returns_false_for_missing_object() {
        let mut store = CollectionStore::new();
        assert!(!store.remove_object(&object_id("nope")).unwrap());
    }

    #[test]
    fn counts_objects_correctly() {
        let mut store = CollectionStore::new();
        assert_eq!(store.object_count(), 0);
        store.put_object(&make_object("a")).unwrap();
        store.put_object(&make_object("b")).unwrap();
        assert_eq!(store.object_count(), 2);
    }

    #[test]
    fn preserves_full_object_shape_through_round_trip() {
        let mut store = CollectionStore::new();
        let mut obj = make_object("rt-1");
        obj.status = Some("active".into());
        obj.tags = vec!["urgent".into(), "bug".into()];
        obj.date = Some("2026-03-15".into());
        obj.description = "A rich description".into();
        obj.color = Some("#ff0000".into());
        obj.pinned = true;
        obj.data.insert("priority".into(), serde_json::json!(1));
        obj.data
            .insert("labels".into(), serde_json::json!(["a", "b"]));

        store.put_object(&obj).unwrap();
        let retrieved = store.get_object(&object_id("rt-1")).unwrap();
        assert_eq!(retrieved, obj);
    }

    // ── Object filtering ──────────────────────────────────────────

    fn filtered_store() -> CollectionStore {
        let mut store = CollectionStore::new();
        let mut t1 = make_object("t1");
        t1.status = Some("active".into());
        t1.tags = vec!["urgent".into()];
        store.put_object(&t1).unwrap();

        let mut t2 = make_object("t2");
        t2.status = Some("done".into());
        t2.tags = vec!["urgent".into(), "bug".into()];
        store.put_object(&t2).unwrap();

        let mut n1 = make_object("n1");
        n1.type_name = "note".into();
        store.put_object(&n1).unwrap();

        let mut d1 = make_object("d1");
        d1.status = Some("active".into());
        d1.deleted_at = Some(ts("2026-01-02T00:00:00Z"));
        store.put_object(&d1).unwrap();

        store
    }

    #[test]
    fn list_objects_exclude_deleted_drops_tombstones() {
        let store = filtered_store();
        let filter = ObjectFilter {
            exclude_deleted: true,
            ..Default::default()
        };
        let list = store.list_objects(Some(&filter));
        assert_eq!(list.len(), 3);
    }

    #[test]
    fn list_objects_filters_by_type() {
        let store = filtered_store();
        let filter = ObjectFilter {
            types: Some(vec!["task".into()]),
            exclude_deleted: true,
            ..Default::default()
        };
        let tasks = store.list_objects(Some(&filter));
        assert_eq!(tasks.len(), 2);
        assert!(tasks.iter().all(|o| o.type_name == "task"));
    }

    #[test]
    fn list_objects_filters_by_tags_with_and_logic() {
        let store = filtered_store();
        let filter = ObjectFilter {
            tags: Some(vec!["urgent".into(), "bug".into()]),
            exclude_deleted: true,
            ..Default::default()
        };
        let urgent_bugs = store.list_objects(Some(&filter));
        assert_eq!(urgent_bugs.len(), 1);
        assert_eq!(urgent_bugs[0].id.0, "t2");
    }

    #[test]
    fn list_objects_filters_by_status() {
        let store = filtered_store();
        let filter = ObjectFilter {
            statuses: Some(vec!["active".into()]),
            exclude_deleted: true,
            ..Default::default()
        };
        let active = store.list_objects(Some(&filter));
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].id.0, "t1");
    }

    #[test]
    fn list_objects_includes_deleted_when_flag_is_off() {
        let store = filtered_store();
        let filter = ObjectFilter::default();
        let all = store.list_objects(Some(&filter));
        assert_eq!(all.len(), 4);
    }

    #[test]
    fn list_objects_filters_by_specific_parent_id() {
        let mut store = filtered_store();
        let mut c1 = make_object("c1");
        c1.parent_id = Some(object_id("t1"));
        store.put_object(&c1).unwrap();

        let filter = ObjectFilter {
            parent_id: ParentIdFilter::Some(object_id("t1")),
            exclude_deleted: true,
            ..Default::default()
        };
        let children = store.list_objects(Some(&filter));
        assert_eq!(children.len(), 1);
        assert_eq!(children[0].id.0, "c1");
    }

    #[test]
    fn list_objects_filters_by_root_parent() {
        let mut store = filtered_store();
        let mut c1 = make_object("c1");
        c1.parent_id = Some(object_id("t1"));
        store.put_object(&c1).unwrap();

        let filter = ObjectFilter {
            parent_id: ParentIdFilter::Root,
            exclude_deleted: true,
            ..Default::default()
        };
        let roots = store.list_objects(Some(&filter));
        // t1, t2, n1 are root; d1 is deleted; c1 has a parent.
        assert_eq!(roots.len(), 3);
    }

    #[test]
    fn list_objects_without_filter_returns_everything() {
        let store = filtered_store();
        let all = store.list_objects(None);
        assert_eq!(all.len(), 4);
    }

    // ── Edge CRUD ─────────────────────────────────────────────────

    #[test]
    fn puts_and_gets_an_edge() {
        let mut store = CollectionStore::new();
        store.put_edge(&make_edge("e1", "a", "b")).unwrap();
        let retrieved = store.get_edge(&edge_id("e1")).unwrap();
        assert_eq!(retrieved.relation, "depends-on");
    }

    #[test]
    fn returns_none_for_missing_edge() {
        let store = CollectionStore::new();
        assert!(store.get_edge(&edge_id("nope")).is_none());
    }

    #[test]
    fn removes_an_edge() {
        let mut store = CollectionStore::new();
        store.put_edge(&make_edge("e1", "a", "b")).unwrap();
        assert!(store.remove_edge(&edge_id("e1")).unwrap());
        assert!(store.get_edge(&edge_id("e1")).is_none());
    }

    #[test]
    fn counts_edges_correctly() {
        let mut store = CollectionStore::new();
        store.put_edge(&make_edge("e1", "a", "b")).unwrap();
        store.put_edge(&make_edge("e2", "a", "c")).unwrap();
        assert_eq!(store.edge_count(), 2);
    }

    // ── Edge filtering ────────────────────────────────────────────

    fn edges_store() -> CollectionStore {
        let mut store = CollectionStore::new();
        store.put_edge(&make_edge("e1", "a", "b")).unwrap();
        let mut e2 = make_edge("e2", "a", "c");
        e2.relation = "assigned-to".into();
        store.put_edge(&e2).unwrap();
        store.put_edge(&make_edge("e3", "b", "c")).unwrap();
        store
    }

    #[test]
    fn list_edges_filters_by_source_id() {
        let store = edges_store();
        let filter = EdgeFilter {
            source_id: Some(object_id("a")),
            ..Default::default()
        };
        let edges = store.list_edges(Some(&filter));
        assert_eq!(edges.len(), 2);
    }

    #[test]
    fn list_edges_filters_by_target_id() {
        let store = edges_store();
        let filter = EdgeFilter {
            target_id: Some(object_id("c")),
            ..Default::default()
        };
        let edges = store.list_edges(Some(&filter));
        assert_eq!(edges.len(), 2);
    }

    #[test]
    fn list_edges_filters_by_relation() {
        let store = edges_store();
        let filter = EdgeFilter {
            relation: Some("depends-on".into()),
            ..Default::default()
        };
        let edges = store.list_edges(Some(&filter));
        assert_eq!(edges.len(), 2);
    }

    #[test]
    fn list_edges_combines_filters() {
        let store = edges_store();
        let filter = EdgeFilter {
            source_id: Some(object_id("a")),
            relation: Some("depends-on".into()),
            ..Default::default()
        };
        let edges = store.list_edges(Some(&filter));
        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0].id.0, "e1");
    }

    #[test]
    fn list_edges_without_filter_returns_everything() {
        let store = edges_store();
        assert_eq!(store.list_edges(None).len(), 3);
    }

    // ── Snapshot / sync ───────────────────────────────────────────

    #[test]
    fn exports_and_imports_a_full_snapshot() {
        let mut store = CollectionStore::new();
        let mut obj = make_object("a");
        obj.name = "Alpha".into();
        store.put_object(&obj).unwrap();
        store.put_edge(&make_edge("e1", "a", "b")).unwrap();

        let snapshot = store.export_snapshot().unwrap();
        assert!(!snapshot.is_empty());

        let mut store2 = CollectionStore::new();
        store2.import(&snapshot).unwrap();

        assert_eq!(store2.get_object(&object_id("a")).unwrap().name, "Alpha");
        assert_eq!(
            store2.get_edge(&edge_id("e1")).unwrap().relation,
            "depends-on"
        );
    }

    #[test]
    fn syncs_between_two_peers_via_snapshots() {
        let mut peer1 = CollectionStore::with_options(CollectionStoreOptions { peer_id: Some(1) });
        let mut peer2 = CollectionStore::with_options(CollectionStoreOptions { peer_id: Some(2) });

        let mut p1_obj = make_object("p1-obj");
        p1_obj.name = "From Peer 1".into();
        peer1.put_object(&p1_obj).unwrap();

        let snapshot = peer1.export_snapshot().unwrap();
        peer2.import(&snapshot).unwrap();
        assert_eq!(
            peer2.get_object(&object_id("p1-obj")).unwrap().name,
            "From Peer 1"
        );

        let mut p2_obj = make_object("p2-obj");
        p2_obj.name = "From Peer 2".into();
        peer2.put_object(&p2_obj).unwrap();

        let update = peer2.export_snapshot().unwrap();
        peer1.import(&update).unwrap();

        assert_eq!(peer1.object_count(), 2);
        assert_eq!(peer2.object_count(), 2);
        assert_eq!(
            peer1.get_object(&object_id("p2-obj")).unwrap().name,
            "From Peer 2"
        );
    }

    // ── Bulk / debug ──────────────────────────────────────────────

    #[test]
    fn all_objects_returns_every_stored_object() {
        let mut store = CollectionStore::new();
        store.put_object(&make_object("a")).unwrap();
        store.put_object(&make_object("b")).unwrap();
        assert_eq!(store.all_objects().len(), 2);
    }

    #[test]
    fn all_edges_returns_every_stored_edge() {
        let mut store = CollectionStore::new();
        store.put_edge(&make_edge("e1", "a", "b")).unwrap();
        store.put_edge(&make_edge("e2", "a", "c")).unwrap();
        assert_eq!(store.all_edges().len(), 2);
    }

    #[test]
    fn to_json_returns_keyed_maps() {
        let mut store = CollectionStore::new();
        let mut obj = make_object("a");
        obj.name = "Alpha".into();
        store.put_object(&obj).unwrap();
        store.put_edge(&make_edge("e1", "a", "b")).unwrap();

        let (objects, edges) = store.to_json();
        assert_eq!(objects.get("a").unwrap().name, "Alpha");
        assert_eq!(edges.get("e1").unwrap().relation, "depends-on");
    }

    // ── Dirty tracking + change subscription ──────────────────────

    #[test]
    fn mutations_mark_the_store_dirty() {
        let mut store = CollectionStore::new();
        assert!(!store.is_dirty());
        store.put_object(&make_object("a")).unwrap();
        assert!(store.is_dirty());
        store.clear_dirty();
        assert!(!store.is_dirty());
    }

    #[test]
    fn change_listeners_fire_synchronously_on_mutation() {
        let mut store = CollectionStore::new();
        let log: std::rc::Rc<std::cell::RefCell<Vec<CollectionChange>>> =
            std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let log_clone = log.clone();
        store.on_change(move |changes| {
            log_clone.borrow_mut().extend_from_slice(changes);
        });

        store.put_object(&make_object("a")).unwrap();
        store.remove_object(&object_id("a")).unwrap();
        store.put_edge(&make_edge("e1", "a", "b")).unwrap();
        store.remove_edge(&edge_id("e1")).unwrap();

        let log = log.borrow();
        assert_eq!(log.len(), 4);
        assert_eq!(log[0].kind, CollectionChangeKind::ObjectPut);
        assert_eq!(log[0].id, "a");
        assert_eq!(log[1].kind, CollectionChangeKind::ObjectRemove);
        assert_eq!(log[2].kind, CollectionChangeKind::EdgePut);
        assert_eq!(log[3].kind, CollectionChangeKind::EdgeRemove);
    }

    #[test]
    fn off_change_stops_further_notifications() {
        let mut store = CollectionStore::new();
        let count = std::rc::Rc::new(std::cell::RefCell::new(0usize));
        let count_clone = count.clone();
        let sub = store.on_change(move |_| *count_clone.borrow_mut() += 1);

        store.put_object(&make_object("a")).unwrap();
        assert_eq!(*count.borrow(), 1);

        store.off_change(sub);
        store.put_object(&make_object("b")).unwrap();
        assert_eq!(*count.borrow(), 1);
        assert_eq!(store.listener_count(), 0);
    }
}
