//! `CommandPaletteService` — the first service whose `on_event`
//! short-circuits while open. The palette's data lives on
//! `OverlaySlot::command_palette` (§20); this service is the
//! mutator side and the modal-capture enforcer.
//!
//! The §24.8 keystone (`palette_short_circuits_other_services_while_open`)
//! is realised here: while open, every `Text`/`Key` event returns
//! `EventOutcome::Handled` so no later service in the fan-out sees
//! it. Shortcuts that resolve *before* this service in the registry
//! (palette.toggle/close on the base scheme) still fire because
//! `InputService` is registered ahead of the palette and the base
//! scheme owns those combos. (§25.)

use prism_ui_runtime::event::Event;

use crate::cmd;
use crate::services::{CommandSpec, CommandTable, EventOutcome, MutCtx, ShellService};

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

    fn on_event(&self, event: &Event, ctx: &mut MutCtx<'_>, cmds: &CommandTable) -> EventOutcome {
        if !ctx.state.overlay.command_palette.open {
            return EventOutcome::Pass;
        }
        match event {
            Event::Text { text } => {
                ctx.state.overlay.command_palette.query.push_str(text);
                ctx.state.overlay.command_palette.selected_index = 0;
                EventOutcome::Handled
            }
            Event::Key {
                code,
                pressed: true,
                ..
            } => {
                let id = match code.as_str() {
                    "escape" => Some("palette.close"),
                    "enter" | "return" => Some("palette.exec-selected"),
                    "arrowup" | "up" => Some("palette.move-up"),
                    "arrowdown" | "down" => Some("palette.move-down"),
                    "backspace" => {
                        ctx.state.overlay.command_palette.query.pop();
                        ctx.state.overlay.command_palette.selected_index = 0;
                        return EventOutcome::Handled;
                    }
                    _ => None,
                };
                if let Some(id) = id {
                    cmds.run(id, ctx);
                }
                // Modal capture: every key terminates here so no later
                // service (Save, Find, …) sees it. The palette's own
                // navigation keys are dispatched first (above); every
                // other key is silently consumed.
                EventOutcome::Handled
            }
            Event::Wheel { .. } => EventOutcome::Handled,
            _ => EventOutcome::Pass,
        }
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

    fn key(code: &str, ctrl: bool, shift: bool) -> Event {
        Event::Key {
            code: code.into(),
            pressed: true,
            modifiers: Modifiers {
                ctrl,
                shift,
                ..Default::default()
            },
        }
    }

    #[test]
    fn palette_open_swallows_save_shortcut() {
        // §24.8 keystone: Ctrl+S must NOT save while the palette is
        // open. We populate `current_file` so a runaway save would
        // succeed (and clear `dirty`) — and then assert it didn't.
        let mut state = AppState::default();
        state.overlay.command_palette.open = true;
        state.project.current_file = Some(std::path::PathBuf::from("/tmp/x.prism"));
        state.project.dirty = true;
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
        };
        let outcome = reg.fan_out(&key("s", true, false), &mut ctx);
        assert_eq!(outcome, EventOutcome::Handled, "palette captures the key");
        assert!(state.project.dirty, "save did NOT run");
    }

    #[test]
    fn text_event_appends_to_query_when_open() {
        let svc = CommandPaletteService;
        let mut state = AppState::default();
        state.overlay.command_palette.open = true;
        let mut undo = UndoStack::default();
        let mut vfs = InMemVfs::default();
        let mut luau = NoopLuauHost::default();
        let mut clipboard = Clipboard::default();
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
        };
        let cmds = CommandTable::default();
        assert_eq!(
            svc.on_event(&Event::Text { text: "und".into() }, &mut ctx, &cmds),
            EventOutcome::Handled
        );
        assert_eq!(state.overlay.command_palette.query, "und");
    }
}
