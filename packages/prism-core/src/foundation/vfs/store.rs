//! Virtual File System store — port of `foundation/vfs/vfs.ts`.
//!
//! Content-addressed blob storage decoupled from the Loro CRDT
//! document. Binary assets (images, video, audio, PDFs, etc.) are
//! stored by SHA-256 hash and referenced from `GraphObject.data` via
//! [`BinaryRef`].
//!
//! Features:
//!   - Content-addressed deduplication (same content = same hash = one blob)
//!   - [`VfsAdapter`] abstraction for pluggable storage backends
//!   - Binary Forking Protocol: lock-based editing for non-mergeable files
//!   - `import_file` / `export_file` for moving binaries in/out of
//!     vault storage
//!   - In-memory adapter for testing; local FS adapter via Tauri IPC
//!     (future)

use std::sync::Mutex;

use chrono::Utc;
use indexmap::IndexMap;
use sha2::{Digest, Sha256};

use super::types::{
    BinaryLock, BinaryRef, FileStat, VfsAdapter, VfsError, VfsManagerOptions, VfsResult,
};

// ── SHA-256 hashing ────────────────────────────────────────────────

/// Compute SHA-256 hash of `data` as a lower-case hex string.
fn sha256_hex(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hex::encode(hasher.finalize())
}

/// Compute SHA-256 hash of `data` without storing it. Useful for
/// checking if a blob already exists before importing.
pub fn compute_binary_hash(data: &[u8]) -> String {
    sha256_hex(data)
}

// ── Memory VFS Adapter ─────────────────────────────────────────────

#[derive(Debug, Clone)]
struct StoredBlob {
    data: Vec<u8>,
    stat: FileStat,
}

/// In-memory [`VfsAdapter`] for testing. All blobs live in an
/// `IndexMap` protected by a `Mutex` so the adapter is `Send + Sync`
/// and can be shared behind a `Box<dyn VfsAdapter>` regardless of
/// whether the caller actually needs interior mutability.
#[derive(Debug, Default)]
pub struct MemoryVfsAdapter {
    blobs: Mutex<IndexMap<String, StoredBlob>>,
}

impl MemoryVfsAdapter {
    pub fn new() -> Self {
        Self::default()
    }
}

impl VfsAdapter for MemoryVfsAdapter {
    fn read(&self, hash: &str) -> Option<Vec<u8>> {
        let blobs = self.blobs.lock().expect("vfs mutex poisoned");
        blobs.get(hash).map(|b| b.data.clone())
    }

    fn write(&self, data: &[u8], mime_type: &str) -> String {
        let hash = sha256_hex(data);

        let mut blobs = self.blobs.lock().expect("vfs mutex poisoned");
        if !blobs.contains_key(&hash) {
            let now = Utc::now();
            blobs.insert(
                hash.clone(),
                StoredBlob {
                    // Defensive copy so later mutations of the
                    // caller's buffer don't reach into the store —
                    // matches the `new Uint8Array(data)` in the TS
                    // original.
                    data: data.to_vec(),
                    stat: FileStat {
                        hash: hash.clone(),
                        size: data.len(),
                        mime_type: mime_type.to_string(),
                        created_at: now,
                        modified_at: now,
                    },
                },
            );
        }

        hash
    }

    fn stat(&self, hash: &str) -> Option<FileStat> {
        let blobs = self.blobs.lock().expect("vfs mutex poisoned");
        blobs.get(hash).map(|b| b.stat.clone())
    }

    fn list(&self) -> Vec<String> {
        let blobs = self.blobs.lock().expect("vfs mutex poisoned");
        blobs.keys().cloned().collect()
    }

    fn delete(&self, hash: &str) -> bool {
        let mut blobs = self.blobs.lock().expect("vfs mutex poisoned");
        blobs.shift_remove(hash).is_some()
    }

    fn has(&self, hash: &str) -> bool {
        let blobs = self.blobs.lock().expect("vfs mutex poisoned");
        blobs.contains_key(hash)
    }

    fn count(&self) -> usize {
        let blobs = self.blobs.lock().expect("vfs mutex poisoned");
        blobs.len()
    }

    fn total_size(&self) -> usize {
        let blobs = self.blobs.lock().expect("vfs mutex poisoned");
        blobs.values().map(|b| b.stat.size).sum()
    }
}

/// Factory helper mirroring the TS `createMemoryVfsAdapter()`.
pub fn create_memory_vfs_adapter() -> Box<dyn VfsAdapter> {
    Box::new(MemoryVfsAdapter::new())
}

// ── VFS Manager ────────────────────────────────────────────────────

/// Binary asset manager. Owns a [`VfsAdapter`] and a lock table
/// implementing the Binary Forking Protocol.
///
/// Usage:
/// ```ignore
/// let vfs = VfsManager::new();
/// let r = vfs.import_file(png_bytes, "photo.png", "image/png");
/// // store `r` in GraphObject.data
/// let bytes = vfs.export_file(&r);
/// ```
pub struct VfsManager {
    adapter: Box<dyn VfsAdapter>,
    locks: Mutex<IndexMap<String, BinaryLock>>,
}

impl VfsManager {
    /// Create a manager with a fresh in-memory adapter.
    pub fn new() -> Self {
        Self::with_options(VfsManagerOptions::default())
    }

    /// Create a manager, optionally supplying a custom adapter.
    pub fn with_options(options: VfsManagerOptions) -> Self {
        let adapter = options
            .adapter
            .unwrap_or_else(|| Box::new(MemoryVfsAdapter::new()));
        Self {
            adapter,
            locks: Mutex::new(IndexMap::new()),
        }
    }

    /// Create a manager wrapping a caller-provided adapter. Handy
    /// for the common case where `VfsManagerOptions` would just be
    /// filled in with a single field.
    pub fn with_adapter(adapter: Box<dyn VfsAdapter>) -> Self {
        Self::with_options(VfsManagerOptions {
            adapter: Some(adapter),
        })
    }

    /// Access the underlying adapter for direct queries (counts,
    /// raw reads, etc.).
    pub fn adapter(&self) -> &dyn VfsAdapter {
        &*self.adapter
    }

    // ── Import / Export ────────────────────────────────────────────

    /// Import a file into vault storage. Returns a [`BinaryRef`]
    /// for storing in `GraphObject.data`. Deduplicates: if the
    /// content already exists (same hash), the returned ref still
    /// points at the existing blob.
    pub fn import_file(&self, data: &[u8], filename: &str, mime_type: &str) -> BinaryRef {
        let hash = self.adapter.write(data, mime_type);
        BinaryRef {
            hash,
            filename: filename.to_string(),
            mime_type: mime_type.to_string(),
            size: data.len(),
            imported_at: Utc::now(),
        }
    }

    /// Export a file from vault storage by its [`BinaryRef`].
    /// Returns the raw bytes, or `None` if not found.
    pub fn export_file(&self, reference: &BinaryRef) -> Option<Vec<u8>> {
        self.adapter.read(&reference.hash)
    }

    /// Remove a blob from storage. Returns `true` if it existed.
    /// Errors if the blob is locked.
    pub fn remove_file(&self, hash: &str) -> VfsResult<bool> {
        {
            let locks = self.locks.lock().expect("vfs locks poisoned");
            if locks.contains_key(hash) {
                return Err(VfsError::RemoveWhileLocked {
                    hash: hash.to_string(),
                });
            }
        }
        Ok(self.adapter.delete(hash))
    }

    // ── Locking (Binary Forking Protocol) ──────────────────────────

    /// Acquire an exclusive lock on a binary blob for editing.
    /// Non-mergeable files (images, video) must be locked before
    /// modification. Returns the existing lock if the same peer is
    /// re-acquiring. Errors if held by another peer.
    pub fn acquire_lock(
        &self,
        hash: &str,
        peer_id: &str,
        reason: Option<&str>,
    ) -> VfsResult<BinaryLock> {
        let mut locks = self.locks.lock().expect("vfs locks poisoned");
        if let Some(existing) = locks.get(hash) {
            if existing.locked_by == peer_id {
                return Ok(existing.clone());
            }
            return Err(VfsError::AlreadyLocked {
                hash: hash.to_string(),
                locked_by: existing.locked_by.clone(),
            });
        }

        let lock = BinaryLock {
            hash: hash.to_string(),
            locked_by: peer_id.to_string(),
            locked_at: Utc::now(),
            reason: reason.map(str::to_string),
        };
        locks.insert(hash.to_string(), lock.clone());
        Ok(lock)
    }

    /// Release a lock. Errors if not locked or locked by a
    /// different peer.
    pub fn release_lock(&self, hash: &str, peer_id: &str) -> VfsResult<()> {
        let mut locks = self.locks.lock().expect("vfs locks poisoned");
        let Some(existing) = locks.get(hash) else {
            return Err(VfsError::NotLocked {
                hash: hash.to_string(),
            });
        };
        if existing.locked_by != peer_id {
            return Err(VfsError::WrongLockHolder {
                hash: hash.to_string(),
                locked_by: existing.locked_by.clone(),
                requester: peer_id.to_string(),
            });
        }
        locks.shift_remove(hash);
        Ok(())
    }

    /// Get the current lock on a blob, or `None` if unlocked.
    pub fn get_lock(&self, hash: &str) -> Option<BinaryLock> {
        let locks = self.locks.lock().expect("vfs locks poisoned");
        locks.get(hash).cloned()
    }

    /// Check if a blob is currently locked.
    pub fn is_locked(&self, hash: &str) -> bool {
        let locks = self.locks.lock().expect("vfs locks poisoned");
        locks.contains_key(hash)
    }

    /// List all active locks.
    pub fn list_locks(&self) -> Vec<BinaryLock> {
        let locks = self.locks.lock().expect("vfs locks poisoned");
        locks.values().cloned().collect()
    }

    // ── Replace Locked File ────────────────────────────────────────

    /// Replace a locked blob with new content (the "fork" in the
    /// Binary Forking Protocol). Writes new content, moves the
    /// lock to the new hash, and returns a new [`BinaryRef`]. The
    /// old blob is kept (not deleted) for history/undo. Errors if
    /// the blob is not locked by `peer_id`.
    pub fn replace_locked_file(
        &self,
        old_hash: &str,
        new_data: &[u8],
        filename: &str,
        mime_type: &str,
        peer_id: &str,
    ) -> VfsResult<BinaryRef> {
        // Validate + read the existing lock under the lock guard,
        // but drop the guard before hitting the adapter so `write`
        // never sees a poisoned / held mutex.
        let existing = {
            let locks = self.locks.lock().expect("vfs locks poisoned");
            let Some(existing) = locks.get(old_hash) else {
                return Err(VfsError::NotLocked {
                    hash: old_hash.to_string(),
                });
            };
            if existing.locked_by != peer_id {
                return Err(VfsError::WrongLockHolder {
                    hash: old_hash.to_string(),
                    locked_by: existing.locked_by.clone(),
                    requester: peer_id.to_string(),
                });
            }
            existing.clone()
        };

        // Write new content (old blob is kept for history).
        let new_hash = self.adapter.write(new_data, mime_type);

        // Move the lock from old_hash -> new_hash, preserving the
        // original locked_at timestamp and reason.
        {
            let mut locks = self.locks.lock().expect("vfs locks poisoned");
            locks.shift_remove(old_hash);
            locks.insert(
                new_hash.clone(),
                BinaryLock {
                    hash: new_hash.clone(),
                    locked_by: peer_id.to_string(),
                    locked_at: existing.locked_at,
                    reason: existing.reason.clone(),
                },
            );
        }

        Ok(BinaryRef {
            hash: new_hash,
            filename: filename.to_string(),
            mime_type: mime_type.to_string(),
            size: new_data.len(),
            imported_at: Utc::now(),
        })
    }

    // ── Stat ───────────────────────────────────────────────────────

    /// Resolve a blob hash to its [`FileStat`].
    pub fn stat(&self, hash: &str) -> Option<FileStat> {
        self.adapter.stat(hash)
    }

    // ── Dispose ────────────────────────────────────────────────────

    /// Clear all locks. Does **not** delete stored blobs.
    pub fn dispose(&self) {
        let mut locks = self.locks.lock().expect("vfs locks poisoned");
        locks.clear();
    }
}

impl Default for VfsManager {
    fn default() -> Self {
        Self::new()
    }
}

// ── Tests ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── computeBinaryHash ──────────────────────────────────────────

    #[test]
    fn compute_binary_hash_returns_hex_sha256() {
        let hash = compute_binary_hash(b"hello");
        assert_eq!(
            hash,
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
        );
    }

    #[test]
    fn compute_binary_hash_same_content_same_hash() {
        assert_eq!(compute_binary_hash(b"data"), compute_binary_hash(b"data"));
    }

    #[test]
    fn compute_binary_hash_different_content_different_hash() {
        assert_ne!(compute_binary_hash(b"aaa"), compute_binary_hash(b"bbb"));
    }

    // ── MemoryVfsAdapter ───────────────────────────────────────────

    #[test]
    fn memory_adapter_writes_and_reads_blob() {
        let adapter = MemoryVfsAdapter::new();
        let data = b"image-data";
        let hash = adapter.write(data, "image/png");
        assert_eq!(hash.len(), 64); // SHA-256 hex
        let read = adapter.read(&hash).expect("blob should exist");
        assert_eq!(read, data);
    }

    #[test]
    fn memory_adapter_read_missing_blob_returns_none() {
        let adapter = MemoryVfsAdapter::new();
        assert!(adapter.read("nonexistent").is_none());
    }

    #[test]
    fn memory_adapter_deduplicates_identical_content() {
        let adapter = MemoryVfsAdapter::new();
        let data = b"same-content";
        let h1 = adapter.write(data, "text/plain");
        let h2 = adapter.write(data, "text/plain");
        assert_eq!(h1, h2);
        assert_eq!(adapter.count(), 1);
    }

    #[test]
    fn memory_adapter_stat_returns_metadata() {
        let adapter = MemoryVfsAdapter::new();
        let data = b"metadata-test";
        let hash = adapter.write(data, "application/pdf");
        let stat = adapter.stat(&hash).expect("stat should exist");
        assert_eq!(stat.hash, hash);
        assert_eq!(stat.size, data.len());
        assert_eq!(stat.mime_type, "application/pdf");
    }

    #[test]
    fn memory_adapter_stat_missing_returns_none() {
        let adapter = MemoryVfsAdapter::new();
        assert!(adapter.stat("missing").is_none());
    }

    #[test]
    fn memory_adapter_list_returns_all_hashes() {
        let adapter = MemoryVfsAdapter::new();
        let h1 = adapter.write(b"one", "text/plain");
        let h2 = adapter.write(b"two", "text/plain");
        let hashes = adapter.list();
        assert!(hashes.contains(&h1));
        assert!(hashes.contains(&h2));
        assert_eq!(hashes.len(), 2);
    }

    #[test]
    fn memory_adapter_delete_removes_blob() {
        let adapter = MemoryVfsAdapter::new();
        let hash = adapter.write(b"delete-me", "text/plain");
        assert!(adapter.delete(&hash));
        assert!(!adapter.has(&hash));
        assert!(adapter.read(&hash).is_none());
    }

    #[test]
    fn memory_adapter_delete_missing_returns_false() {
        let adapter = MemoryVfsAdapter::new();
        assert!(!adapter.delete("nope"));
    }

    #[test]
    fn memory_adapter_has_checks_existence() {
        let adapter = MemoryVfsAdapter::new();
        let hash = adapter.write(b"exists", "text/plain");
        assert!(adapter.has(&hash));
        assert!(!adapter.has("nope"));
    }

    #[test]
    fn memory_adapter_count_and_total_size_track_storage() {
        let adapter = MemoryVfsAdapter::new();
        assert_eq!(adapter.count(), 0);
        assert_eq!(adapter.total_size(), 0);

        adapter.write(b"1234567890", "text/plain"); // 10 bytes
        adapter.write(b"abcde", "text/plain"); // 5 bytes

        assert_eq!(adapter.count(), 2);
        assert_eq!(adapter.total_size(), 15);
    }

    #[test]
    fn memory_adapter_stores_defensive_copy() {
        let adapter = MemoryVfsAdapter::new();
        let mut data = b"original".to_vec();
        let hash = adapter.write(&data, "text/plain");

        // Mutate the caller's buffer — it must not reach the store.
        data[0] = 0xff;
        let read = adapter.read(&hash).expect("blob");
        assert_ne!(read[0], 0xff);
        assert_eq!(read, b"original");
    }

    // ── VfsManager: import/export ──────────────────────────────────

    #[test]
    fn vfs_import_returns_binary_ref() {
        let vfs = VfsManager::new();
        let data = b"photo-bytes";
        let reference = vfs.import_file(data, "photo.png", "image/png");

        assert_eq!(reference.hash.len(), 64);
        assert_eq!(reference.filename, "photo.png");
        assert_eq!(reference.mime_type, "image/png");
        assert_eq!(reference.size, data.len());
    }

    #[test]
    fn vfs_export_by_ref() {
        let vfs = VfsManager::new();
        let data = b"export-test";
        let reference = vfs.import_file(data, "doc.txt", "text/plain");
        let exported = vfs.export_file(&reference).expect("exported");
        assert_eq!(exported, data);
    }

    #[test]
    fn vfs_export_missing_returns_none() {
        let vfs = VfsManager::new();
        let fake = BinaryRef {
            hash: "0".repeat(64),
            filename: "missing.txt".into(),
            mime_type: "text/plain".into(),
            size: 0,
            imported_at: Utc::now(),
        };
        assert!(vfs.export_file(&fake).is_none());
    }

    #[test]
    fn vfs_import_deduplicates() {
        let vfs = VfsManager::new();
        let data = b"duplicate-content";
        let r1 = vfs.import_file(data, "a.bin", "application/octet-stream");
        let r2 = vfs.import_file(data, "b.bin", "application/octet-stream");

        assert_eq!(r1.hash, r2.hash);
        assert_eq!(r1.filename, "a.bin");
        assert_eq!(r2.filename, "b.bin");
        assert_eq!(vfs.adapter().count(), 1);
    }

    #[test]
    fn vfs_remove_file() {
        let vfs = VfsManager::new();
        let data = b"removable";
        let reference = vfs.import_file(data, "temp.bin", "application/octet-stream");
        assert!(vfs.remove_file(&reference.hash).unwrap());
        assert!(vfs.export_file(&reference).is_none());
    }

    #[test]
    fn vfs_stat_returns_file_info() {
        let vfs = VfsManager::new();
        let data = b"stat-test";
        let reference = vfs.import_file(data, "info.txt", "text/plain");
        let stat = vfs.stat(&reference.hash).expect("stat");
        assert_eq!(stat.hash, reference.hash);
        assert_eq!(stat.size, data.len());
        assert_eq!(stat.mime_type, "text/plain");
    }

    // ── VfsManager: Binary Forking Protocol (locking) ──────────────

    #[test]
    fn vfs_acquire_lock() {
        let vfs = VfsManager::new();
        let reference = vfs.import_file(b"lockable", "img.png", "image/png");

        let lock = vfs
            .acquire_lock(&reference.hash, "peer-alice", Some("editing"))
            .unwrap();
        assert_eq!(lock.hash, reference.hash);
        assert_eq!(lock.locked_by, "peer-alice");
        assert_eq!(lock.reason.as_deref(), Some("editing"));
    }

    #[test]
    fn vfs_is_locked_and_get_lock_reflect_state() {
        let vfs = VfsManager::new();
        let reference = vfs.import_file(b"x", "x.bin", "application/octet-stream");

        assert!(!vfs.is_locked(&reference.hash));
        assert!(vfs.get_lock(&reference.hash).is_none());

        vfs.acquire_lock(&reference.hash, "peer-bob", None).unwrap();

        assert!(vfs.is_locked(&reference.hash));
        let lock = vfs.get_lock(&reference.hash).expect("lock");
        assert_eq!(lock.locked_by, "peer-bob");
    }

    #[test]
    fn vfs_same_peer_can_reacquire_lock() {
        let vfs = VfsManager::new();
        let reference = vfs.import_file(b"mine", "m.bin", "application/octet-stream");

        vfs.acquire_lock(&reference.hash, "peer-alice", None)
            .unwrap();
        let lock2 = vfs
            .acquire_lock(&reference.hash, "peer-alice", None)
            .unwrap();
        assert_eq!(lock2.locked_by, "peer-alice");
    }

    #[test]
    fn vfs_other_peer_cannot_acquire_existing_lock() {
        let vfs = VfsManager::new();
        let reference = vfs.import_file(b"contested", "c.bin", "application/octet-stream");

        vfs.acquire_lock(&reference.hash, "peer-alice", None)
            .unwrap();
        let err = vfs
            .acquire_lock(&reference.hash, "peer-bob", None)
            .unwrap_err();
        assert!(matches!(
            err,
            VfsError::AlreadyLocked { ref locked_by, .. } if locked_by == "peer-alice"
        ));
    }

    #[test]
    fn vfs_release_lock() {
        let vfs = VfsManager::new();
        let reference = vfs.import_file(b"release", "r.bin", "application/octet-stream");

        vfs.acquire_lock(&reference.hash, "peer-alice", None)
            .unwrap();
        vfs.release_lock(&reference.hash, "peer-alice").unwrap();
        assert!(!vfs.is_locked(&reference.hash));
    }

    #[test]
    fn vfs_release_lock_wrong_peer() {
        let vfs = VfsManager::new();
        let reference = vfs.import_file(b"foreign", "f.bin", "application/octet-stream");

        vfs.acquire_lock(&reference.hash, "peer-alice", None)
            .unwrap();
        let err = vfs.release_lock(&reference.hash, "peer-bob").unwrap_err();
        assert!(matches!(
            err,
            VfsError::WrongLockHolder { ref locked_by, ref requester, .. }
                if locked_by == "peer-alice" && requester == "peer-bob"
        ));
    }

    #[test]
    fn vfs_release_unlocked_blob_errors() {
        let vfs = VfsManager::new();
        let err = vfs.release_lock("no-such-hash", "peer-alice").unwrap_err();
        assert!(matches!(err, VfsError::NotLocked { .. }));
    }

    #[test]
    fn vfs_cannot_remove_locked_blob() {
        let vfs = VfsManager::new();
        let reference = vfs.import_file(b"protected", "p.bin", "application/octet-stream");
        vfs.acquire_lock(&reference.hash, "peer-alice", None)
            .unwrap();
        let err = vfs.remove_file(&reference.hash).unwrap_err();
        assert!(matches!(err, VfsError::RemoveWhileLocked { .. }));
    }

    #[test]
    fn vfs_list_locks_returns_all_active() {
        let vfs = VfsManager::new();
        let r1 = vfs.import_file(b"a", "a.bin", "application/octet-stream");
        let r2 = vfs.import_file(b"b", "b.bin", "application/octet-stream");

        vfs.acquire_lock(&r1.hash, "peer-alice", None).unwrap();
        vfs.acquire_lock(&r2.hash, "peer-bob", None).unwrap();

        let locks = vfs.list_locks();
        assert_eq!(locks.len(), 2);
        let mut peers: Vec<String> = locks.iter().map(|l| l.locked_by.clone()).collect();
        peers.sort();
        assert_eq!(peers, vec!["peer-alice", "peer-bob"]);
    }

    // ── VfsManager: replace_locked_file ────────────────────────────

    #[test]
    fn vfs_replace_locked_file() {
        let vfs = VfsManager::new();
        let old_data = b"original-image";
        let reference = vfs.import_file(old_data, "img.png", "image/png");

        vfs.acquire_lock(&reference.hash, "peer-alice", None)
            .unwrap();

        let new_data = b"edited-image";
        let new_ref = vfs
            .replace_locked_file(
                &reference.hash,
                new_data,
                "img.png",
                "image/png",
                "peer-alice",
            )
            .unwrap();

        assert_ne!(new_ref.hash, reference.hash);
        assert_eq!(new_ref.size, new_data.len());

        // New content is readable
        let exported = vfs.export_file(&new_ref).expect("exported");
        assert_eq!(exported, new_data);

        // Old content is still preserved
        let old_exported = vfs.adapter().read(&reference.hash).expect("old");
        assert_eq!(old_exported, old_data);

        // Lock moved to new hash
        assert!(!vfs.is_locked(&reference.hash));
        assert!(vfs.is_locked(&new_ref.hash));
        let new_lock = vfs.get_lock(&new_ref.hash).expect("new lock");
        assert_eq!(new_lock.locked_by, "peer-alice");
    }

    #[test]
    fn vfs_replace_unlocked_blob_errors() {
        let vfs = VfsManager::new();
        let err = vfs
            .replace_locked_file("nope", b"x", "x.bin", "text/plain", "peer-alice")
            .unwrap_err();
        assert!(matches!(err, VfsError::NotLocked { .. }));
    }

    #[test]
    fn vfs_replace_wrong_peer_errors() {
        let vfs = VfsManager::new();
        let reference = vfs.import_file(b"guarded", "g.bin", "application/octet-stream");
        vfs.acquire_lock(&reference.hash, "peer-alice", None)
            .unwrap();

        let err = vfs
            .replace_locked_file(
                &reference.hash,
                b"hacked",
                "g.bin",
                "application/octet-stream",
                "peer-bob",
            )
            .unwrap_err();
        assert!(matches!(
            err,
            VfsError::WrongLockHolder { ref locked_by, ref requester, .. }
                if locked_by == "peer-alice" && requester == "peer-bob"
        ));
    }

    // ── VfsManager: dispose ────────────────────────────────────────

    #[test]
    fn vfs_dispose_clears_locks_but_keeps_blobs() {
        let vfs = VfsManager::new();
        let reference = vfs.import_file(b"persist", "p.bin", "application/octet-stream");
        vfs.acquire_lock(&reference.hash, "peer-alice", None)
            .unwrap();

        vfs.dispose();

        assert_eq!(vfs.list_locks().len(), 0);
        assert!(!vfs.is_locked(&reference.hash));
        // Blob is still there
        assert_eq!(
            vfs.export_file(&reference).as_deref(),
            Some(b"persist".as_ref())
        );
    }

    // ── VfsManager: custom adapter ─────────────────────────────────

    #[test]
    fn vfs_with_custom_adapter() {
        let vfs = VfsManager::with_adapter(Box::new(MemoryVfsAdapter::new()));
        let reference = vfs.import_file(b"custom", "c.txt", "text/plain");

        let from_adapter = vfs.adapter().read(&reference.hash).expect("from adapter");
        assert_eq!(from_adapter, b"custom");
    }
}
