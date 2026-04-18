//! `network::discovery` — peer and relay discovery protocol.
//!
//! Port of the discovery concepts from the legacy TS tree. Provides
//! `VaultRoster` — a local address book of known peers and relays
//! for a vault — and `DiscoveryService` which scans for new peers
//! via relay directory feeds and peer announcements.
//!
//! The discovery layer is host-driven: actual network I/O is
//! performed by the host and fed back through the service's
//! `process_*` methods, mirroring `relay_manager`.

pub mod roster;
pub mod service;

pub use roster::{
    PeerRecord, RelayRecord, RosterEvent, RosterEventKind, RosterListener, VaultRoster,
    VaultRosterOptions,
};
pub use service::{DiscoveryEvent, DiscoveryEventKind, DiscoveryListener, DiscoveryService};
