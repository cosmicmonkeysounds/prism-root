//! `network::relay::connection` — relay connection state machine.
//!
//! Port of `relay/relay-connection.ts`. Manages the lifecycle of a
//! single relay connection: connecting, handshake, keep-alive pings,
//! reconnection attempts, and graceful shutdown.

use std::cell::RefCell;
use std::rc::Rc;

use serde::{Deserialize, Serialize};

use super::message::{MessageId, RelayEnvelope, RelayMessageKind};
use super::transport::TransportError;
use super::types::{RelayConfig, RelayInfo};

/// Connection state — higher-level than transport state, includes
/// handshake and reconnection semantics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RelayConnectionState {
    /// Not connected
    #[default]
    Disconnected,
    /// TCP/WebSocket connecting
    Connecting,
    /// Transport connected, waiting for handshake
    Handshaking,
    /// Fully connected and authenticated
    Connected,
    /// Graceful disconnect in progress
    Disconnecting,
    /// Waiting to reconnect (with backoff)
    Reconnecting,
    /// Connection permanently failed
    Failed,
}

impl RelayConnectionState {
    pub fn is_connected(&self) -> bool {
        matches!(self, Self::Connected)
    }

    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Disconnected | Self::Failed)
    }

    pub fn can_send(&self) -> bool {
        matches!(self, Self::Connected)
    }
}

/// Connection event emitted to subscribers.
#[derive(Debug, Clone, PartialEq)]
pub enum ConnectionEvent {
    StateChanged(RelayConnectionState),
    MessageReceived(RelayEnvelope),
    HandshakeComplete(RelayInfo),
    Error(TransportError),
    ReconnectScheduled { attempt: u32, delay_ms: u64 },
}

pub type ConnectionListener = Box<dyn FnMut(&ConnectionEvent)>;

/// Statistics for a relay connection.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionStats {
    pub messages_sent: u64,
    pub messages_received: u64,
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub reconnect_count: u32,
    pub last_ping_rtt_ms: Option<u64>,
}

/// Opaque subscription handle.
pub struct ConnectionSubscription {
    inner: Rc<RefCell<ListenerBus>>,
    id: u64,
    active: bool,
}

impl ConnectionSubscription {
    pub fn unsubscribe(mut self) {
        if self.active {
            self.active = false;
            self.inner.borrow_mut().remove(self.id);
        }
    }
}

impl Drop for ConnectionSubscription {
    fn drop(&mut self) {
        if self.active {
            self.inner.borrow_mut().remove(self.id);
        }
    }
}

struct ListenerBus {
    next_id: u64,
    entries: Vec<(u64, ConnectionListener)>,
}

impl ListenerBus {
    fn new() -> Self {
        Self {
            next_id: 0,
            entries: Vec::new(),
        }
    }

    fn add(&mut self, listener: ConnectionListener) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        self.entries.push((id, listener));
        id
    }

    fn remove(&mut self, id: u64) {
        self.entries.retain(|(i, _)| *i != id);
    }

    fn notify(&mut self, event: &ConnectionEvent) {
        for (_, listener) in &mut self.entries {
            listener(event);
        }
    }

    fn clear(&mut self) {
        self.entries.clear();
    }
}

/// Pending request waiting for a response.
struct PendingRequest {
    id: MessageId,
    callback: Box<dyn FnOnce(Result<RelayEnvelope, TransportError>)>,
}

/// Inner connection state (behind Rc<RefCell<...>>).
struct Inner {
    config: RelayConfig,
    state: RelayConnectionState,
    relay_info: Option<RelayInfo>,
    stats: ConnectionStats,
    reconnect_attempt: u32,
    pending_requests: Vec<PendingRequest>,
    listeners: Rc<RefCell<ListenerBus>>,
    last_ping_id: Option<MessageId>,
    ping_sent_at: Option<u64>,
}

impl Inner {
    fn set_state(&mut self, state: RelayConnectionState) {
        if self.state != state {
            self.state = state;
            self.listeners
                .borrow_mut()
                .notify(&ConnectionEvent::StateChanged(state));
        }
    }

    fn handle_message(&mut self, envelope: RelayEnvelope) {
        self.stats.messages_received += 1;

        // Handle pong correlation
        if envelope.kind == RelayMessageKind::Pong {
            if let (Some(ref last_ping), Some(_sent_at)) = (&self.last_ping_id, self.ping_sent_at) {
                if envelope.correlation_id.as_ref() == Some(last_ping) {
                    // Calculate RTT — would need a timer provider here
                    // For now, just clear the pending ping
                    self.last_ping_id = None;
                    self.ping_sent_at = None;
                }
            }
        }

        // Handle handshake ack
        if envelope.kind == RelayMessageKind::HandshakeAck {
            if let Some(ref payload) = envelope.payload {
                if let Ok(info) = serde_json::from_value::<RelayInfo>(payload.clone()) {
                    self.relay_info = Some(info.clone());
                    self.set_state(RelayConnectionState::Connected);
                    self.listeners
                        .borrow_mut()
                        .notify(&ConnectionEvent::HandshakeComplete(info));
                    return;
                }
            }
        }

        // Check for pending request correlation
        if let Some(ref corr) = envelope.correlation_id {
            if let Some(idx) = self.pending_requests.iter().position(|p| &p.id == corr) {
                let pending = self.pending_requests.remove(idx);
                (pending.callback)(Ok(envelope.clone()));
                return;
            }
        }

        // Emit to listeners
        self.listeners
            .borrow_mut()
            .notify(&ConnectionEvent::MessageReceived(envelope));
    }
}

/// A single relay connection.
///
/// This is the pure-logic layer — it tracks state, manages subscriptions,
/// and processes incoming messages. The actual network I/O is driven by
/// the host through `process_*` methods.
pub struct RelayConnection {
    inner: Rc<RefCell<Inner>>,
}

impl RelayConnection {
    /// Create a new connection with the given config.
    pub fn new(config: RelayConfig) -> Self {
        let listeners = Rc::new(RefCell::new(ListenerBus::new()));
        Self {
            inner: Rc::new(RefCell::new(Inner {
                config,
                state: RelayConnectionState::Disconnected,
                relay_info: None,
                stats: ConnectionStats::default(),
                reconnect_attempt: 0,
                pending_requests: Vec::new(),
                listeners,
                last_ping_id: None,
                ping_sent_at: None,
            })),
        }
    }

    /// Current connection state.
    pub fn state(&self) -> RelayConnectionState {
        self.inner.borrow().state
    }

    /// Connection configuration.
    pub fn config(&self) -> RelayConfig {
        self.inner.borrow().config.clone()
    }

    /// Relay info from handshake (None if not connected).
    pub fn relay_info(&self) -> Option<RelayInfo> {
        self.inner.borrow().relay_info.clone()
    }

    /// Connection statistics.
    pub fn stats(&self) -> ConnectionStats {
        self.inner.borrow().stats.clone()
    }

    /// Subscribe to connection events.
    pub fn subscribe(&self, listener: ConnectionListener) -> ConnectionSubscription {
        let listeners = Rc::clone(&self.inner.borrow().listeners);
        let id = listeners.borrow_mut().add(listener);
        ConnectionSubscription {
            inner: listeners,
            id,
            active: true,
        }
    }

    // ── Host-driven lifecycle ───────────────────────────────────────────

    /// Called by host when transport starts connecting.
    pub fn process_connecting(&self) {
        let mut inner = self.inner.borrow_mut();
        inner.set_state(RelayConnectionState::Connecting);
    }

    /// Called by host when transport is connected, before handshake.
    pub fn process_connected(&self) {
        let mut inner = self.inner.borrow_mut();
        inner.set_state(RelayConnectionState::Handshaking);
    }

    /// Called by host when a message is received.
    pub fn process_message(&self, envelope: RelayEnvelope) {
        self.inner.borrow_mut().handle_message(envelope);
    }

    /// Called by host when transport disconnects or errors.
    pub fn process_disconnect(&self, error: Option<TransportError>) {
        let mut inner = self.inner.borrow_mut();

        // Notify pending requests of failure
        let pending = std::mem::take(&mut inner.pending_requests);
        let err = error
            .clone()
            .unwrap_or_else(|| TransportError::remote_closed(None, None));
        for p in pending {
            (p.callback)(Err(err.clone()));
        }

        // Clear relay info
        inner.relay_info = None;
        inner.last_ping_id = None;
        inner.ping_sent_at = None;

        if let Some(ref e) = error {
            inner
                .listeners
                .borrow_mut()
                .notify(&ConnectionEvent::Error(e.clone()));
        }

        // Decide whether to reconnect
        let should_reconnect = inner.config.auto_reconnect
            && error.as_ref().is_none_or(|e| e.is_recoverable())
            && (inner.config.max_reconnect_attempts == 0
                || inner.reconnect_attempt < inner.config.max_reconnect_attempts);

        if should_reconnect {
            inner.reconnect_attempt += 1;
            inner.stats.reconnect_count += 1;
            let delay = inner.config.reconnect_delay_ms * (inner.reconnect_attempt as u64);
            inner
                .listeners
                .borrow_mut()
                .notify(&ConnectionEvent::ReconnectScheduled {
                    attempt: inner.reconnect_attempt,
                    delay_ms: delay,
                });
            inner.set_state(RelayConnectionState::Reconnecting);
        } else {
            inner.set_state(if error.is_some() {
                RelayConnectionState::Failed
            } else {
                RelayConnectionState::Disconnected
            });
        }
    }

    /// Called by host when starting a reconnection attempt.
    pub fn process_reconnecting(&self) {
        let mut inner = self.inner.borrow_mut();
        inner.set_state(RelayConnectionState::Connecting);
    }

    /// Reset reconnect counter (e.g., after successful connection).
    pub fn reset_reconnect_counter(&self) {
        self.inner.borrow_mut().reconnect_attempt = 0;
    }

    // ── Outbound message helpers ────────────────────────────────────────

    /// Build the handshake message the host should send after connect.
    pub fn build_handshake(&self) -> RelayEnvelope {
        // In a real impl, this would include auth tokens, client info, etc.
        RelayEnvelope::new(RelayMessageKind::Handshake).with_payload(serde_json::json!({
            "clientVersion": env!("CARGO_PKG_VERSION"),
            "protocol": "prism-relay-v1"
        }))
    }

    /// Build a ping message. The host should track the send time.
    pub fn build_ping(&self) -> RelayEnvelope {
        let ping = RelayEnvelope::ping();
        self.inner.borrow_mut().last_ping_id = Some(ping.id.clone());
        ping
    }

    /// Record that a message was sent (for stats).
    pub fn record_sent(&self, bytes: u64) {
        let mut inner = self.inner.borrow_mut();
        inner.stats.messages_sent += 1;
        inner.stats.bytes_sent += bytes;
    }

    /// Dispose the connection — clears all listeners.
    pub fn dispose(&self) {
        let inner = self.inner.borrow();
        inner.listeners.borrow_mut().clear();
    }
}

#[cfg(test)]
mod tests {
    use super::super::types::RelayId;
    use super::*;
    use std::cell::Cell;

    fn make_conn() -> RelayConnection {
        RelayConnection::new(RelayConfig::default())
    }

    #[test]
    fn initial_state_is_disconnected() {
        let conn = make_conn();
        assert_eq!(conn.state(), RelayConnectionState::Disconnected);
        assert!(conn.relay_info().is_none());
    }

    #[test]
    fn lifecycle_connecting_to_handshaking() {
        let conn = make_conn();
        let states: Rc<RefCell<Vec<RelayConnectionState>>> = Rc::new(RefCell::new(Vec::new()));
        let s = Rc::clone(&states);
        let _sub = conn.subscribe(Box::new(move |e| {
            if let ConnectionEvent::StateChanged(state) = e {
                s.borrow_mut().push(*state);
            }
        }));

        conn.process_connecting();
        assert_eq!(conn.state(), RelayConnectionState::Connecting);

        conn.process_connected();
        assert_eq!(conn.state(), RelayConnectionState::Handshaking);

        let ss = states.borrow();
        assert_eq!(ss.len(), 2);
        assert_eq!(ss[0], RelayConnectionState::Connecting);
        assert_eq!(ss[1], RelayConnectionState::Handshaking);
    }

    #[test]
    fn handshake_ack_transitions_to_connected() {
        let conn = make_conn();
        conn.process_connecting();
        conn.process_connected();

        let info = RelayInfo {
            id: RelayId::new("relay-1"),
            name: "Test".into(),
            version: Some("1.0".into()),
            endpoints: vec![],
            features: vec!["sync".into()],
            max_message_size: 0,
            server_time: None,
        };

        let ack = RelayEnvelope::new(RelayMessageKind::HandshakeAck)
            .with_payload(serde_json::to_value(&info).unwrap());

        conn.process_message(ack);
        assert_eq!(conn.state(), RelayConnectionState::Connected);
        assert!(conn.relay_info().is_some());
        assert_eq!(conn.relay_info().unwrap().name, "Test");
    }

    #[test]
    fn disconnect_schedules_reconnect() {
        let conn = make_conn();
        let scheduled: Rc<Cell<bool>> = Rc::new(Cell::new(false));
        let s = Rc::clone(&scheduled);
        let _sub = conn.subscribe(Box::new(move |e| {
            if matches!(e, ConnectionEvent::ReconnectScheduled { .. }) {
                s.set(true);
            }
        }));

        conn.process_connecting();
        conn.process_connected();
        conn.process_disconnect(Some(TransportError::connection_failed("test")));

        assert_eq!(conn.state(), RelayConnectionState::Reconnecting);
        assert!(scheduled.get());
    }

    #[test]
    fn non_recoverable_error_goes_to_failed() {
        let config = RelayConfig {
            auto_reconnect: true,
            ..Default::default()
        };
        let conn = RelayConnection::new(config);

        conn.process_connecting();
        conn.process_connected();
        conn.process_disconnect(Some(TransportError::auth_failed("invalid token")));

        assert_eq!(conn.state(), RelayConnectionState::Failed);
    }

    #[test]
    fn disabled_auto_reconnect_goes_to_disconnected() {
        let conn = RelayConnection::new(RelayConfig::default().without_auto_reconnect());
        conn.process_connecting();
        conn.process_connected();
        conn.process_disconnect(None);

        assert_eq!(conn.state(), RelayConnectionState::Disconnected);
    }

    #[test]
    fn stats_track_messages() {
        let conn = make_conn();
        conn.process_connecting();
        conn.process_connected();

        let msg = RelayEnvelope::new(RelayMessageKind::Event);
        conn.process_message(msg);
        conn.record_sent(100);

        let stats = conn.stats();
        assert_eq!(stats.messages_received, 1);
        assert_eq!(stats.messages_sent, 1);
        assert_eq!(stats.bytes_sent, 100);
    }

    #[test]
    fn subscription_unsubscribe_stops_events() {
        let conn = make_conn();
        let count: Rc<Cell<u32>> = Rc::new(Cell::new(0));
        let c = Rc::clone(&count);
        let sub = conn.subscribe(Box::new(move |_| {
            c.set(c.get() + 1);
        }));

        conn.process_connecting();
        assert_eq!(count.get(), 1);

        sub.unsubscribe();
        conn.process_connected();
        assert_eq!(count.get(), 1); // no increment
    }

    #[test]
    fn build_handshake_includes_protocol() {
        let conn = make_conn();
        let hs = conn.build_handshake();
        assert_eq!(hs.kind, RelayMessageKind::Handshake);
        assert!(hs.payload.is_some());
        let p = hs.payload.unwrap();
        assert_eq!(p["protocol"], "prism-relay-v1");
    }

    #[test]
    fn build_ping_tracks_id() {
        let conn = make_conn();
        let ping = conn.build_ping();
        assert_eq!(ping.kind, RelayMessageKind::Ping);
        assert!(conn.inner.borrow().last_ping_id.is_some());
    }
}
