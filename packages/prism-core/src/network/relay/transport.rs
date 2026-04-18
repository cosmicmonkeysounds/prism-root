//! `network::relay::transport` — transport abstraction for relay connections.
//!
//! Port of `relay/relay-transport.ts`. Defines the trait that transport
//! implementations (WebSocket, HTTP polling, WebRTC) must satisfy, plus
//! the common error and event types they emit.

use serde::{Deserialize, Serialize};

use super::message::RelayEnvelope;

/// Transport connection state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TransportState {
    /// Not connected
    #[default]
    Disconnected,
    /// Connection in progress
    Connecting,
    /// Connected and ready
    Connected,
    /// Graceful disconnect in progress
    Disconnecting,
    /// Connection failed (see error)
    Failed,
}

impl TransportState {
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

/// Transport error categories.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TransportError {
    /// Connection failed (network unreachable, DNS failure, etc.)
    ConnectionFailed { message: String },
    /// Connection timed out
    Timeout { timeout_ms: u64 },
    /// Connection closed by remote
    RemoteClosed {
        code: Option<u16>,
        reason: Option<String>,
    },
    /// Protocol error (malformed message, unexpected frame, etc.)
    ProtocolError { message: String },
    /// Authentication failed
    AuthFailed { message: String },
    /// Send failed (connection not ready, buffer full, etc.)
    SendFailed { message: String },
    /// Generic transport error
    Other { message: String },
}

impl TransportError {
    pub fn connection_failed(msg: impl Into<String>) -> Self {
        Self::ConnectionFailed {
            message: msg.into(),
        }
    }

    pub fn timeout(ms: u64) -> Self {
        Self::Timeout { timeout_ms: ms }
    }

    pub fn remote_closed(code: Option<u16>, reason: Option<String>) -> Self {
        Self::RemoteClosed { code, reason }
    }

    pub fn protocol(msg: impl Into<String>) -> Self {
        Self::ProtocolError {
            message: msg.into(),
        }
    }

    pub fn auth_failed(msg: impl Into<String>) -> Self {
        Self::AuthFailed {
            message: msg.into(),
        }
    }

    pub fn send_failed(msg: impl Into<String>) -> Self {
        Self::SendFailed {
            message: msg.into(),
        }
    }

    pub fn other(msg: impl Into<String>) -> Self {
        Self::Other {
            message: msg.into(),
        }
    }

    pub fn is_recoverable(&self) -> bool {
        match self {
            Self::ConnectionFailed { .. } => true,
            Self::Timeout { .. } => true,
            Self::RemoteClosed { code, .. } => {
                // WebSocket close codes: 1000 = normal, 1001 = going away
                !matches!(code, Some(1000) | Some(1001))
            }
            Self::ProtocolError { .. } => false,
            Self::AuthFailed { .. } => false,
            Self::SendFailed { .. } => true,
            Self::Other { .. } => false,
        }
    }
}

impl std::fmt::Display for TransportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ConnectionFailed { message } => write!(f, "connection failed: {message}"),
            Self::Timeout { timeout_ms } => write!(f, "connection timed out after {timeout_ms}ms"),
            Self::RemoteClosed { code, reason } => {
                write!(f, "remote closed")?;
                if let Some(c) = code {
                    write!(f, " (code {c})")?;
                }
                if let Some(r) = reason {
                    write!(f, ": {r}")?;
                }
                Ok(())
            }
            Self::ProtocolError { message } => write!(f, "protocol error: {message}"),
            Self::AuthFailed { message } => write!(f, "auth failed: {message}"),
            Self::SendFailed { message } => write!(f, "send failed: {message}"),
            Self::Other { message } => write!(f, "{message}"),
        }
    }
}

impl std::error::Error for TransportError {}

/// Events emitted by a transport.
#[derive(Debug, Clone, PartialEq)]
pub enum TransportEvent {
    /// State changed
    StateChanged(TransportState),
    /// Message received from remote
    MessageReceived(RelayEnvelope),
    /// Error occurred
    Error(TransportError),
}

/// Transport trait — the contract every relay transport must satisfy.
///
/// This is a synchronous trait. Actual async transports (e.g.,
/// `tokio-tungstenite`) wrap their handles and expose this interface
/// through a send/recv queue pattern.
pub trait Transport {
    /// Current transport state.
    fn state(&self) -> TransportState;

    /// Send a message to the remote. Returns an error if the transport
    /// is not connected or the send fails.
    fn send(&self, envelope: RelayEnvelope) -> Result<(), TransportError>;

    /// Close the transport gracefully. Idempotent — calling on an
    /// already-closed transport is a no-op.
    fn close(&self);

    /// Subscribe to transport events. Returns a subscription id.
    fn subscribe(&self, listener: Box<dyn FnMut(TransportEvent)>) -> u64;

    /// Unsubscribe from transport events.
    fn unsubscribe(&self, id: u64);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transport_state_queries() {
        assert!(!TransportState::Disconnected.is_connected());
        assert!(TransportState::Disconnected.is_terminal());
        assert!(!TransportState::Disconnected.can_send());

        assert!(TransportState::Connected.is_connected());
        assert!(!TransportState::Connected.is_terminal());
        assert!(TransportState::Connected.can_send());

        assert!(TransportState::Failed.is_terminal());
        assert!(!TransportState::Connecting.is_terminal());
    }

    #[test]
    fn transport_error_display() {
        let err = TransportError::connection_failed("host unreachable");
        assert_eq!(err.to_string(), "connection failed: host unreachable");

        let err = TransportError::timeout(5000);
        assert_eq!(err.to_string(), "connection timed out after 5000ms");

        let err = TransportError::remote_closed(Some(1001), Some("going away".into()));
        assert_eq!(err.to_string(), "remote closed (code 1001): going away");
    }

    #[test]
    fn transport_error_recoverability() {
        assert!(TransportError::connection_failed("x").is_recoverable());
        assert!(TransportError::timeout(1000).is_recoverable());
        assert!(TransportError::send_failed("x").is_recoverable());

        assert!(!TransportError::protocol("x").is_recoverable());
        assert!(!TransportError::auth_failed("x").is_recoverable());

        // Normal close is not recoverable
        assert!(!TransportError::remote_closed(Some(1000), None).is_recoverable());
        // Abnormal close is recoverable
        assert!(TransportError::remote_closed(Some(1006), None).is_recoverable());
    }

    #[test]
    fn transport_error_serde_round_trip() {
        let err = TransportError::remote_closed(Some(1001), Some("bye".into()));
        let json = serde_json::to_string(&err).unwrap();
        let parsed: TransportError = serde_json::from_str(&json).unwrap();
        assert_eq!(err, parsed);
    }
}
