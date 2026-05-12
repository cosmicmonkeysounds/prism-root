//! End-to-end coverage for `#[daemon_fn]` — Phase 6 of
//! `docs/dev/dioxus-inspiration.md`.
//!
//! Lives in the daemon's tests dir (rather than in
//! `prism-luau-derive`) because the macro expands to paths in
//! `prism_daemon::*` and `prism_core::reactive::ipc::*`, and the
//! generated client stub takes any `&dyn DaemonInvoker`. Here we
//! pair the emitted `register_<name>` helper with a `MockInvoker`
//! that loops the request straight back through the actual
//! `CommandRegistry` — same shape the real shell↔daemon socket
//! traffic will take, minus the postcard framing.

use prism_core::reactive::ipc::{DaemonFnError, DaemonInvoker, IpcPayload, MockInvoker};
use prism_daemon::registry::CommandRegistry;
use prism_luau_derive::daemon_fn;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub struct Greet {
    pub name: String,
}

#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub struct GreetResp {
    pub msg: String,
}

#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub enum GreetError {
    Empty,
}

impl std::fmt::Display for GreetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

/// The exemplar `#[daemon_fn]` — one function declaration emits the
/// register helper *and* the typed client stub.
#[daemon_fn(id = "rpc.greet")]
pub fn greet(req: Greet) -> Result<GreetResp, GreetError> {
    if req.name.is_empty() {
        return Err(GreetError::Empty);
    }
    Ok(GreetResp {
        msg: format!("hello, {}", req.name),
    })
}

/// Adapter from `MockInvoker` to the real registry. We don't want
/// the daemon dragged into the macro's expectation surface — the
/// real daemon RPC path uses postcard-over-interprocess (a Phase 7
/// follow-up), but the registry's `invoke(name, JsonValue)`
/// signature is wire-shape-equivalent for testing.
struct RegistryInvoker(Arc<CommandRegistry>);

impl DaemonInvoker for RegistryInvoker {
    fn invoke(
        &self,
        id: &str,
        payload: IpcPayload,
    ) -> Result<IpcPayload, prism_core::reactive::ipc::RemoteError> {
        self.0
            .invoke(id, payload)
            .map_err(|e| prism_core::reactive::ipc::RemoteError::Remote(e.to_string()))
    }
}

#[test]
fn id_const_is_emitted() {
    assert_eq!(GREET_DAEMON_FN_ID, "rpc.greet");
}

#[test]
fn register_helper_wires_handler_into_registry() {
    let reg = CommandRegistry::default();
    register_greet(&reg).unwrap();
    let out = reg
        .invoke("rpc.greet", serde_json::json!({ "name": "Prism" }))
        .unwrap();
    assert_eq!(out["msg"], serde_json::json!("hello, Prism"));
}

#[test]
fn client_stub_round_trips_against_mock_invoker() {
    let inv = MockInvoker::new().on("rpc.greet", |args| {
        let req: Greet = serde_json::from_value(args).unwrap();
        let resp = greet(req).unwrap();
        Ok(serde_json::to_value(&resp).unwrap())
    });
    let resp = greet_client(
        &inv,
        Greet {
            name: "Lattice".into(),
        },
    )
    .expect("client stub round trips");
    assert_eq!(resp.msg, "hello, Lattice");
}

#[test]
fn client_stub_round_trips_against_real_registry() {
    let reg = Arc::new(CommandRegistry::default());
    register_greet(&reg).unwrap();
    let invoker = RegistryInvoker(reg);

    let resp = greet_client(
        &invoker,
        Greet {
            name: "Studio".into(),
        },
    )
    .expect("registry-backed invoker round trips");
    assert_eq!(resp.msg, "hello, Studio");
}

#[test]
fn client_stub_surfaces_transport_errors() {
    // No handler registered → MockInvoker reports Remote("no handler …").
    let inv = MockInvoker::new();
    let err = greet_client(&inv, Greet { name: "x".into() }).unwrap_err();
    match err {
        DaemonFnError::Transport(prism_core::reactive::ipc::RemoteError::Remote(msg)) => {
            assert!(msg.contains("rpc.greet"));
        }
        other => panic!("expected Transport(Remote(_)), got {other:?}"),
    }
}
