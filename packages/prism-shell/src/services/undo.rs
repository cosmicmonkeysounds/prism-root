//! `UndoRedoService` + the `UndoStack` it owns. Every mutation that
//! wants to be undoable snapshots `AppState` *before* it runs and
//! pushes the snapshot onto the stack.
//!
//! The stack lives on `MutCtx` (`undo` field) so any command body
//! can call `ctx.undo.snapshot(ctx.state)` before mutating; every
//! consumer reaches the same single stack.

use crate::cmd;
use crate::services::{CommandSpec, ShellService};
use crate::AppState;

/// 100-entry circular undo stack. Stores full `AppState` snapshots
/// (cheap because `AppState` is `Clone` and most slots are small).
/// When the snapshot cost grows past the threshold of "noticeable
/// in a sustained drag," this becomes a per-slot delta stack —
/// not an interface change, the `MutCtx::undo` field stays.
#[derive(Default)]
pub struct UndoStack {
    past: Vec<AppState>,
    future: Vec<AppState>,
}

impl UndoStack {
    pub const LIMIT: usize = 100;

    /// Capture the current state. Call this *before* a mutation;
    /// `undo()` then restores the captured state.
    pub fn snapshot(&mut self, state: &AppState) {
        self.past.push(state.clone());
        if self.past.len() > Self::LIMIT {
            self.past.remove(0);
        }
        self.future.clear();
    }

    pub fn undo(&mut self, state: &mut AppState) -> bool {
        if let Some(prev) = self.past.pop() {
            let current = std::mem::replace(state, prev);
            self.future.push(current);
            true
        } else {
            false
        }
    }

    pub fn redo(&mut self, state: &mut AppState) -> bool {
        if let Some(next) = self.future.pop() {
            let current = std::mem::replace(state, next);
            self.past.push(current);
            true
        } else {
            false
        }
    }

    pub fn can_undo(&self) -> bool {
        !self.past.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.future.is_empty()
    }
}

#[derive(Default)]
pub struct UndoRedoService;

impl ShellService for UndoRedoService {
    fn id(&self) -> &'static str {
        "undo-redo"
    }

    fn commands(&self) -> Vec<CommandSpec> {
        vec![
            cmd!("edit.undo", "Undo", "Edit", "Ctrl+Z", |ctx| {
                ctx.undo.undo(ctx.state);
            }),
            cmd!("edit.redo", "Redo", "Edit", "Ctrl+Shift+Z", |ctx| {
                ctx.undo.redo(ctx.state);
            }),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::{
        vfs::test_support::InMemVfs, Clipboard, MutCtx, NoopLuauHost, ServiceRegistry,
    };
    use prism_ui_runtime::layout::Viewport;

    fn ctx<'a>(
        state: &'a mut AppState,
        undo: &'a mut UndoStack,
        vfs: &'a mut InMemVfs,
        luau: &'a mut NoopLuauHost,
        clipboard: &'a mut Clipboard,
    ) -> MutCtx<'a> {
        MutCtx {
            state,
            viewport: Viewport {
                width: 0.0,
                height: 0.0,
            },
            undo,
            vfs,
            luau,
            clipboard,
            registry: None,
            modifier_registry: None,
        }
    }

    #[test]
    fn snapshot_then_undo_restores_state() {
        let mut state = AppState::default();
        let mut undo = UndoStack::default();
        state.chrome.status = "before".into();
        undo.snapshot(&state);
        state.chrome.status = "after".into();
        assert!(undo.undo(&mut state));
        assert_eq!(state.chrome.status, "before");
        assert!(undo.redo(&mut state));
        assert_eq!(state.chrome.status, "after");
    }

    #[test]
    fn undo_empty_is_noop() {
        let mut undo = UndoStack::default();
        let mut state = AppState::default();
        assert!(!undo.undo(&mut state));
    }

    #[test]
    fn undo_redo_commands_dispatch_through_table() {
        let mut state = AppState::default();
        let mut undo = UndoStack::default();
        state.chrome.status = "v0".into();
        undo.snapshot(&state);
        state.chrome.status = "v1".into();
        let reg = ServiceRegistry::with_builtins();
        let mut vfs = InMemVfs::default();
        let mut luau = NoopLuauHost::default();
        let mut clipboard = Clipboard::default();
        let mut c = ctx(&mut state, &mut undo, &mut vfs, &mut luau, &mut clipboard);
        assert!(reg.commands().run("edit.undo", &mut c));
        assert_eq!(state.chrome.status, "v0");
        let mut c2 = ctx(&mut state, &mut undo, &mut vfs, &mut luau, &mut clipboard);
        assert!(reg.commands().run("edit.redo", &mut c2));
        assert_eq!(state.chrome.status, "v1");
    }
}
