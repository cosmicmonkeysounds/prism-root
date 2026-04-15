//! `vault_manager` — load/save [`CollectionStore`]s against a pluggable
//! storage adapter.
//!
//! Port of `foundation/persistence/vault-persistence.ts` from the
//! legacy TS tree (commit `8426588`). Two layers:
//!
//! - [`PersistenceAdapter`] — the pluggable I/O trait. Hosts plug in a
//!   real filesystem / IPC backend; [`MemoryAdapter`] is included for
//!   tests and ephemeral vaults. Synchronous to match Loro's own sync
//!   API surface.
//! - [`VaultManager`] — orchestrates a [`PrismManifest`]'s collections
//!   against an adapter. Lazy-loads [`CollectionStore`]s on first
//!   [`open_collection`](VaultManager::open_collection), tracks dirty
//!   state via per-store change listeners, and ships Loro snapshots
//!   to disk on demand.
//!
//! Storage layout mirrors the TS port: each collection lives at
//! `data/collections/{id}.loro` under the vault root.

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::rc::Rc;

use crate::identity::manifest::PrismManifest;

use super::collection_store::{CollectionStore, CollectionStoreOptions, PersistenceError};

/// Pluggable storage I/O for Loro snapshots. Paths are relative to the
/// vault root; the adapter resolves them against whatever backing
/// store it wraps (filesystem, Tauri IPC, in-memory map, …).
pub trait PersistenceAdapter {
    /// Load a binary blob. Returns `None` if the path does not exist.
    fn load(&self, path: &str) -> Option<Vec<u8>>;
    /// Save a binary blob, creating parent directories as needed.
    fn save(&mut self, path: &str, data: &[u8]);
    /// Delete a blob. Returns `true` if the path existed.
    fn delete(&mut self, path: &str) -> bool;
    /// Check if a path exists.
    fn exists(&self, path: &str) -> bool;
    /// List direct children of a directory. Entries are returned as
    /// names relative to the directory, sorted lexicographically, and
    /// never include nested paths.
    fn list(&self, directory: &str) -> Vec<String>;
}

/// In-memory [`PersistenceAdapter`] for tests and ephemeral vaults.
#[derive(Debug, Default)]
pub struct MemoryAdapter {
    store: BTreeMap<String, Vec<u8>>,
}

impl MemoryAdapter {
    pub fn new() -> Self {
        Self::default()
    }
}

impl PersistenceAdapter for MemoryAdapter {
    fn load(&self, path: &str) -> Option<Vec<u8>> {
        self.store.get(path).cloned()
    }

    fn save(&mut self, path: &str, data: &[u8]) {
        self.store.insert(path.to_string(), data.to_vec());
    }

    fn delete(&mut self, path: &str) -> bool {
        self.store.remove(path).is_some()
    }

    fn exists(&self, path: &str) -> bool {
        self.store.contains_key(path)
    }

    fn list(&self, directory: &str) -> Vec<String> {
        let prefix = if directory.ends_with('/') {
            directory.to_string()
        } else {
            format!("{directory}/")
        };
        let mut entries: Vec<String> = self
            .store
            .keys()
            .filter_map(|k| k.strip_prefix(&prefix))
            .filter(|rel| !rel.contains('/'))
            .map(|s| s.to_string())
            .collect();
        entries.sort();
        entries
    }
}

/// Options passed to [`VaultManager::new`].
#[derive(Debug, Clone, Default)]
pub struct VaultManagerOptions {
    /// Loro peer id applied to every [`CollectionStore`] created by
    /// this manager. `None` lets Loro assign a random id per store.
    pub peer_id: Option<u64>,
}

/// Shared dirty-set handle. `Rc<RefCell<_>>` so the per-store change
/// listener (which has `'static` lifetime requirements) can flip a
/// bit that the owning `VaultManager` immediately observes on the
/// next poll. Kept private to the module.
type DirtySet = Rc<RefCell<HashSet<String>>>;

/// Orchestrates a [`PrismManifest`]'s collections against a
/// [`PersistenceAdapter`]. Stores are lazy-loaded on first open, dirty
/// tracking is wired through each store's change listener, and saves
/// only touch the adapter when something actually changed.
pub struct VaultManager<A: PersistenceAdapter> {
    manifest: PrismManifest,
    adapter: A,
    options: VaultManagerOptions,
    cache: HashMap<String, CollectionStore>,
    dirty: DirtySet,
}

impl<A: PersistenceAdapter> VaultManager<A> {
    /// Build a new manager. Collections are not loaded until the first
    /// [`open_collection`](Self::open_collection) call.
    pub fn new(manifest: PrismManifest, adapter: A) -> Self {
        Self::with_options(manifest, adapter, VaultManagerOptions::default())
    }

    pub fn with_options(manifest: PrismManifest, adapter: A, options: VaultManagerOptions) -> Self {
        Self {
            manifest,
            adapter,
            options,
            cache: HashMap::new(),
            dirty: Rc::new(RefCell::new(HashSet::new())),
        }
    }

    pub fn manifest(&self) -> &PrismManifest {
        &self.manifest
    }

    pub fn adapter(&self) -> &A {
        &self.adapter
    }

    pub fn adapter_mut(&mut self) -> &mut A {
        &mut self.adapter
    }

    /// Open a collection by its ref id. Returns a mutable borrow of a
    /// cached [`CollectionStore`], creating and hydrating from disk on
    /// first access.
    pub fn open_collection(
        &mut self,
        collection_id: &str,
    ) -> Result<&mut CollectionStore, PersistenceError> {
        if !self.cache.contains_key(collection_id) {
            self.resolve_ref(collection_id)?;

            let mut store = CollectionStore::with_options(CollectionStoreOptions {
                peer_id: self.options.peer_id,
            });

            let path = collection_path(collection_id);
            if let Some(snapshot) = self.adapter.load(&path) {
                store.import(&snapshot)?;
            }

            let dirty = self.dirty.clone();
            let id = collection_id.to_string();
            store.on_change(move |_| {
                dirty.borrow_mut().insert(id.clone());
            });

            self.cache.insert(collection_id.to_string(), store);
        }
        Ok(self.cache.get_mut(collection_id).expect("just inserted"))
    }

    /// Save a single collection's snapshot to the adapter. No-op if
    /// the collection has not been opened or is not dirty.
    pub fn save_collection(&mut self, collection_id: &str) -> Result<(), PersistenceError> {
        if !self.dirty.borrow().contains(collection_id) {
            return Ok(());
        }
        let Some(store) = self.cache.get_mut(collection_id) else {
            return Ok(());
        };
        let snapshot = store.export_snapshot()?;
        self.adapter
            .save(&collection_path(collection_id), &snapshot);
        self.dirty.borrow_mut().remove(collection_id);
        Ok(())
    }

    /// Save every dirty collection. Returns the ids that were written.
    pub fn save_all(&mut self) -> Result<Vec<String>, PersistenceError> {
        let dirty_ids: Vec<String> = self.dirty.borrow().iter().cloned().collect();
        let mut saved = Vec::new();
        for id in dirty_ids {
            if let Some(store) = self.cache.get_mut(&id) {
                let snapshot = store.export_snapshot()?;
                self.adapter.save(&collection_path(&id), &snapshot);
                saved.push(id);
            }
        }
        self.dirty.borrow_mut().clear();
        Ok(saved)
    }

    /// Close a collection: save if dirty, then drop it from the cache.
    /// No-op if the collection is not open.
    pub fn close_collection(&mut self, collection_id: &str) -> Result<(), PersistenceError> {
        if !self.cache.contains_key(collection_id) {
            return Ok(());
        }
        self.save_collection(collection_id)?;
        self.cache.remove(collection_id);
        self.dirty.borrow_mut().remove(collection_id);
        Ok(())
    }

    /// Whether the collection currently has unsaved changes. Returns
    /// `false` for collections that have never been opened.
    pub fn is_dirty(&self, collection_id: &str) -> bool {
        self.dirty.borrow().contains(collection_id)
    }

    /// Ids of currently open collections (in arbitrary order).
    pub fn open_collections(&self) -> Vec<String> {
        self.cache.keys().cloned().collect()
    }

    fn resolve_ref(&self, collection_id: &str) -> Result<(), PersistenceError> {
        let found = self
            .manifest
            .collections
            .as_ref()
            .map(|list| list.iter().any(|c| c.id == collection_id))
            .unwrap_or(false);
        if !found {
            return Err(PersistenceError::UnknownCollection(format!(
                "Collection '{collection_id}' not found in manifest '{}'",
                self.manifest.name
            )));
        }
        Ok(())
    }
}

fn collection_path(collection_id: &str) -> String {
    format!("data/collections/{collection_id}.loro")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::foundation::object_model::{object_id, GraphObject};
    use crate::identity::manifest::{add_collection, default_manifest, CollectionRef};
    use chrono::{DateTime, Utc};
    use std::collections::BTreeMap;

    // ── Helpers ───────────────────────────────────────────────────

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

    fn make_manifest() -> PrismManifest {
        let m = default_manifest("Test Vault", "vault-1");
        let m = add_collection(&m, CollectionRef::new("tasks", "Tasks")).unwrap();
        add_collection(&m, CollectionRef::new("contacts", "Contacts")).unwrap()
    }

    // ── MemoryAdapter ─────────────────────────────────────────────

    #[test]
    fn load_returns_none_for_missing_path() {
        let adapter = MemoryAdapter::new();
        assert!(adapter.load("nonexistent").is_none());
    }

    #[test]
    fn save_and_load_round_trip() {
        let mut adapter = MemoryAdapter::new();
        let data = vec![1u8, 2, 3, 4];
        adapter.save("data/test.loro", &data);
        assert_eq!(adapter.load("data/test.loro"), Some(data));
    }

    #[test]
    fn exists_returns_true_after_save() {
        let mut adapter = MemoryAdapter::new();
        assert!(!adapter.exists("file.bin"));
        adapter.save("file.bin", &[0u8]);
        assert!(adapter.exists("file.bin"));
    }

    #[test]
    fn delete_removes_a_file_and_returns_true() {
        let mut adapter = MemoryAdapter::new();
        adapter.save("file.bin", &[0u8]);
        assert!(adapter.delete("file.bin"));
        assert!(!adapter.exists("file.bin"));
    }

    #[test]
    fn delete_returns_false_for_missing_file() {
        let mut adapter = MemoryAdapter::new();
        assert!(!adapter.delete("nonexistent"));
    }

    #[test]
    fn list_returns_direct_children_only() {
        let mut adapter = MemoryAdapter::new();
        adapter.save("data/a.loro", &[1u8]);
        adapter.save("data/b.loro", &[2u8]);
        adapter.save("data/sub/c.loro", &[3u8]);

        assert_eq!(
            adapter.list("data"),
            vec!["a.loro".to_string(), "b.loro".to_string()]
        );
    }

    #[test]
    fn list_returns_empty_for_missing_directory() {
        let adapter = MemoryAdapter::new();
        assert!(adapter.list("nonexistent").is_empty());
    }

    // ── VaultManager ──────────────────────────────────────────────

    #[test]
    fn exposes_manifest_and_adapter() {
        let vault = VaultManager::new(make_manifest(), MemoryAdapter::new());
        assert_eq!(vault.manifest().id, "vault-1");
        let _ = vault.adapter();
    }

    #[test]
    fn opens_a_collection_and_returns_a_store() {
        let mut vault = VaultManager::new(make_manifest(), MemoryAdapter::new());
        let store = vault.open_collection("tasks").unwrap();
        assert_eq!(store.object_count(), 0);
    }

    #[test]
    fn repeated_opens_share_the_same_cached_store() {
        let mut vault = VaultManager::new(make_manifest(), MemoryAdapter::new());
        vault
            .open_collection("tasks")
            .unwrap()
            .put_object(&make_object("a"))
            .unwrap();
        assert_eq!(vault.open_collection("tasks").unwrap().object_count(), 1);
    }

    #[test]
    fn errors_for_unknown_collection_id() {
        let mut vault = VaultManager::new(make_manifest(), MemoryAdapter::new());
        let msg = match vault.open_collection("nonexistent") {
            Ok(_) => panic!("expected error"),
            Err(e) => format!("{e}"),
        };
        assert!(msg.contains("nonexistent"), "got: {msg}");
    }

    #[test]
    fn tracks_open_collections() {
        let mut vault = VaultManager::new(make_manifest(), MemoryAdapter::new());
        assert!(vault.open_collections().is_empty());
        vault.open_collection("tasks").unwrap();
        vault.open_collection("contacts").unwrap();
        let mut open = vault.open_collections();
        open.sort();
        assert_eq!(open, vec!["contacts".to_string(), "tasks".to_string()]);
    }

    // ── Dirty tracking ────────────────────────────────────────────

    #[test]
    fn marks_collection_dirty_after_mutation() {
        let mut vault = VaultManager::new(make_manifest(), MemoryAdapter::new());
        vault.open_collection("tasks").unwrap();
        assert!(!vault.is_dirty("tasks"));

        vault
            .open_collection("tasks")
            .unwrap()
            .put_object(&make_object("a"))
            .unwrap();
        assert!(vault.is_dirty("tasks"));
    }

    #[test]
    fn is_dirty_false_for_unopened_collection() {
        let vault = VaultManager::new(make_manifest(), MemoryAdapter::new());
        assert!(!vault.is_dirty("tasks"));
    }

    // ── Save ──────────────────────────────────────────────────────

    #[test]
    fn save_collection_persists_snapshot_to_adapter() {
        let mut vault = VaultManager::new(make_manifest(), MemoryAdapter::new());
        vault
            .open_collection("tasks")
            .unwrap()
            .put_object(&make_object("a"))
            .unwrap();
        vault.save_collection("tasks").unwrap();

        assert!(vault.adapter().exists("data/collections/tasks.loro"));
        assert!(!vault.is_dirty("tasks"));
    }

    #[test]
    fn save_collection_is_a_no_op_when_not_dirty() {
        let mut vault = VaultManager::new(make_manifest(), MemoryAdapter::new());
        vault.open_collection("tasks").unwrap();
        vault.save_collection("tasks").unwrap();
        assert!(!vault.adapter().exists("data/collections/tasks.loro"));
    }

    #[test]
    fn save_collection_is_a_no_op_for_unopened_collection() {
        let mut vault = VaultManager::new(make_manifest(), MemoryAdapter::new());
        vault.save_collection("tasks").unwrap();
        assert!(!vault.adapter().exists("data/collections/tasks.loro"));
    }

    #[test]
    fn save_all_persists_all_dirty_collections() {
        let mut vault = VaultManager::new(make_manifest(), MemoryAdapter::new());
        vault
            .open_collection("tasks")
            .unwrap()
            .put_object(&make_object("t1"))
            .unwrap();
        vault
            .open_collection("contacts")
            .unwrap()
            .put_object(&make_object("c1"))
            .unwrap();

        let mut saved = vault.save_all().unwrap();
        saved.sort();
        assert_eq!(saved, vec!["contacts".to_string(), "tasks".to_string()]);
        assert!(!vault.is_dirty("tasks"));
        assert!(!vault.is_dirty("contacts"));
    }

    #[test]
    fn save_all_returns_empty_when_nothing_dirty() {
        let mut vault = VaultManager::new(make_manifest(), MemoryAdapter::new());
        vault.open_collection("tasks").unwrap();
        assert!(vault.save_all().unwrap().is_empty());
    }

    // ── Load from disk ────────────────────────────────────────────

    #[test]
    fn hydrates_collection_from_existing_disk_data() {
        let manifest = make_manifest();
        let adapter = MemoryAdapter::new();

        let mut vault = VaultManager::new(manifest.clone(), adapter);
        let mut obj = make_object("persisted");
        obj.name = "Persisted".into();
        vault
            .open_collection("tasks")
            .unwrap()
            .put_object(&obj)
            .unwrap();
        vault.save_collection("tasks").unwrap();

        // Re-open against the same adapter state — move the adapter
        // out of the first vault into a second one.
        let VaultManager { adapter, .. } = vault;
        let mut vault2 = VaultManager::new(manifest, adapter);
        let store2 = vault2.open_collection("tasks").unwrap();
        assert_eq!(
            store2.get_object(&object_id("persisted")).unwrap().name,
            "Persisted"
        );
        assert_eq!(store2.object_count(), 1);
    }

    // ── Close ─────────────────────────────────────────────────────

    #[test]
    fn close_collection_saves_and_evicts() {
        let mut vault = VaultManager::new(make_manifest(), MemoryAdapter::new());
        vault
            .open_collection("tasks")
            .unwrap()
            .put_object(&make_object("a"))
            .unwrap();
        vault.close_collection("tasks").unwrap();

        assert!(vault.open_collections().is_empty());
        assert!(!vault.is_dirty("tasks"));
        assert!(vault.adapter().exists("data/collections/tasks.loro"));
    }

    #[test]
    fn close_collection_is_a_no_op_for_unopened_collection() {
        let mut vault = VaultManager::new(make_manifest(), MemoryAdapter::new());
        vault.close_collection("tasks").unwrap();
        assert!(vault.open_collections().is_empty());
    }

    #[test]
    fn re_opening_a_closed_collection_hydrates_from_disk() {
        let mut vault = VaultManager::new(make_manifest(), MemoryAdapter::new());
        let mut obj = make_object("re-open");
        obj.name = "Reopened".into();
        vault
            .open_collection("tasks")
            .unwrap()
            .put_object(&obj)
            .unwrap();
        vault.close_collection("tasks").unwrap();

        let store2 = vault.open_collection("tasks").unwrap();
        assert_eq!(
            store2.get_object(&object_id("re-open")).unwrap().name,
            "Reopened"
        );
    }

    // ── Peer id ───────────────────────────────────────────────────

    #[test]
    fn passes_peer_id_to_collection_stores() {
        let mut vault = VaultManager::with_options(
            make_manifest(),
            MemoryAdapter::new(),
            VaultManagerOptions { peer_id: Some(42) },
        );
        let store = vault.open_collection("tasks").unwrap();
        // Smoke test: peer id makes it to the underlying Loro doc.
        assert_eq!(store.doc().peer_id(), 42);
    }
}
