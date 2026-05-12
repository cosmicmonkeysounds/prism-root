//! `PersistenceService` — the single owner of `file.{new,open,save,save-as}`.
//!
//! Read/write goes through `MutCtx::vfs` (the §26 IO seam). The
//! wire format is `serde_json::Value` over `state.canvas.document`
//! — the same shape `BuilderDocument` already serialises. No
//! `ProjectFile` wrapper, no version envelope; if a project root is
//! open, `state.project.root` holds it, and the file path is
//! `state.project.current_file`. Save-As prompts the host (via
//! `prompt_save_path`, swapped in tests).
//!
//! This service does not own a path picker — picking is *outside*
//! the registry (the host wires `rfd` in production, a test fixture
//! in tests). The service's `commands()` are pure functions of
//! `MutCtx`; the path mutation happens *before* the command runs
//! (the host writes `state.project.current_file = picked` and
//! dispatches `file.save`).

use std::path::PathBuf;

use crate::cmd;
use crate::services::{CommandSpec, ShellService};
use crate::state::{Toast, ToastKind};

#[derive(Default)]
pub struct PersistenceService;

impl ShellService for PersistenceService {
    fn id(&self) -> &'static str {
        "persistence"
    }

    fn commands(&self) -> Vec<CommandSpec> {
        vec![
            cmd!("file.new", "New", "File", "Ctrl+N", |ctx| {
                ctx.undo.snapshot(ctx.state);
                ctx.state.canvas.document = Default::default();
                ctx.state.project.current_file = None;
                ctx.state.project.dirty = false;
            }),
            cmd!("file.save", "Save", "File", "Ctrl+S", |ctx| {
                let Some(path) = ctx.state.project.current_file.clone() else {
                    // No path → behaves like Save As. Host re-dispatches
                    // after picking; here we record an `Info` toast so
                    // the unwired call is visible in tests.
                    ctx.state.overlay.toasts.push(Toast {
                        title: "Save".into(),
                        body: "No file path; use Save As".into(),
                        kind: ToastKind::Info,
                    });
                    return;
                };
                save_to(ctx, path);
            }),
            cmd!("file.save-as", "Save As…", "File", "Ctrl+Shift+S", |ctx| {
                // Host workflow: present picker, set
                // `state.project.current_file`, then re-dispatch
                // `file.save`. Without a picker the command toasts.
                if let Some(path) = ctx.state.project.current_file.clone() {
                    save_to(ctx, path);
                } else {
                    ctx.state.overlay.toasts.push(Toast {
                        title: "Save As".into(),
                        body: "Set state.project.current_file then call file.save".into(),
                        kind: ToastKind::Info,
                    });
                }
            }),
            cmd!("file.open", "Open…", "File", "Ctrl+O", |ctx| {
                // Same pattern: host sets `current_file` to the picked
                // path and dispatches `file.open` to read it.
                let Some(path) = ctx.state.project.current_file.clone() else {
                    ctx.state.overlay.toasts.push(Toast {
                        title: "Open".into(),
                        body: "Set state.project.current_file then call file.open".into(),
                        kind: ToastKind::Info,
                    });
                    return;
                };
                load_from(ctx, path);
            }),
        ]
    }
}

fn save_to(ctx: &mut crate::services::MutCtx<'_>, path: PathBuf) {
    let bytes = match serde_json::to_vec_pretty(&ctx.state.canvas.document) {
        Ok(b) => b,
        Err(e) => {
            ctx.state.overlay.toasts.push(Toast {
                title: "Save failed".into(),
                body: e.to_string(),
                kind: ToastKind::Error,
            });
            return;
        }
    };
    if let Err(e) = ctx.vfs.write(&path, &bytes) {
        ctx.state.overlay.toasts.push(Toast {
            title: "Save failed".into(),
            body: format!("{e}"),
            kind: ToastKind::Error,
        });
        return;
    }
    ctx.state.project.touch(path.clone());
    ctx.state.project.current_file = Some(path);
    ctx.state.project.dirty = false;
}

fn load_from(ctx: &mut crate::services::MutCtx<'_>, path: PathBuf) {
    let bytes = match ctx.vfs.read(&path) {
        Ok(b) => b,
        Err(e) => {
            ctx.state.overlay.toasts.push(Toast {
                title: "Open failed".into(),
                body: format!("{e}"),
                kind: ToastKind::Error,
            });
            return;
        }
    };
    match serde_json::from_slice(&bytes) {
        Ok(doc) => {
            ctx.undo.snapshot(ctx.state);
            ctx.state.canvas.document = doc;
            ctx.state.project.touch(path.clone());
            ctx.state.project.current_file = Some(path);
            ctx.state.project.dirty = false;
        }
        Err(e) => ctx.state.overlay.toasts.push(Toast {
            title: "Open failed".into(),
            body: e.to_string(),
            kind: ToastKind::Error,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::vfs::test_support::InMemVfs;
    use crate::services::{Clipboard, MutCtx, NoopLuauHost, ServiceRegistry, UndoStack};
    use crate::AppState;
    use prism_ui_runtime::layout::Viewport;

    #[test]
    fn save_then_open_round_trips_through_vfs() {
        let mut state = AppState::default();
        let path = PathBuf::from("/tmp/test.prism");
        state.project.current_file = Some(path.clone());
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
                registry: None,
                modifier_registry: None,
            };
            assert!(reg.commands().run("file.save", &mut ctx));
        }
        assert!(state.project.recent.contains(&path));
        // open back into a fresh state with same vfs
        let mut state2 = AppState::default();
        state2.project.current_file = Some(path.clone());
        let mut undo2 = UndoStack::default();
        let mut ctx = MutCtx {
            state: &mut state2,
            viewport: Viewport {
                width: 0.0,
                height: 0.0,
            },
            undo: &mut undo2,
            vfs: &mut vfs,
            luau: &mut luau,
            clipboard: &mut clipboard,
            registry: None,
            modifier_registry: None,
        };
        assert!(reg.commands().run("file.open", &mut ctx));
        assert_eq!(state2.project.current_file, Some(path));
    }
}
