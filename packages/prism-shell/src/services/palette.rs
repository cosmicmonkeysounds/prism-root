//! `CommandPaletteService` — owns the palette's command rows
//! (`palette.{open, move-up, move-down, exec-selected}`).
//!
//! Event handling — typing into the query, modal-capture for Ctrl+S,
//! Enter/Esc/arrow result navigation — moved to a
//! [`TextInputDeclaration`](crate::services::text_input::TextInputDeclaration)
//! on `DeclarativeTextInputService` during the editor-unify pass. The
//! palette's modal-capture invariant (§24.8) is now expressed
//! declaratively via `TextInputDeclaration::modal_capture` + the
//! plain-key passthrough list.
//!
//! This service only contributes commands; it has no `on_event` impl.

use crate::cmd;
use crate::services::{CommandSpec, ShellService};

#[derive(Default)]
pub struct CommandPaletteService;

impl ShellService for CommandPaletteService {
    fn id(&self) -> &'static str {
        "command-palette"
    }

    fn commands(&self) -> Vec<CommandSpec> {
        vec![
            cmd!("palette.open", "Open Command Palette", "View", |ctx| {
                ctx.state.overlay.command_palette.open = true;
                ctx.state.overlay.command_palette.selected_index = 0;
            }),
            cmd!("palette.move-up", "Previous Command", "View", "Up", |ctx| {
                let p = &mut ctx.state.overlay.command_palette;
                let n = p.results.len();
                if n > 0 {
                    p.selected_index = (p.selected_index + n - 1) % n;
                }
            }),
            cmd!("palette.move-down", "Next Command", "View", "Down", |ctx| {
                let p = &mut ctx.state.overlay.command_palette;
                let n = p.results.len();
                if n > 0 {
                    p.selected_index = (p.selected_index + 1) % n;
                }
            }),
            cmd!(
                "palette.exec-selected",
                "Run Selected Command",
                "View",
                "Enter",
                |_ctx| {
                    // Filled in by the host: it reads
                    // `state.overlay.command_palette.results[selected_index].id`
                    // and re-dispatches through the same command table
                    // before clearing the palette. Service-side body is
                    // intentionally empty so the dispatch path is one
                    // table lookup, never a reentrant `run`-from-handler.
                }
            ),
        ]
    }
}
