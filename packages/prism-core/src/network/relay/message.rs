//! `network::relay::message` — relay message envelope and protocol types.
//!
//! Port of `relay/relay-message.ts`. Defines the wire format for
//! relay communication — request/response envelopes, message kinds,
//! and the standard message payload types.

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

/// Unique message identifier for request/response correlation.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MessageId(pub String);

impl MessageId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn generate() -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(1);
        let id = COUNTER.fetch_add(1, Ordering::Relaxed);
        Self(format!("msg-{id}"))
    }
}

impl std::fmt::Display for MessageId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Message kind discriminator for relay protocol.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RelayMessageKind {
    // Connection lifecycle
    Handshake,
    HandshakeAck,
    Ping,
    Pong,
    Disconnect,

    // Presence
    PresenceUpdate,
    PresenceSync,

    // Document sync
    SyncRequest,
    SyncResponse,
    SyncPush,
    SyncAck,

    // Portal
    PortalRequest,
    PortalResponse,

    // Generic envelope
    Request,
    Response,
    Event,
    Error,
}

/// Relay message envelope — wraps all protocol messages.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RelayEnvelope {
    /// Message identifier for correlation
    pub id: MessageId,
    /// Message kind discriminator
    pub kind: RelayMessageKind,
    /// Optional correlation id (for responses)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub correlation_id: Option<MessageId>,
    /// Message payload (kind-specific)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payload: Option<JsonValue>,
    /// ISO-8601 timestamp
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<String>,
}

impl RelayEnvelope {
    pub fn new(kind: RelayMessageKind) -> Self {
        Self {
            id: MessageId::generate(),
            kind,
            correlation_id: None,
            payload: None,
            timestamp: None,
        }
    }

    pub fn with_id(mut self, id: MessageId) -> Self {
        self.id = id;
        self
    }

    pub fn with_correlation(mut self, id: MessageId) -> Self {
        self.correlation_id = Some(id);
        self
    }

    pub fn with_payload(mut self, payload: JsonValue) -> Self {
        self.payload = Some(payload);
        self
    }

    pub fn with_timestamp(mut self, ts: impl Into<String>) -> Self {
        self.timestamp = Some(ts.into());
        self
    }

    pub fn ping() -> Self {
        Self::new(RelayMessageKind::Ping)
    }

    pub fn pong(correlation: MessageId) -> Self {
        Self::new(RelayMessageKind::Pong).with_correlation(correlation)
    }

    pub fn disconnect() -> Self {
        Self::new(RelayMessageKind::Disconnect)
    }

    pub fn error(message: impl Into<String>) -> Self {
        Self::new(RelayMessageKind::Error)
            .with_payload(serde_json::json!({ "message": message.into() }))
    }
}

/// Typed relay request (client → server).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RelayRequest {
    /// Request type (e.g., "portal.get", "sync.push")
    #[serde(rename = "type")]
    pub request_type: String,
    /// Request payload
    #[serde(default)]
    pub params: JsonValue,
}

impl RelayRequest {
    pub fn new(request_type: impl Into<String>) -> Self {
        Self {
            request_type: request_type.into(),
            params: JsonValue::Null,
        }
    }

    pub fn with_params(mut self, params: JsonValue) -> Self {
        self.params = params;
        self
    }

    pub fn portal_get(portal_id: &str) -> Self {
        Self::new("portal.get").with_params(serde_json::json!({ "portalId": portal_id }))
    }

    pub fn portal_list() -> Self {
        Self::new("portal.list")
    }

    pub fn sync_push(collection_id: &str, changes: JsonValue) -> Self {
        Self::new("sync.push").with_params(serde_json::json!({
            "collectionId": collection_id,
            "changes": changes
        }))
    }
}

/// Typed relay response (server → client).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RelayResponse {
    /// Whether the request succeeded
    pub success: bool,
    /// Response data (if success)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<JsonValue>,
    /// Error details (if !success)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<RelayError>,
}

impl RelayResponse {
    pub fn ok(data: JsonValue) -> Self {
        Self {
            success: true,
            data: Some(data),
            error: None,
        }
    }

    pub fn empty_ok() -> Self {
        Self {
            success: true,
            data: None,
            error: None,
        }
    }

    pub fn err(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            success: false,
            data: None,
            error: Some(RelayError {
                code: code.into(),
                message: message.into(),
                details: None,
            }),
        }
    }
}

/// Relay error payload.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RelayError {
    /// Error code (e.g., "NOT_FOUND", "UNAUTHORIZED")
    pub code: String,
    /// Human-readable message
    pub message: String,
    /// Additional error details
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub details: Option<JsonValue>,
}

impl RelayError {
    pub fn not_found(resource: &str) -> Self {
        Self {
            code: "NOT_FOUND".into(),
            message: format!("{resource} not found"),
            details: None,
        }
    }

    pub fn unauthorized() -> Self {
        Self {
            code: "UNAUTHORIZED".into(),
            message: "Authentication required".into(),
            details: None,
        }
    }

    pub fn forbidden() -> Self {
        Self {
            code: "FORBIDDEN".into(),
            message: "Permission denied".into(),
            details: None,
        }
    }

    pub fn bad_request(message: impl Into<String>) -> Self {
        Self {
            code: "BAD_REQUEST".into(),
            message: message.into(),
            details: None,
        }
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self {
            code: "INTERNAL_ERROR".into(),
            message: message.into(),
            details: None,
        }
    }
}

/// High-level relay message — the union of all message payload types.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RelayMessage {
    Envelope(RelayEnvelope),
    Request(RelayRequest),
    Response(RelayResponse),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn message_id_generate_is_unique() {
        let a = MessageId::generate();
        let b = MessageId::generate();
        assert_ne!(a, b);
    }

    #[test]
    fn envelope_ping_pong() {
        let ping = RelayEnvelope::ping();
        assert_eq!(ping.kind, RelayMessageKind::Ping);

        let pong = RelayEnvelope::pong(ping.id.clone());
        assert_eq!(pong.kind, RelayMessageKind::Pong);
        assert_eq!(pong.correlation_id, Some(ping.id));
    }

    #[test]
    fn envelope_error_has_payload() {
        let err = RelayEnvelope::error("something went wrong");
        assert_eq!(err.kind, RelayMessageKind::Error);
        let payload = err.payload.unwrap();
        let msg = payload["message"].as_str().unwrap();
        assert_eq!(msg, "something went wrong");
    }

    #[test]
    fn request_portal_get() {
        let req = RelayRequest::portal_get("welcome");
        assert_eq!(req.request_type, "portal.get");
        assert_eq!(req.params["portalId"], "welcome");
    }

    #[test]
    fn response_ok_and_err() {
        let ok = RelayResponse::ok(serde_json::json!({"id": 1}));
        assert!(ok.success);
        assert!(ok.data.is_some());
        assert!(ok.error.is_none());

        let err = RelayResponse::err("NOT_FOUND", "Portal not found");
        assert!(!err.success);
        assert!(err.data.is_none());
        assert_eq!(err.error.as_ref().unwrap().code, "NOT_FOUND");
    }

    #[test]
    fn relay_error_constructors() {
        let not_found = RelayError::not_found("Portal");
        assert_eq!(not_found.code, "NOT_FOUND");
        assert!(not_found.message.contains("Portal"));

        let unauth = RelayError::unauthorized();
        assert_eq!(unauth.code, "UNAUTHORIZED");
    }

    #[test]
    fn envelope_serde_round_trip() {
        let env = RelayEnvelope::new(RelayMessageKind::SyncPush)
            .with_payload(serde_json::json!({"changes": [1,2,3]}))
            .with_timestamp("2026-04-18T12:00:00Z");
        let json = serde_json::to_string(&env).unwrap();
        let parsed: RelayEnvelope = serde_json::from_str(&json).unwrap();
        assert_eq!(env.kind, parsed.kind);
        assert_eq!(env.payload, parsed.payload);
        assert_eq!(env.timestamp, parsed.timestamp);
    }

    #[test]
    fn request_serde_round_trip() {
        let req = RelayRequest::sync_push("col-1", serde_json::json!({"op": "insert"}));
        let json = serde_json::to_string(&req).unwrap();
        let parsed: RelayRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(req, parsed);
    }
}
