//! Shared types for the Trust & Safety layer. Port of
//! `identity/trust/trust-types.ts`. Grouped by subsystem — the
//! individual implementations live in sibling modules.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

// ── Luau Sandbox ────────────────────────────────────────────────────────────

/// Capabilities a sandboxed Luau plugin may request. Serialises as
/// the same colon-prefixed strings the TS enum used.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SandboxCapability {
    #[serde(rename = "crdt:read")]
    CrdtRead,
    #[serde(rename = "crdt:write")]
    CrdtWrite,
    #[serde(rename = "net:fetch")]
    NetFetch,
    #[serde(rename = "net:websocket")]
    NetWebsocket,
    #[serde(rename = "fs:read")]
    FsRead,
    #[serde(rename = "fs:write")]
    FsWrite,
    #[serde(rename = "ui:notify")]
    UiNotify,
    #[serde(rename = "ui:dialog")]
    UiDialog,
    #[serde(rename = "process:spawn")]
    ProcessSpawn,
    #[serde(rename = "ai:complete")]
    AiComplete,
    #[serde(rename = "ai:inline")]
    AiInline,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SandboxPolicy {
    #[serde(rename = "pluginId")]
    pub plugin_id: String,
    pub capabilities: Vec<SandboxCapability>,
    #[serde(rename = "maxDurationMs")]
    pub max_duration_ms: u64,
    #[serde(rename = "maxMemoryBytes")]
    pub max_memory_bytes: u64,
    #[serde(rename = "allowedUrls")]
    pub allowed_urls: Vec<String>,
    #[serde(rename = "allowedPaths")]
    pub allowed_paths: Vec<String>,
}

/// A single recorded sandbox violation. The `capability` field is a
/// free-form string rather than a typed enum because the TS source
/// allowed both real [`SandboxCapability`] values and the
/// pseudo-capabilities `"timeout"` / `"memory"` — we preserve that by
/// just keeping the serialised form.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SandboxViolation {
    pub capability: String,
    pub message: String,
    pub timestamp: String,
    #[serde(rename = "pluginId")]
    pub plugin_id: String,
}

impl SandboxCapability {
    /// The colon-prefixed wire form. Handy when logging a
    /// [`SandboxViolation`] whose `capability` field is the raw string.
    pub fn as_str(self) -> &'static str {
        match self {
            SandboxCapability::CrdtRead => "crdt:read",
            SandboxCapability::CrdtWrite => "crdt:write",
            SandboxCapability::NetFetch => "net:fetch",
            SandboxCapability::NetWebsocket => "net:websocket",
            SandboxCapability::FsRead => "fs:read",
            SandboxCapability::FsWrite => "fs:write",
            SandboxCapability::UiNotify => "ui:notify",
            SandboxCapability::UiDialog => "ui:dialog",
            SandboxCapability::ProcessSpawn => "process:spawn",
            SandboxCapability::AiComplete => "ai:complete",
            SandboxCapability::AiInline => "ai:inline",
        }
    }
}

// ── Schema Poison Pill ──────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SchemaValidationSeverity {
    Error,
    Warning,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SchemaValidationIssue {
    pub path: String,
    pub message: String,
    pub severity: SchemaValidationSeverity,
    pub rule: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SchemaValidationResult {
    pub valid: bool,
    pub issues: Vec<SchemaValidationIssue>,
}

/// Options for building a [`super::schema_validator::SchemaValidator`].
#[derive(Debug, Clone)]
pub struct SchemaValidatorOptions {
    pub max_depth: usize,
    pub max_string_length: usize,
    pub max_array_length: usize,
    pub max_total_keys: usize,
    pub disallowed_key_patterns: Vec<regex::Regex>,
}

impl Default for SchemaValidatorOptions {
    fn default() -> Self {
        Self {
            max_depth: 20,
            max_string_length: 1_000_000,
            max_array_length: 10_000,
            max_total_keys: 50_000,
            disallowed_key_patterns: default_disallowed_keys(),
        }
    }
}

/// Default disallowed key patterns: prototype-pollution vectors.
pub fn default_disallowed_keys() -> Vec<regex::Regex> {
    vec![
        regex::Regex::new(r"^__proto__$").expect("static regex"),
        regex::Regex::new(r"^constructor$").expect("static regex"),
        regex::Regex::new(r"^prototype$").expect("static regex"),
    ]
}

// ── Hashcash ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HashcashChallenge {
    pub resource: String,
    pub bits: u32,
    #[serde(rename = "issuedAt")]
    pub issued_at: String,
    pub salt: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HashcashProof {
    pub challenge: HashcashChallenge,
    pub counter: u64,
    pub hash: String,
}

// ── Web of Trust ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TrustLevel {
    Unknown,
    Untrusted,
    Neutral,
    Trusted,
    HighlyTrusted,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PeerReputation {
    #[serde(rename = "peerId")]
    pub peer_id: String,
    #[serde(rename = "trustLevel")]
    pub trust_level: TrustLevel,
    pub score: i32,
    #[serde(rename = "positiveInteractions")]
    pub positive_interactions: u32,
    #[serde(rename = "negativeInteractions")]
    pub negative_interactions: u32,
    pub banned: bool,
    #[serde(rename = "banReason")]
    pub ban_reason: Option<String>,
    #[serde(rename = "lastSeenAt")]
    pub last_seen_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContentHash {
    pub hash: String,
    pub category: String,
    #[serde(rename = "reportedBy")]
    pub reported_by: String,
    #[serde(rename = "reportedAt")]
    pub reported_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum TrustGraphEvent {
    PeerAdded {
        #[serde(rename = "peerId")]
        peer_id: String,
    },
    PeerUpdated {
        #[serde(rename = "peerId")]
        peer_id: String,
    },
    PeerBanned {
        #[serde(rename = "peerId")]
        peer_id: String,
    },
    PeerUnbanned {
        #[serde(rename = "peerId")]
        peer_id: String,
    },
    ContentFlagged {
        #[serde(rename = "contentHash")]
        content_hash: String,
    },
}

/// Options for a [`super::peer_trust_graph::PeerTrustGraph`]. Matches
/// the TS defaults one-for-one.
#[derive(Debug, Clone, Copy)]
pub struct TrustGraphOptions {
    pub trusted_threshold: i32,
    pub highly_trusted_threshold: i32,
    pub positive_weight: i32,
    pub negative_weight: i32,
}

impl Default for TrustGraphOptions {
    fn default() -> Self {
        Self {
            trusted_threshold: 30,
            highly_trusted_threshold: 70,
            positive_weight: 5,
            negative_weight: -10,
        }
    }
}

// ── Shamir Secret Sharing ──────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShamirShare {
    pub index: u8,
    pub data: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShamirConfig {
    pub total_shares: u8,
    pub threshold: u8,
}

// ── Encrypted Escrow ───────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EscrowDeposit {
    pub id: String,
    #[serde(rename = "depositorId")]
    pub depositor_id: String,
    #[serde(rename = "encryptedPayload")]
    pub encrypted_payload: String,
    #[serde(rename = "depositedAt")]
    pub deposited_at: String,
    #[serde(rename = "expiresAt")]
    pub expires_at: Option<String>,
    pub claimed: bool,
}

// ── Password Authentication ────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PasswordAuthRecord {
    pub username: String,
    pub did: String,
    pub salt: String,
    #[serde(rename = "passwordHash")]
    pub password_hash: String,
    pub iterations: u32,
    #[serde(rename = "createdAt")]
    pub created_at: String,
    #[serde(rename = "updatedAt")]
    pub updated_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<BTreeMap<String, String>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PasswordAuthResult {
    Ok(PasswordAuthRecord),
    UnknownUser,
    WrongPassword,
}

impl PasswordAuthResult {
    pub fn is_ok(&self) -> bool {
        matches!(self, PasswordAuthResult::Ok(_))
    }
}

#[derive(Debug, Clone, Copy)]
pub struct PasswordAuthManagerOptions {
    pub iterations: u32,
    pub salt_bytes: usize,
}

impl Default for PasswordAuthManagerOptions {
    fn default() -> Self {
        Self {
            iterations: 600_000,
            salt_bytes: 16,
        }
    }
}
