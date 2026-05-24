//! HTTP route handlers. Each submodule owns one cluster of endpoints.
//! Phase 1 ships only the health probe; auth, workspaces, tokens, and
//! the `/ws` upgrade follow in Phases 2 and 3.

pub mod health;
