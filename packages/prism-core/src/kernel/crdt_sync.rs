//! `crdt_sync` — bidirectional bridge between [`CollectionStore`]
//! and the reactive [`Atom`] layer.
//!
//! `CrdtSync` wraps a shared `CollectionStore` and maintains
//! per-object / per-edge atoms that fire only when a specific
//! record changes. All mutations should flow through `CrdtSync`
//! so the CRDT stays the source of truth and changes fan out to
//! atom subscribers automatically.
//!
//! For remote sync, [`import_and_refresh`](CrdtSync::import_and_refresh)
//! imports a peer snapshot and refreshes every tracked atom to
//! reflect the merged state.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::foundation::object_model::{EdgeId, GraphObject, ObjectEdge, ObjectId};
use crate::foundation::persistence::{
    CollectionChange, CollectionChangeKind, CollectionStore, PersistenceError,
};
use crate::kernel::atom::{Atom, SharedAtom};

/// Change event emitted by [`CrdtSync`] to its global listeners.
#[derive(Debug, Clone)]
pub enum SyncEvent {
    ObjectChanged {
        id: String,
        object: Option<GraphObject>,
    },
    EdgeChanged {
        id: String,
        edge: Option<ObjectEdge>,
    },
}

type SyncListener = Box<dyn FnMut(&SyncEvent)>;

/// Handle returned by [`CrdtSync::on_sync`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SyncSubscription(u64);

impl SyncSubscription {
    pub fn raw(&self) -> u64 {
        self.0
    }
}

/// Bidirectional bridge between a [`CollectionStore`] and per-record
/// reactive atoms.
///
/// Atoms are created lazily via [`object_atom`](CrdtSync::object_atom)
/// / [`edge_atom`](CrdtSync::edge_atom) and automatically reflect
/// the current CRDT state. All writes go through
/// [`write_object`](CrdtSync::write_object) /
/// [`write_edge`](CrdtSync::write_edge) which update the CRDT first,
/// then sync the corresponding atom.
pub struct CrdtSync {
    store: Rc<RefCell<CollectionStore>>,
    object_atoms: HashMap<String, SharedAtom<Option<GraphObject>>>,
    edge_atoms: HashMap<String, SharedAtom<Option<ObjectEdge>>>,
    listeners: Vec<(u64, SyncListener)>,
    next_listener_id: u64,
}

impl CrdtSync {
    pub fn new(store: Rc<RefCell<CollectionStore>>) -> Self {
        Self {
            store,
            object_atoms: HashMap::new(),
            edge_atoms: HashMap::new(),
            listeners: Vec::new(),
            next_listener_id: 0,
        }
    }

    pub fn collection(&self) -> &Rc<RefCell<CollectionStore>> {
        &self.store
    }

    // ── Object atoms ─────────────────────────────────────────────

    /// Get or create a reactive atom tracking a specific object.
    /// Holds `Some(GraphObject)` while the object exists, `None`
    /// after removal.
    pub fn object_atom(&mut self, id: &str) -> SharedAtom<Option<GraphObject>> {
        if let Some(atom) = self.object_atoms.get(id) {
            return atom.clone();
        }
        let current = self.store.borrow().get_object(&ObjectId(id.to_string()));
        let atom = Rc::new(RefCell::new(Atom::new(current)));
        self.object_atoms.insert(id.to_string(), atom.clone());
        atom
    }

    /// Get or create a reactive atom tracking a specific edge.
    pub fn edge_atom(&mut self, id: &str) -> SharedAtom<Option<ObjectEdge>> {
        if let Some(atom) = self.edge_atoms.get(id) {
            return atom.clone();
        }
        let current = self.store.borrow().get_edge(&EdgeId(id.to_string()));
        let atom = Rc::new(RefCell::new(Atom::new(current)));
        self.edge_atoms.insert(id.to_string(), atom.clone());
        atom
    }

    // ── Mutations (CRDT-first) ───────────────────────────────────

    /// Write an object through the CRDT and sync its atom.
    pub fn write_object(&mut self, obj: &GraphObject) -> Result<(), PersistenceError> {
        self.store.borrow_mut().put_object(obj)?;
        self.sync_object(&obj.id.0);
        self.emit(SyncEvent::ObjectChanged {
            id: obj.id.0.clone(),
            object: Some(obj.clone()),
        });
        Ok(())
    }

    /// Remove an object through the CRDT and sync its atom.
    pub fn remove_object(&mut self, id: &ObjectId) -> Result<bool, PersistenceError> {
        let removed = self.store.borrow_mut().remove_object(id)?;
        if removed {
            self.sync_object(&id.0);
            self.emit(SyncEvent::ObjectChanged {
                id: id.0.clone(),
                object: None,
            });
        }
        Ok(removed)
    }

    /// Write an edge through the CRDT and sync its atom.
    pub fn write_edge(&mut self, edge: &ObjectEdge) -> Result<(), PersistenceError> {
        self.store.borrow_mut().put_edge(edge)?;
        self.sync_edge(&edge.id.0);
        self.emit(SyncEvent::EdgeChanged {
            id: edge.id.0.clone(),
            edge: Some(edge.clone()),
        });
        Ok(())
    }

    /// Remove an edge through the CRDT and sync its atom.
    pub fn remove_edge(&mut self, id: &EdgeId) -> Result<bool, PersistenceError> {
        let removed = self.store.borrow_mut().remove_edge(id)?;
        if removed {
            self.sync_edge(&id.0);
            self.emit(SyncEvent::EdgeChanged {
                id: id.0.clone(),
                edge: None,
            });
        }
        Ok(removed)
    }

    // ── Remote sync ──────────────────────────────────────────────

    /// Import a snapshot from another peer and refresh all tracked
    /// atoms to reflect the merged state.
    pub fn import_and_refresh(&mut self, data: &[u8]) -> Result<(), PersistenceError> {
        self.store.borrow_mut().import(data)?;
        self.refresh_all();
        Ok(())
    }

    /// Export the current document state for sending to a peer.
    pub fn export_snapshot(&self) -> Result<Vec<u8>, PersistenceError> {
        self.store.borrow().export_snapshot()
    }

    // ── Process external changes ─────────────────────────────────

    /// Process a batch of changes from the `CollectionStore`'s
    /// `on_change` listener. Updates atoms and fires sync events.
    ///
    /// Use this when external code mutates the `CollectionStore`
    /// directly (bypassing `CrdtSync`), e.g. daemon IPC commands.
    pub fn process_changes(&mut self, changes: &[CollectionChange]) {
        for change in changes {
            match change.kind {
                CollectionChangeKind::ObjectPut | CollectionChangeKind::ObjectRemove => {
                    self.sync_object(&change.id);
                    let object = self.store.borrow().get_object(&ObjectId(change.id.clone()));
                    self.emit(SyncEvent::ObjectChanged {
                        id: change.id.clone(),
                        object,
                    });
                }
                CollectionChangeKind::EdgePut | CollectionChangeKind::EdgeRemove => {
                    self.sync_edge(&change.id);
                    let edge = self.store.borrow().get_edge(&EdgeId(change.id.clone()));
                    self.emit(SyncEvent::EdgeChanged {
                        id: change.id.clone(),
                        edge,
                    });
                }
            }
        }
    }

    // ── Refresh ──────────────────────────────────────────────────

    /// Refresh all tracked atoms from the CRDT.
    pub fn refresh_all(&mut self) {
        let store = self.store.borrow();
        for (id, atom) in &self.object_atoms {
            let current = store.get_object(&ObjectId(id.clone()));
            atom.borrow_mut().set(current);
        }
        for (id, atom) in &self.edge_atoms {
            let current = store.get_edge(&EdgeId(id.clone()));
            atom.borrow_mut().set(current);
        }
    }

    // ── Sync event bus ───────────────────────────────────────────

    /// Subscribe to all sync events.
    pub fn on_sync<F>(&mut self, listener: F) -> SyncSubscription
    where
        F: FnMut(&SyncEvent) + 'static,
    {
        let id = self.next_listener_id;
        self.next_listener_id += 1;
        self.listeners.push((id, Box::new(listener)));
        SyncSubscription(id)
    }

    pub fn off_sync(&mut self, sub: SyncSubscription) {
        self.listeners.retain(|(id, _)| *id != sub.0);
    }

    pub fn tracked_object_count(&self) -> usize {
        self.object_atoms.len()
    }

    pub fn tracked_edge_count(&self) -> usize {
        self.edge_atoms.len()
    }

    pub fn sync_listener_count(&self) -> usize {
        self.listeners.len()
    }

    // ── Internal ─────────────────────────────────────────────────

    fn sync_object(&mut self, id: &str) {
        if let Some(atom) = self.object_atoms.get(id) {
            let current = self.store.borrow().get_object(&ObjectId(id.to_string()));
            atom.borrow_mut().set(current);
        }
    }

    fn sync_edge(&mut self, id: &str) {
        if let Some(atom) = self.edge_atoms.get(id) {
            let current = self.store.borrow().get_edge(&EdgeId(id.to_string()));
            atom.borrow_mut().set(current);
        }
    }

    fn emit(&mut self, event: SyncEvent) {
        for (_, listener) in self.listeners.iter_mut() {
            listener(&event);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::foundation::object_model::{edge_id, object_id};
    use crate::foundation::persistence::CollectionStoreOptions;
    use chrono::{DateTime, Utc};
    use std::cell::Cell;
    use std::collections::BTreeMap;

    fn ts(s: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(s)
            .expect("valid rfc3339")
            .with_timezone(&Utc)
    }

    fn make_object(id: &str, name: &str) -> GraphObject {
        GraphObject {
            id: object_id(id),
            type_name: "task".into(),
            name: name.into(),
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

    fn make_sync() -> CrdtSync {
        let store = Rc::new(RefCell::new(CollectionStore::new()));
        CrdtSync::new(store)
    }

    // ── Object atom lifecycle ────────────────────────────────────

    #[test]
    fn object_atom_starts_none_for_missing_object() {
        let mut sync = make_sync();
        let atom = sync.object_atom("obj-1");
        assert!(atom.borrow().get().is_none());
    }

    #[test]
    fn object_atom_reflects_existing_object() {
        let store = Rc::new(RefCell::new(CollectionStore::new()));
        store
            .borrow_mut()
            .put_object(&make_object("obj-1", "Alpha"))
            .unwrap();
        let mut sync = CrdtSync::new(store);

        let atom = sync.object_atom("obj-1");
        assert_eq!(atom.borrow().get().as_ref().unwrap().name, "Alpha");
    }

    #[test]
    fn write_object_updates_atom() {
        let mut sync = make_sync();
        let atom = sync.object_atom("obj-1");

        sync.write_object(&make_object("obj-1", "First")).unwrap();
        assert_eq!(atom.borrow().get().as_ref().unwrap().name, "First");

        let mut updated = make_object("obj-1", "Second");
        updated.status = Some("active".into());
        sync.write_object(&updated).unwrap();
        assert_eq!(atom.borrow().get().as_ref().unwrap().name, "Second");
    }

    #[test]
    fn remove_object_sets_atom_to_none() {
        let mut sync = make_sync();
        let atom = sync.object_atom("obj-1");

        sync.write_object(&make_object("obj-1", "Doomed")).unwrap();
        assert!(atom.borrow().get().is_some());

        sync.remove_object(&object_id("obj-1")).unwrap();
        assert!(atom.borrow().get().is_none());
    }

    #[test]
    fn atom_subscriber_fires_on_write() {
        let mut sync = make_sync();
        let atom = sync.object_atom("obj-1");

        let names: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
        let nc = names.clone();
        atom.borrow_mut().subscribe(move |obj| {
            let name = obj.as_ref().map(|o| o.name.clone()).unwrap_or_default();
            nc.borrow_mut().push(name);
        });

        sync.write_object(&make_object("obj-1", "A")).unwrap();
        sync.write_object(&make_object("obj-1", "B")).unwrap();

        assert_eq!(&*names.borrow(), &["A", "B"]);
    }

    #[test]
    fn atom_subscriber_does_not_fire_when_value_unchanged() {
        let mut sync = make_sync();
        let atom = sync.object_atom("obj-1");

        let fires = Rc::new(Cell::new(0usize));
        let fc = fires.clone();
        atom.borrow_mut().subscribe(move |_| fc.set(fc.get() + 1));

        let obj = make_object("obj-1", "Same");
        sync.write_object(&obj).unwrap();
        assert_eq!(fires.get(), 1);

        sync.write_object(&obj).unwrap();
        assert_eq!(fires.get(), 1);
    }

    // ── Edge atom lifecycle ──────────────────────────────────────

    #[test]
    fn edge_atom_starts_none_for_missing_edge() {
        let mut sync = make_sync();
        let atom = sync.edge_atom("e-1");
        assert!(atom.borrow().get().is_none());
    }

    #[test]
    fn write_edge_updates_atom() {
        let mut sync = make_sync();
        let atom = sync.edge_atom("e-1");

        sync.write_edge(&make_edge("e-1", "a", "b")).unwrap();
        assert_eq!(atom.borrow().get().as_ref().unwrap().relation, "depends-on");
    }

    #[test]
    fn remove_edge_sets_atom_to_none() {
        let mut sync = make_sync();
        let atom = sync.edge_atom("e-1");

        sync.write_edge(&make_edge("e-1", "a", "b")).unwrap();
        assert!(atom.borrow().get().is_some());

        sync.remove_edge(&edge_id("e-1")).unwrap();
        assert!(atom.borrow().get().is_none());
    }

    // ── Remote sync ──────────────────────────────────────────────

    #[test]
    fn import_and_refresh_updates_tracked_atoms() {
        let store1 = Rc::new(RefCell::new(CollectionStore::with_options(
            CollectionStoreOptions { peer_id: Some(1) },
        )));
        store1
            .borrow_mut()
            .put_object(&make_object("shared", "Original"))
            .unwrap();
        let snapshot = store1.borrow().export_snapshot().unwrap();

        let store2 = Rc::new(RefCell::new(CollectionStore::with_options(
            CollectionStoreOptions { peer_id: Some(2) },
        )));
        let mut sync = CrdtSync::new(store2);
        let atom = sync.object_atom("shared");
        assert!(atom.borrow().get().is_none());

        sync.import_and_refresh(&snapshot).unwrap();
        assert_eq!(atom.borrow().get().as_ref().unwrap().name, "Original");
    }

    #[test]
    fn export_snapshot_round_trips() {
        let mut sync = make_sync();
        sync.write_object(&make_object("a", "Alpha")).unwrap();
        sync.write_edge(&make_edge("e1", "a", "b")).unwrap();

        let snapshot = sync.export_snapshot().unwrap();
        assert!(!snapshot.is_empty());

        let store2 = Rc::new(RefCell::new(CollectionStore::new()));
        let mut sync2 = CrdtSync::new(store2);
        sync2.import_and_refresh(&snapshot).unwrap();

        let atom = sync2.object_atom("a");
        assert_eq!(atom.borrow().get().as_ref().unwrap().name, "Alpha");
    }

    // ── process_changes ──────────────────────────────────────────

    #[test]
    fn process_changes_updates_tracked_atoms() {
        let store = Rc::new(RefCell::new(CollectionStore::new()));
        let mut sync = CrdtSync::new(store.clone());
        let atom = sync.object_atom("obj-1");

        store
            .borrow_mut()
            .put_object(&make_object("obj-1", "External"))
            .unwrap();

        sync.process_changes(&[CollectionChange {
            kind: CollectionChangeKind::ObjectPut,
            id: "obj-1".into(),
        }]);

        assert_eq!(atom.borrow().get().as_ref().unwrap().name, "External");
    }

    #[test]
    fn process_changes_ignores_untracked_ids() {
        let store = Rc::new(RefCell::new(CollectionStore::new()));
        let mut sync = CrdtSync::new(store.clone());

        store
            .borrow_mut()
            .put_object(&make_object("untracked", "Ghost"))
            .unwrap();

        sync.process_changes(&[CollectionChange {
            kind: CollectionChangeKind::ObjectPut,
            id: "untracked".into(),
        }]);

        assert_eq!(sync.tracked_object_count(), 0);
    }

    // ── Sync event bus ───────────────────────────────────────────

    #[test]
    fn on_sync_fires_for_writes() {
        let mut sync = make_sync();
        let events: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
        let ec = events.clone();
        sync.on_sync(move |event| match event {
            SyncEvent::ObjectChanged { id, .. } => ec.borrow_mut().push(id.clone()),
            SyncEvent::EdgeChanged { id, .. } => ec.borrow_mut().push(id.clone()),
        });

        sync.write_object(&make_object("a", "A")).unwrap();
        sync.write_edge(&make_edge("e1", "a", "b")).unwrap();

        assert_eq!(&*events.borrow(), &["a", "e1"]);
    }

    #[test]
    fn off_sync_stops_notifications() {
        let mut sync = make_sync();
        let count = Rc::new(Cell::new(0usize));
        let cc = count.clone();
        let sub = sync.on_sync(move |_| cc.set(cc.get() + 1));

        sync.write_object(&make_object("a", "A")).unwrap();
        assert_eq!(count.get(), 1);

        sync.off_sync(sub);
        sync.write_object(&make_object("b", "B")).unwrap();
        assert_eq!(count.get(), 1);
    }

    // ── Tracking counts ──────────────────────────────────────────

    #[test]
    fn tracked_counts_reflect_created_atoms() {
        let mut sync = make_sync();
        assert_eq!(sync.tracked_object_count(), 0);
        assert_eq!(sync.tracked_edge_count(), 0);

        sync.object_atom("a");
        sync.object_atom("b");
        sync.edge_atom("e1");

        assert_eq!(sync.tracked_object_count(), 2);
        assert_eq!(sync.tracked_edge_count(), 1);
    }

    #[test]
    fn same_id_returns_same_atom() {
        let mut sync = make_sync();
        let a1 = sync.object_atom("x");
        let a2 = sync.object_atom("x");
        assert!(Rc::ptr_eq(&a1, &a2));
        assert_eq!(sync.tracked_object_count(), 1);
    }

    // ── Bidirectional flow ───────────────────────────────────────

    #[test]
    fn write_then_read_from_collection_store() {
        let mut sync = make_sync();
        sync.write_object(&make_object("obj-1", "Written")).unwrap();

        let from_store = sync
            .collection()
            .borrow()
            .get_object(&object_id("obj-1"))
            .unwrap();
        assert_eq!(from_store.name, "Written");
    }

    #[test]
    fn full_bidirectional_flow() {
        let mut sync = make_sync();
        let atom = sync.object_atom("task-1");

        let seen: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
        let sc = seen.clone();
        atom.borrow_mut().subscribe(move |obj| {
            let name = obj
                .as_ref()
                .map(|o| o.name.clone())
                .unwrap_or("(none)".into());
            sc.borrow_mut().push(name);
        });

        sync.write_object(&make_object("task-1", "Created"))
            .unwrap();

        sync.write_object(&make_object("task-1", "Updated"))
            .unwrap();

        sync.remove_object(&object_id("task-1")).unwrap();

        assert_eq!(&*seen.borrow(), &["Created", "Updated", "(none)"]);

        assert!(sync
            .collection()
            .borrow()
            .get_object(&object_id("task-1"))
            .is_none());
    }
}
