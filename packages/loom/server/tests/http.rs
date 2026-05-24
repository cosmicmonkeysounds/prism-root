//! Integration tests for the loom-server HTTP surface. Drives the
//! router with `tower::ServiceExt::oneshot` so the suite stays
//! synchronous + reusable in CI without a real TCP bind.

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use loom_server::{build_router, LoomRelayState};
use tower::ServiceExt;

#[tokio::test]
async fn health_returns_ok_with_modules() {
    let state = Arc::new(LoomRelayState::new("did:key:test-relay"));
    let app = build_router(state);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["status"], "ok");
    assert_eq!(json["relayDid"], "did:key:test-relay");

    let modules: Vec<String> = json["modules"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();
    assert!(modules.contains(&"collection-host".to_string()));
    assert!(modules.contains(&"password-auth".to_string()));
    assert!(modules.contains(&"capability-tokens".to_string()));
}

#[tokio::test]
async fn capability_accessors_round_trip() {
    let state = LoomRelayState::new("did:key:test-relay");
    let _ = state.collections();
    let _ = state.password_auth();
    let _ = state.tokens();
}
