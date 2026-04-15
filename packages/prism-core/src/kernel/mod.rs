//! `kernel` — runtime wiring primitives shared by every Prism host.
//!
//! Phase 2 destination for the legacy `@prism/core/kernel` subtree:
//! `actor`, `automation`, `state-machine`, `config`, `plugin`,
//! `plugin-bundles`, `builder`, `initializer`. Landed so far:
//!
//! - [`store`] — the hand-rolled `AppState` + reducer + subscription
//!   bus that replaces `zustand` per §6.1 of the Slint migration plan.
//!   Backs the shell's single reloadable root state and satisfies the
//!   §7 hot-reload constraints (snapshot / restore via serde,
//!   everything reloadable behind one struct, no global mutable
//!   state).
//! - [`state_machine`] — the flat, context-free finite state machine
//!   ported from `kernel/state-machine/machine.ts`. The xstate-backed
//!   tool machine from the legacy tree is deferred to a later `statig`
//!   pass and is not exported here yet.

pub mod state_machine;
pub mod store;

pub use store::{Action, Store, Subscription};
