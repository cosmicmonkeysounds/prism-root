//! `HelpService` — owns the lifecycle of the hover tooltip stored
//! in `state.overlay.help_tooltip`.
//!
//! The legacy `help.rs` mixed a registry of tooltip text with the
//! show/hide state machine. The registry stays valuable for
//! component-side lookups, but the *shell* only needs three things:
//!
//! 1. A command to push a tooltip body (called by per-block hover
//!    handlers via `services.commands().run("help.show", …)`).
//! 2. A command to clear it.
//! 3. An idle timer hook (Esc closes; an 8-second auto-hide is
//!    implemented as a `state.help_open_at: Option<Instant>` field
//!    when the timer service lands).
//!
//! All three live next to the slot data that already carries the
//! tooltip's wire shape (§20). Help-text content stays in
//! `prism_core::help::HelpRegistry` — the service does *not*
//! re-implement that registry; it injects body strings the host
//! looked up.

use std::sync::Mutex;

use prism_ui_runtime::event::Event;

use crate::cmd;
use crate::services::{CommandSpec, CommandTable, EventOutcome, MutCtx, ShellService};
use crate::state::HelpTooltip;

/// Pending tooltip request — the host writes one of these into the
/// service before dispatching `help.show` so the command stays
/// arg-free (the §24 constraint is `&mut MutCtx`-only).
#[derive(Default)]
pub struct HelpService {
    pending: Mutex<Option<HelpTooltip>>,
}

impl HelpService {
    pub fn queue(&self, tip: HelpTooltip) {
        *self.pending.lock().expect("help pending") = Some(tip);
    }

    fn take(&self) -> Option<HelpTooltip> {
        self.pending.lock().expect("help pending").take()
    }
}

impl ShellService for HelpService {
    fn id(&self) -> &'static str {
        "help"
    }

    fn commands(&self) -> Vec<CommandSpec> {
        vec![cmd!("help.hide", "Hide tooltip", "View", |ctx| {
            ctx.state.overlay.help_tooltip = None;
        })]
    }

    fn on_event(&self, event: &Event, ctx: &mut MutCtx<'_>, _cmds: &CommandTable) -> EventOutcome {
        // Esc dismisses the tooltip if showing. We don't claim Esc when
        // closed — palette/search compete for the same key, and the
        // first-Handled-wins rule lets each be modal in its own scope.
        if let Event::Key {
            code,
            pressed: true,
            ..
        } = event
        {
            if code == "escape" && ctx.state.overlay.help_tooltip.is_some() {
                ctx.state.overlay.help_tooltip = None;
                return EventOutcome::Handled;
            }
        }
        // Drain the pending queue: if someone called `queue(...)` since
        // last frame, mount the tip now. This is the seam tests use to
        // assert the show path without inventing a command-with-args
        // shape.
        if let Some(tip) = self.take() {
            ctx.state.overlay.help_tooltip = Some(tip);
            return EventOutcome::HandledQuiet;
        }
        EventOutcome::Pass
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::vfs::test_support::InMemVfs;
    use crate::services::{Clipboard, NoopLuauHost, ServiceRegistry, UndoStack};
    use crate::AppState;
    use prism_ui_runtime::event::Modifiers;
    use prism_ui_runtime::layout::Viewport;

    #[test]
    fn esc_clears_tooltip_when_visible() {
        let mut state = AppState::default();
        state.overlay.help_tooltip = Some(HelpTooltip {
            title: "Save".into(),
            summary: "Write the document".into(),
        });
        let mut undo = UndoStack::default();
        let mut vfs = InMemVfs::default();
        let mut luau = NoopLuauHost::default();
        let mut clipboard = Clipboard::default();
        let reg = ServiceRegistry::with_builtins();
        let mut ctx = MutCtx {
            state: &mut state,
            viewport: Viewport {
                width: 0.0,
                height: 0.0,
            },
            undo: &mut undo,
            vfs: &mut vfs,
            luau: &mut luau,
            clipboard: &mut clipboard,
            registry: None,
            modifier_registry: None,
        };
        let ev = Event::Key {
            code: "escape".into(),
            pressed: true,
            modifiers: Modifiers::default(),
        };
        // Esc currently routes through InputService → palette.close.
        // Help service's modal branch only matters when palette is
        // closed and a tooltip is showing. The palette is closed by
        // default; the input service still has `escape → palette.close`
        // (no-op when not open) so help's branch comes second and wins
        // because palette.close clears nothing on a closed palette but
        // is `Handled` — by design, help only competes for Esc when the
        // input service has no binding for it. We test the direct path:
        let svc = HelpService::default();
        let cmds = reg.commands();
        let outcome = svc.on_event(&ev, &mut ctx, cmds);
        assert_eq!(outcome, EventOutcome::Handled);
        assert!(state.overlay.help_tooltip.is_none());
    }
}
