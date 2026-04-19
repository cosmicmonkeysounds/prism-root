//! `prism-relay` — Sovereign Portal SSR server + full relay protocol.
//!
//! Rust rewrite of the Hono JSX TypeScript relay. Ships the complete
//! 18-module feature surface: blind mailbox, relay router, timestamper,
//! blind ping, capability tokens, webhooks, sovereign portals (L1 SSR),
//! WebRTC signaling, collection host, vault host, hashcash,
//! peer trust, escrow, federation, password auth, ACME certificates,
//! and portal templates.
//!
//! The SSR portal path walks [`prism_builder::BuilderDocument`] trees
//! through [`prism_builder::render_document_html`]. The API surface
//! (~80 endpoints) and WebSocket relay protocol are wired through
//! [`build_full_router`].

pub mod config;
pub mod middleware;
pub mod persistence;
pub mod portal;
pub mod relay_state;
pub mod router;
pub mod routes;
pub mod ssr_routes;
pub mod state;
pub mod util;
pub mod ws;

pub use portal::{Portal, PortalId, PortalStore};
pub use relay_state::FullRelayState;
pub use router::build_full_router;
pub use ssr_routes::build_router;
pub use state::AppState;
