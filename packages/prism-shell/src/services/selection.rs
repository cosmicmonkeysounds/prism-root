//! `SelectionService` — mutators for the selection cursor that
//! already lives on `CanvasSlot::selection` (and indirectly on
//! `BuilderSlot` through the shared resolution path). The §22
//! pointer arms own the *capture* side; this service owns the
//! *keyboard* side: arrow nudging, shift-extend, and the cross-slot
//! `Esc` clear.
//!
//! The cross-slot invariant ("Esc clears canvas + builder") lives on
//! `AppState::clear_selection` — one method, two slots, zero
//! per-service awareness of the other slot's shape. (§25.)

use crate::cmd;
use crate::services::{CommandSpec, ShellService};

#[derive(Default)]
pub struct SelectionService;

impl ShellService for SelectionService {
    fn id(&self) -> &'static str {
        "selection"
    }

    fn commands(&self) -> Vec<CommandSpec> {
        vec![
            cmd!(
                "selection.clear",
                "Clear Selection",
                "Edit",
                "Escape",
                |ctx| {
                    ctx.state.clear_selection();
                }
            ),
            cmd!(
                "selection.move-up",
                "Move Selection Up",
                "Edit",
                "Up",
                |ctx| {
                    ctx.state.canvas.nudge_selection(0.0, -1.0);
                }
            ),
            cmd!(
                "selection.move-down",
                "Move Selection Down",
                "Edit",
                "Down",
                |ctx| {
                    ctx.state.canvas.nudge_selection(0.0, 1.0);
                }
            ),
            cmd!(
                "selection.move-left",
                "Move Selection Left",
                "Edit",
                "Left",
                |ctx| {
                    ctx.state.canvas.nudge_selection(-1.0, 0.0);
                }
            ),
            cmd!(
                "selection.move-right",
                "Move Selection Right",
                "Edit",
                "Right",
                |ctx| {
                    ctx.state.canvas.nudge_selection(1.0, 0.0);
                }
            ),
        ]
    }
}

#[cfg(test)]
mod tests {
    use crate::services::vfs::test_support::InMemVfs;
    use crate::services::{Clipboard, MutCtx, NoopLuauHost, ServiceRegistry, UndoStack};
    use crate::AppState;
    use prism_builder::{BuilderDocument, Node};
    use prism_core::foundation::spatial::Transform2D;
    use prism_ui_runtime::layout::Viewport;

    fn populated_state() -> AppState {
        let mut state = AppState::default();
        state.canvas.document = BuilderDocument {
            root: Some(Node {
                id: "n".into(),
                component: "container".into(),
                transform: Transform2D {
                    position: [10.0, 20.0],
                    ..Default::default()
                },
                ..Default::default()
            }),
            ..Default::default()
        };
        state.canvas.selection = Some("n".into());
        state
    }

    #[test]
    fn esc_clears_canvas_and_builder_selection_in_one_call() {
        let mut state = populated_state();
        // Mark a builder inspector node as selected to mirror the
        // canvas selection — the cross-slot invariant says Esc clears
        // both at once.
        state.builder.inspector.push(crate::state::InspectorNode {
            id: "n".into(),
            label: "n".into(),
            depth: 0,
            selected: true,
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
        };
        assert!(reg.commands().run("selection.clear", &mut ctx));
        assert!(state.canvas.selection.is_none());
        assert!(state.builder.inspector.iter().all(|n| !n.selected));
    }

    #[test]
    fn arrow_keys_dispatch_through_one_dxdy_table() {
        let mut state = populated_state();
        let mut undo = UndoStack::default();
        let mut vfs = InMemVfs::default();
        let mut luau = NoopLuauHost::default();
        let mut clipboard = Clipboard::default();
        let reg = ServiceRegistry::with_builtins();
        let cases = [
            ("selection.move-up", 0.0, -1.0),
            ("selection.move-down", 0.0, 1.0),
            ("selection.move-left", -1.0, 0.0),
            ("selection.move-right", 1.0, 0.0),
        ];
        let start = state
            .canvas
            .document
            .root
            .as_ref()
            .unwrap()
            .transform
            .position;
        let mut expect = start;
        for (id, dx, dy) in cases {
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
            };
            assert!(reg.commands().run(id, &mut ctx), "command `{id}` not found");
            expect[0] += dx;
            expect[1] += dy;
            let pos = state
                .canvas
                .document
                .root
                .as_ref()
                .unwrap()
                .transform
                .position;
            assert_eq!(pos, expect, "after `{id}`");
        }
    }
}
