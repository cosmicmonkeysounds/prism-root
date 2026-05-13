//! End-to-end coverage for `#[relay_fn]` — §3.4 of
//! `docs/dev/dioxus-inspiration.md`.
//!
//! Sister of `daemon_fn_macro.rs`. The `relay_fn` macro emits a
//! client stub typed against `RelayInvoker` rather than
//! `DaemonInvoker`; tests live in the daemon crate because that's
//! where the macro+core combination is fully available.

use prism_core::reactive::ipc::{MockRelayInvoker, RelayFnError, RelayInvoker};
use prism_luau_derive::relay_fn;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub struct LookupRequest {
    pub portal_id: String,
}

#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub struct PortalSummary {
    pub label: String,
    pub authoritative_relay: String,
}

#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub enum LookupError {
    NotFound,
}

impl std::fmt::Display for LookupError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

/// The exemplar `#[relay_fn]` — one declaration emits both the
/// in-place handler and the typed `lookup_portal_client` stub.
#[relay_fn(id = "portals.lookup")]
pub fn lookup_portal(req: LookupRequest) -> Result<PortalSummary, LookupError> {
    if req.portal_id.is_empty() {
        return Err(LookupError::NotFound);
    }
    Ok(PortalSummary {
        label: format!("Portal {}", req.portal_id),
        authoritative_relay: "relay-1".into(),
    })
}

#[test]
fn id_const_is_emitted() {
    assert_eq!(LOOKUP_PORTAL_RELAY_FN_ID, "portals.lookup");
}

#[test]
fn client_stub_round_trips_against_mock_relay_invoker() {
    let inv = MockRelayInvoker::new().on("portals.lookup", |args| {
        let req: LookupRequest = serde_json::from_value(args).unwrap();
        let resp = lookup_portal(req).unwrap();
        Ok(serde_json::to_value(&resp).unwrap())
    });
    let resp = lookup_portal_client(
        &inv,
        LookupRequest {
            portal_id: "alpha".into(),
        },
    )
    .expect("client stub round trips");
    assert_eq!(resp.label, "Portal alpha");
    assert_eq!(resp.authoritative_relay, "relay-1");
}

#[test]
fn client_stub_surfaces_transport_error_when_handler_errors() {
    let inv = MockRelayInvoker::new();
    let err = lookup_portal_client(
        &inv,
        LookupRequest {
            portal_id: "missing".into(),
        },
    )
    .expect_err("no handler registered");
    assert!(matches!(err, RelayFnError::Transport(_)));
}

/// A tiny custom `RelayInvoker` impl exercises the trait surface
/// directly — same shape host crates will use to wrap their
/// production WebSocket transport.
#[test]
fn custom_relay_invoker_impl_drives_client_stub() {
    struct StaticOk;
    impl RelayInvoker for StaticOk {
        fn invoke(
            &self,
            _id: &str,
            _payload: serde_json::Value,
        ) -> Result<serde_json::Value, prism_core::reactive::ipc::RemoteError> {
            Ok(serde_json::json!({
                "label": "static",
                "authoritative_relay": "relay-static",
            }))
        }
    }
    let inv = StaticOk;
    let resp = lookup_portal_client(
        &inv,
        LookupRequest {
            portal_id: "x".into(),
        },
    )
    .unwrap();
    assert_eq!(resp.label, "static");
}
