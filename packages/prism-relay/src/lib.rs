//! `prism-relay` — Sovereign Portal SSR server.
//!
//! This is the Rust rewrite of the Hono JSX TypeScript relay. It owns
//! the HTTP surface anonymous visitors hit when they browse a
//! published Prism portal: the Level 1-4 portal renderer, the SEO
//! helpers (sitemap, robots.txt), a healthcheck, and the landing
//! index. Every portal route walks a [`prism_builder::BuilderDocument`]
//! against the [`prism_builder::ComponentRegistry`] through
//! [`prism_builder::render_document_html`] — the same semantic-HTML
//! pipeline Studio will re-target to Clay for its interactive path.
//!
//! Scope today is deliberately narrow. The legacy relay carried 17
//! modules (federation, ACME, escrow, hashcash, capability tokens,
//! WebSocket envelope routing, …); those land in follow-on phases.
//! What's here is the spine: an axum router wired end-to-end to the
//! [`prism_builder`] SSR path with real tests, so the rest of the
//! modules can be ported route-by-route without architectural
//! questions left open.
//!
//! ```no_run
//! use prism_relay::{AppState, build_router};
//! use std::sync::Arc;
//!
//! # #[tokio::main]
//! # async fn main() -> anyhow::Result<()> {
//! let state = Arc::new(AppState::with_sample_portals());
//! let app = build_router(state);
//! let listener = tokio::net::TcpListener::bind("127.0.0.1:1420").await?;
//! axum::serve(listener, app).await?;
//! # Ok(())
//! # }
//! ```

pub mod components;
pub mod portal;
pub mod routes;
pub mod state;

pub use portal::{Portal, PortalId, PortalStore};
pub use routes::build_router;
pub use state::AppState;
