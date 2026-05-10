//! `ShellBaseService` — the shell-global slice of the legacy
//! `command::with_builtins` list. Owns commands that no other
//! service can reasonably claim as its domain: command-palette
//! visibility toggles, panel/page switches, and the toast clear.
//!
//! Per-feature services (persistence, project, search, …) own their
//! own commands; this service is deliberately thin.

use crate::cmd;
use crate::services::{CommandSpec, ShellService};

#[derive(Default)]
pub struct ShellBaseService;

impl ShellService for ShellBaseService {
    fn id(&self) -> &'static str {
        "shell.base"
    }

    fn commands(&self) -> Vec<CommandSpec> {
        vec![
            cmd!(
                "palette.toggle",
                "Command Palette",
                "View",
                "Ctrl+Shift+P",
                |ctx| {
                    let p = &mut ctx.state.overlay.command_palette;
                    p.open = !p.open;
                    if !p.open {
                        p.query.clear();
                        p.selected_index = 0;
                    }
                }
            ),
            cmd!("palette.close", "Close Command Palette", "View", |ctx| {
                let p = &mut ctx.state.overlay.command_palette;
                p.open = false;
                p.query.clear();
                p.selected_index = 0;
            }),
            cmd!("toasts.clear", "Clear Notifications", "View", |ctx| {
                ctx.state.overlay.toasts.clear();
            }),
        ]
    }
}

#[cfg(test)]
mod tests {
    use crate::services::{MutCtx, ServiceRegistry, UndoStack};
    use crate::AppState;
    use prism_ui_runtime::layout::Viewport;

    #[test]
    fn palette_toggle_flips_open_flag() {
        let mut state = AppState::default();
        let mut undo = UndoStack::default();
        let reg = ServiceRegistry::with_builtins();
        {
            let mut ctx = MutCtx {
                state: &mut state,
                viewport: Viewport {
                    width: 0.0,
                    height: 0.0,
                },
                undo: &mut undo,
            };
            assert!(reg.commands().run("palette.toggle", &mut ctx));
        }
        assert!(state.overlay.command_palette.open);
        {
            let mut ctx = MutCtx {
                state: &mut state,
                viewport: Viewport {
                    width: 0.0,
                    height: 0.0,
                },
                undo: &mut undo,
            };
            assert!(reg.commands().run("palette.toggle", &mut ctx));
        }
        assert!(!state.overlay.command_palette.open);
    }
}
