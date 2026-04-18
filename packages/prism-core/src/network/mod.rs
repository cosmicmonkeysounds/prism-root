//! `network` — Prism's networking / communication-fabric subtree.
//!
//! Port of `@prism/core`'s `network/*` TS subtree. Currently lands
//! leaf-first: `presence` is live, the remaining siblings
//! (`discovery`, `session`, `server`, `relay`, `relay_manager`) will
//! light up as they're ported in.

pub mod presence;
