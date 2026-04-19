//! Virtual File System types — port of
//! `foundation/vfs/vfs-types.ts`.
//!
//! Decouples the object-graph (text/CRDTs) from heavy binary assets.
//! Binary blobs are content-addressed by SHA-256 hash and stored
//! separately from the Loro CRDT document. Non-mergeable files use
//! lock-based editing via the Binary Forking Protocol.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;

// ── Content Addressing ─────────────────────────────────────────────

/// Content-addressed reference to a binary blob.
/// Stored in `GraphObject.data` to link CRDT nodes to binary assets.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BinaryRef {
    /// SHA-256 hash of the blob content (hex-encoded).
    pub hash: String,
    /// Original filename (for display/export).
    pub filename: String,
    /// MIME type.
    pub mime_type: String,
    /// Size in bytes.
    pub size: usize,
    /// Timestamp when the blob was imported.
    pub imported_at: DateTime<Utc>,
}

// ── File Stat ──────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileStat {
    /// Content-addressed hash.
    pub hash: String,
    /// Size in bytes.
    pub size: usize,
    /// MIME type.
    pub mime_type: String,
    /// Creation timestamp.
    pub created_at: DateTime<Utc>,
    /// Last-modified timestamp.
    pub modified_at: DateTime<Utc>,
}

// ── Binary Forking Protocol (Locking) ──────────────────────────────

/// Lock state for non-mergeable binary files.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BinaryLock {
    /// Hash of the locked blob.
    pub hash: String,
    /// DID or peer ID of the lock holder.
    pub locked_by: String,
    /// Timestamp when the lock was acquired.
    pub locked_at: DateTime<Utc>,
    /// Optional reason/description.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub reason: Option<String>,
}

// ── Errors ─────────────────────────────────────────────────────────

/// Errors returned from [`crate::foundation::vfs::VfsManager`] and
/// implementors of [`VfsAdapter`]. The variants mirror the legacy
/// JS `throw new Error("...")` cases so parity tests can match on
/// the error kind rather than free-form strings.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum VfsError {
    #[error("Blob {hash} is already locked by {locked_by}")]
    AlreadyLocked { hash: String, locked_by: String },

    #[error("Blob {hash} is not locked")]
    NotLocked { hash: String },

    #[error("Blob {hash} is locked by {locked_by}, not {requester}")]
    WrongLockHolder {
        hash: String,
        locked_by: String,
        requester: String,
    },

    #[error("Cannot remove locked blob: {hash}")]
    RemoveWhileLocked { hash: String },

    #[error("VFS adapter error: {0}")]
    Adapter(String),
}

pub type VfsResult<T> = Result<T, VfsError>;

// ── VFS Adapter ────────────────────────────────────────────────────

/// Abstract file I/O interface. Unlike the TS original this is a
/// synchronous trait — the in-memory implementation has no real I/O
/// and any future networked backend can wrap its own executor. When
/// a host binds an async filesystem it can wrap these calls in
/// `spawn_blocking` at the boundary.
///
/// Implementations:
/// - [`crate::foundation::vfs::MemoryVfsAdapter`] — in-memory for testing
/// - Local filesystem via daemon IPC — future
pub trait VfsAdapter: Send + Sync {
    /// Read a blob by its content hash. Returns `None` if not found.
    fn read(&self, hash: &str) -> Option<Vec<u8>>;

    /// Write a blob. Returns the content-addressed hash.
    fn write(&self, data: &[u8], mime_type: &str) -> String;

    /// Get file metadata by hash. Returns `None` if not found.
    fn stat(&self, hash: &str) -> Option<FileStat>;

    /// List all stored blob hashes.
    fn list(&self) -> Vec<String>;

    /// Delete a blob by hash. Returns `true` if it existed.
    fn delete(&self, hash: &str) -> bool;

    /// Check if a blob exists.
    fn has(&self, hash: &str) -> bool;

    /// Total number of stored blobs.
    fn count(&self) -> usize;

    /// Total size of all stored blobs in bytes.
    fn total_size(&self) -> usize;
}

// ── Manager options ────────────────────────────────────────────────

/// Construction options for [`crate::foundation::vfs::VfsManager`].
#[derive(Default)]
pub struct VfsManagerOptions {
    /// Storage adapter. Defaults to in-memory.
    pub adapter: Option<Box<dyn VfsAdapter>>,
}
