//! `network::presence` — real-time collaborative presence tracker.
//!
//! Port of `packages/prism-core/src/network/presence/*` from the
//! legacy TS tree. Tracks the local peer plus a live map of remote
//! peers, TTL-expires stale entries on a pluggable timer, and
//! notifies subscribers of joined / updated / left transitions.
//!
//! The timer is abstracted through [`TimerProvider`] so tests can
//! inject a deterministic fake clock. `SystemTimer` ships for real
//! callers but its interval scheduling is a no-op (hosts drive the
//! sweep from their own event loop).

pub mod manager;
pub mod types;

pub use manager::{create_presence_manager, PresenceManager, SystemTimer};
pub use types::{
    CursorPosition, PeerIdentity, PresenceChange, PresenceChangeKind, PresenceListener,
    PresenceManagerOptions, PresenceState, SelectionRange, TimerHandle, TimerProvider,
};
