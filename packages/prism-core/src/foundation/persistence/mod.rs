//! `persistence` — Loro-backed collection stores and vault-level
//! orchestration.
//!
//! Port of `foundation/persistence/` from the legacy TS tree (commit
//! `8426588`): `collection-store.ts` plus `vault-persistence.ts`.
//! Gated behind the `crdt` feature because it takes a hard dependency
//! on the `loro` Rust crate — pure-logic consumers of `prism-core`
//! stay on the default feature set and don't drag Loro in.
//!
//! Two layers:
//!
//! - [`CollectionStore`] wraps a single `LoroDoc` holding two top-level
//!   maps (`objects` and `edges`) and exposes CRUD + filter + snapshot
//!   import/export for the `GraphObject` / `ObjectEdge` world. The
//!   documents are stored as JSON strings inside the Loro maps exactly
//!   like the TS port — keeping the wire format stable across
//!   languages and letting us round-trip snapshots between the old
//!   JS runtime and the new Rust runtime during the migration.
//! - [`VaultManager`] orchestrates a `PrismManifest`'s collections
//!   against a [`PersistenceAdapter`]. It lazy-loads stores on first
//!   open, tracks dirty state, and ships snapshots to disk on demand.
//!   [`MemoryAdapter`] is included for tests and ephemeral vaults;
//!   host crates plug in real filesystem / IPC adapters.
//!
//! Unlike the TS port, dirty tracking does **not** go through a
//! `LoroDoc::subscribe_root` callback. Loro's Rust event delivery is
//! more nuanced than the JS binding (subscribers need `Send + Sync`
//! and events fire out of commit scope), and the tests the TS port
//! shipped explicitly tolerate async event delivery. To keep the
//! `isDirty` path strictly synchronous and the `CollectionStore`
//! surface `!Send` / `!Sync` by default, we mark the store dirty
//! *directly* inside `put_*` / `remove_*` and notify registered
//! change listeners from the same call site.

mod collection_store;
mod fs_adapter;
mod vault_manager;

pub use collection_store::{
    CollectionChange, CollectionChangeKind, CollectionStore, CollectionStoreOptions, EdgeFilter,
    ObjectFilter, PersistenceError, Subscription,
};
pub use fs_adapter::FileSystemAdapter;
pub use vault_manager::{MemoryAdapter, PersistenceAdapter, VaultManager, VaultManagerOptions};
