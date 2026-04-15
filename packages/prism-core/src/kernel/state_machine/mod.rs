//! `state_machine` — finite state machines used by the kernel.
//!
//! Port destination for `kernel/state-machine/` in the legacy TS tree
//! (commit `8426588`). The legacy package shipped two machines:
//!
//! - `machine.ts` — a flat, hand-rolled FSM with no dependencies. This
//!   is what the kernel and internal subsystems use. Ported in full
//!   as [`machine`].
//! - `tool.machine.ts` — an xstate-backed machine for Studio's tool
//!   mode tracking. The migration plan originally pencilled in a
//!   `statig` rewrite; the Rust port collapses it onto the already-
//!   ported flat [`machine::Machine`] instead (3 states, 6 events —
//!   zero reason to pull a new runtime crate). Exposed as [`tool`].

pub mod machine;
pub mod tool;

pub use machine::{
    Machine, MachineDefinition, MachineOptions, StateNode, Subscription, Transition, TransitionFrom,
};
pub use tool::{create_tool_machine, tool_machine_definition, ToolEvent, ToolMode};
