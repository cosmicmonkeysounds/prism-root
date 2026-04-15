//! `activity` — append-only audit trail + human-readable formatter.
//!
//! Ported from `interaction/activity/*`:
//! - [`log`]       — ring-buffer `ActivityStore` per object, plus the
//!   `ActivityEvent` / `FieldChange` / `ActivityVerb` shapes.
//! - [`formatter`] — pure functions that turn events into
//!   `ActivityDescription { text, html }` for timeline rendering.
//!
//! The `activity-tracker.ts` diff-driven tracker is deliberately
//! deferred: the Rust `CollectionStore` only exposes a generic
//! root-level `on_change` listener, not per-object subscriptions, so
//! a tracker port needs to be rethought against the new listener
//! shape and is left as a Phase 2 follow-up.

pub mod formatter;
pub mod log;

pub use formatter::{
    format_activity, format_field_name, format_field_value, group_activity_by_date,
};
pub use log::{
    ActivityDescription, ActivityEvent, ActivityEventInput, ActivityGroup, ActivityListener,
    ActivityStore, ActivityStoreOptions, ActivitySubscription, ActivityVerb, FieldChange,
    GetEventsOptions,
};
