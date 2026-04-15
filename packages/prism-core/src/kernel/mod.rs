//! `kernel` — runtime wiring primitives shared by every Prism host.
//!
//! Phase 2 destination for the legacy `@prism/core/kernel` subtree:
//! `actor`, `automation`, `state-machine`, `config`, `plugin`,
//! `plugin-bundles`, `builder`, `initializer`. Landed so far:
//!
//! - [`store`] — the hand-rolled `AppState` + reducer + subscription
//!   bus that replaces `zustand` per §6.1 of the Clay migration plan.
//!   Backs the shell's single reloadable root state and satisfies the
//!   §7 hot-reload constraints (snapshot / restore via serde,
//!   everything reloadable behind one struct, no global mutable
//!   state).

pub mod store;

pub use store::{Action, Store, Subscription};
