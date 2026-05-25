//! Integration tests for the loom-server HTTP surface. Drives the
//! router with `tower::ServiceExt::oneshot` so the suite stays
//! synchronous + reusable in CI without a real TCP bind.

use std::sync::Arc;

use axum::body::{Body, Bytes};
use axum::http::{header, Request, StatusCode};
use http_body_util::BodyExt;
use loom_server::{build_router, LoomRelayState};
use serde_json::{json, Value};
use tower::ServiceExt;

const TEST_DID: &str = "did:key:test-relay";

fn state() -> Arc<LoomRelayState> {
    Arc::new(LoomRelayState::new(TEST_DID))
}

async fn call(
    state: Arc<LoomRelayState>,
    method: &str,
    uri: &str,
    bearer: Option<&str>,
    body: Body,
    content_type: Option<&str>,
) -> (StatusCode, Bytes) {
    let mut req = Request::builder().method(method).uri(uri);
    if let Some(token) = bearer {
        req = req.header(header::AUTHORIZATION, format!("Bearer {token}"));
    }
    if let Some(ct) = content_type {
        req = req.header(header::CONTENT_TYPE, ct);
    }
    let response = build_router(state)
        .oneshot(req.body(body).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, bytes)
}

async fn json_call(
    state: Arc<LoomRelayState>,
    method: &str,
    uri: &str,
    bearer: Option<&str>,
    body: Value,
) -> (StatusCode, Value) {
    let (status, bytes) = call(
        state,
        method,
        uri,
        bearer,
        Body::from(body.to_string()),
        Some("application/json"),
    )
    .await;
    let value: Value = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap()
    };
    (status, value)
}

#[tokio::test]
async fn health_returns_modules() {
    let (status, body) = json_call(state(), "GET", "/api/health", None, json!(null)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "ok");
    assert_eq!(body["relayDid"], TEST_DID);
    let modules: Vec<&str> = body["modules"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert!(modules.contains(&"collection-host"));
    assert!(modules.contains(&"password-auth"));
    assert!(modules.contains(&"capability-tokens"));
}

#[tokio::test]
async fn register_then_login_then_change() {
    let state = state();
    let (status, body) = json_call(
        state.clone(),
        "POST",
        "/api/auth/register",
        None,
        json!({"username": "alice", "password": "hunter2"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["username"], "alice");
    let token1 = body["sessionToken"].as_str().unwrap().to_string();
    assert!(!token1.is_empty());

    // Duplicate registration is a conflict.
    let (status, _) = json_call(
        state.clone(),
        "POST",
        "/api/auth/register",
        None,
        json!({"username": "alice", "password": "hunter2"}),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);

    let (status, body) = json_call(
        state.clone(),
        "POST",
        "/api/auth/login",
        None,
        json!({"username": "alice", "password": "hunter2"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["sessionToken"].as_str().is_some());

    let (status, body) = json_call(
        state.clone(),
        "POST",
        "/api/auth/login",
        None,
        json!({"username": "alice", "password": "wrong"}),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert!(body["error"].is_string());

    let (status, _) = json_call(
        state,
        "POST",
        "/api/auth/change",
        None,
        json!({
            "username": "alice",
            "oldPassword": "hunter2",
            "newPassword": "letmein"
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn workspace_requires_auth() {
    let (status, _) =
        json_call(state(), "GET", "/api/workspaces", None, json!(null)).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn workspace_crud_and_snapshot_round_trip() {
    let state = state();

    let (_, body) = json_call(
        state.clone(),
        "POST",
        "/api/auth/register",
        None,
        json!({"username": "bob", "password": "p@ss"}),
    )
    .await;
    let token = body["sessionToken"].as_str().unwrap().to_string();

    let (status, body) = json_call(
        state.clone(),
        "POST",
        "/api/workspaces",
        Some(&token),
        json!({"name": "Saltmere"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let id = body["id"].as_str().unwrap().to_string();
    assert_eq!(body["name"], "Saltmere");
    assert_eq!(body["owner"], "bob");

    let (status, body) =
        json_call(state.clone(), "GET", "/api/workspaces", Some(&token), json!(null)).await;
    assert_eq!(status, StatusCode::OK);
    let workspaces = body["workspaces"].as_array().unwrap();
    assert_eq!(workspaces.len(), 1);
    assert_eq!(workspaces[0]["id"], id);

    // Round-trip an opaque snapshot through CollectionHost.
    let snapshot = vec![1u8, 2, 3, 4, 5];
    let (status, _) = call(
        state.clone(),
        "POST",
        &format!("/api/workspaces/{id}/snapshot"),
        Some(&token),
        Body::from(snapshot.clone()),
        Some("application/octet-stream"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (status, bytes) = call(
        state.clone(),
        "GET",
        &format!("/api/workspaces/{id}/snapshot"),
        Some(&token),
        Body::empty(),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(bytes.as_ref(), snapshot.as_slice());

    // Another user can't see bob's workspaces.
    let (_, body) = json_call(
        state.clone(),
        "POST",
        "/api/auth/register",
        None,
        json!({"username": "eve", "password": "trespass"}),
    )
    .await;
    let eve_token = body["sessionToken"].as_str().unwrap().to_string();

    let (status, body) = json_call(
        state.clone(),
        "GET",
        "/api/workspaces",
        Some(&eve_token),
        json!(null),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["workspaces"].as_array().unwrap().is_empty());

    let (status, _) = json_call(
        state.clone(),
        "GET",
        &format!("/api/workspaces/{id}"),
        Some(&eve_token),
        json!(null),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    // Delete + verify gone.
    let (status, _) = json_call(
        state.clone(),
        "DELETE",
        &format!("/api/workspaces/{id}"),
        Some(&token),
        json!(null),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (status, _) = json_call(
        state,
        "GET",
        &format!("/api/workspaces/{id}"),
        Some(&token),
        json!(null),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn token_issue_and_verify() {
    let state = state();
    let (_, body) = json_call(
        state.clone(),
        "POST",
        "/api/auth/register",
        None,
        json!({"username": "carol", "password": "secret"}),
    )
    .await;
    let token = body["sessionToken"].as_str().unwrap().to_string();

    let (_, body) = json_call(
        state.clone(),
        "POST",
        "/api/workspaces",
        Some(&token),
        json!({"name": "Shared"}),
    )
    .await;
    let workspace_id = body["id"].as_str().unwrap().to_string();

    let (status, body) = json_call(
        state.clone(),
        "POST",
        "/api/tokens/issue",
        Some(&token),
        json!({
            "workspaceId": workspace_id,
            "permissions": ["read"],
            "ttlSeconds": 3600
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let share_token = body["token"].as_str().unwrap().to_string();

    let (status, body) = json_call(
        state.clone(),
        "POST",
        "/api/tokens/verify",
        None,
        json!({"token": share_token}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["valid"], true);
    assert_eq!(body["subject"], "carol");
    assert_eq!(body["scope"], workspace_id);
    assert_eq!(body["permissions"][0], "read");

    // Garbage token verifies false, not error.
    let (status, body) = json_call(
        state,
        "POST",
        "/api/tokens/verify",
        None,
        json!({"token": "not-a-real-token"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["valid"], false);
}
