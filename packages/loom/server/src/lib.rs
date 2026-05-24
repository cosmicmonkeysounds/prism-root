//! `loom-server` — multi-user backbone for the Loom web editor.
//!
//! Axum HTTP + WebSocket server that hosts per-workspace Loro CRDTs.
//! Built on the `prism-core::network::relay` module system with a
//! trimmed set of three modules (`collection_host`, `password_auth`,
//! `capability_tokens`) — we depend on `prism-core` directly rather
//! than `prism-relay` to keep the binary surface small.
//!
//! See `docs/dev/loom-multiuser.md` for scope, wire protocol, and the
//! phased roadmap.

use std::sync::Arc;

use axum::{routing::get, Router};
use prism_core::network::relay::module_system::{
    capabilities, RelayBuilder, RelayInstance, RelayServerConfig,
};
use prism_core::network::relay::modules::{
    capability_tokens::{CapabilityTokenManager, CapabilityTokenModule},
    collection_host::{CollectionHost, CollectionHostModule},
    password_auth::{PasswordAuthModule, RelayPasswordAuth},
};

pub mod routes;

/// Shared application state. Cloneable as `Arc<LoomRelayState>` into
/// each axum handler.
pub struct LoomRelayState {
    pub relay: Arc<RelayInstance>,
    pub relay_did: String,
    pub started_at: String,
}

impl LoomRelayState {
    /// Build the relay with Loom's three modules installed.
    pub fn new(relay_did: impl Into<String>) -> Self {
        let relay_did = relay_did.into();
        let server_config = RelayServerConfig {
            relay_did: relay_did.clone(),
            ..Default::default()
        };

        let relay = RelayBuilder::new(server_config)
            .use_module(CollectionHostModule)
            .use_module(PasswordAuthModule)
            .use_module(CapabilityTokenModule)
            .build()
            .expect("loom-server module build must succeed");

        Self {
            relay: Arc::new(relay),
            relay_did,
            started_at: now_rfc3339(),
        }
    }

    pub fn collections(&self) -> Arc<CollectionHost> {
        self.relay
            .get_capability(capabilities::COLLECTIONS)
            .expect("collection-host installed")
    }

    pub fn password_auth(&self) -> Arc<RelayPasswordAuth> {
        self.relay
            .get_capability(capabilities::PASSWORD_AUTH)
            .expect("password-auth installed")
    }

    pub fn tokens(&self) -> Arc<CapabilityTokenManager> {
        self.relay
            .get_capability(capabilities::TOKENS)
            .expect("capability-tokens installed")
    }
}

/// Build the axum router for Phase 1: just `/api/health`.
///
/// Subsequent phases append `/api/auth/*`, `/api/workspaces/*`,
/// `/api/tokens/*`, and `/ws` here.
pub fn build_router(state: Arc<LoomRelayState>) -> Router {
    Router::new()
        .route("/api/health", get(routes::health::health))
        .with_state(state)
}

fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339()
}
