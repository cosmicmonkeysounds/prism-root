//! `vfs` — content-addressed virtual file system.
//!
//! Port of `foundation/vfs/` from the legacy `@prism/core` TS
//! package. Binary assets (images, video, audio, PDFs, …) live in
//! a SHA-256-keyed blob store decoupled from the Loro CRDT
//! document. Nodes in the object graph reference them via a
//! [`BinaryRef`] stored in `GraphObject.data`.
//!
//! The module exposes:
//! * [`VfsAdapter`] — pluggable storage backend trait.
//! * [`MemoryVfsAdapter`] / [`create_memory_vfs_adapter`] — in-memory
//!   implementation used for tests and early bring-up.
//! * [`VfsManager`] — lock-aware manager implementing the Binary
//!   Forking Protocol on top of any `VfsAdapter`.
//! * [`compute_binary_hash`] — standalone SHA-256 helper.

pub mod store;
pub mod types;

pub use store::{compute_binary_hash, create_memory_vfs_adapter, MemoryVfsAdapter, VfsManager};
pub use types::{
    BinaryLock, BinaryRef, FileStat, VfsAdapter, VfsError, VfsManagerOptions, VfsResult,
};
