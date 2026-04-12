//! VFS module — content-addressed blob storage behind six `vfs.*`
//! commands.
//!
//! SPEC motivation: the Virtual File System decouples the lightweight
//! CRDT graph from heavy binaries (video, 3D assets, audio). Files are
//! stored once, keyed by their SHA-256 content hash, and looked up by
//! that hash from inside the CRDT.
//!
//! | Command        | Payload                         | Result                       |
//! |----------------|---------------------------------|------------------------------|
//! | `vfs.put`      | `{ bytes: [u8] }`               | `{ hash, size }`             |
//! | `vfs.get`      | `{ hash }`                      | `{ bytes: [u8] }`            |
//! | `vfs.has`      | `{ hash }`                      | `{ present: bool, size? }`   |
//! | `vfs.delete`   | `{ hash }`                      | `{ deleted: bool }`          |
//! | `vfs.list`     | `{}`                            | `{ entries: [{hash,size}] }` |
//! | `vfs.stats`    | `{}`                            | `{ entries, total_bytes }`   |
//!
//! Hashes are lowercase hex-encoded SHA-256 digests (64 chars).
//!
//! ## Pluggable backends
//!
//! Storage is abstracted behind the [`VfsBackend`] trait. Three
//! backends ship in-tree:
//!
//! - [`LocalVfsBackend`]: pure `std::fs` under a root directory. The
//!   default — this is what the built-in module installs when a host
//!   hasn't injected anything else. Atomic write-temp + rename, same
//!   semantics as a single-writer Git object store.
//! - [`InMemoryVfsBackend`]: `HashMap`-backed, no persistence. Intended
//!   for tests and for ephemeral hosts (e.g. a Luau repl kernel).
//! - `S3VfsBackend` / `GcsVfsBackend`: remote content-addressed stores
//!   living behind the `vfs-s3` and `vfs-gcs` features. They plug into
//!   the exact same [`VfsBackend`] interface, so any host that already
//!   speaks the `vfs.*` command shape can swap its storage layer
//!   without touching its call sites. See [`s3`] for details.
//!
//! Hosts inject a pre-built [`VfsManager`] via
//! [`DaemonBuilder::set_vfs_manager`](crate::builder::DaemonBuilder::set_vfs_manager);
//! the module lazily creates a `LocalVfsBackend` under the OS temp
//! directory if no host plugged anything in.

use crate::builder::DaemonBuilder;
use crate::module::DaemonModule;
use crate::registry::CommandError;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value as JsonValue};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

#[cfg(any(feature = "vfs-s3", feature = "vfs-gcs"))]
pub mod s3;

#[cfg(feature = "vfs-s3")]
pub use s3::S3VfsBackend;

#[cfg(feature = "vfs-gcs")]
pub use s3::GcsVfsBackend;

// ── Backend trait ──────────────────────────────────────────────────────

/// Pluggable storage backend for the content-addressed blob store.
///
/// Implementations MUST be keyed strictly on the SHA-256 hex hash and
/// MUST make `put` idempotent (re-writing identical bytes is a no-op).
/// Concurrency: the façade [`VfsManager`] serializes writes across
/// threads with its own mutex, so backends only need to be `Send + Sync`,
/// not write-parallel-safe.
pub trait VfsBackend: Send + Sync {
    /// Write `bytes` under `hash`. If `hash` already exists the call is
    /// a successful no-op (content-addressed: same hash ⇒ same bytes).
    fn put(&self, hash: &str, bytes: &[u8]) -> Result<(), String>;

    /// Read a blob by hash.
    fn get(&self, hash: &str) -> Result<Vec<u8>, String>;

    /// Return `Some(size)` if the hash is present, else `None`.
    fn has(&self, hash: &str) -> Result<Option<u64>, String>;

    /// Remove a blob. Returns true if it was present, false if it
    /// wasn't (not an error).
    fn delete(&self, hash: &str) -> Result<bool, String>;

    /// Enumerate every blob in the store. Intended for dev tools and
    /// small stores — remote backends may choose to cap the result set.
    fn list(&self) -> Result<Vec<VfsEntry>, String>;

    /// Human-readable name for logs / diagnostics.
    fn backend_name(&self) -> &'static str;

    /// Downcast hook used by [`VfsManager::root`]. Backends that want
    /// their concrete type to be reachable from the façade override
    /// this to return `Some(self)`; everything else opts out.
    fn as_any(&self) -> Option<&dyn std::any::Any> {
        None
    }
}

// ── Façade ──────────────────────────────────────────────────────────────

/// Content-addressed blob store façade. All `vfs.*` commands flow
/// through here before dispatching to the injected [`VfsBackend`].
///
/// Constructing with [`VfsManager::new`] keeps the pre-refactor API:
/// you get a local-filesystem-backed manager rooted at the given path.
/// Pass [`VfsManager::with_backend`] any other implementation (S3,
/// GCS, in-memory, a custom cache-over-S3 wrapper you wrote) to swap
/// storage engines without touching call sites.
pub struct VfsManager {
    backend: Arc<dyn VfsBackend>,
    /// Serializes writes across threads so two concurrent writes can't
    /// race on whatever the backend's atomic-write primitive is. Reads
    /// are lock-free.
    write_lock: Mutex<()>,
}

impl VfsManager {
    /// Create a manager backed by the on-disk [`LocalVfsBackend`] under
    /// `root`. Same semantics as the pre-refactor constructor.
    pub fn new(root: impl Into<PathBuf>) -> Result<Self, String> {
        let backend = LocalVfsBackend::new(root)?;
        Ok(Self::with_backend(Arc::new(backend)))
    }

    /// Create a manager backed by an arbitrary implementation of
    /// [`VfsBackend`]. This is how remote backends (S3, GCS) and test
    /// doubles (in-memory) are plugged in.
    pub fn with_backend(backend: Arc<dyn VfsBackend>) -> Self {
        Self {
            backend,
            write_lock: Mutex::new(()),
        }
    }

    /// Borrow the underlying backend. Rarely needed at call sites but
    /// handy for the built-in `backend_name()` introspection exposed by
    /// `vfs.stats`.
    pub fn backend(&self) -> &Arc<dyn VfsBackend> {
        &self.backend
    }

    /// For the on-disk backend, return the root directory. Returns
    /// `None` for any other backend. Kept as a convenience because the
    /// Tauri shell used to call `.root()` directly on the manager.
    pub fn root(&self) -> Option<PathBuf> {
        self.backend
            .as_any()
            .and_then(|any| any.downcast_ref::<LocalVfsBackend>())
            .map(|b| b.root.clone())
    }

    /// SHA-256 hex digest of `bytes`. Exposed because hosts may want to
    /// compute a hash without writing the blob yet (e.g. for dedupe).
    pub fn hash(bytes: &[u8]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(bytes);
        hex::encode(hasher.finalize())
    }

    /// Write `bytes`, returning the content hash.
    pub fn put(&self, bytes: &[u8]) -> Result<String, String> {
        let hash = Self::hash(bytes);
        let _guard = self
            .write_lock
            .lock()
            .map_err(|_| "vfs write lock poisoned".to_string())?;
        if self.backend.has(&hash)?.is_some() {
            return Ok(hash);
        }
        self.backend.put(&hash, bytes)?;
        Ok(hash)
    }

    /// Read a blob by hash.
    pub fn get(&self, hash: &str) -> Result<Vec<u8>, String> {
        validate_hash(hash)?;
        self.backend.get(hash)
    }

    /// Check whether `hash` is present.
    pub fn has(&self, hash: &str) -> Result<Option<u64>, String> {
        validate_hash(hash)?;
        self.backend.has(hash)
    }

    /// Remove a blob by hash. Returns false if it didn't exist.
    pub fn delete(&self, hash: &str) -> Result<bool, String> {
        validate_hash(hash)?;
        let _guard = self
            .write_lock
            .lock()
            .map_err(|_| "vfs write lock poisoned".to_string())?;
        self.backend.delete(hash)
    }

    /// Enumerate every blob in the store.
    pub fn list(&self) -> Result<Vec<VfsEntry>, String> {
        let mut entries = self.backend.list()?;
        entries.sort_by(|a, b| a.hash.cmp(&b.hash));
        Ok(entries)
    }

    /// Aggregate statistics — entry count + total bytes.
    pub fn stats(&self) -> Result<VfsStats, String> {
        let entries = self.list()?;
        let total_bytes: u64 = entries.iter().map(|e| e.size).sum();
        Ok(VfsStats {
            entries: entries.len() as u64,
            total_bytes,
        })
    }
}

// ── LocalVfsBackend ────────────────────────────────────────────────────

/// On-disk blob store. Pre-refactor, this was the entire module.
pub struct LocalVfsBackend {
    root: PathBuf,
}

impl LocalVfsBackend {
    pub fn new(root: impl Into<PathBuf>) -> Result<Self, String> {
        let root = root.into();
        fs::create_dir_all(&root)
            .map_err(|e| format!("failed to create vfs root {}: {e}", root.display()))?;
        Ok(Self { root })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn path_for(&self, hash: &str) -> PathBuf {
        self.root.join(hash)
    }
}

impl VfsBackend for LocalVfsBackend {
    fn put(&self, hash: &str, bytes: &[u8]) -> Result<(), String> {
        let final_path = self.path_for(hash);
        // Atomic write: temp file in the same dir, then rename. Same
        // dir is important so the rename never crosses a filesystem.
        let tmp = final_path.with_extension("tmp");
        fs::write(&tmp, bytes)
            .map_err(|e| format!("failed to write temp {}: {e}", tmp.display()))?;
        fs::rename(&tmp, &final_path)
            .map_err(|e| format!("failed to rename temp into place: {e}"))?;
        Ok(())
    }

    fn get(&self, hash: &str) -> Result<Vec<u8>, String> {
        let path = self.path_for(hash);
        fs::read(&path).map_err(|e| format!("failed to read {}: {e}", path.display()))
    }

    fn has(&self, hash: &str) -> Result<Option<u64>, String> {
        let path = self.path_for(hash);
        match fs::metadata(&path) {
            Ok(meta) if meta.is_file() => Ok(Some(meta.len())),
            Ok(_) => Ok(None),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(format!("failed to stat {}: {e}", path.display())),
        }
    }

    fn delete(&self, hash: &str) -> Result<bool, String> {
        let path = self.path_for(hash);
        match fs::remove_file(&path) {
            Ok(()) => Ok(true),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(e) => Err(format!("failed to delete {}: {e}", path.display())),
        }
    }

    fn list(&self) -> Result<Vec<VfsEntry>, String> {
        let iter = fs::read_dir(&self.root)
            .map_err(|e| format!("failed to read vfs root {}: {e}", self.root.display()))?;
        let mut entries = Vec::new();
        for entry in iter {
            let entry = entry.map_err(|e| format!("vfs read_dir entry: {e}"))?;
            let path = entry.path();
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            if name.ends_with(".tmp") || !looks_like_hash(name) {
                continue;
            }
            let meta = fs::metadata(&path)
                .map_err(|e| format!("failed to stat {}: {e}", path.display()))?;
            if meta.is_file() {
                entries.push(VfsEntry {
                    hash: name.to_string(),
                    size: meta.len(),
                });
            }
        }
        Ok(entries)
    }

    fn backend_name(&self) -> &'static str {
        "local"
    }

    fn as_any(&self) -> Option<&dyn std::any::Any> {
        Some(self)
    }
}

// ── InMemoryVfsBackend ─────────────────────────────────────────────────

/// Pure in-memory backend. Useful for tests, ephemeral kernels, and
/// as a reference implementation.
pub struct InMemoryVfsBackend {
    inner: Mutex<HashMap<String, Vec<u8>>>,
}

impl Default for InMemoryVfsBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl InMemoryVfsBackend {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(HashMap::new()),
        }
    }
}

impl VfsBackend for InMemoryVfsBackend {
    fn put(&self, hash: &str, bytes: &[u8]) -> Result<(), String> {
        self.inner
            .lock()
            .map_err(|_| "in-memory vfs poisoned".to_string())?
            .insert(hash.to_string(), bytes.to_vec());
        Ok(())
    }

    fn get(&self, hash: &str) -> Result<Vec<u8>, String> {
        self.inner
            .lock()
            .map_err(|_| "in-memory vfs poisoned".to_string())?
            .get(hash)
            .cloned()
            .ok_or_else(|| format!("hash not found: {hash}"))
    }

    fn has(&self, hash: &str) -> Result<Option<u64>, String> {
        Ok(self
            .inner
            .lock()
            .map_err(|_| "in-memory vfs poisoned".to_string())?
            .get(hash)
            .map(|b| b.len() as u64))
    }

    fn delete(&self, hash: &str) -> Result<bool, String> {
        Ok(self
            .inner
            .lock()
            .map_err(|_| "in-memory vfs poisoned".to_string())?
            .remove(hash)
            .is_some())
    }

    fn list(&self) -> Result<Vec<VfsEntry>, String> {
        let guard = self
            .inner
            .lock()
            .map_err(|_| "in-memory vfs poisoned".to_string())?;
        Ok(guard
            .iter()
            .map(|(hash, bytes)| VfsEntry {
                hash: hash.clone(),
                size: bytes.len() as u64,
            })
            .collect())
    }

    fn backend_name(&self) -> &'static str {
        "in-memory"
    }
}

// ── Wire types ─────────────────────────────────────────────────────────

/// One entry in a `vfs.list` result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VfsEntry {
    pub hash: String,
    pub size: u64,
}

/// Aggregate store stats.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VfsStats {
    pub entries: u64,
    pub total_bytes: u64,
}

fn validate_hash(hash: &str) -> Result<(), String> {
    if !looks_like_hash(hash) {
        return Err(format!(
            "invalid vfs hash {:?}: expected 64 lowercase hex characters",
            hash
        ));
    }
    Ok(())
}

fn looks_like_hash(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}

// ── Module wiring ──────────────────────────────────────────────────────

/// The built-in VFS module. Stateless — the state lives on the shared
/// [`VfsManager`] stashed on the builder (or lazily created on install
/// if no host injected one).
pub struct VfsModule;

impl DaemonModule for VfsModule {
    fn id(&self) -> &str {
        "prism.vfs"
    }

    fn install(&self, builder: &mut DaemonBuilder) -> Result<(), CommandError> {
        let manager = builder
            .vfs_manager_slot()
            .get_or_insert_with(|| {
                let root = std::env::temp_dir().join("prism-daemon-vfs");
                // A failure here would only happen if the temp dir is
                // un-writable, which we can't recover from anyway.
                Arc::new(VfsManager::new(root).expect("default vfs root must be writable"))
            })
            .clone();
        let registry = builder.registry().clone();

        let m = manager.clone();
        registry.register("vfs.put", move |payload| {
            let args: PutArgs = parse(payload, "vfs.put")?;
            let hash = m
                .put(&args.bytes)
                .map_err(|e| CommandError::handler("vfs.put", e))?;
            Ok(json!({ "hash": hash, "size": args.bytes.len() }))
        })?;

        let m = manager.clone();
        registry.register("vfs.get", move |payload| {
            let args: HashArgs = parse(payload, "vfs.get")?;
            let bytes = m
                .get(&args.hash)
                .map_err(|e| CommandError::handler("vfs.get", e))?;
            Ok(json!({ "bytes": bytes }))
        })?;

        let m = manager.clone();
        registry.register("vfs.has", move |payload| {
            let args: HashArgs = parse(payload, "vfs.has")?;
            match m
                .has(&args.hash)
                .map_err(|e| CommandError::handler("vfs.has", e))?
            {
                Some(size) => Ok(json!({ "present": true, "size": size })),
                None => Ok(json!({ "present": false })),
            }
        })?;

        let m = manager.clone();
        registry.register("vfs.delete", move |payload| {
            let args: HashArgs = parse(payload, "vfs.delete")?;
            let deleted = m
                .delete(&args.hash)
                .map_err(|e| CommandError::handler("vfs.delete", e))?;
            Ok(json!({ "deleted": deleted }))
        })?;

        let m = manager.clone();
        registry.register("vfs.list", move |_payload| {
            let entries = m.list().map_err(|e| CommandError::handler("vfs.list", e))?;
            Ok(json!({ "entries": entries }))
        })?;

        let m = manager;
        registry.register("vfs.stats", move |_payload| {
            let stats = m
                .stats()
                .map_err(|e| CommandError::handler("vfs.stats", e))?;
            let mut value = serde_json::to_value(stats).unwrap_or(JsonValue::Null);
            if let Some(obj) = value.as_object_mut() {
                obj.insert(
                    "backend".to_string(),
                    JsonValue::String(m.backend().backend_name().to_string()),
                );
            }
            Ok(value)
        })?;

        Ok(())
    }
}

fn parse<T: for<'de> Deserialize<'de>>(
    payload: JsonValue,
    command: &str,
) -> Result<T, CommandError> {
    serde_json::from_value::<T>(payload)
        .map_err(|e| CommandError::handler(command.to_string(), e.to_string()))
}

#[derive(Debug, Deserialize)]
struct PutArgs {
    bytes: Vec<u8>,
}

#[derive(Debug, Deserialize)]
struct HashArgs {
    hash: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builder::DaemonBuilder;
    use tempfile::tempdir;

    fn kernel_with_tmp_vfs() -> (crate::DaemonKernel, tempfile::TempDir) {
        let dir = tempdir().unwrap();
        let manager = Arc::new(VfsManager::new(dir.path()).unwrap());
        let mut builder = DaemonBuilder::new();
        *builder.vfs_manager_slot() = Some(manager);
        let kernel = builder.with_module(VfsModule).build().unwrap();
        (kernel, dir)
    }

    #[test]
    fn vfs_module_registers_six_commands() {
        let (kernel, _dir) = kernel_with_tmp_vfs();
        let caps = kernel.capabilities();
        for name in [
            "vfs.put",
            "vfs.get",
            "vfs.has",
            "vfs.delete",
            "vfs.list",
            "vfs.stats",
        ] {
            assert!(caps.contains(&name.to_string()), "missing {name}");
        }
    }

    #[test]
    fn put_hashes_bytes_and_allows_roundtrip_via_get() {
        let (kernel, _dir) = kernel_with_tmp_vfs();
        let payload = b"hello prism vfs".to_vec();

        let put = kernel
            .invoke("vfs.put", json!({ "bytes": payload }))
            .unwrap();
        let hash = put["hash"].as_str().unwrap().to_string();
        assert_eq!(hash.len(), 64);
        assert_eq!(put["size"], 15);

        let got = kernel.invoke("vfs.get", json!({ "hash": hash })).unwrap();
        let bytes: Vec<u8> = serde_json::from_value(got["bytes"].clone()).unwrap();
        assert_eq!(bytes, payload);
    }

    #[test]
    fn put_is_deterministic_for_identical_bytes() {
        let (kernel, _dir) = kernel_with_tmp_vfs();
        let payload = b"same bytes".to_vec();

        let a = kernel
            .invoke("vfs.put", json!({ "bytes": payload.clone() }))
            .unwrap();
        let b = kernel
            .invoke("vfs.put", json!({ "bytes": payload }))
            .unwrap();
        assert_eq!(a["hash"], b["hash"]);
    }

    #[test]
    fn has_reports_size_for_existing_entries() {
        let (kernel, _dir) = kernel_with_tmp_vfs();
        let put = kernel
            .invoke("vfs.put", json!({ "bytes": vec![1u8, 2, 3, 4] }))
            .unwrap();
        let hash = put["hash"].as_str().unwrap().to_string();

        let has = kernel.invoke("vfs.has", json!({ "hash": hash })).unwrap();
        assert_eq!(has["present"], true);
        assert_eq!(has["size"], 4);
    }

    #[test]
    fn has_returns_present_false_for_missing_hash() {
        let (kernel, _dir) = kernel_with_tmp_vfs();
        let hash = "a".repeat(64);
        let has = kernel.invoke("vfs.has", json!({ "hash": hash })).unwrap();
        assert_eq!(has["present"], false);
    }

    #[test]
    fn get_errors_on_unknown_hash() {
        let (kernel, _dir) = kernel_with_tmp_vfs();
        let hash = "b".repeat(64);
        let err = kernel
            .invoke("vfs.get", json!({ "hash": hash }))
            .unwrap_err();
        matches!(err, CommandError::Handler { .. });
    }

    #[test]
    fn delete_removes_the_entry() {
        let (kernel, _dir) = kernel_with_tmp_vfs();
        let put = kernel
            .invoke("vfs.put", json!({ "bytes": vec![9u8, 9, 9] }))
            .unwrap();
        let hash = put["hash"].as_str().unwrap().to_string();

        let del = kernel
            .invoke("vfs.delete", json!({ "hash": hash.clone() }))
            .unwrap();
        assert_eq!(del["deleted"], true);

        let has = kernel.invoke("vfs.has", json!({ "hash": hash })).unwrap();
        assert_eq!(has["present"], false);
    }

    #[test]
    fn delete_returns_false_for_missing_entry() {
        let (kernel, _dir) = kernel_with_tmp_vfs();
        let hash = "c".repeat(64);
        let out = kernel
            .invoke("vfs.delete", json!({ "hash": hash }))
            .unwrap();
        assert_eq!(out["deleted"], false);
    }

    #[test]
    fn list_enumerates_every_written_entry() {
        let (kernel, _dir) = kernel_with_tmp_vfs();
        kernel
            .invoke("vfs.put", json!({ "bytes": vec![1u8] }))
            .unwrap();
        kernel
            .invoke("vfs.put", json!({ "bytes": vec![1u8, 2] }))
            .unwrap();
        kernel
            .invoke("vfs.put", json!({ "bytes": vec![1u8, 2, 3] }))
            .unwrap();

        let out = kernel.invoke("vfs.list", json!({})).unwrap();
        let entries = out["entries"].as_array().unwrap();
        assert_eq!(entries.len(), 3);
    }

    #[test]
    fn stats_sum_sizes_across_entries_and_report_backend() {
        let (kernel, _dir) = kernel_with_tmp_vfs();
        kernel
            .invoke("vfs.put", json!({ "bytes": vec![0u8; 10] }))
            .unwrap();
        kernel
            .invoke("vfs.put", json!({ "bytes": vec![1u8; 15] }))
            .unwrap();

        let stats = kernel.invoke("vfs.stats", json!({})).unwrap();
        assert_eq!(stats["entries"], 2);
        assert_eq!(stats["total_bytes"], 25);
        assert_eq!(stats["backend"], "local");
    }

    #[test]
    fn invalid_hash_is_rejected_structurally() {
        let (kernel, _dir) = kernel_with_tmp_vfs();
        let err = kernel
            .invoke("vfs.get", json!({ "hash": "not-a-hash" }))
            .unwrap_err();
        if let CommandError::Handler { message, .. } = err {
            assert!(message.contains("invalid vfs hash"));
        } else {
            panic!("wrong error variant");
        }
    }

    #[test]
    fn pure_manager_api_matches_command_api() {
        let dir = tempdir().unwrap();
        let mgr = VfsManager::new(dir.path()).unwrap();
        let hash = mgr.put(b"direct").unwrap();
        assert_eq!(mgr.has(&hash).unwrap(), Some(6));
        assert_eq!(mgr.get(&hash).unwrap(), b"direct");
        assert!(mgr.delete(&hash).unwrap());
        assert_eq!(mgr.has(&hash).unwrap(), None);
    }

    #[test]
    fn default_manager_works_when_host_did_not_inject_one() {
        let kernel = DaemonBuilder::new().with_module(VfsModule).build().unwrap();
        let put = kernel
            .invoke("vfs.put", json!({ "bytes": vec![42u8, 43] }))
            .unwrap();
        let hash = put["hash"].as_str().unwrap().to_string();
        let got = kernel
            .invoke("vfs.get", json!({ "hash": hash.clone() }))
            .unwrap();
        let bytes: Vec<u8> = serde_json::from_value(got["bytes"].clone()).unwrap();
        assert_eq!(bytes, vec![42u8, 43]);
        kernel
            .invoke("vfs.delete", json!({ "hash": hash }))
            .unwrap();
    }

    // ── Backend abstraction tests ───────────────────────────────────

    #[test]
    fn in_memory_backend_roundtrip() {
        let mgr = VfsManager::with_backend(Arc::new(InMemoryVfsBackend::new()));
        let hash = mgr.put(b"hello memory").unwrap();
        assert_eq!(mgr.backend().backend_name(), "in-memory");
        assert_eq!(mgr.has(&hash).unwrap(), Some(12));
        assert_eq!(mgr.get(&hash).unwrap(), b"hello memory");
        let entries = mgr.list().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].hash, hash);
        assert!(mgr.delete(&hash).unwrap());
        assert_eq!(mgr.has(&hash).unwrap(), None);
    }

    #[test]
    fn in_memory_backend_via_vfs_module() {
        let mut builder = DaemonBuilder::new();
        *builder.vfs_manager_slot() = Some(Arc::new(VfsManager::with_backend(Arc::new(
            InMemoryVfsBackend::new(),
        ))));
        let kernel = builder.with_vfs().build().unwrap();

        let put = kernel
            .invoke("vfs.put", json!({ "bytes": b"memory-backed".to_vec() }))
            .unwrap();
        let hash = put["hash"].as_str().unwrap().to_string();
        let stats = kernel.invoke("vfs.stats", json!({})).unwrap();
        assert_eq!(stats["backend"], "in-memory");
        assert_eq!(stats["entries"], 1);
        assert_eq!(stats["total_bytes"], 13);
        let _ = kernel.invoke("vfs.delete", json!({ "hash": hash }));
    }
}
