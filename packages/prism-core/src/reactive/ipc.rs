//! `reactive::ipc` — Phase 6 of the Dioxus-inspired reactive
//! overhaul (`docs/dev/dioxus-inspiration.md`).
//!
//! Cross-process reactive primitives:
//!
//! * [`RemoteState<T>`] — the connectivity-aware carrier the plan's
//!   §8 "Remote signal failure modes" open question settled on.
//!   Wraps `T` with `Loading` / `Live` / `Stale` / `Errored`
//!   variants so subscribers that care about connectivity can
//!   match, while subscribers that don't can call
//!   [`RemoteState::last_known`] for the latest available value.
//! * [`RemoteSignal<T>`] — a [`crate::reactive::Signal`] of
//!   `RemoteState<T>` plus accessor sugar: `read` / `last_known` /
//!   `try_read` / `is_live` / `state`. Same reactive plumbing as
//!   any other Signal; connectivity transitions propagate through
//!   the graph because the carrier value (the enum) changes.
//! * [`DaemonInvoker`] — abstract transport seam for client stubs.
//!   The `#[daemon_fn]` proc macro (Phase 6 sibling) emits
//!   client-side stubs that call through this trait, so callers can
//!   swap in the live daemon socket, the in-process registry (for
//!   tests), an HTTP transport, etc., without changing the call
//!   sites.
//!
//! The four `Remote* / Federated* / Peer* / RelaySignal` variants
//! from §6 Phases 6–7 all share the same `RemoteState<T>` carrier;
//! the difference between them is how the underlying transport
//! populates which variant under which conditions. This module
//! defines the shape every transport speaks.

use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::reactive::{Owner, Signal};

/// Error carrier for cross-process / cross-host signals. Captured
/// inside [`RemoteState::Errored`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
pub enum RemoteError {
    /// The transport's socket / connection is down.
    #[error("transport offline: {0}")]
    Offline(String),
    /// The remote peer did not respond within the configured timeout.
    #[error("timeout after {millis}ms")]
    Timeout { millis: u64 },
    /// The remote returned a structurally valid error response.
    /// Carries the remote's message verbatim.
    #[error("remote error: {0}")]
    Remote(String),
    /// Payload (de)serialization failed somewhere on the wire.
    #[error("decode error: {0}")]
    Decode(String),
    /// The capability token the subscription presented was rejected.
    /// Phase 7's `RelaySignal` (capability-token-gated) uses this.
    #[error("permission denied: {0}")]
    PermissionDenied(String),
}

/// Connectivity-aware carrier for a cross-process value.
///
/// See `docs/dev/dioxus-inspiration.md` §8 for the design rationale
/// — both accessor styles ("best effort: `last_known`" and
/// "fallible: `try_read`") share this single carrier so subscribers
/// can choose how much connectivity awareness they want.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum RemoteState<T> {
    /// Subscription has been opened but no value has been received
    /// yet. Subscribers typically show a spinner.
    Loading,
    /// Connected; the carried value is fresh.
    Live(T),
    /// Connection has been lost but the last received value is
    /// still available. `since_ms` is the wall-clock millisecond
    /// timestamp of the connectivity transition (not a duration),
    /// in epoch ms — callers compute "how long" by subtracting from
    /// `SystemTime::now()`.
    Stale { value: T, since_ms: u64 },
    /// The connection failed terminally for this subscription.
    /// `last` carries the most recent live value if one was ever
    /// received; `None` means the subscription failed before any
    /// value arrived.
    Errored { last: Option<T>, err: RemoteError },
}

impl<T> RemoteState<T> {
    /// Best-effort accessor: returns `Some(&T)` whenever any value
    /// is available, regardless of connectivity. Subscribers that
    /// don't distinguish Live from Stale write
    /// `if let Some(v) = state.last_known() { ... }`.
    pub fn last_known(&self) -> Option<&T> {
        match self {
            Self::Live(v) | Self::Stale { value: v, .. } => Some(v),
            Self::Errored { last, .. } => last.as_ref(),
            Self::Loading => None,
        }
    }

    pub fn is_live(&self) -> bool {
        matches!(self, Self::Live(_))
    }

    pub fn is_loading(&self) -> bool {
        matches!(self, Self::Loading)
    }

    pub fn is_stale(&self) -> bool {
        matches!(self, Self::Stale { .. })
    }

    pub fn is_errored(&self) -> bool {
        matches!(self, Self::Errored { .. })
    }
}

/// Convenience for transports building `Stale` transitions: capture
/// the current epoch millisecond. Falls back to `0` if the system
/// clock pre-dates the unix epoch (which it won't, but documenting
/// the contract).
pub fn now_epoch_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// A reactive signal whose backing store lives across a process or
/// network boundary. `Clone`-cheap (the inner `Signal<RemoteState<T>>`
/// is `Copy + 'static`).
///
/// The transport (an [`crate::reactive::ipc::DaemonInvoker`]
/// implementation for Phase 6, the relay's federation/signaling
/// modules for Phase 7) is responsible for populating the carrier:
/// initial `Loading`, then `Live` once the first value arrives,
/// `Stale` on disconnect, `Errored` on terminal failure.
pub struct RemoteSignal<T: 'static> {
    signal: Signal<RemoteState<T>>,
}

impl<T: 'static> Copy for RemoteSignal<T> {}
impl<T: 'static> Clone for RemoteSignal<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: 'static> RemoteSignal<T> {
    /// Construct a fresh remote signal in `owner`, starting in the
    /// `Loading` state. The transport is responsible for advancing
    /// the state via [`RemoteSignal::set_state`] as the connection
    /// progresses.
    pub fn new(owner: &Owner) -> Self {
        Self {
            signal: owner.insert(RemoteState::Loading),
        }
    }

    /// The inner [`Signal`] for callers that want to compose with
    /// the general reactive substrate (effects, memos, …).
    pub fn signal(&self) -> Signal<RemoteState<T>> {
        self.signal
    }

    /// Subscribing read of the full state. Use when subscribers
    /// care about the connectivity transitions themselves
    /// ("disconnected" badge, stale tint, offline banner).
    pub fn state(&self) -> RemoteState<T>
    where
        T: Clone,
    {
        self.signal.get()
    }

    /// Subscribing read for code that doesn't distinguish live
    /// from stale. Returns `None` only during `Loading` or
    /// unrecoverable `Errored { last: None, .. }`.
    pub fn last_known(&self) -> Option<T>
    where
        T: Clone,
    {
        self.signal.read(|s| s.last_known().cloned())
    }

    /// Subscribing read; `Ok` only when the carrier is `Live`.
    pub fn try_read(&self) -> Result<T, RemoteError>
    where
        T: Clone,
    {
        self.signal.read(|s| match s {
            RemoteState::Live(v) => Ok(v.clone()),
            RemoteState::Loading => Err(RemoteError::Offline("loading".into())),
            RemoteState::Stale { .. } => Err(RemoteError::Offline("stale".into())),
            RemoteState::Errored { err, .. } => Err(err.clone()),
        })
    }

    /// Non-subscribing peek of the full state. Used by transport
    /// code that needs to inspect the current carrier without
    /// forming a dependency edge.
    pub fn peek_state(&self) -> RemoteState<T>
    where
        T: Clone,
    {
        self.signal.snapshot()
    }

    /// True iff currently `Live`. Subscribes (the state itself is
    /// reactive).
    pub fn is_live(&self) -> bool {
        self.signal.read(RemoteState::is_live)
    }

    /// Push a new state into the carrier. Transport plumbing
    /// (Phase 6 `#[daemon_fn]` shell stub, Phase 7 federation
    /// bridge, etc.) calls this when the upstream value updates.
    pub fn set_state(&self, state: RemoteState<T>) {
        self.signal.set(state);
    }

    /// Convenience shorthand for a fresh `Live(v)` push.
    pub fn set_live(&self, value: T) {
        self.signal.set(RemoteState::Live(value));
    }

    /// Convenience shorthand for transitioning the carrier to
    /// `Errored`, preserving the last known value if the current
    /// state has one.
    pub fn set_errored(&self, err: RemoteError)
    where
        T: Clone,
    {
        let last = self.signal.peek(|s| s.last_known().cloned());
        self.signal.set(RemoteState::Errored { last, err });
    }
}

// ───── DaemonInvoker — the transport seam ────────────────────────

/// JSON-shaped request/response payload type. Phase 6 lands on JSON
/// to compose with the existing `prism-daemon::registry::invoke`
/// machinery; the plan's eventual postcard-over-interprocess path
/// can swap in by implementing this trait with a different payload
/// encoding.
pub type IpcPayload = serde_json::Value;

/// Sync transport seam for daemon-side RPCs. Implementations
/// abstract over whichever real channel the call site is using —
/// the in-process registry (for tests), an interprocess socket, an
/// HTTP transport, etc.
///
/// `#[daemon_fn]` generates client-side stubs that take any
/// `&dyn DaemonInvoker`, serialise their typed args via
/// `serde_json::to_value`, call [`DaemonInvoker::invoke`], and
/// deserialise the response. Type-equal contract at the call
/// site; the wire format is one indirection down.
pub trait DaemonInvoker {
    /// Send a request to the daemon for the command identified by
    /// `id` with the JSON-encoded `payload`, and return the
    /// JSON-encoded response (or a transport error).
    fn invoke(&self, id: &str, payload: IpcPayload) -> Result<IpcPayload, RemoteError>;
}

/// Boxed handler shape used by [`MockInvoker`].
type MockHandler = Box<dyn Fn(IpcPayload) -> Result<IpcPayload, RemoteError>>;

/// Test invoker: stores a static `id → handler` map. Used by the
/// macro's expansion tests and by host-side unit tests that want
/// to exercise client stubs without booting a real daemon.
pub struct MockInvoker {
    handlers: std::collections::HashMap<String, MockHandler>,
}

impl MockInvoker {
    pub fn new() -> Self {
        Self {
            handlers: std::collections::HashMap::new(),
        }
    }

    pub fn on<F>(mut self, id: impl Into<String>, handler: F) -> Self
    where
        F: Fn(IpcPayload) -> Result<IpcPayload, RemoteError> + 'static,
    {
        self.handlers.insert(id.into(), Box::new(handler));
        self
    }
}

impl Default for MockInvoker {
    fn default() -> Self {
        Self::new()
    }
}

impl DaemonInvoker for MockInvoker {
    fn invoke(&self, id: &str, payload: IpcPayload) -> Result<IpcPayload, RemoteError> {
        match self.handlers.get(id) {
            Some(h) => h(payload),
            None => Err(RemoteError::Remote(format!("no handler for '{id}'"))),
        }
    }
}

// ───── #[daemon_fn] client-stub error carrier ────────────────────

/// Error type returned by the `<name>_client` stubs emitted by
/// `#[daemon_fn]`. Generic over the typed daemon-side error so the
/// stub preserves the same Result<Ok, Err> shape callers see on the
/// server side (modulo `Encode` / `Decode` / `Transport` arms for
/// the cross-process plumbing).
///
/// The macro lives in the proc-macro crate `prism-luau-derive`, but
/// proc-macro crates can't export regular types — so the carrier
/// lives here and is referenced as
/// `prism_core::reactive::ipc::DaemonFnError<E>` from the
/// macro expansion. Re-exported through `prism_luau_derive` for
/// convenience.
#[derive(Debug)]
pub enum DaemonFnError<E> {
    /// Args serialisation failed on the client side.
    Encode(String),
    /// Response deserialisation failed on the client side.
    Decode(String),
    /// Underlying transport reported a failure
    /// ([`RemoteError::Offline`] / `Timeout` / `PermissionDenied`).
    Transport(RemoteError),
    /// The daemon ran the handler and returned a typed `Err(_)`.
    /// Lifted via [`DaemonFnError::from_remote`] when the wire
    /// payload represents an `Err(E)`.
    Remote(E),
}

impl<E> DaemonFnError<E> {
    /// Lift a transport-level [`RemoteError`] into the typed error
    /// carrier. Used by the client-stub plumbing to keep the
    /// `Result<T, DaemonFnError<E>>` shape uniform.
    pub fn from_remote(err: RemoteError) -> Self {
        Self::Transport(err)
    }
}

impl<E: std::fmt::Display> std::fmt::Display for DaemonFnError<E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Encode(msg) => write!(f, "encode: {msg}"),
            Self::Decode(msg) => write!(f, "decode: {msg}"),
            Self::Transport(err) => write!(f, "transport: {err}"),
            Self::Remote(err) => write!(f, "remote: {err}"),
        }
    }
}

impl<E: std::fmt::Debug + std::fmt::Display> std::error::Error for DaemonFnError<E> {}

// ───── RelayInvoker — §3.4 of dioxus-inspiration.md ───────────────

/// Sync transport seam for **relay-routed** RPCs.
///
/// Mirrors [`DaemonInvoker`] but pointed at the WebSocket relay
/// (`prism_core::network::relay`) rather than the local daemon
/// sidecar. The `#[relay_fn]` proc-macro (Phase 7 of
/// `docs/dev/dioxus-inspiration.md` §3.4) emits client stubs that
/// take a `&dyn RelayInvoker` so test/host hookups stay
/// interchangeable.
///
/// **Boundary** with `DaemonInvoker`: same JSON-shaped payload, same
/// `RemoteError` carrier, different *destination*. A daemon call
/// reaches the sidecar on the same machine; a relay call traverses
/// a WebSocket envelope and may serve a remote peer. Two traits
/// rather than one so the macro family makes the destination
/// explicit at the call site — `read_file_client(&inv, …)` reads
/// type-locally as a daemon call, `query_portal_client(&rel, …)`
/// reads as a relay call.
pub trait RelayInvoker {
    /// Send a request to the relay for the function identified by
    /// `id` with the JSON-encoded `payload`, and return the
    /// JSON-encoded response (or a transport error).
    fn invoke(&self, id: &str, payload: IpcPayload) -> Result<IpcPayload, RemoteError>;
}

/// Test invoker: stores a static `id → handler` map. Sister of
/// [`MockInvoker`] for the relay transport. Used by `#[relay_fn]`
/// macro tests and host-side unit tests that exercise client stubs
/// without booting a relay.
pub struct MockRelayInvoker {
    handlers: std::collections::HashMap<String, MockHandler>,
}

impl MockRelayInvoker {
    pub fn new() -> Self {
        Self {
            handlers: std::collections::HashMap::new(),
        }
    }

    pub fn on<F>(mut self, id: impl Into<String>, handler: F) -> Self
    where
        F: Fn(IpcPayload) -> Result<IpcPayload, RemoteError> + 'static,
    {
        self.handlers.insert(id.into(), Box::new(handler));
        self
    }
}

impl Default for MockRelayInvoker {
    fn default() -> Self {
        Self::new()
    }
}

impl RelayInvoker for MockRelayInvoker {
    fn invoke(&self, id: &str, payload: IpcPayload) -> Result<IpcPayload, RemoteError> {
        match self.handlers.get(id) {
            Some(h) => h(payload),
            None => Err(RemoteError::Remote(format!("no handler for '{id}'"))),
        }
    }
}

/// Error type returned by the `<name>_client` stubs emitted by
/// `#[relay_fn]`. Same shape as [`DaemonFnError`] — kept as a
/// separate type so a function emitted by `#[relay_fn]` cannot be
/// accidentally consumed by code expecting a daemon-side error
/// (the two travel different transports and the call sites should
/// have to be explicit about which).
#[derive(Debug)]
pub enum RelayFnError<E> {
    /// Args serialisation failed on the client side.
    Encode(String),
    /// Response deserialisation failed on the client side.
    Decode(String),
    /// Underlying transport reported a failure (offline / timeout /
    /// permission denied).
    Transport(RemoteError),
    /// The relay handler returned a typed `Err(_)`.
    Remote(E),
}

impl<E> RelayFnError<E> {
    pub fn from_remote(err: RemoteError) -> Self {
        Self::Transport(err)
    }
}

impl<E: std::fmt::Display> std::fmt::Display for RelayFnError<E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Encode(msg) => write!(f, "encode: {msg}"),
            Self::Decode(msg) => write!(f, "decode: {msg}"),
            Self::Transport(err) => write!(f, "transport: {err}"),
            Self::Remote(err) => write!(f, "remote: {err}"),
        }
    }
}

impl<E: std::fmt::Debug + std::fmt::Display> std::error::Error for RelayFnError<E> {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reactive::{Effect, Owner};
    use serde_json::json;
    use std::cell::Cell;
    use std::rc::Rc;

    // ── RemoteState accessors ─────────────────────────────────

    #[test]
    fn last_known_returns_value_for_live_and_stale() {
        let s: RemoteState<i32> = RemoteState::Live(7);
        assert_eq!(s.last_known(), Some(&7));
        let s: RemoteState<i32> = RemoteState::Stale {
            value: 9,
            since_ms: 42,
        };
        assert_eq!(s.last_known(), Some(&9));
    }

    #[test]
    fn last_known_returns_none_for_loading() {
        let s: RemoteState<i32> = RemoteState::Loading;
        assert_eq!(s.last_known(), None);
    }

    #[test]
    fn last_known_passes_through_errored_history() {
        let s: RemoteState<i32> = RemoteState::Errored {
            last: Some(3),
            err: RemoteError::Timeout { millis: 1000 },
        };
        assert_eq!(s.last_known(), Some(&3));

        let s: RemoteState<i32> = RemoteState::Errored {
            last: None,
            err: RemoteError::Offline("boot".into()),
        };
        assert_eq!(s.last_known(), None);
    }

    #[test]
    fn state_predicates_match_variants() {
        let live: RemoteState<i32> = RemoteState::Live(0);
        assert!(live.is_live() && !live.is_loading() && !live.is_stale() && !live.is_errored());
        let loading: RemoteState<i32> = RemoteState::Loading;
        assert!(loading.is_loading());
        let stale: RemoteState<i32> = RemoteState::Stale {
            value: 1,
            since_ms: 0,
        };
        assert!(stale.is_stale());
        let err: RemoteState<i32> = RemoteState::Errored {
            last: None,
            err: RemoteError::Offline("x".into()),
        };
        assert!(err.is_errored());
    }

    #[test]
    fn remote_state_serde_round_trips() {
        let s: RemoteState<i32> = RemoteState::Stale {
            value: 42,
            since_ms: 1234567890,
        };
        let json = serde_json::to_string(&s).unwrap();
        let back: RemoteState<i32> = serde_json::from_str(&json).unwrap();
        assert_eq!(s, back);
    }

    // ── RemoteSignal lifecycle ────────────────────────────────

    #[test]
    fn remote_signal_starts_loading() {
        let owner = Owner::new();
        let r: RemoteSignal<i32> = RemoteSignal::new(&owner);
        assert!(r.peek_state().is_loading());
        assert_eq!(r.last_known(), None);
    }

    #[test]
    fn set_live_transitions_carrier() {
        let owner = Owner::new();
        let r: RemoteSignal<i32> = RemoteSignal::new(&owner);
        r.set_live(7);
        assert_eq!(r.state(), RemoteState::Live(7));
        assert_eq!(r.last_known(), Some(7));
        assert_eq!(r.try_read(), Ok(7));
    }

    #[test]
    fn set_errored_preserves_last_known() {
        let owner = Owner::new();
        let r: RemoteSignal<i32> = RemoteSignal::new(&owner);
        r.set_live(99);
        r.set_errored(RemoteError::Offline("link down".into()));
        match r.state() {
            RemoteState::Errored { last, err } => {
                assert_eq!(last, Some(99));
                assert!(matches!(err, RemoteError::Offline(_)));
            }
            other => panic!("expected Errored, got {other:?}"),
        }
        // `last_known` still returns the cached value — UI keeps
        // displaying the previous reading with an error indicator.
        assert_eq!(r.last_known(), Some(99));
        // `try_read` reflects the error.
        assert!(matches!(r.try_read(), Err(RemoteError::Offline(_))));
    }

    #[test]
    fn set_errored_with_no_history_is_none() {
        let owner = Owner::new();
        let r: RemoteSignal<i32> = RemoteSignal::new(&owner);
        r.set_errored(RemoteError::PermissionDenied("token expired".into()));
        match r.state() {
            RemoteState::Errored { last, .. } => assert_eq!(last, None),
            _ => panic!("expected Errored"),
        }
        assert_eq!(r.last_known(), None);
    }

    #[test]
    fn reactive_effect_wakes_on_state_transition() {
        // The reactive graph propagates connectivity transitions —
        // the "disconnected badge" / "stale tint" use case from §8
        // of the plan.
        let owner = Owner::new();
        let r: RemoteSignal<i32> = RemoteSignal::new(&owner);
        let r_for = r;
        let states: Rc<std::cell::RefCell<Vec<&'static str>>> =
            Rc::new(std::cell::RefCell::new(Vec::new()));
        let states_for = Rc::clone(&states);
        let _e = Effect::new(move || {
            let label = r_for.signal().read(|s| match s {
                RemoteState::Loading => "loading",
                RemoteState::Live(_) => "live",
                RemoteState::Stale { .. } => "stale",
                RemoteState::Errored { .. } => "errored",
            });
            states_for.borrow_mut().push(label);
        });
        assert_eq!(&*states.borrow(), &["loading"]);

        r.set_live(1);
        assert_eq!(&*states.borrow(), &["loading", "live"]);

        r.set_state(RemoteState::Stale {
            value: 1,
            since_ms: now_epoch_ms(),
        });
        assert_eq!(&*states.borrow(), &["loading", "live", "stale"]);

        r.set_errored(RemoteError::Timeout { millis: 500 });
        assert_eq!(&*states.borrow(), &["loading", "live", "stale", "errored"]);
    }

    #[test]
    fn reconnect_with_same_value_still_wakes_effect_on_outer_enum_change() {
        // §8's "Reconnect with the same value" edge case: the inner
        // T is unchanged but the connectivity went Stale → Live, so
        // the outer enum is different. PartialEq is gated on the
        // *whole carrier*, so subscribers do wake.
        let owner = Owner::new();
        let r: RemoteSignal<i32> = RemoteSignal::new(&owner);
        r.set_live(7);

        let runs = Rc::new(Cell::new(0));
        let runs_for = Rc::clone(&runs);
        let r_for = r;
        let _e = Effect::new(move || {
            let _ = r_for.signal().get();
            runs_for.set(runs_for.get() + 1);
        });
        assert_eq!(runs.get(), 1);

        // Go stale on the same value.
        r.set_state(RemoteState::Stale {
            value: 7,
            since_ms: 1,
        });
        assert_eq!(runs.get(), 2);

        // Reconnect, same value — outer enum changes Stale→Live.
        r.set_live(7);
        assert_eq!(runs.get(), 3);
    }

    // ── DaemonInvoker / MockInvoker ───────────────────────────

    #[test]
    fn mock_invoker_dispatches_by_id() {
        let inv = MockInvoker::new().on("greet", |args| {
            let name = args.get("name").and_then(|v| v.as_str()).unwrap_or("world");
            Ok(json!({ "hello": name }))
        });
        let resp = inv.invoke("greet", json!({ "name": "Prism" })).unwrap();
        assert_eq!(resp, json!({ "hello": "Prism" }));
    }

    #[test]
    fn mock_invoker_returns_remote_error_for_unknown_id() {
        let inv = MockInvoker::new();
        let err = inv.invoke("missing", json!(null)).unwrap_err();
        match err {
            RemoteError::Remote(msg) => assert!(msg.contains("missing")),
            other => panic!("expected Remote, got {other:?}"),
        }
    }
}
