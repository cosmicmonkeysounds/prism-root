//! `state_machine` — finite state machines used by the kernel.
//!
//! Port destination for `kernel/state-machine/` in the legacy TS tree
//! (commit `8426588`). The legacy package shipped two machines:
//!
//! - `machine.ts` — a flat, hand-rolled FSM with no dependencies. This
//!   is what the kernel and internal subsystems use. Ported in full
//!   as [`machine`].
//! - `tool.machine.ts` — an xstate-backed machine for Studio's tool
//!   mode tracking. The migration plan §9 calls for a `statig` rewrite
//!   rather than a 1:1 port; that lands alongside the rest of the
//!   Studio kernel wiring and has its own design pass.
//!
//! Only `machine` is exported here. The Studio tool machine slots in
//! under its own submodule when the statig port arrives.

pub mod machine;

pub use machine::{
    Machine, MachineDefinition, MachineOptions, StateNode, Subscription, Transition, TransitionFrom,
};
