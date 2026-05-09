//! `dispatch_event` — single router from `prism_ui_runtime::event::Event`
//! into `ShellInner` mutations. Replaces the deleted `app/callbacks/`
//! directory (6 files of Slint-callback wiring).
//!
//! Returns `true` when the next frame must re-render. The supervisor
//! in `app::shell::Shell::run` drives this: every event arm either
//! mutates the store (re-render) or is a no-op (skip the redraw).
//!
//! See `docs/dev/clay-migration-plan.md` §17.

use std::cell::RefCell;
use std::rc::Rc;

use prism_ui_runtime::event::Event;

use crate::shell::ShellInner;

/// One match over every runtime event variant. Each arm is a one-line
/// dispatch into the existing `ShellInner` mutation methods.
pub fn dispatch_event(_inner: &Rc<RefCell<ShellInner>>, _event: &Event) -> bool {
    // TODO(§17): per-variant arms land as the existing
    // `app/callbacks/{builder,chrome,editor,navigation,overlay,properties}.rs`
    // bodies are ported. The translation is mechanical — every
    // closure that took `&AppWindow + Rc<RefCell<ShellInner>>`
    // becomes one arm here that takes `&Event + &Rc<RefCell<ShellInner>>`
    // and forwards to the same `ShellInner::*` mutator it always did.
    false
}
