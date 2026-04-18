//! `network::relay_manager` — connection manager for multiple relays.
//!
//! Port of `kernel/relay-manager.ts` from the legacy TS tree. Manages
//! a pool of [`RelayConnection`]s, routes messages to/from them, and
//! tracks which relay is primary for each operation type.
//!
//! The manager is synchronous and host-driven — actual transport I/O
//! is performed by the host (using `tokio-tungstenite`, `reqwest`, etc.)
//! and fed back through the manager's `process_*` methods.

pub mod manager;
pub mod policy;
pub mod types;

pub use manager::{RelayManager, RelayManagerOptions};
pub use policy::{RelayPolicy, RelayPolicyKind, RelaySelector};
pub use types::{
    ManagedRelay, RelayHealth, RelayManagerEvent, RelayManagerListener, RelayManagerStats,
    RelayStatus,
};
