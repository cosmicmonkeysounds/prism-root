//! `foundation` — pure data primitives with no external concerns.
//!
//! Port of `packages/prism-core/src/foundation/*` from the legacy
//! TS tree. Ordered leaf-first per the Phase 2 plan in
//! `docs/dev/slint-migration-plan.md`.

pub mod batch;
pub mod clipboard;
pub mod date;
pub mod object_model;
#[cfg(feature = "crdt")]
pub mod persistence;
pub mod template;
pub mod undo;
pub mod vfs;
