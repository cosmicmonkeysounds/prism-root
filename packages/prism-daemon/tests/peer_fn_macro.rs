//! End-to-end coverage for `#[peer_fn]` — completes the §3.4 macro
//! family in `docs/dev/dioxus-inspiration.md`.

use prism_core::reactive::ipc::{MockPeerInvoker, PeerFnError, PeerInvoker};
use prism_luau_derive::peer_fn;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub struct PingPayload {
    pub seq: u32,
}

#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub struct PongPayload {
    pub seq: u32,
    pub took_ms: u32,
}

#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub enum PingError {
    Disconnected,
}

impl std::fmt::Display for PingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

#[peer_fn(id = "peer.ping")]
pub fn ping(req: PingPayload) -> Result<PongPayload, PingError> {
    Ok(PongPayload {
        seq: req.seq,
        took_ms: 7,
    })
}

#[test]
fn id_const_is_emitted() {
    assert_eq!(PING_PEER_FN_ID, "peer.ping");
}

#[test]
fn client_stub_round_trips_against_mock_peer_invoker() {
    let inv = MockPeerInvoker::new().on("peer.ping", |args| {
        let req: PingPayload = serde_json::from_value(args).unwrap();
        let resp = ping(req).unwrap();
        Ok(serde_json::to_value(&resp).unwrap())
    });
    let resp = ping_client(&inv, PingPayload { seq: 42 }).expect("client stub round trips");
    assert_eq!(resp.seq, 42);
    assert_eq!(resp.took_ms, 7);
}

#[test]
fn client_stub_surfaces_transport_error_for_missing_handler() {
    let inv = MockPeerInvoker::new();
    let err = ping_client(&inv, PingPayload { seq: 1 }).expect_err("no handler");
    assert!(matches!(err, PeerFnError::Transport(_)));
}

#[test]
fn custom_peer_invoker_impl_drives_client_stub() {
    struct StaticPong;
    impl PeerInvoker for StaticPong {
        fn invoke(
            &self,
            _id: &str,
            _payload: serde_json::Value,
        ) -> Result<serde_json::Value, prism_core::reactive::ipc::RemoteError> {
            Ok(serde_json::json!({ "seq": 0, "took_ms": 999 }))
        }
    }
    let resp = ping_client(&StaticPong, PingPayload { seq: 0 }).unwrap();
    assert_eq!(resp.took_ms, 999);
}
