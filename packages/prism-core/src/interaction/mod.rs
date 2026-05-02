//! `interaction` — pure-logic counterparts to the legacy
//! `@prism/core/interaction/*` subtree.
//!
//! Phase-2 destination for the runtime data behind lenses, layouts,
//! notifications, activity, query, search, and the page-builder. The
//! rendering half (Slint) lives elsewhere; only the
//! data + reducers land here so hosts can drive them from any UI.
//!
//! Ported so far:
//!
//! - [`notification`] — in-memory notification registry + debouncing
//!   queue. Pure data, no UI or timer assumptions (timers are pluggable).
//! - [`activity`]     — append-only activity log + per-object
//!   formatter / date bucketing. Pure data.
//! - [`query`]        — filter / sort / group pipeline over
//!   `GraphObject`. Useful half of the legacy `view/view-config.ts`;
//!   the `ViewMode` enum + `SavedView` + `LiveView` are deliberately
//!   **not** ported (views are `prism_builder::Component`s now).

pub mod activity;
pub mod comments;
pub mod dashboard;
pub mod notification;
pub mod object_detail;
pub mod query;
pub mod quick_create;
