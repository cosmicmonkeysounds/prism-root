//! `ClipboardService` — copy / cut / paste / duplicate. The wire
//! format is `serde_json::Value` (the same shape `BuilderDocument`
//! already uses for nodes); there is no second serialiser, no
//! `ClipboardEntry { kind, data }` envelope. (§25.)
//!
//! `Clipboard` is a one-field newtype around `Option<Value>`,
//! held on `MutCtx`. The four commands are five-line handlers each;
//! `duplicate` is `serialize_selection` + `insert_at_offset` (the
//! same two methods `paste` calls), so the rule-of-three is met
//! and the wire format lives in exactly one place — on `CanvasSlot`.

use serde_json::Value;

use crate::cmd;
use crate::services::{CommandSpec, ShellService};

/// One-cell internal clipboard. Cleared on cut, set on copy, read
/// on paste. System-clipboard integration plugs in at the service
/// level later through `arboard` — the in-memory contract is what
/// every test exercises.
#[derive(Default)]
pub struct Clipboard {
    pub contents: Option<Value>,
}

impl Clipboard {
    pub fn set(&mut self, value: Value) {
        self.contents = Some(value);
    }
    pub fn take(&mut self) -> Option<Value> {
        self.contents.take()
    }
    pub fn get(&self) -> Option<&Value> {
        self.contents.as_ref()
    }
}

#[derive(Default)]
pub struct ClipboardService;

impl ShellService for ClipboardService {
    fn id(&self) -> &'static str {
        "clipboard"
    }

    fn commands(&self) -> Vec<CommandSpec> {
        vec![
            cmd!("clipboard.copy", "Copy", "Edit", "Ctrl+C", |ctx| {
                if let Some(v) = ctx.state.canvas.serialize_selection() {
                    ctx.clipboard.set(v);
                }
            }),
            cmd!("clipboard.cut", "Cut", "Edit", "Ctrl+X", |ctx| {
                if let Some(v) = ctx.state.canvas.serialize_selection() {
                    ctx.undo.snapshot(ctx.state);
                    ctx.clipboard.set(v);
                    ctx.state.canvas.delete_selection();
                }
            }),
            cmd!("clipboard.paste", "Paste", "Edit", "Ctrl+V", |ctx| {
                if let Some(v) = ctx.clipboard.get().cloned() {
                    ctx.undo.snapshot(ctx.state);
                    ctx.state.canvas.insert_at_offset(v, 0);
                }
            }),
            cmd!(
                "clipboard.duplicate",
                "Duplicate",
                "Edit",
                "Ctrl+D",
                |ctx| {
                    if let Some(v) = ctx.state.canvas.serialize_selection() {
                        ctx.undo.snapshot(ctx.state);
                        ctx.state.canvas.insert_at_offset(v, 1);
                    }
                }
            ),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::vfs::test_support::InMemVfs;
    use crate::services::{MutCtx, NoopLuauHost, ServiceRegistry, UndoStack};
    use crate::AppState;
    use prism_builder::{BuilderDocument, Node};
    use prism_ui_runtime::layout::Viewport;

    fn populated() -> AppState {
        let mut state = AppState::default();
        state.canvas.document = BuilderDocument {
            root: Some(Node {
                id: "root".into(),
                component: "container".into(),
                children: vec![Node {
                    id: "n".into(),
                    component: "text".into(),
                    ..Default::default()
                }],
                ..Default::default()
            }),
            ..Default::default()
        };
        state.canvas.selection = Some("n".into());
        state
    }

    #[test]
    fn copy_paste_round_trips_selection_through_value_only() {
        let mut state = populated();
        let mut undo = UndoStack::default();
        let mut vfs = InMemVfs::default();
        let mut luau = NoopLuauHost::default();
        let mut clipboard = Clipboard::default();
        let reg = ServiceRegistry::with_builtins();
        {
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
            assert!(reg.commands().run("clipboard.copy", &mut ctx));
        }
        assert!(clipboard.get().is_some(), "copy populates clipboard");
        let before = state.canvas.document.root.as_ref().unwrap().children.len();
        {
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
            assert!(reg.commands().run("clipboard.paste", &mut ctx));
        }
        let after = state.canvas.document.root.as_ref().unwrap().children.len();
        assert_eq!(after, before + 1, "paste appends a sibling");
    }

    #[test]
    fn cut_then_paste_restores_subtree() {
        let mut state = populated();
        let mut undo = UndoStack::default();
        let mut vfs = InMemVfs::default();
        let mut luau = NoopLuauHost::default();
        let mut clipboard = Clipboard::default();
        let reg = ServiceRegistry::with_builtins();
        {
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
            assert!(reg.commands().run("clipboard.cut", &mut ctx));
        }
        assert_eq!(
            state.canvas.document.root.as_ref().unwrap().children.len(),
            0
        );
        // Re-select the root so paste targets it.
        state.canvas.selection = Some("root".into());
        {
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
            assert!(reg.commands().run("clipboard.paste", &mut ctx));
        }
        // After paste, the root has at least one child again.
        assert!(!state
            .canvas
            .document
            .root
            .as_ref()
            .unwrap()
            .children
            .is_empty());
    }
}
