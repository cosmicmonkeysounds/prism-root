//! Relay modules — the 17 pluggable capabilities from the old Hono relay.
//!
//! Each module implements `RelayModule` and registers its capability
//! into the `RelayContext` at install time. Modules that wrap existing
//! `identity::trust` primitives delegate to them; the rest are new.

pub mod acme;
pub mod blind_mailbox;
pub mod blind_ping;
pub mod capability_tokens;
pub mod collection_host;
pub mod escrow;
pub mod federation;
pub mod hashcash;
pub mod oauth;
pub mod password_auth;
pub mod peer_trust;
pub mod portal_templates;
pub mod relay_router;
pub mod signaling;
pub mod sovereign_portals;
pub mod timestamper;
pub mod vault_host;
pub mod webhooks;
