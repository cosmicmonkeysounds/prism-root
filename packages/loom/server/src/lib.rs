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

use std::path::PathBuf;
use std::sync::Arc;

use axum::{
    body::Body,
    extract::Request,
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Router,
};
use prism_core::network::relay::module_system::{
    capabilities, RelayBuilder, RelayInstance, RelayServerConfig,
};
use prism_core::network::relay::modules::{
    capability_tokens::{CapabilityTokenManager, CapabilityTokenModule},
    collection_host::{CollectionHost, CollectionHostModule},
    password_auth::{PasswordAuthModule, RelayPasswordAuth},
};
use tower::ServiceExt;
use tower_http::cors::CorsLayer;
use tower_http::services::ServeDir;

pub mod auth;
pub mod error;
pub mod extractors;
pub mod play;
pub mod routes;
pub mod workspaces;
pub mod ws;

use play::PlayHub;
use workspaces::WorkspaceRegistry;
use ws::WsHub;

/// Shared application state. Cloneable as `Arc<LoomRelayState>` into
/// each axum handler.
pub struct LoomRelayState {
    pub relay: Arc<RelayInstance>,
    pub workspaces: WorkspaceRegistry,
    pub ws_hub: WsHub,
    /// Phase 7 — server-hosted Loom play sessions.
    pub play: PlayHub,
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
            workspaces: WorkspaceRegistry::new(),
            ws_hub: WsHub::new(),
            play: PlayHub::new(),
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

/// CORS posture for the assembled router. Phase 8 lets `loom-relayd`
/// serve the static editor itself; once the editor is same-origin we
/// don't want stray cross-origin requests, but the Vite dev server on
/// `:5173` still needs permissive headers when it talks to the relay
/// on `:7878`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum CorsMode {
    /// No `Access-Control-*` headers — appropriate when the editor is
    /// served from the same origin as the API.
    #[default]
    SameOrigin,
    /// `tower_http::cors::CorsLayer::permissive()` — for development
    /// against the Vite dev server, or for explicitly embedded
    /// deployments.
    Permissive,
}

/// Optional knobs for `build_router_with`. Phase 8 — controls whether
/// the assembled router also serves the React editor as static files,
/// and the CORS posture for the API + WS routes.
#[derive(Debug, Clone, Default)]
pub struct LoomServeConfig {
    /// Path to the `editor/dist/` directory produced by `pnpm build`.
    /// When `Some`, the router mounts a `ServeDir` fallback that
    /// serves the editor with SPA-style `index.html` fallback for
    /// unknown paths.
    pub editor_dist: Option<PathBuf>,
    /// CORS posture for the API + WS routes.
    pub cors: CorsMode,
}

/// Build the axum router with the bare API surface — no static editor,
/// same-origin CORS. Equivalent to `build_router_with(state,
/// LoomServeConfig::default())`. Kept as the existing public entry
/// point so Phases 1–7 integration tests don't need to update.
pub fn build_router(state: Arc<LoomRelayState>) -> Router {
    build_router_with(state, LoomServeConfig::default())
}

/// Build the axum router. HTTP surface: health + auth + workspaces +
/// share-link tokens. WebSocket: `/ws` carries CRDT sync (Phase 3) +
/// presence fan-out (Phase 5) on the same connection. Phase 8 — when
/// `config.editor_dist` is set the router also serves the React
/// editor's `dist/` directory as a fallback (SPA semantics: unknown
/// paths re-serve `index.html`).
pub fn build_router_with(state: Arc<LoomRelayState>, config: LoomServeConfig) -> Router {
    let mut router = Router::new()
        .route("/api/health", get(routes::health::health))
        .route("/api/auth/register", post(routes::auth::register))
        .route("/api/auth/login", post(routes::auth::login))
        .route("/api/auth/change", post(routes::auth::change))
        .route(
            "/api/workspaces",
            get(routes::workspaces::list).post(routes::workspaces::create),
        )
        .route(
            "/api/workspaces/:id",
            get(routes::workspaces::get).delete(routes::workspaces::delete),
        )
        .route(
            "/api/workspaces/:id/snapshot",
            get(routes::workspaces::get_snapshot).post(routes::workspaces::put_snapshot),
        )
        .route("/api/tokens/issue", post(routes::tokens::issue))
        .route("/api/tokens/verify", post(routes::tokens::verify))
        .route("/ws", get(ws::ws_handler))
        .with_state(state);

    if let Some(dist) = config.editor_dist {
        // ServeDir handles the happy path (`/`, `/assets/foo.js`,
        // `/index.html`); for everything else (`/workspace/abc`,
        // `/play/123`, …) we fall back to `index.html` so the React
        // router can pick up the deep link. We can't just use
        // `ServeDir::not_found_service(ServeFile::new(index))` —
        // tower-http's `ServeFile` derives the served path from the
        // request URI, which makes it 404 on nested paths. Instead
        // we wrap ServeDir in a fallback handler that re-reads
        // `index.html` from disk when ServeDir returns 404.
        let dist = Arc::new(dist);
        let serve_dir = ServeDir::new(dist.as_ref());
        let dist_for_fallback = Arc::clone(&dist);
        let fallback = move |req: Request| {
            let serve_dir = serve_dir.clone();
            let dist = Arc::clone(&dist_for_fallback);
            async move {
                let response = serve_dir.oneshot(req).await.into_response();
                if response.status() != StatusCode::NOT_FOUND {
                    return response;
                }
                serve_index_html(&dist).await
            }
        };
        router = router.fallback(fallback);
    }

    if matches!(config.cors, CorsMode::Permissive) {
        router = router.layer(CorsLayer::permissive());
    }

    router
}

fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339()
}

async fn serve_index_html(dist: &std::path::Path) -> Response {
    let index = dist.join("index.html");
    match tokio::fs::read(&index).await {
        Ok(bytes) => Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
            .body(Body::from(bytes))
            .unwrap(),
        Err(err) => {
            tracing::error!(
                index = %index.display(),
                error = %err,
                "editor dist index.html missing — serving 500"
            );
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "editor index.html missing",
            )
                .into_response()
        }
    }
}
