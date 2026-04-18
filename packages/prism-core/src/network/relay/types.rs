//! `network::relay::types` — core relay data types.
//!
//! Direct port of `relay/relay-types.ts`. Defines relay identification,
//! configuration, and endpoint metadata.

use serde::{Deserialize, Serialize};

/// Unique relay identifier. Typically a URL or DID.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RelayId(pub String);

impl RelayId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<String> for RelayId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for RelayId {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

impl std::fmt::Display for RelayId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Transport protocol for relay connections.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RelayProtocol {
    /// WebSocket (wss:// or ws://)
    #[default]
    WebSocket,
    /// HTTP/HTTPS for polling or REST-style interactions
    Http,
    /// WebRTC data channel (peer-to-peer via relay signaling)
    WebRtc,
}

/// Relay endpoint configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RelayEndpoint {
    /// Endpoint URL (e.g., "wss://relay.prism.local/ws")
    pub url: String,
    /// Transport protocol
    #[serde(default)]
    pub protocol: RelayProtocol,
    /// Whether this endpoint requires authentication
    #[serde(default)]
    pub requires_auth: bool,
}

impl RelayEndpoint {
    pub fn websocket(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            protocol: RelayProtocol::WebSocket,
            requires_auth: false,
        }
    }

    pub fn http(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            protocol: RelayProtocol::Http,
            requires_auth: false,
        }
    }

    pub fn with_auth(mut self) -> Self {
        self.requires_auth = true;
        self
    }
}

/// Relay server metadata returned from discovery or handshake.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RelayInfo {
    /// Relay's self-reported identifier
    pub id: RelayId,
    /// Human-readable name
    pub name: String,
    /// Relay version string
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// Available endpoints (may include multiple protocols)
    pub endpoints: Vec<RelayEndpoint>,
    /// Supported feature flags
    #[serde(default)]
    pub features: Vec<String>,
    /// Maximum message size in bytes (0 = unlimited)
    #[serde(default)]
    pub max_message_size: u64,
    /// Server's UTC timestamp at handshake time (ISO-8601)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server_time: Option<String>,
}

impl RelayInfo {
    pub fn supports_feature(&self, feature: &str) -> bool {
        self.features.iter().any(|f| f == feature)
    }
}

/// Client-side relay configuration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RelayConfig {
    /// Primary endpoint to connect to
    pub endpoint: RelayEndpoint,
    /// Reconnection delay in milliseconds (0 = no auto-reconnect)
    #[serde(default = "default_reconnect_delay")]
    pub reconnect_delay_ms: u64,
    /// Maximum reconnection attempts (0 = unlimited)
    #[serde(default)]
    pub max_reconnect_attempts: u32,
    /// Connection timeout in milliseconds
    #[serde(default = "default_connect_timeout")]
    pub connect_timeout_ms: u64,
    /// Heartbeat/ping interval in milliseconds (0 = disabled)
    #[serde(default = "default_heartbeat_interval")]
    pub heartbeat_interval_ms: u64,
    /// Whether to automatically reconnect on disconnect
    #[serde(default = "default_auto_reconnect")]
    pub auto_reconnect: bool,
}

fn default_reconnect_delay() -> u64 {
    1000
}

fn default_connect_timeout() -> u64 {
    10_000
}

fn default_heartbeat_interval() -> u64 {
    30_000
}

fn default_auto_reconnect() -> bool {
    true
}

impl RelayConfig {
    pub fn new(endpoint: RelayEndpoint) -> Self {
        Self {
            endpoint,
            reconnect_delay_ms: default_reconnect_delay(),
            max_reconnect_attempts: 0,
            connect_timeout_ms: default_connect_timeout(),
            heartbeat_interval_ms: default_heartbeat_interval(),
            auto_reconnect: default_auto_reconnect(),
        }
    }

    pub fn websocket(url: impl Into<String>) -> Self {
        Self::new(RelayEndpoint::websocket(url))
    }

    pub fn with_reconnect_delay(mut self, ms: u64) -> Self {
        self.reconnect_delay_ms = ms;
        self
    }

    pub fn with_max_attempts(mut self, attempts: u32) -> Self {
        self.max_reconnect_attempts = attempts;
        self
    }

    pub fn with_timeout(mut self, ms: u64) -> Self {
        self.connect_timeout_ms = ms;
        self
    }

    pub fn without_auto_reconnect(mut self) -> Self {
        self.auto_reconnect = false;
        self
    }
}

impl Default for RelayConfig {
    fn default() -> Self {
        Self::new(RelayEndpoint::websocket("ws://localhost:1420/ws"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relay_id_equality() {
        let a = RelayId::new("relay-1");
        let b = RelayId::from("relay-1");
        let c: RelayId = "relay-1".into();
        assert_eq!(a, b);
        assert_eq!(b, c);
    }

    #[test]
    fn relay_endpoint_builders() {
        let ws = RelayEndpoint::websocket("wss://example.com/ws");
        assert_eq!(ws.protocol, RelayProtocol::WebSocket);
        assert!(!ws.requires_auth);

        let http = RelayEndpoint::http("https://example.com/api").with_auth();
        assert_eq!(http.protocol, RelayProtocol::Http);
        assert!(http.requires_auth);
    }

    #[test]
    fn relay_config_defaults() {
        let cfg = RelayConfig::websocket("ws://localhost:1420/ws");
        assert_eq!(cfg.reconnect_delay_ms, 1000);
        assert_eq!(cfg.max_reconnect_attempts, 0);
        assert_eq!(cfg.connect_timeout_ms, 10_000);
        assert_eq!(cfg.heartbeat_interval_ms, 30_000);
        assert!(cfg.auto_reconnect);
    }

    #[test]
    fn relay_config_builder_chain() {
        let cfg = RelayConfig::websocket("ws://example.com/ws")
            .with_reconnect_delay(500)
            .with_max_attempts(5)
            .with_timeout(5000)
            .without_auto_reconnect();
        assert_eq!(cfg.reconnect_delay_ms, 500);
        assert_eq!(cfg.max_reconnect_attempts, 5);
        assert_eq!(cfg.connect_timeout_ms, 5000);
        assert!(!cfg.auto_reconnect);
    }

    #[test]
    fn relay_info_feature_check() {
        let info = RelayInfo {
            id: RelayId::new("relay-1"),
            name: "Test Relay".into(),
            version: Some("1.0.0".into()),
            endpoints: vec![RelayEndpoint::websocket("wss://example.com/ws")],
            features: vec!["presence".into(), "sync".into()],
            max_message_size: 1024 * 1024,
            server_time: None,
        };
        assert!(info.supports_feature("presence"));
        assert!(info.supports_feature("sync"));
        assert!(!info.supports_feature("unknown"));
    }

    #[test]
    fn relay_endpoint_serde_round_trip() {
        let endpoint = RelayEndpoint::websocket("wss://example.com/ws").with_auth();
        let json = serde_json::to_string(&endpoint).unwrap();
        let parsed: RelayEndpoint = serde_json::from_str(&json).unwrap();
        assert_eq!(endpoint, parsed);
    }

    #[test]
    fn relay_config_serde_round_trip() {
        let cfg = RelayConfig::websocket("ws://example.com/ws")
            .with_reconnect_delay(2000)
            .with_max_attempts(10);
        let json = serde_json::to_string(&cfg).unwrap();
        let parsed: RelayConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(cfg, parsed);
    }
}
