//! `reactive::net` — Phase 7 of the Dioxus-inspired reactive
//! overhaul (`docs/dev/dioxus-inspiration.md`).
//!
//! Three reactive primitives, one for each network scope on the
//! ladder:
//!
//! * [`FederatedSignal<T>`] — value lives in a relay's federation
//!   feed; writes propagate cross-relay via the existing
//!   `network::relay::modules::federation` module. Eventually
//!   consistent; reconciles via Loro CRDT when `T` is a CRDT
//!   container.
//! * [`PeerSignal<T>`] — value lives in a WebRTC data channel
//!   maintained by the `signaling` module. Lower latency, no
//!   eventual-consistency guarantee — suited for presence /
//!   cursor / live-edit traffic.
//! * [`RelaySignal<T>`] — value's canonical store is a specific
//!   relay (one writer, many readers). Subscribers receive push
//!   updates over a relay-managed WebSocket.
//!
//! All three share the [`crate::reactive::ipc::RemoteState`]
//! carrier, the [`crate::reactive::ipc::RemoteSignal`] wrapper, and
//! the same `read` / `last_known` / `try_read` ergonomics. The
//! difference is **which transport populates which state under
//! which conditions** — and that's expressed here by an injectable
//! trait seam, one per scope.
//!
//! ### Why traits, not direct module references
//!
//! The actual federation / signaling / relay modules live in
//! `prism-core::network::*` and require host configuration (relay
//! pool, peer trust graph, capability tokens). Wiring those at the
//! type level here would create a transitive dep cycle in the
//! workspace. Instead each scope exposes a transport trait
//! (`FederationTransport`, `PeerTransport`, `RelayTransport`); the
//! host or test wires the trait impl. The `Mock*Transport` types
//! ship for tests and demonstrate the expected shape.

use std::cell::RefCell;
use std::rc::Rc;

use serde::{de::DeserializeOwned, Serialize};

use crate::reactive::ipc::{IpcPayload, RemoteError, RemoteSignal, RemoteState};
use crate::reactive::Owner;

// ───── Federated (cross-relay) ───────────────────────────────────

/// Address of a subscription on the federation bus. The federation
/// module already keys subscriptions by `(relay_id, topic)`; this
/// is the reactive-layer mirror.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FederatedSubscription {
    pub topic: String,
}

/// Pluggable seam for the federation transport. Production code
/// wires this to `network::relay::modules::federation`; tests use
/// [`MockFederationTransport`].
pub trait FederationTransport {
    /// Open a subscription for `topic`. Returns a handle the caller
    /// stores for the lifetime of the signal; dropping it through
    /// [`FederationTransport::close`] tears the subscription down.
    fn subscribe(&self, topic: &str) -> Result<FederatedSubscription, RemoteError>;

    /// Push a new value into the federation bus for the topic. The
    /// transport is responsible for fan-out to other relays.
    fn publish(&self, topic: &str, payload: IpcPayload) -> Result<(), RemoteError>;

    /// Tear down the subscription. The transport stops pushing
    /// updates to its associated [`FederatedSignal`].
    fn close(&self, sub: &FederatedSubscription) -> Result<(), RemoteError>;
}

/// Reactive signal whose canonical value lives on the federation
/// bus. Writes propagate cross-relay; reads return the local cache,
/// which the transport refreshes asynchronously.
pub struct FederatedSignal<T: 'static + Clone + Serialize + DeserializeOwned> {
    remote: RemoteSignal<T>,
    topic: String,
    transport: Rc<dyn FederationTransport>,
    #[allow(dead_code)]
    sub: FederatedSubscription,
}

impl<T: 'static + Clone + Serialize + DeserializeOwned> FederatedSignal<T> {
    /// Open a new federated signal on `topic`, allocating its
    /// `RemoteSignal` in `owner`. The transport subscribes; once
    /// the first published value arrives, the carrier transitions
    /// Loading → Live.
    pub fn subscribe(
        owner: &Owner,
        topic: impl Into<String>,
        transport: Rc<dyn FederationTransport>,
    ) -> Result<Self, RemoteError> {
        let topic = topic.into();
        let sub = transport.subscribe(&topic)?;
        let remote = RemoteSignal::new(owner);
        Ok(Self {
            remote,
            topic,
            transport,
            sub,
        })
    }

    /// The reactive carrier. Use the [`RemoteSignal`] API to read
    /// the current `RemoteState<T>` (subscribing into the reactive
    /// graph).
    pub fn remote(&self) -> RemoteSignal<T> {
        self.remote
    }

    pub fn topic(&self) -> &str {
        &self.topic
    }

    /// Publish a new value to the federation bus. The transport
    /// fans this out to other relays; the local carrier transitions
    /// to `Live(v)` immediately (optimistic).
    pub fn publish(&self, value: T) -> Result<(), RemoteError> {
        let payload =
            serde_json::to_value(&value).map_err(|e| RemoteError::Decode(e.to_string()))?;
        self.transport.publish(&self.topic, payload)?;
        self.remote.set_live(value);
        Ok(())
    }

    /// The transport delivered a new value from another relay.
    /// Host-side plumbing calls this when the federation module's
    /// receive loop fires.
    pub fn ingest_remote(&self, payload: IpcPayload) -> Result<(), RemoteError> {
        let value: T =
            serde_json::from_value(payload).map_err(|e| RemoteError::Decode(e.to_string()))?;
        self.remote.set_live(value);
        Ok(())
    }

    /// Host-side plumbing reports that the subscription's transport
    /// is offline; the carrier transitions to `Stale` if a value
    /// existed or `Errored` otherwise.
    pub fn mark_disconnected(&self) {
        let cur = self.remote.peek_state();
        match cur {
            RemoteState::Live(v) => {
                self.remote.set_state(RemoteState::Stale {
                    value: v,
                    since_ms: crate::reactive::ipc::now_epoch_ms(),
                });
            }
            RemoteState::Stale { .. } => {}
            _ => {
                self.remote
                    .set_errored(RemoteError::Offline("federation".into()));
            }
        }
    }
}

impl<T: 'static + Clone + Serialize + DeserializeOwned> Drop for FederatedSignal<T> {
    fn drop(&mut self) {
        // Best-effort teardown — host code can also call `close`
        // explicitly via the transport handle.
        let _ = self.transport.close(&self.sub);
    }
}

// ───── Peer (WebRTC data channel) ────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PeerSubscription {
    pub peer_id: String,
    pub channel: String,
}

pub trait PeerTransport {
    fn subscribe(&self, peer_id: &str, channel: &str) -> Result<PeerSubscription, RemoteError>;
    fn send(&self, sub: &PeerSubscription, payload: IpcPayload) -> Result<(), RemoteError>;
    fn close(&self, sub: &PeerSubscription) -> Result<(), RemoteError>;
}

/// Reactive signal mirrored across a WebRTC data channel to a
/// specific peer. Lower latency than [`FederatedSignal`] (no
/// store-and-forward), but no eventual-consistency guarantee —
/// suitable for presence / cursor / live-edit channels.
pub struct PeerSignal<T: 'static + Clone + Serialize + DeserializeOwned> {
    remote: RemoteSignal<T>,
    transport: Rc<dyn PeerTransport>,
    sub: PeerSubscription,
}

impl<T: 'static + Clone + Serialize + DeserializeOwned> PeerSignal<T> {
    pub fn subscribe(
        owner: &Owner,
        peer_id: impl Into<String>,
        channel: impl Into<String>,
        transport: Rc<dyn PeerTransport>,
    ) -> Result<Self, RemoteError> {
        let peer_id = peer_id.into();
        let channel = channel.into();
        let sub = transport.subscribe(&peer_id, &channel)?;
        Ok(Self {
            remote: RemoteSignal::new(owner),
            transport,
            sub,
        })
    }

    pub fn remote(&self) -> RemoteSignal<T> {
        self.remote
    }

    pub fn peer_id(&self) -> &str {
        &self.sub.peer_id
    }

    pub fn channel(&self) -> &str {
        &self.sub.channel
    }

    pub fn send(&self, value: T) -> Result<(), RemoteError> {
        let payload =
            serde_json::to_value(&value).map_err(|e| RemoteError::Decode(e.to_string()))?;
        self.transport.send(&self.sub, payload)?;
        self.remote.set_live(value);
        Ok(())
    }

    pub fn ingest_remote(&self, payload: IpcPayload) -> Result<(), RemoteError> {
        let value: T =
            serde_json::from_value(payload).map_err(|e| RemoteError::Decode(e.to_string()))?;
        self.remote.set_live(value);
        Ok(())
    }

    pub fn mark_peer_offline(&self) {
        let cur = self.remote.peek_state();
        match cur {
            RemoteState::Live(v) => self.remote.set_state(RemoteState::Stale {
                value: v,
                since_ms: crate::reactive::ipc::now_epoch_ms(),
            }),
            _ => self
                .remote
                .set_errored(RemoteError::Offline(format!("peer {}", self.sub.peer_id))),
        }
    }
}

impl<T: 'static + Clone + Serialize + DeserializeOwned> Drop for PeerSignal<T> {
    fn drop(&mut self) {
        let _ = self.transport.close(&self.sub);
    }
}

// ───── Relay (one-writer / many-reader push) ─────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RelaySubscription {
    pub relay_id: String,
    pub stream: String,
}

pub trait RelayTransport {
    fn subscribe(&self, relay_id: &str, stream: &str) -> Result<RelaySubscription, RemoteError>;
    fn write(&self, sub: &RelaySubscription, payload: IpcPayload) -> Result<(), RemoteError>;
    fn close(&self, sub: &RelaySubscription) -> Result<(), RemoteError>;
}

/// Reactive signal whose canonical store is a specific relay. One
/// writer (the relay), many readers (subscribed clients). Updates
/// propagate via a relay-managed WebSocket.
pub struct RelaySignal<T: 'static + Clone + Serialize + DeserializeOwned> {
    remote: RemoteSignal<T>,
    transport: Rc<dyn RelayTransport>,
    sub: RelaySubscription,
}

impl<T: 'static + Clone + Serialize + DeserializeOwned> RelaySignal<T> {
    pub fn subscribe(
        owner: &Owner,
        relay_id: impl Into<String>,
        stream: impl Into<String>,
        transport: Rc<dyn RelayTransport>,
    ) -> Result<Self, RemoteError> {
        let relay_id = relay_id.into();
        let stream = stream.into();
        let sub = transport.subscribe(&relay_id, &stream)?;
        Ok(Self {
            remote: RemoteSignal::new(owner),
            transport,
            sub,
        })
    }

    pub fn remote(&self) -> RemoteSignal<T> {
        self.remote
    }

    pub fn relay_id(&self) -> &str {
        &self.sub.relay_id
    }

    pub fn stream(&self) -> &str {
        &self.sub.stream
    }

    /// Push a new value to the relay (capability-token-gated by
    /// the relay-side `capability_tokens` module). The carrier
    /// transitions to `Live(v)` on success.
    pub fn write(&self, value: T) -> Result<(), RemoteError> {
        let payload =
            serde_json::to_value(&value).map_err(|e| RemoteError::Decode(e.to_string()))?;
        self.transport.write(&self.sub, payload)?;
        self.remote.set_live(value);
        Ok(())
    }

    /// The relay's push channel delivered a new value. Host-side
    /// plumbing calls this when the WebSocket receive loop fires.
    pub fn ingest_push(&self, payload: IpcPayload) -> Result<(), RemoteError> {
        let value: T =
            serde_json::from_value(payload).map_err(|e| RemoteError::Decode(e.to_string()))?;
        self.remote.set_live(value);
        Ok(())
    }
}

impl<T: 'static + Clone + Serialize + DeserializeOwned> Drop for RelaySignal<T> {
    fn drop(&mut self) {
        let _ = self.transport.close(&self.sub);
    }
}

// ───── Mock transports for tests ─────────────────────────────────

struct MockState<S> {
    subs: Vec<S>,
    closed: Vec<S>,
    sent: Vec<(S, IpcPayload)>,
}

impl<S> Default for MockState<S> {
    fn default() -> Self {
        Self {
            subs: Vec::new(),
            closed: Vec::new(),
            sent: Vec::new(),
        }
    }
}

/// Test seam: records every subscribe/publish/close call so unit
/// tests can assert against the transport's interaction trace.
pub struct MockFederationTransport {
    state: RefCell<MockState<FederatedSubscription>>,
    fail_subscribe: bool,
}

impl MockFederationTransport {
    pub fn new() -> Self {
        Self {
            state: RefCell::new(MockState::default()),
            fail_subscribe: false,
        }
    }

    pub fn failing() -> Self {
        Self {
            state: RefCell::new(MockState::default()),
            fail_subscribe: true,
        }
    }

    pub fn published(&self) -> Vec<(FederatedSubscription, IpcPayload)> {
        self.state.borrow().sent.clone()
    }

    pub fn closed(&self) -> Vec<FederatedSubscription> {
        self.state.borrow().closed.clone()
    }
}

impl Default for MockFederationTransport {
    fn default() -> Self {
        Self::new()
    }
}

impl FederationTransport for MockFederationTransport {
    fn subscribe(&self, topic: &str) -> Result<FederatedSubscription, RemoteError> {
        if self.fail_subscribe {
            return Err(RemoteError::Offline("mock failing".into()));
        }
        let sub = FederatedSubscription {
            topic: topic.into(),
        };
        self.state.borrow_mut().subs.push(sub.clone());
        Ok(sub)
    }

    fn publish(&self, topic: &str, payload: IpcPayload) -> Result<(), RemoteError> {
        self.state.borrow_mut().sent.push((
            FederatedSubscription {
                topic: topic.into(),
            },
            payload,
        ));
        Ok(())
    }

    fn close(&self, sub: &FederatedSubscription) -> Result<(), RemoteError> {
        self.state.borrow_mut().closed.push(sub.clone());
        Ok(())
    }
}

pub struct MockPeerTransport {
    state: RefCell<MockState<PeerSubscription>>,
}

impl MockPeerTransport {
    pub fn new() -> Self {
        Self {
            state: RefCell::new(MockState::default()),
        }
    }

    pub fn sent(&self) -> Vec<(PeerSubscription, IpcPayload)> {
        self.state.borrow().sent.clone()
    }

    pub fn closed(&self) -> Vec<PeerSubscription> {
        self.state.borrow().closed.clone()
    }
}

impl Default for MockPeerTransport {
    fn default() -> Self {
        Self::new()
    }
}

impl PeerTransport for MockPeerTransport {
    fn subscribe(&self, peer_id: &str, channel: &str) -> Result<PeerSubscription, RemoteError> {
        let sub = PeerSubscription {
            peer_id: peer_id.into(),
            channel: channel.into(),
        };
        self.state.borrow_mut().subs.push(sub.clone());
        Ok(sub)
    }

    fn send(&self, sub: &PeerSubscription, payload: IpcPayload) -> Result<(), RemoteError> {
        self.state.borrow_mut().sent.push((sub.clone(), payload));
        Ok(())
    }

    fn close(&self, sub: &PeerSubscription) -> Result<(), RemoteError> {
        self.state.borrow_mut().closed.push(sub.clone());
        Ok(())
    }
}

pub struct MockRelayTransport {
    state: RefCell<MockState<RelaySubscription>>,
}

impl MockRelayTransport {
    pub fn new() -> Self {
        Self {
            state: RefCell::new(MockState::default()),
        }
    }

    pub fn written(&self) -> Vec<(RelaySubscription, IpcPayload)> {
        self.state.borrow().sent.clone()
    }

    pub fn closed(&self) -> Vec<RelaySubscription> {
        self.state.borrow().closed.clone()
    }
}

impl Default for MockRelayTransport {
    fn default() -> Self {
        Self::new()
    }
}

impl RelayTransport for MockRelayTransport {
    fn subscribe(&self, relay_id: &str, stream: &str) -> Result<RelaySubscription, RemoteError> {
        let sub = RelaySubscription {
            relay_id: relay_id.into(),
            stream: stream.into(),
        };
        self.state.borrow_mut().subs.push(sub.clone());
        Ok(sub)
    }

    fn write(&self, sub: &RelaySubscription, payload: IpcPayload) -> Result<(), RemoteError> {
        self.state.borrow_mut().sent.push((sub.clone(), payload));
        Ok(())
    }

    fn close(&self, sub: &RelaySubscription) -> Result<(), RemoteError> {
        self.state.borrow_mut().closed.push(sub.clone());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reactive::{Effect, Owner};
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    struct Cursor {
        x: f64,
        y: f64,
    }

    // ── FederatedSignal ──────────────────────────────────────

    #[test]
    fn federated_subscribe_starts_loading() {
        let owner = Owner::new();
        let transport = Rc::new(MockFederationTransport::new());
        let sig: FederatedSignal<i32> =
            FederatedSignal::subscribe(&owner, "counts", transport).expect("subscribed");
        assert_eq!(sig.topic(), "counts");
        assert!(sig.remote().peek_state().is_loading());
    }

    #[test]
    fn federated_publish_transitions_to_live_and_calls_transport() {
        let owner = Owner::new();
        let transport = Rc::new(MockFederationTransport::new());
        let t_for_check = Rc::clone(&transport);
        let sig: FederatedSignal<i32> =
            FederatedSignal::subscribe(&owner, "counts", transport).expect("ok");
        sig.publish(7).expect("publish");
        assert_eq!(sig.remote().last_known(), Some(7));
        let pubs = t_for_check.published();
        assert_eq!(pubs.len(), 1);
        assert_eq!(pubs[0].0.topic, "counts");
        assert_eq!(pubs[0].1, serde_json::json!(7));
    }

    #[test]
    fn federated_ingest_remote_populates_live() {
        let owner = Owner::new();
        let transport = Rc::new(MockFederationTransport::new());
        let sig: FederatedSignal<i32> =
            FederatedSignal::subscribe(&owner, "counts", transport).expect("ok");
        sig.ingest_remote(serde_json::json!(42)).expect("ingest");
        assert_eq!(sig.remote().last_known(), Some(42));
        assert!(sig.remote().peek_state().is_live());
    }

    #[test]
    fn federated_mark_disconnected_keeps_last_known() {
        let owner = Owner::new();
        let transport = Rc::new(MockFederationTransport::new());
        let sig: FederatedSignal<i32> =
            FederatedSignal::subscribe(&owner, "counts", transport).expect("ok");
        sig.ingest_remote(serde_json::json!(99)).unwrap();
        sig.mark_disconnected();
        assert!(sig.remote().peek_state().is_stale());
        assert_eq!(sig.remote().last_known(), Some(99));
    }

    #[test]
    fn federated_subscribe_propagates_transport_error() {
        let owner = Owner::new();
        let transport = Rc::new(MockFederationTransport::failing());
        let result = FederatedSignal::<i32>::subscribe(&owner, "x", transport);
        // Avoid `unwrap_err` so we don't require `FederatedSignal:
        // Debug` (its inner `Rc<dyn Trait>` carrier doesn't impl
        // Debug, which is fine for production code).
        match result {
            Ok(_) => panic!("subscribe should have failed"),
            Err(RemoteError::Offline(_)) => {}
            Err(other) => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn federated_drop_calls_close() {
        let owner = Owner::new();
        let transport = Rc::new(MockFederationTransport::new());
        let t_for_check = Rc::clone(&transport);
        {
            let _sig: FederatedSignal<i32> =
                FederatedSignal::subscribe(&owner, "x", transport).expect("ok");
        }
        let closed = t_for_check.closed();
        assert_eq!(closed.len(), 1);
        assert_eq!(closed[0].topic, "x");
    }

    // ── PeerSignal ───────────────────────────────────────────

    #[test]
    fn peer_subscribe_records_peer_and_channel() {
        let owner = Owner::new();
        let transport = Rc::new(MockPeerTransport::new());
        let sig: PeerSignal<Cursor> =
            PeerSignal::subscribe(&owner, "peer-7", "cursor", transport).expect("ok");
        assert_eq!(sig.peer_id(), "peer-7");
        assert_eq!(sig.channel(), "cursor");
    }

    #[test]
    fn peer_send_writes_through_transport_and_updates_carrier() {
        let owner = Owner::new();
        let transport = Rc::new(MockPeerTransport::new());
        let t_for_check = Rc::clone(&transport);
        let sig: PeerSignal<Cursor> =
            PeerSignal::subscribe(&owner, "peer-1", "cursor", transport).expect("ok");
        sig.send(Cursor { x: 1.0, y: 2.0 }).unwrap();
        let sent = t_for_check.sent();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].1, serde_json::json!({ "x": 1.0, "y": 2.0 }));
        assert_eq!(sig.remote().last_known(), Some(Cursor { x: 1.0, y: 2.0 }));
    }

    #[test]
    fn peer_ingest_remote_populates_live() {
        let owner = Owner::new();
        let transport = Rc::new(MockPeerTransport::new());
        let sig: PeerSignal<Cursor> =
            PeerSignal::subscribe(&owner, "peer-1", "cursor", transport).expect("ok");
        sig.ingest_remote(serde_json::json!({"x": 3.0, "y": 4.0}))
            .unwrap();
        assert_eq!(sig.remote().last_known(), Some(Cursor { x: 3.0, y: 4.0 }));
    }

    #[test]
    fn peer_mark_offline_after_live_value_becomes_stale() {
        let owner = Owner::new();
        let transport = Rc::new(MockPeerTransport::new());
        let sig: PeerSignal<Cursor> =
            PeerSignal::subscribe(&owner, "peer-1", "cursor", transport).expect("ok");
        sig.ingest_remote(serde_json::json!({"x": 1.0, "y": 1.0}))
            .unwrap();
        sig.mark_peer_offline();
        assert!(sig.remote().peek_state().is_stale());
        assert_eq!(sig.remote().last_known(), Some(Cursor { x: 1.0, y: 1.0 }));
    }

    // ── RelaySignal ──────────────────────────────────────────

    #[test]
    fn relay_subscribe_records_relay_and_stream() {
        let owner = Owner::new();
        let transport = Rc::new(MockRelayTransport::new());
        let sig: RelaySignal<i32> =
            RelaySignal::subscribe(&owner, "relay-main", "tasks", transport).expect("ok");
        assert_eq!(sig.relay_id(), "relay-main");
        assert_eq!(sig.stream(), "tasks");
    }

    #[test]
    fn relay_write_pushes_value_through_transport() {
        let owner = Owner::new();
        let transport = Rc::new(MockRelayTransport::new());
        let t_for_check = Rc::clone(&transport);
        let sig: RelaySignal<i32> =
            RelaySignal::subscribe(&owner, "relay", "tasks", transport).expect("ok");
        sig.write(7).unwrap();
        let written = t_for_check.written();
        assert_eq!(written.len(), 1);
        assert_eq!(written[0].1, serde_json::json!(7));
        assert_eq!(sig.remote().last_known(), Some(7));
    }

    #[test]
    fn relay_ingest_push_wakes_reactive_subscribers() {
        let owner = Owner::new();
        let transport = Rc::new(MockRelayTransport::new());
        let sig: RelaySignal<i32> =
            RelaySignal::subscribe(&owner, "relay", "tasks", transport).expect("ok");
        let remote = sig.remote();

        let fires = Rc::new(std::cell::Cell::new(0));
        let fc = Rc::clone(&fires);
        let _e = Effect::new(move || {
            let _ = remote.signal().get();
            fc.set(fc.get() + 1);
        });
        assert_eq!(fires.get(), 1, "initial run");

        sig.ingest_push(serde_json::json!(42)).unwrap();
        assert_eq!(fires.get(), 2);
        assert_eq!(sig.remote().last_known(), Some(42));
    }
}
