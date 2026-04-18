//! `network::presence::types` — data model for the presence manager.
//!
//! Direct port of `presence/presence-types.ts`. Everything here is
//! ephemeral: nothing is persisted to the CRDT layer. Serde uses
//! camelCase on the wire so state round-trips with legacy TS peers
//! that still ship presence snapshots over an awareness protocol.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

// ── Cursor & Selection ──────────────────────────────────────────────────────

/// A single cursor position within an object (optionally narrowed to a
/// field + character offset).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CursorPosition {
    pub object_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub field: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub offset: Option<u32>,
}

/// An inline selection range for multi-cursor support.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectionRange {
    pub object_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub field: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anchor: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub head: Option<u32>,
}

// ── Peer Identity ───────────────────────────────────────────────────────────

/// Stable per-peer identity used by overlay renderers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PeerIdentity {
    pub peer_id: String,
    pub display_name: String,
    pub color: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub avatar_url: Option<String>,
}

// ── Presence State ──────────────────────────────────────────────────────────

/// The full awareness snapshot for a single peer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PresenceState {
    pub identity: PeerIdentity,
    #[serde(default)]
    pub cursor: Option<CursorPosition>,
    #[serde(default)]
    pub selections: Vec<SelectionRange>,
    #[serde(default)]
    pub active_view: Option<String>,
    /// ISO-8601 timestamp of the last update observed for this peer.
    pub last_seen: String,
    #[serde(default)]
    pub data: BTreeMap<String, JsonValue>,
}

// ── Events ──────────────────────────────────────────────────────────────────

/// The kind of presence transition the manager is emitting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PresenceChangeKind {
    Joined,
    Updated,
    Left,
}

/// A single event emitted by `PresenceManager::subscribe`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PresenceChange {
    #[serde(rename = "type")]
    pub kind: PresenceChangeKind,
    pub peer_id: String,
    pub state: Option<PresenceState>,
}

pub type PresenceListener = Box<dyn FnMut(&PresenceChange)>;

// ── Timer Provider ──────────────────────────────────────────────────────────

/// Opaque handle returned by [`TimerProvider::set_interval`]. Hosts
/// map it back to whatever primitive they use internally.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TimerHandle(pub u64);

/// Pluggable timer abstraction so the manager can be driven by a
/// deterministic fake clock in tests. `now` is a millisecond epoch.
pub trait TimerProvider {
    fn now(&self) -> u64;
    fn set_interval(&self, callback: Box<dyn FnMut()>, interval_ms: u64) -> TimerHandle;
    fn clear_interval(&self, handle: TimerHandle);
}

// ── Manager Options ─────────────────────────────────────────────────────────

/// Construction options for `PresenceManager`. `ttl_ms` / `sweep_interval_ms`
/// default to 30s / 5s respectively.
pub struct PresenceManagerOptions {
    pub local_identity: PeerIdentity,
    pub ttl_ms: u64,
    pub sweep_interval_ms: u64,
    pub timers: Box<dyn TimerProvider>,
}
