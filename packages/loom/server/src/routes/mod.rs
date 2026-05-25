//! HTTP route handlers. Each submodule owns one cluster of endpoints.
//! The `/ws` upgrade lands in Phase 3 alongside CRDT sync.

pub mod auth;
pub mod health;
pub mod tokens;
pub mod workspaces;
