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
//! **Phase 2b — pending.** The ADR-002 §Part C `PrismKernel`
//! orchestration kit has not landed yet: `actor` (ProcessQueue,
//! ActorRuntime), `intelligence` (AI provider registry + context
//! builder, split out of the legacy `actor/`), `automation`
//! (trigger/condition/action engine), `plugin` + `plugin_bundles`,
//! `builder` (`BuildExecutor`-backed page build manager), and
//! `initializer`. None are on Phase 3's critical path, so they run in
//! parallel with the builder/shell port.

pub mod actor;
pub mod automation;
pub mod config;
pub mod initializer;
pub mod intelligence;
pub mod plugin;
pub mod state_machine;
pub mod store;

pub use initializer::{
    install_initializers, noop_disposer, Disposer, KernelInitializer, KernelInitializerContext,
};
pub use store::{Action, Store, Subscription};
