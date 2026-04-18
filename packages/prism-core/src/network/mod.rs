//! `network` — Prism's networking / communication-fabric subtree.
//!
//! Port of `@prism/core`'s `network/*` TS subtree. All six siblings
//! are live: `presence`, `relay`, `relay_manager`, `discovery`,
//! `session`, and `server`.

pub mod discovery;
pub mod presence;
pub mod relay;
pub mod relay_manager;
pub mod server;
pub mod session;
