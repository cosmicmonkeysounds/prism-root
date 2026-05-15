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
use crate::services::vfs::{FilePickerSpec, VfsError};
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
                // Existing path → write through. No path → fall back
                // to Save As (prompt then save). Closes D3 of
                // `docs/dev/ui-migration-followups.md`: services drive
                // the picker themselves rather than relying on a host
                // pre-set + re-dispatch loop.
                if let Some(path) = ctx.state.project.current_file.clone() {
                    save_to(ctx, path);
                } else {
                    save_as_with_picker(ctx);
                }
            }),
            cmd!("file.save-as", "Save As…", "File", "Ctrl+Shift+S", |ctx| {
                save_as_with_picker(ctx);
            }),
            cmd!("file.open", "Open…", "File", "Ctrl+O", |ctx| {
                // Same pattern as Save: with a path, load it; without
                // one, prompt via the picker first.
                if let Some(path) = ctx.state.project.current_file.clone() {
                    load_from(ctx, path);
                } else {
                    open_with_picker(ctx);
                }
            }),
        ]
    }
}

/// Pick a save path through `ctx.vfs.pick_file(save)`, populate
/// `state.project.current_file`, and write the document. A `Cancelled`
/// pick is silent; `Unsupported` (headless / wasm / test) toasts so
/// the missing wiring is visible without being noisy.
fn save_as_with_picker(ctx: &mut crate::services::MutCtx<'_>) {
    let path = match ctx.vfs.pick_file(&prism_spec_save()) {
        Ok(paths) => match paths.into_iter().next() {
            Some(p) => p,
            None => return, // shouldn't happen — picker returns Cancelled when empty
        },
        Err(VfsError::Cancelled) => return,
        Err(VfsError::Unsupported) => {
            ctx.state.overlay.toasts.push(Toast {
                title: "Save As".into(),
                body: "No file dialog available on this host".into(),
                kind: ToastKind::Info,
            });
            return;
        }
        Err(e) => {
            ctx.state.overlay.toasts.push(Toast {
                title: "Save As failed".into(),
                body: format!("{e}"),
                kind: ToastKind::Error,
            });
            return;
        }
    };
    ctx.state.project.current_file = Some(path.clone());
    save_to(ctx, path);
}

/// Same shape as `save_as_with_picker` but for the open flow — picks
/// a path through the file dialog, sets `current_file`, and reads
/// the document.
fn open_with_picker(ctx: &mut crate::services::MutCtx<'_>) {
    let path = match ctx.vfs.pick_file(&prism_spec_open()) {
        Ok(paths) => match paths.into_iter().next() {
            Some(p) => p,
            None => return,
        },
        Err(VfsError::Cancelled) => return,
        Err(VfsError::Unsupported) => {
            ctx.state.overlay.toasts.push(Toast {
                title: "Open".into(),
                body: "No file dialog available on this host".into(),
                kind: ToastKind::Info,
            });
            return;
        }
        Err(e) => {
            ctx.state.overlay.toasts.push(Toast {
                title: "Open failed".into(),
                body: format!("{e}"),
                kind: ToastKind::Error,
            });
            return;
        }
    };
    ctx.state.project.current_file = Some(path.clone());
    load_from(ctx, path);
}

fn prism_spec_save() -> FilePickerSpec {
    FilePickerSpec::save("Save Prism document").with_filter("Prism", &["prism", "json"])
}

fn prism_spec_open() -> FilePickerSpec {
    FilePickerSpec::open("Open Prism document").with_filter("Prism", &["prism", "json"])
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

    /// D3 — when `current_file` is `None`, `file.save` now drives a
    /// picker through `ctx.vfs.pick_file` (returning a queued path
    /// in the InMemVfs harness) and writes against the picked path.
    /// The picker-cancelled branch is a quiet noop (no toast).
    #[test]
    fn file_save_without_path_drives_picker() {
        let mut state = AppState::default();
        let mut undo = UndoStack::default();
        let vfs_holder = InMemVfs::default();
        let picked = PathBuf::from("/tmp/picked.prism");
        vfs_holder.queue_pick(Ok(vec![picked.clone()]));
        let mut vfs = vfs_holder;
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
        assert!(reg.commands().run("file.save", &mut ctx));
        assert_eq!(state.project.current_file, Some(picked));
        // No "Save As" info toast — the picker ran cleanly.
        assert!(state.overlay.toasts.is_empty());
    }

    #[test]
    fn file_save_cancelled_pick_is_a_silent_noop() {
        let mut state = AppState::default();
        let mut undo = UndoStack::default();
        let vfs_holder = InMemVfs::default();
        vfs_holder.queue_pick(Err(VfsError::Cancelled));
        let mut vfs = vfs_holder;
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
        assert!(reg.commands().run("file.save", &mut ctx));
        assert!(state.project.current_file.is_none());
        assert!(state.overlay.toasts.is_empty());
    }
}
