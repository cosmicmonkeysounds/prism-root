//! WebSocket integration tests for `/ws`. Boots a real `axum::serve`
//! against `127.0.0.1:0` and drives it with `tokio_tungstenite`,
//! because the in-memory `tower::ServiceExt::oneshot` plumbing the
//! HTTP suite uses can't carry a true upgrade handshake.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use base64::Engine;
use futures_util::{SinkExt, StreamExt};
use http_body_util::BodyExt;
use loom_server::{build_router, LoomRelayState};
use serde_json::{json, Value};
use tokio::net::TcpListener;
use tokio_tungstenite::tungstenite::Message;

const TEST_DID: &str = "did:key:test-relay-ws";

async fn spawn_server() -> (SocketAddr, Arc<LoomRelayState>) {
    let state = Arc::new(LoomRelayState::new(TEST_DID));
    let router = build_router(state.clone());
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    (addr, state)
}

async fn register_and_create(
    addr: SocketAddr,
    username: &str,
    password: &str,
    workspace_name: &str,
) -> (String, String) {
    let client = reqwest_like_post(
        addr,
        "/api/auth/register",
        None,
        json!({ "username": username, "password": password }),
    )
    .await;
    let token = client["sessionToken"].as_str().unwrap().to_string();

    let ws_body = reqwest_like_post(
        addr,
        "/api/workspaces",
        Some(&token),
        json!({ "name": workspace_name }),
    )
    .await;
    let id = ws_body["id"].as_str().unwrap().to_string();
    (token, id)
}

/// Tiny hand-rolled POST helper so we don't add `reqwest` as a
/// dev-dep just for these tests. Uses `hyper` via the same
/// `http-body-util` path the existing HTTP suite imports.
async fn reqwest_like_post(
    addr: SocketAddr,
    path: &str,
    bearer: Option<&str>,
    body: Value,
) -> Value {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
    let body_bytes = body.to_string();
    let mut req = format!(
        "POST {path} HTTP/1.1\r\nHost: {addr}\r\nContent-Type: application/json\r\nContent-Length: {len}\r\n",
        len = body_bytes.len()
    );
    if let Some(b) = bearer {
        req.push_str(&format!("Authorization: Bearer {b}\r\n"));
    }
    req.push_str("Connection: close\r\n\r\n");
    req.push_str(&body_bytes);
    stream.write_all(req.as_bytes()).await.unwrap();
    let mut buf = Vec::new();
    stream.read_to_end(&mut buf).await.unwrap();
    let s = String::from_utf8_lossy(&buf);
    let body = s.split("\r\n\r\n").nth(1).unwrap_or("");
    serde_json::from_str(body).unwrap_or(Value::Null)
}

async fn connect_ws(
    addr: SocketAddr,
) -> tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>> {
    let url = format!("ws://{addr}/ws");
    let (stream, _) = tokio_tungstenite::connect_async(url).await.unwrap();
    stream
}

async fn send_json<S>(stream: &mut S, value: Value)
where
    S: SinkExt<Message, Error = tokio_tungstenite::tungstenite::Error> + Unpin,
{
    stream.send(Message::Text(value.to_string())).await.unwrap();
}

async fn recv_json<S>(stream: &mut S) -> Value
where
    S: StreamExt<Item = Result<Message, tokio_tungstenite::tungstenite::Error>> + Unpin,
{
    loop {
        let msg = tokio::time::timeout(Duration::from_secs(5), stream.next())
            .await
            .expect("ws recv timed out")
            .expect("stream closed")
            .expect("ws error");
        match msg {
            Message::Text(t) => return serde_json::from_str(&t).unwrap(),
            Message::Ping(_) | Message::Pong(_) => continue,
            other => panic!("unexpected frame: {other:?}"),
        }
    }
}

// Silence a clippy false-positive: BodyExt is referenced via the
// dev-dep table on the HTTP suite, and Rust's unused-import lint
// fires inside this file when we don't actually call into it.
#[allow(dead_code)]
fn _link_body_ext<B>() -> std::marker::PhantomData<B>
where
    B: BodyExt,
{
    std::marker::PhantomData
}

#[tokio::test]
async fn update_round_trip_between_two_clients() {
    let (addr, _state) = spawn_server().await;
    let (token, workspace_id) = register_and_create(addr, "alice", "hunter2", "Shared").await;

    let mut a = connect_ws(addr).await;
    let mut b = connect_ws(addr).await;

    send_json(
        &mut a,
        json!({ "kind": "auth", "payload": { "token": token } }),
    )
    .await;
    assert_eq!(recv_json(&mut a).await["kind"], "auth-ok");
    send_json(
        &mut b,
        json!({ "kind": "auth", "payload": { "token": token } }),
    )
    .await;
    assert_eq!(recv_json(&mut b).await["kind"], "auth-ok");

    send_json(
        &mut a,
        json!({ "kind": "subscribe", "payload": { "workspace": workspace_id } }),
    )
    .await;
    let snap_a = recv_json(&mut a).await;
    assert_eq!(snap_a["kind"], "snapshot");
    assert_eq!(snap_a["payload"]["workspace"], workspace_id);

    send_json(
        &mut b,
        json!({ "kind": "subscribe", "payload": { "workspace": workspace_id } }),
    )
    .await;
    let snap_b = recv_json(&mut b).await;
    assert_eq!(snap_b["kind"], "snapshot");

    let bytes = vec![9u8, 8, 7, 6, 5];
    let encoded = base64::engine::general_purpose::STANDARD.encode(&bytes);
    send_json(
        &mut a,
        json!({
            "kind": "update",
            "payload": { "workspace": workspace_id, "bytes": encoded }
        }),
    )
    .await;

    let got = recv_json(&mut b).await;
    assert_eq!(got["kind"], "update");
    assert_eq!(got["payload"]["workspace"], workspace_id);
    assert_eq!(got["payload"]["bytes"], encoded);
}

#[tokio::test]
async fn presence_fans_out_peer_list() {
    let (addr, _state) = spawn_server().await;
    let (token, workspace_id) = register_and_create(addr, "bob", "p@ss", "Presence").await;

    let mut a = connect_ws(addr).await;
    let mut b = connect_ws(addr).await;

    for s in [&mut a, &mut b] {
        send_json(s, json!({ "kind": "auth", "payload": { "token": token } })).await;
        assert_eq!(recv_json(s).await["kind"], "auth-ok");
        send_json(
            s,
            json!({ "kind": "subscribe", "payload": { "workspace": workspace_id } }),
        )
        .await;
        assert_eq!(recv_json(s).await["kind"], "snapshot");
    }

    let pstate = json!({
        "identity": {
            "peerId": "peer-a",
            "displayName": "Alice",
            "color": "#abcdef"
        },
        "lastSeen": "2026-05-25T00:00:00Z"
    });
    send_json(
        &mut a,
        json!({
            "kind": "presence",
            "payload": { "workspace": workspace_id, "state": pstate }
        }),
    )
    .await;

    // Originator gets the echoed peer list.
    let echo = recv_json(&mut a).await;
    assert_eq!(echo["kind"], "presence");
    let peers = echo["payload"]["peers"].as_array().unwrap();
    assert_eq!(peers.len(), 1);

    // Other subscriber sees A's presence.
    let got = recv_json(&mut b).await;
    assert_eq!(got["kind"], "presence");
    let peers = got["payload"]["peers"].as_array().unwrap();
    assert_eq!(peers.len(), 1);
    assert_eq!(peers[0]["identity"]["peerId"], "peer-a");
}

#[tokio::test]
async fn subscribe_without_auth_errors() {
    let (addr, _state) = spawn_server().await;
    let mut a = connect_ws(addr).await;
    send_json(
        &mut a,
        json!({ "kind": "subscribe", "payload": { "workspace": "ws-nonexistent" } }),
    )
    .await;
    let got = recv_json(&mut a).await;
    assert_eq!(got["kind"], "error");
}

#[tokio::test]
async fn non_owner_subscribe_forbidden() {
    let (addr, _state) = spawn_server().await;
    let (_token, workspace_id) = register_and_create(addr, "carol", "secret", "Private").await;
    // Second user registers but doesn't own the workspace.
    let other = reqwest_like_post(
        addr,
        "/api/auth/register",
        None,
        json!({ "username": "dave", "password": "x" }),
    )
    .await;
    let other_token = other["sessionToken"].as_str().unwrap().to_string();

    let mut a = connect_ws(addr).await;
    send_json(
        &mut a,
        json!({ "kind": "auth", "payload": { "token": other_token } }),
    )
    .await;
    assert_eq!(recv_json(&mut a).await["kind"], "auth-ok");
    send_json(
        &mut a,
        json!({ "kind": "subscribe", "payload": { "workspace": workspace_id } }),
    )
    .await;
    let got = recv_json(&mut a).await;
    assert_eq!(got["kind"], "error");
    assert!(got["payload"]["message"]
        .as_str()
        .unwrap()
        .contains("forbidden"));
}
