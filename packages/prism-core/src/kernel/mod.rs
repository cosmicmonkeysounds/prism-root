//! `kernel` — runtime wiring primitives shared by every Prism host.
//!
//! Phase-2 destination for the legacy `@prism/core/kernel` subtree.
//! Phase 2a (Slint migration plan §6) landed the three leaves every
//! other crate actually needs at Phase-3 boot:
//!
//! - [`store`] — the hand-rolled `AppState` + reducer + subscription
//!   bus that replaces `zustand` per §6.1 of the Slint migration plan.
//!   Backs the shell's single reloadable root state and satisfies the
//!   §7 hot-reload constraints (snapshot / restore via serde,
//!   everything reloadable behind one struct, no global mutable
//!   state).
//! - [`state_machine`] — the flat, context-free finite state machine
//!   ported from `kernel/state-machine/machine.ts`. The xstate-backed
//!   tool machine (`tool.machine.ts`) is a Phase-2b `statig` rewrite
//!   and is not exported here yet.
//! - [`config`] — layered config system: `ConfigRegistry` +
//!   `ConfigModel` (scope-cascaded resolution, watchers, attached
//!   stores), `MemoryConfigStore`, JSON Schema subset validator, and
//!   `FeatureFlags` bound to the model via the `on_change` bus. Ports
//!   `kernel/config/*` from the legacy TS tree.
//!
//! Phase 2b (ADR-002 §Part C) landed the orchestration kit on top:
//! `actor` (ProcessQueue + ActorRuntime trait), `intelligence` (AI
//! provider registry + context builder), `automation`
//! (trigger/condition/action engine), `plugin` + `plugin_bundles`
//! (contribution fan-out + six built-in bundles), `builder`
//! (`BuildExecutor`-backed profile/plan manager), `initializer`
//! (self-installing startup hooks), and the [`PrismKernel`] struct
//! itself — the canonical composition that wires all of the above
//! into a single handle for hosts to hand around.

pub mod actor;
pub mod atom;
pub mod automation;
pub mod builder;
pub mod config;
#[cfg(feature = "crdt")]
pub mod crdt_sync;
pub mod initializer;
pub mod intelligence;
pub mod plugin;
pub mod plugin_bundles;
pub mod prism_kernel;
pub mod state_machine;
pub mod store;

pub use atom::{select, select_ref, Atom, AtomSubscription, SharedAtom};
pub use initializer::{
    install_initializers, noop_disposer, Disposer, KernelInitializer, KernelInitializerContext,
};
pub use prism_kernel::{PrismKernel, PrismKernelOptions};
pub use store::{Action, Store, Subscription};
