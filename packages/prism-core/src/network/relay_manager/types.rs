//! `network::relay_manager::types` — data types for the relay manager.

use serde::{Deserialize, Serialize};

use crate::network::relay::{
    RelayConfig, RelayConnectionState, RelayId, RelayInfo, TransportError,
};

/// Status of a relay within the manager.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RelayStatus {
    /// Relay is configured but not connected
    #[default]
    Idle,
    /// Relay is connecting or reconnecting
    Connecting,
    /// Relay is connected and healthy
    Active,
    /// Relay is connected but degraded (high latency, errors)
    Degraded,
    /// Relay is temporarily unavailable (will retry)
    Unavailable,
    /// Relay is permanently disabled
    Disabled,
}

impl RelayStatus {
    pub fn is_available(&self) -> bool {
        matches!(self, Self::Active | Self::Degraded)
    }

    pub fn is_healthy(&self) -> bool {
        matches!(self, Self::Active)
    }
}

/// Health metrics for a relay connection.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RelayHealth {
    /// Last successful ping RTT in milliseconds
    pub last_ping_ms: Option<u64>,
    /// Rolling average ping RTT
    pub avg_ping_ms: Option<u64>,
    /// Number of consecutive failures
    pub consecutive_failures: u32,
    /// Total messages sent
    pub messages_sent: u64,
    /// Total messages received
    pub messages_received: u64,
    /// Last error (if any)
    #[serde(skip)]
    pub last_error: Option<TransportError>,
    /// ISO-8601 timestamp of last successful message
    pub last_success_at: Option<String>,
    /// ISO-8601 timestamp of last failure
    pub last_failure_at: Option<String>,
}

impl RelayHealth {
    pub fn record_success(&mut self, now_iso: &str) {
        self.consecutive_failures = 0;
        self.last_success_at = Some(now_iso.to_string());
        self.last_error = None;
    }

    pub fn record_failure(&mut self, error: TransportError, now_iso: &str) {
        self.consecutive_failures += 1;
        self.last_failure_at = Some(now_iso.to_string());
        self.last_error = Some(error);
    }

    pub fn record_ping(&mut self, rtt_ms: u64) {
        self.last_ping_ms = Some(rtt_ms);
        self.avg_ping_ms = Some(match self.avg_ping_ms {
            Some(avg) => (avg + rtt_ms) / 2,
            None => rtt_ms,
        });
    }
}

/// A relay tracked by the manager.
#[derive(Debug, Clone)]
pub struct ManagedRelay {
    /// Relay identifier
    pub id: RelayId,
    /// Connection configuration
    pub config: RelayConfig,
    /// Current status
    pub status: RelayStatus,
    /// Connection state (from underlying connection)
    pub connection_state: RelayConnectionState,
    /// Health metrics
    pub health: RelayHealth,
    /// Relay info (from handshake, if connected)
    pub info: Option<RelayInfo>,
    /// Whether this relay is the primary for its role
    pub is_primary: bool,
    /// Priority for selection (lower = preferred)
    pub priority: u32,
    /// User-defined tags for filtering
    pub tags: Vec<String>,
}

impl ManagedRelay {
    pub fn new(id: RelayId, config: RelayConfig) -> Self {
        Self {
            id,
            config,
            status: RelayStatus::Idle,
            connection_state: RelayConnectionState::Disconnected,
            health: RelayHealth::default(),
            info: None,
            is_primary: false,
            priority: 100,
            tags: Vec::new(),
        }
    }

    pub fn with_priority(mut self, priority: u32) -> Self {
        self.priority = priority;
        self
    }

    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        self.tags.push(tag.into());
        self
    }

    pub fn with_tags(mut self, tags: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.tags.extend(tags.into_iter().map(Into::into));
        self
    }
}

/// Events emitted by the relay manager.
#[derive(Debug, Clone, PartialEq)]
pub enum RelayManagerEvent {
    /// A relay's status changed
    RelayStatusChanged {
        relay_id: RelayId,
        old_status: RelayStatus,
        new_status: RelayStatus,
    },
    /// A relay connected successfully
    RelayConnected { relay_id: RelayId, info: RelayInfo },
    /// A relay disconnected
    RelayDisconnected {
        relay_id: RelayId,
        error: Option<TransportError>,
    },
    /// Primary relay changed for a role
    PrimaryChanged {
        role: String,
        old_relay: Option<RelayId>,
        new_relay: Option<RelayId>,
    },
    /// All relays are unavailable
    AllUnavailable,
    /// At least one relay recovered from all-unavailable
    Recovered { relay_id: RelayId },
}

pub type RelayManagerListener = Box<dyn FnMut(&RelayManagerEvent)>;

/// Aggregate statistics for the manager.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RelayManagerStats {
    /// Total relays registered
    pub total_relays: usize,
    /// Relays currently active
    pub active_relays: usize,
    /// Relays currently connecting
    pub connecting_relays: usize,
    /// Relays currently unavailable
    pub unavailable_relays: usize,
    /// Total messages sent across all relays
    pub total_messages_sent: u64,
    /// Total messages received across all relays
    pub total_messages_received: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relay_status_availability() {
        assert!(RelayStatus::Active.is_available());
        assert!(RelayStatus::Degraded.is_available());
        assert!(!RelayStatus::Idle.is_available());
        assert!(!RelayStatus::Unavailable.is_available());
        assert!(!RelayStatus::Disabled.is_available());
    }

    #[test]
    fn relay_health_success_clears_failures() {
        let mut health = RelayHealth {
            consecutive_failures: 5,
            last_error: Some(TransportError::timeout(1000)),
            ..Default::default()
        };

        health.record_success("2026-04-18T12:00:00Z");
        assert_eq!(health.consecutive_failures, 0);
        assert!(health.last_error.is_none());
        assert!(health.last_success_at.is_some());
    }

    #[test]
    fn relay_health_ping_averaging() {
        let mut health = RelayHealth::default();
        health.record_ping(100);
        assert_eq!(health.avg_ping_ms, Some(100));

        health.record_ping(200);
        assert_eq!(health.avg_ping_ms, Some(150));
    }

    #[test]
    fn managed_relay_builder() {
        let relay = ManagedRelay::new(
            RelayId::new("relay-1"),
            RelayConfig::websocket("ws://example.com/ws"),
        )
        .with_priority(10)
        .with_tag("primary")
        .with_tags(["sync", "presence"]);

        assert_eq!(relay.priority, 10);
        assert_eq!(relay.tags, vec!["primary", "sync", "presence"]);
    }
}
