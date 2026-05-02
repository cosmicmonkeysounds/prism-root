//! `comments` — in-memory comment store with threading and
//! per-object subscription.
//!
//! Port of `@core/comments`. One-level threading (root + replies),
//! soft-delete, emoji reactions, resolve/unresolve, per-object
//! listener bus, and pure utility functions for thread building.
//!
//! Persistence lives one layer up (`foundation::persistence`) — this
//! module deliberately knows nothing about Loro, daemon IPC, or disks.

pub mod store;
pub mod types;

pub use store::{
    build_threads, can_edit, count_comments, last_activity, truncate_body, widget_contributions,
    CommentListener, CommentStore,
};
pub use types::{Comment, CommentThread, NewComment};
