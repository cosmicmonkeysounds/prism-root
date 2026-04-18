//! `network::relay_manager::manager` — the relay connection manager.
//!
//! Port of `kernel/relay-manager.ts`. Orchestrates multiple relay
//! connections, tracks their health, selects primaries, and routes
//! messages.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use super::policy::{RelayPolicy, RelaySelector};
use super::types::{
    ManagedRelay, RelayManagerEvent, RelayManagerListener, RelayManagerStats, RelayStatus,
};
use crate::network::relay::{
    RelayConfig, RelayConnection, RelayConnectionState, RelayId, RelayInfo, TransportError,
};

/// Options for creating a RelayManager.
#[derive(Debug, Clone)]
pub struct RelayManagerOptions {
    /// Default policy for relay selection
    pub default_policy: RelayPolicy,
    /// Whether to auto-connect relays on add
    pub auto_connect: bool,
    /// Health check interval in milliseconds (0 = disabled)
    pub health_check_interval_ms: u64,
    /// Maximum relays to connect simultaneously
    pub max_concurrent_connections: usize,
}

impl Default for RelayManagerOptions {
    fn default() -> Self {
        Self {
            default_policy: RelayPolicy::first_available(),
            auto_connect: true,
            health_check_interval_ms: 30_000,
            max_concurrent_connections: 5,
        }
    }
}

/// Listener bus for manager events.
struct ListenerBus {
    next_id: u64,
    entries: Vec<(u64, RelayManagerListener)>,
}

impl ListenerBus {
    fn new() -> Self {
        Self {
            next_id: 0,
            entries: Vec::new(),
        }
    }

    fn add(&mut self, listener: RelayManagerListener) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        self.entries.push((id, listener));
        id
    }

    fn remove(&mut self, id: u64) {
        self.entries.retain(|(i, _)| *i != id);
    }

    fn notify(&mut self, event: &RelayManagerEvent) {
        for (_, listener) in &mut self.entries {
            listener(event);
        }
    }

    fn clear(&mut self) {
        self.entries.clear();
    }
}

/// Opaque subscription handle.
pub struct ManagerSubscription {
    inner: Rc<RefCell<ListenerBus>>,
    id: u64,
    active: bool,
}

impl ManagerSubscription {
    pub fn unsubscribe(mut self) {
        if self.active {
            self.active = false;
            self.inner.borrow_mut().remove(self.id);
        }
    }
}

impl Drop for ManagerSubscription {
    fn drop(&mut self) {
        if self.active {
            self.inner.borrow_mut().remove(self.id);
        }
    }
}

/// Per-role primary tracking.
#[derive(Debug, Clone, Default)]
struct RolePrimaries {
    /// role name → relay id
    primaries: HashMap<String, RelayId>,
}

impl RolePrimaries {
    fn get(&self, role: &str) -> Option<&RelayId> {
        self.primaries.get(role)
    }

    fn set(&mut self, role: &str, relay_id: RelayId) -> Option<RelayId> {
        self.primaries.insert(role.to_string(), relay_id)
    }

    #[allow(dead_code)]
    fn remove(&mut self, role: &str) -> Option<RelayId> {
        self.primaries.remove(role)
    }

    fn clear_relay(&mut self, relay_id: &RelayId) -> Vec<String> {
        let roles: Vec<String> = self
            .primaries
            .iter()
            .filter(|(_, id)| *id == relay_id)
            .map(|(role, _)| role.clone())
            .collect();
        for role in &roles {
            self.primaries.remove(role);
        }
        roles
    }
}

/// Inner manager state.
struct Inner {
    #[allow(dead_code)]
    options: RelayManagerOptions,
    relays: HashMap<RelayId, ManagedRelay>,
    connections: HashMap<RelayId, RelayConnection>,
    primaries: RolePrimaries,
    selector: RelaySelector,
    listeners: Rc<RefCell<ListenerBus>>,
    was_all_unavailable: bool,
}

impl Inner {
    #[allow(dead_code)]
    fn set_relay_status(&mut self, relay_id: &RelayId, new_status: RelayStatus) {
        if let Some(relay) = self.relays.get_mut(relay_id) {
            let old_status = relay.status;
            if old_status != new_status {
                relay.status = new_status;
                self.listeners
                    .borrow_mut()
                    .notify(&RelayManagerEvent::RelayStatusChanged {
                        relay_id: relay_id.clone(),
                        old_status,
                        new_status,
                    });
            }
        }
    }

    #[allow(dead_code)]
    fn sync_relay_state(&mut self, relay_id: &RelayId) {
        if let Some(conn) = self.connections.get(relay_id) {
            let conn_state = conn.state();
            if let Some(relay) = self.relays.get_mut(relay_id) {
                relay.connection_state = conn_state;
                relay.info = conn.relay_info();

                // Derive status from connection state
                let new_status = match conn_state {
                    RelayConnectionState::Disconnected => RelayStatus::Idle,
                    RelayConnectionState::Connecting | RelayConnectionState::Handshaking => {
                        RelayStatus::Connecting
                    }
                    RelayConnectionState::Connected => {
                        if relay.health.consecutive_failures > 0 {
                            RelayStatus::Degraded
                        } else {
                            RelayStatus::Active
                        }
                    }
                    RelayConnectionState::Disconnecting => RelayStatus::Connecting,
                    RelayConnectionState::Reconnecting => RelayStatus::Connecting,
                    RelayConnectionState::Failed => RelayStatus::Unavailable,
                };

                let old_status = relay.status;
                if old_status != new_status {
                    relay.status = new_status;
                    self.listeners
                        .borrow_mut()
                        .notify(&RelayManagerEvent::RelayStatusChanged {
                            relay_id: relay_id.clone(),
                            old_status,
                            new_status,
                        });
                }
            }
        }
    }

    fn check_all_unavailable(&mut self) {
        let any_available = self.relays.values().any(|r| r.status.is_available());

        if !any_available && !self.was_all_unavailable {
            self.was_all_unavailable = true;
            self.listeners
                .borrow_mut()
                .notify(&RelayManagerEvent::AllUnavailable);
        } else if any_available && self.was_all_unavailable {
            self.was_all_unavailable = false;
            if let Some(relay) = self.relays.values().find(|r| r.status.is_available()) {
                self.listeners
                    .borrow_mut()
                    .notify(&RelayManagerEvent::Recovered {
                        relay_id: relay.id.clone(),
                    });
            }
        }
    }

    fn compute_stats(&self) -> RelayManagerStats {
        let mut active_relays = 0;
        let mut connecting_relays = 0;
        let mut unavailable_relays = 0;
        let mut total_messages_sent = 0u64;
        let mut total_messages_received = 0u64;

        for relay in self.relays.values() {
            match relay.status {
                RelayStatus::Active | RelayStatus::Degraded => active_relays += 1,
                RelayStatus::Connecting => connecting_relays += 1,
                RelayStatus::Unavailable | RelayStatus::Disabled => unavailable_relays += 1,
                _ => {}
            }
            total_messages_sent += relay.health.messages_sent;
            total_messages_received += relay.health.messages_received;
        }

        RelayManagerStats {
            total_relays: self.relays.len(),
            active_relays,
            connecting_relays,
            unavailable_relays,
            total_messages_sent,
            total_messages_received,
        }
    }
}

/// Manages multiple relay connections.
///
/// Host-driven: the manager tracks state and makes decisions, but
/// actual network I/O is performed by the host through the `process_*`
/// methods.
pub struct RelayManager {
    inner: Rc<RefCell<Inner>>,
}

impl RelayManager {
    /// Create a new manager with the given options.
    pub fn new(options: RelayManagerOptions) -> Self {
        let selector = RelaySelector::new(options.default_policy.clone());
        let listeners = Rc::new(RefCell::new(ListenerBus::new()));
        Self {
            inner: Rc::new(RefCell::new(Inner {
                options,
                relays: HashMap::new(),
                connections: HashMap::new(),
                primaries: RolePrimaries::default(),
                selector,
                listeners,
                was_all_unavailable: false,
            })),
        }
    }

    /// Add a relay to the manager.
    pub fn add_relay(&self, id: RelayId, config: RelayConfig) -> bool {
        let mut inner = self.inner.borrow_mut();
        if inner.relays.contains_key(&id) {
            return false;
        }

        let relay = ManagedRelay::new(id.clone(), config.clone());
        inner.relays.insert(id.clone(), relay);

        // Create the connection
        let conn = RelayConnection::new(config);
        inner.connections.insert(id, conn);

        true
    }

    /// Remove a relay from the manager.
    pub fn remove_relay(&self, id: &RelayId) -> bool {
        let mut inner = self.inner.borrow_mut();

        // Clear from primaries
        let cleared_roles = inner.primaries.clear_relay(id);
        for role in cleared_roles {
            inner
                .listeners
                .borrow_mut()
                .notify(&RelayManagerEvent::PrimaryChanged {
                    role,
                    old_relay: Some(id.clone()),
                    new_relay: None,
                });
        }

        // Remove connection and relay
        inner.connections.remove(id);
        inner.relays.remove(id).is_some()
    }

    /// Get a relay by id.
    pub fn get_relay(&self, id: &RelayId) -> Option<ManagedRelay> {
        self.inner.borrow().relays.get(id).cloned()
    }

    /// Get the underlying connection for a relay.
    pub fn get_connection(&self, _id: &RelayId) -> Option<RelayConnection> {
        // RelayConnection is Clone-as-handle (Rc internally)
        // For now, return None since our RelayConnection doesn't impl Clone
        // In practice, the host would hold its own handle
        None
    }

    /// List all relays.
    pub fn list_relays(&self) -> Vec<ManagedRelay> {
        self.inner.borrow().relays.values().cloned().collect()
    }

    /// Get the current primary relay for a role.
    pub fn get_primary(&self, role: &str) -> Option<RelayId> {
        self.inner.borrow().primaries.get(role).cloned()
    }

    /// Set the primary relay for a role.
    pub fn set_primary(&self, role: &str, relay_id: RelayId) {
        let mut inner = self.inner.borrow_mut();
        let old = inner.primaries.set(role, relay_id.clone());
        if old.as_ref() != Some(&relay_id) {
            inner
                .listeners
                .borrow_mut()
                .notify(&RelayManagerEvent::PrimaryChanged {
                    role: role.to_string(),
                    old_relay: old,
                    new_relay: Some(relay_id),
                });
        }
    }

    /// Select a relay using the default policy.
    pub fn select_relay(&self) -> Option<RelayId> {
        let mut inner = self.inner.borrow_mut();
        let relays: Vec<_> = inner.relays.values().cloned().collect();
        inner.selector.select(&relays).map(|r| r.id.clone())
    }

    /// Select a relay using a custom policy.
    pub fn select_relay_with_policy(&self, policy: RelayPolicy) -> Option<RelayId> {
        let inner = self.inner.borrow();
        let relays: Vec<_> = inner.relays.values().cloned().collect();
        let mut selector = RelaySelector::new(policy);
        selector.select(&relays).map(|r| r.id.clone())
    }

    /// Get aggregate statistics.
    pub fn stats(&self) -> RelayManagerStats {
        self.inner.borrow().compute_stats()
    }

    /// Subscribe to manager events.
    pub fn subscribe(&self, listener: RelayManagerListener) -> ManagerSubscription {
        let listeners = Rc::clone(&self.inner.borrow().listeners);
        let id = listeners.borrow_mut().add(listener);
        ManagerSubscription {
            inner: listeners,
            id,
            active: true,
        }
    }

    // ── Host-driven lifecycle ───────────────────────────────────────────

    /// Called by host when a relay connects.
    pub fn process_relay_connected(&self, relay_id: &RelayId, info: RelayInfo) {
        let mut inner = self.inner.borrow_mut();

        if let Some(relay) = inner.relays.get_mut(relay_id) {
            relay.info = Some(info.clone());
            relay.connection_state = RelayConnectionState::Connected;
            relay
                .health
                .record_success(&chrono::Utc::now().to_rfc3339());

            // Update status based on health
            let old_status = relay.status;
            let new_status = if relay.health.consecutive_failures > 0 {
                RelayStatus::Degraded
            } else {
                RelayStatus::Active
            };
            if old_status != new_status {
                relay.status = new_status;
                inner
                    .listeners
                    .borrow_mut()
                    .notify(&RelayManagerEvent::RelayStatusChanged {
                        relay_id: relay_id.clone(),
                        old_status,
                        new_status,
                    });
            }
        }

        inner.check_all_unavailable();

        inner
            .listeners
            .borrow_mut()
            .notify(&RelayManagerEvent::RelayConnected {
                relay_id: relay_id.clone(),
                info,
            });
    }

    /// Called by host when a relay disconnects.
    pub fn process_relay_disconnected(&self, relay_id: &RelayId, error: Option<TransportError>) {
        let mut inner = self.inner.borrow_mut();

        if let Some(e) = &error {
            if let Some(relay) = inner.relays.get_mut(relay_id) {
                relay
                    .health
                    .record_failure(e.clone(), &chrono::Utc::now().to_rfc3339());
            }
        }

        inner.sync_relay_state(relay_id);
        inner.check_all_unavailable();

        inner
            .listeners
            .borrow_mut()
            .notify(&RelayManagerEvent::RelayDisconnected {
                relay_id: relay_id.clone(),
                error,
            });
    }

    /// Called by host when a relay's state changes.
    pub fn process_relay_state_changed(&self, relay_id: &RelayId, state: RelayConnectionState) {
        let mut inner = self.inner.borrow_mut();

        if let Some(relay) = inner.relays.get_mut(relay_id) {
            relay.connection_state = state;

            // Derive status from connection state
            let old_status = relay.status;
            let new_status = match state {
                RelayConnectionState::Disconnected => RelayStatus::Idle,
                RelayConnectionState::Connecting | RelayConnectionState::Handshaking => {
                    RelayStatus::Connecting
                }
                RelayConnectionState::Connected => {
                    if relay.health.consecutive_failures > 0 {
                        RelayStatus::Degraded
                    } else {
                        RelayStatus::Active
                    }
                }
                RelayConnectionState::Disconnecting => RelayStatus::Connecting,
                RelayConnectionState::Reconnecting => RelayStatus::Connecting,
                RelayConnectionState::Failed => RelayStatus::Unavailable,
            };
            if old_status != new_status {
                relay.status = new_status;
                inner
                    .listeners
                    .borrow_mut()
                    .notify(&RelayManagerEvent::RelayStatusChanged {
                        relay_id: relay_id.clone(),
                        old_status,
                        new_status,
                    });
            }
        }

        inner.check_all_unavailable();
    }

    /// Called by host when a ping response is received.
    pub fn process_ping_response(&self, relay_id: &RelayId, rtt_ms: u64) {
        let mut inner = self.inner.borrow_mut();
        if let Some(relay) = inner.relays.get_mut(relay_id) {
            relay.health.record_ping(rtt_ms);
        }
    }

    /// Called by host to record a sent message.
    pub fn process_message_sent(&self, relay_id: &RelayId) {
        let mut inner = self.inner.borrow_mut();
        if let Some(relay) = inner.relays.get_mut(relay_id) {
            relay.health.messages_sent += 1;
        }
    }

    /// Called by host to record a received message.
    pub fn process_message_received(&self, relay_id: &RelayId) {
        let mut inner = self.inner.borrow_mut();
        if let Some(relay) = inner.relays.get_mut(relay_id) {
            relay.health.messages_received += 1;
        }
    }

    /// Dispose the manager — clears all listeners and connections.
    pub fn dispose(&self) {
        let mut inner = self.inner.borrow_mut();
        for conn in inner.connections.values() {
            conn.dispose();
        }
        inner.connections.clear();
        inner.relays.clear();
        inner.primaries = RolePrimaries::default();
        inner.listeners.borrow_mut().clear();
    }
}

impl Default for RelayManager {
    fn default() -> Self {
        Self::new(RelayManagerOptions::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    fn make_manager() -> RelayManager {
        RelayManager::new(RelayManagerOptions::default())
    }

    #[test]
    fn add_and_remove_relay() {
        let mgr = make_manager();
        let id = RelayId::new("relay-1");

        assert!(mgr.add_relay(id.clone(), RelayConfig::default()));
        assert!(!mgr.add_relay(id.clone(), RelayConfig::default())); // duplicate

        assert!(mgr.get_relay(&id).is_some());
        assert_eq!(mgr.list_relays().len(), 1);

        assert!(mgr.remove_relay(&id));
        assert!(mgr.get_relay(&id).is_none());
        assert!(mgr.list_relays().is_empty());
    }

    #[test]
    fn primary_relay_tracking() {
        let mgr = make_manager();
        let id1 = RelayId::new("relay-1");
        let id2 = RelayId::new("relay-2");

        mgr.add_relay(id1.clone(), RelayConfig::default());
        mgr.add_relay(id2.clone(), RelayConfig::default());

        assert!(mgr.get_primary("sync").is_none());

        mgr.set_primary("sync", id1.clone());
        assert_eq!(mgr.get_primary("sync"), Some(id1.clone()));

        mgr.set_primary("sync", id2.clone());
        assert_eq!(mgr.get_primary("sync"), Some(id2.clone()));
    }

    #[test]
    fn remove_relay_clears_primary() {
        let mgr = make_manager();
        let id = RelayId::new("relay-1");

        mgr.add_relay(id.clone(), RelayConfig::default());
        mgr.set_primary("sync", id.clone());
        mgr.set_primary("presence", id.clone());

        mgr.remove_relay(&id);
        assert!(mgr.get_primary("sync").is_none());
        assert!(mgr.get_primary("presence").is_none());
    }

    #[test]
    fn process_connected_updates_relay() {
        let mgr = make_manager();
        let id = RelayId::new("relay-1");
        mgr.add_relay(id.clone(), RelayConfig::default());

        let info = RelayInfo {
            id: id.clone(),
            name: "Test Relay".into(),
            version: Some("1.0".into()),
            endpoints: vec![],
            features: vec![],
            max_message_size: 0,
            server_time: None,
        };

        mgr.process_relay_connected(&id, info.clone());

        let relay = mgr.get_relay(&id).unwrap();
        assert!(relay.info.is_some());
        assert_eq!(relay.info.unwrap().name, "Test Relay");
        assert_eq!(relay.health.consecutive_failures, 0);
    }

    #[test]
    fn process_disconnected_records_failure() {
        let mgr = make_manager();
        let id = RelayId::new("relay-1");
        mgr.add_relay(id.clone(), RelayConfig::default());

        mgr.process_relay_disconnected(&id, Some(TransportError::timeout(5000)));

        let relay = mgr.get_relay(&id).unwrap();
        assert_eq!(relay.health.consecutive_failures, 1);
        assert!(relay.health.last_error.is_some());
    }

    #[test]
    fn ping_response_updates_latency() {
        let mgr = make_manager();
        let id = RelayId::new("relay-1");
        mgr.add_relay(id.clone(), RelayConfig::default());

        mgr.process_ping_response(&id, 100);
        mgr.process_ping_response(&id, 200);

        let relay = mgr.get_relay(&id).unwrap();
        assert_eq!(relay.health.last_ping_ms, Some(200));
        assert_eq!(relay.health.avg_ping_ms, Some(150));
    }

    #[test]
    fn stats_aggregate_correctly() {
        let mgr = make_manager();
        mgr.add_relay(RelayId::new("r1"), RelayConfig::default());
        mgr.add_relay(RelayId::new("r2"), RelayConfig::default());

        mgr.process_message_sent(&RelayId::new("r1"));
        mgr.process_message_sent(&RelayId::new("r1"));
        mgr.process_message_received(&RelayId::new("r2"));

        let stats = mgr.stats();
        assert_eq!(stats.total_relays, 2);
        assert_eq!(stats.total_messages_sent, 2);
        assert_eq!(stats.total_messages_received, 1);
    }

    #[test]
    fn subscription_fires_on_status_change() {
        let mgr = make_manager();
        let id = RelayId::new("relay-1");
        mgr.add_relay(id.clone(), RelayConfig::default());

        let events: Rc<RefCell<Vec<RelayManagerEvent>>> = Rc::new(RefCell::new(Vec::new()));
        let e = Rc::clone(&events);
        let _sub = mgr.subscribe(Box::new(move |ev| {
            e.borrow_mut().push(ev.clone());
        }));

        let info = RelayInfo {
            id: id.clone(),
            name: "Test".into(),
            version: None,
            endpoints: vec![],
            features: vec![],
            max_message_size: 0,
            server_time: None,
        };
        mgr.process_relay_connected(&id, info);

        let evs = events.borrow();
        assert!(evs
            .iter()
            .any(|e| matches!(e, RelayManagerEvent::RelayConnected { .. })));
    }

    #[test]
    fn all_unavailable_event_fires() {
        let mgr = make_manager();
        let id = RelayId::new("relay-1");
        mgr.add_relay(id.clone(), RelayConfig::default());

        // First, connect then disconnect with error
        let info = RelayInfo {
            id: id.clone(),
            name: "Test".into(),
            version: None,
            endpoints: vec![],
            features: vec![],
            max_message_size: 0,
            server_time: None,
        };
        mgr.process_relay_connected(&id, info);

        let all_unavailable: Rc<Cell<bool>> = Rc::new(Cell::new(false));
        let a = Rc::clone(&all_unavailable);
        let _sub = mgr.subscribe(Box::new(move |ev| {
            if matches!(ev, RelayManagerEvent::AllUnavailable) {
                a.set(true);
            }
        }));

        mgr.process_relay_state_changed(&id, RelayConnectionState::Failed);
        assert!(all_unavailable.get());
    }

    #[test]
    fn dispose_clears_everything() {
        let mgr = make_manager();
        mgr.add_relay(RelayId::new("r1"), RelayConfig::default());
        mgr.set_primary("sync", RelayId::new("r1"));

        mgr.dispose();

        assert!(mgr.list_relays().is_empty());
        assert!(mgr.get_primary("sync").is_none());
    }
}
