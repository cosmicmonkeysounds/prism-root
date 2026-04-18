//! `network::relay` — relay connection types, transport abstractions, and module system.
//!
//! Port of `packages/prism-core/src/network/relay/*` from the legacy TS
//! tree. Defines the data model for connecting to Prism relays (the
//! Sovereign Portal servers that host portals and route messages),
//! plus the composable module system that powers the server side.
//!
//! Client-side: pure data + trait definitions. Actual transport
//! implementations (WebSocket, HTTP) live in host crates (`prism-shell`,
//! `prism-studio`) that have access to the appropriate async runtimes.
//!
//! Server-side: the `module_system` + `modules` subtree provides the
//! 17-module pluggable architecture ported from the old Hono relay.

pub mod connection;
pub mod message;
pub mod module_system;
pub mod modules;
pub mod transport;
pub mod types;

pub use connection::{
    ConnectionEvent, ConnectionListener, ConnectionStats, ConnectionSubscription, RelayConnection,
    RelayConnectionState,
};
pub use message::{
    MessageId, RelayEnvelope, RelayError, RelayMessage, RelayMessageKind, RelayRequest,
    RelayResponse,
};
pub use module_system::{
    capabilities, RelayBuildError, RelayBuilder, RelayContext, RelayInstance, RelayModule,
    RelayServerConfig,
};
pub use transport::{Transport, TransportError, TransportEvent, TransportState};
pub use types::{RelayConfig, RelayEndpoint, RelayId, RelayInfo, RelayProtocol};
