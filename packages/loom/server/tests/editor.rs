//! Phase 8 — integration test for the self-hosted editor surface.
//!
//! Boots `build_router_with` against a fake editor `dist/` (just an
//! `index.html` + a hashed asset) and exercises the four properties
//! the topology depends on:
//!
//! 1. `GET /` returns the editor's `index.html`.
//! 2. `GET /assets/<hashed>.js` returns the asset bytes verbatim.
//! 3. `GET /workspace/abc` (a SPA deep link) falls back to
//!    `index.html` rather than 404ing.
//! 4. `GET /api/health` still routes to the API handler — the
//!    static fallback only fires for unmatched paths.

use std::sync::Arc;

use axum::body::{Body, Bytes};
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use loom_server::{build_router_with, CorsMode, LoomRelayState, LoomServeConfig};
use tower::ServiceExt;

const INDEX_HTML: &str = "<!doctype html><html><body><div id=root></div></body></html>";
const ASSET_BODY: &[u8] = b"console.log('hello from /assets/index-fake.js');";

fn write_fake_dist() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("create tempdir");
    let dist = dir.path();
    std::fs::write(dist.join("index.html"), INDEX_HTML).expect("write index.html");
    let assets = dist.join("assets");
    std::fs::create_dir_all(&assets).expect("create assets/");
    std::fs::write(assets.join("index-fake.js"), ASSET_BODY).expect("write asset");
    dir
}

async fn call(
    state: Arc<LoomRelayState>,
    dist: std::path::PathBuf,
    uri: &str,
) -> (StatusCode, Bytes) {
    let app = build_router_with(
        state,
        LoomServeConfig {
            editor_dist: Some(dist),
            cors: CorsMode::SameOrigin,
        },
    );
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(uri)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, bytes)
}

fn state() -> Arc<LoomRelayState> {
    Arc::new(LoomRelayState::new("did:key:editor-test"))
}

#[tokio::test]
async fn root_returns_editor_index() {
    let dist = write_fake_dist();
    let (status, body) = call(state(), dist.path().to_path_buf(), "/").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(std::str::from_utf8(&body).unwrap(), INDEX_HTML);
}

#[tokio::test]
async fn explicit_index_path_returns_index() {
    let dist = write_fake_dist();
    let (status, body) = call(state(), dist.path().to_path_buf(), "/index.html").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(std::str::from_utf8(&body).unwrap(), INDEX_HTML);
}

#[tokio::test]
async fn hashed_asset_returns_bytes_verbatim() {
    let dist = write_fake_dist();
    let (status, body) = call(state(), dist.path().to_path_buf(), "/assets/index-fake.js").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body.as_ref(), ASSET_BODY);
}

#[tokio::test]
async fn unknown_path_falls_back_to_index_for_spa_routing() {
    // Vite produces a single-page app — deep links like
    // /workspace/abc must serve `index.html` and let React Router
    // handle the rest. `ServeDir::not_found_service(ServeFile::new(
    // index))` is what makes that work.
    let dist = write_fake_dist();
    let (status, body) = call(state(), dist.path().to_path_buf(), "/workspace/abc").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(std::str::from_utf8(&body).unwrap(), INDEX_HTML);
}

#[tokio::test]
async fn api_routes_still_take_precedence_over_static_fallback() {
    let dist = write_fake_dist();
    let (status, body) = call(state(), dist.path().to_path_buf(), "/api/health").await;
    assert_eq!(status, StatusCode::OK);
    let parsed: serde_json::Value = serde_json::from_slice(&body).expect("json body");
    assert_eq!(parsed["status"], "ok");
    assert_eq!(parsed["relayDid"], "did:key:editor-test");
}

#[tokio::test]
async fn no_dist_means_no_fallback_and_unknown_404s() {
    // Sanity: without `editor_dist`, the API-only router falls
    // through to a 404 for unmatched paths (existing Phases 1–7
    // behaviour).
    let app = build_router_with(state(), LoomServeConfig::default());
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}
