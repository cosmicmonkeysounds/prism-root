//! `EditorFilesService` — owns `editor.file.{new, open, save, save-as,
//! close, next-tab, prev-tab}`. The existing `PersistenceService`
//! handles the `BuilderDocument` (`.prism` files); this service is
//! for the in-shell code editor's open files (Luau / Rust / JS /
//! anything plain-text).
//!
//! The wire path mirrors `PersistenceService`: `MutCtx::vfs` is the
//! IO seam; commands take no args (the §24 constraint); a host with
//! no file dialog (headless / wasm) surfaces a one-time toast and
//! noops. All open files live on `CanvasSlot::code_tabs` + the live
//! `CanvasSlot::code_buffer`/`code_buffer_meta` pair the editor
//! reads from.

use std::path::PathBuf;

use crate::cmd;
use crate::services::vfs::{FilePickerSpec, VfsError};
use crate::services::{CommandSpec, EventOutcome, MutCtx, ShellService};
use crate::state::{Toast, ToastKind};
use prism_ui_runtime::event::Event;

#[derive(Default)]
pub struct EditorFilesService;

impl ShellService for EditorFilesService {
    fn id(&self) -> &'static str {
        "editor-files"
    }

    fn commands(&self) -> Vec<CommandSpec> {
        vec![
            cmd!("editor.file.new", "New File", "File", "Ctrl+N", |ctx| {
                ctx.state.canvas.new_editor_tab();
            }),
            cmd!("editor.file.open", "Open File…", "File", "Ctrl+O", |ctx| {
                open_with_picker(ctx);
            }),
            cmd!("editor.file.save", "Save File", "File", "Ctrl+S", |ctx| {
                let path = ctx.state.canvas.code_buffer_meta.path.clone();
                match path {
                    Some(p) => save_to(ctx, p),
                    None => save_as_with_picker(ctx),
                }
            }),
            cmd!(
                "editor.file.save-as",
                "Save File As…",
                "File",
                "Ctrl+Shift+S",
                |ctx| {
                    save_as_with_picker(ctx);
                }
            ),
            cmd!("editor.file.close", "Close File", "File", "Ctrl+W", |ctx| {
                ctx.state.canvas.close_active_editor_tab();
            }),
            cmd!(
                "editor.file.next-tab",
                "Next Tab",
                "View",
                "Ctrl+Tab",
                |ctx| {
                    ctx.state.canvas.next_editor_tab();
                }
            ),
            cmd!(
                "editor.file.prev-tab",
                "Previous Tab",
                "View",
                "Ctrl+Shift+Tab",
                |ctx| {
                    ctx.state.canvas.prev_editor_tab();
                }
            ),
        ]
    }

    fn on_event(
        &self,
        event: &Event,
        ctx: &mut MutCtx<'_>,
        cmds: &crate::services::CommandTable,
    ) -> EventOutcome {
        // The shell's editor file shortcuts only fire while the
        // code editor itself has focus. Property-row text fields
        // and the rest of the chrome route their own way — Ctrl+S
        // on those should save the document (PersistenceService),
        // not the active buffer.
        if !ctx.state.code_editor_focused {
            return EventOutcome::Pass;
        }
        let Event::Key {
            code,
            pressed: true,
            modifiers,
        } = event
        else {
            return EventOutcome::Pass;
        };
        let cmd_held = modifiers.ctrl || modifiers.meta;
        if !cmd_held {
            return EventOutcome::Pass;
        }
        let id = match (code.as_str(), modifiers.shift) {
            ("n", false) => "editor.file.new",
            ("o", false) => "editor.file.open",
            ("s", false) => "editor.file.save",
            ("s", true) => "editor.file.save-as",
            ("w", false) => "editor.file.close",
            ("tab", false) => "editor.file.next-tab",
            ("tab", true) => "editor.file.prev-tab",
            _ => return EventOutcome::Pass,
        };
        if cmds.run(id, ctx) {
            EventOutcome::Handled
        } else {
            EventOutcome::Pass
        }
    }
}

fn save_as_with_picker(ctx: &mut MutCtx<'_>) {
    let spec = FilePickerSpec::save("Save File");
    let path = match ctx.vfs.pick_file(&spec) {
        Ok(paths) => match paths.into_iter().next() {
            Some(p) => p,
            None => return,
        },
        Err(VfsError::Cancelled) => return,
        Err(VfsError::Unsupported) => {
            ctx.state.overlay.toasts.push(Toast {
                title: "Save File".into(),
                body: "No file dialog available on this host".into(),
                kind: ToastKind::Info,
            });
            return;
        }
        Err(e) => {
            ctx.state.overlay.toasts.push(Toast {
                title: "Save failed".into(),
                body: format!("{e}"),
                kind: ToastKind::Error,
            });
            return;
        }
    };
    save_to(ctx, path);
}

fn save_to(ctx: &mut MutCtx<'_>, path: PathBuf) {
    let bytes = ctx.state.canvas.code_buffer.source().as_bytes().to_vec();
    if let Err(e) = ctx.vfs.write(&path, &bytes) {
        ctx.state.overlay.toasts.push(Toast {
            title: "Save failed".into(),
            body: format!("{e}"),
            kind: ToastKind::Error,
        });
        return;
    }
    ctx.state.canvas.record_active_tab_saved(path);
}

fn open_with_picker(ctx: &mut MutCtx<'_>) {
    let spec = FilePickerSpec::open("Open File");
    let path = match ctx.vfs.pick_file(&spec) {
        Ok(paths) => match paths.into_iter().next() {
            Some(p) => p,
            None => return,
        },
        Err(VfsError::Cancelled) => return,
        Err(VfsError::Unsupported) => {
            ctx.state.overlay.toasts.push(Toast {
                title: "Open File".into(),
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
    open_path_into_tab(ctx, path);
}

fn open_path_into_tab(ctx: &mut MutCtx<'_>, path: PathBuf) {
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
    let source = match String::from_utf8(bytes) {
        Ok(s) => s,
        Err(_) => {
            ctx.state.overlay.toasts.push(Toast {
                title: "Open failed".into(),
                body: "File contains non-UTF-8 bytes".into(),
                kind: ToastKind::Error,
            });
            return;
        }
    };
    let language = language_from_path(&path);
    ctx.state.canvas.open_editor_tab(path, source, language);
}

fn language_from_path(path: &std::path::Path) -> &'static str {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "luau" | "lua" => "luau",
        "rs" => "rust",
        "js" | "jsx" => "javascript",
        "ts" | "tsx" => "typescript",
        "py" => "python",
        "sh" | "bash" => "shell",
        _ => "",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::vfs::test_support::InMemVfs;
    use crate::services::{Clipboard, NoopLuauHost, ServiceRegistry, UndoStack};
    use crate::AppState;
    use prism_ui_runtime::layout::Viewport;

    fn registered() -> ServiceRegistry {
        ServiceRegistry::with_builtins()
    }

    fn focused_state() -> AppState {
        AppState {
            code_editor_focused: true,
            ..AppState::default()
        }
    }

    fn run_cmd(state: &mut AppState, id: &str, vfs: &mut dyn crate::services::Vfs) -> bool {
        let reg = registered();
        let mut undo = UndoStack::default();
        let mut luau = NoopLuauHost::default();
        let mut clipboard = Clipboard::default();
        let cmds = reg.commands();
        let mut ctx = MutCtx {
            state,
            viewport: Viewport {
                width: 0.0,
                height: 0.0,
            },
            undo: &mut undo,
            vfs,
            luau: &mut luau,
            clipboard: &mut clipboard,
            registry: None,
            modifier_registry: None,
        };
        cmds.run(id, &mut ctx)
    }

    #[test]
    fn new_file_appends_an_untitled_tab() {
        let mut state = focused_state();
        let mut vfs = InMemVfs::default();
        assert_eq!(state.canvas.editor_tab_count(), 1);
        assert!(run_cmd(&mut state, "editor.file.new", &mut vfs));
        assert_eq!(state.canvas.editor_tab_count(), 2);
        // Active tab is the new one.
        assert_eq!(state.canvas.code_buffer_meta.title, "Untitled");
        assert!(state.canvas.code_buffer.source().is_empty());
    }

    #[test]
    fn next_tab_cycles_through_open_tabs() {
        let mut state = focused_state();
        let mut vfs = InMemVfs::default();
        run_cmd(&mut state, "editor.file.new", &mut vfs);
        let before = state.canvas.code_active_tab;
        run_cmd(&mut state, "editor.file.next-tab", &mut vfs);
        assert_ne!(state.canvas.code_active_tab, before);
        // Cycle back.
        run_cmd(&mut state, "editor.file.next-tab", &mut vfs);
        assert_eq!(state.canvas.code_active_tab, before);
    }

    #[test]
    fn close_tab_decrements_count() {
        let mut state = focused_state();
        let mut vfs = InMemVfs::default();
        run_cmd(&mut state, "editor.file.new", &mut vfs);
        assert_eq!(state.canvas.editor_tab_count(), 2);
        run_cmd(&mut state, "editor.file.close", &mut vfs);
        assert_eq!(state.canvas.editor_tab_count(), 1);
    }

    #[test]
    fn close_last_tab_leaves_fresh_untitled() {
        let mut state = focused_state();
        state.canvas.code_buffer.load("hello", "luau");
        state.canvas.mark_active_tab_dirty();
        let mut vfs = InMemVfs::default();
        run_cmd(&mut state, "editor.file.close", &mut vfs);
        assert_eq!(state.canvas.editor_tab_count(), 1);
        assert!(state.canvas.code_buffer.source().is_empty());
        assert!(!state.canvas.code_buffer_meta.dirty);
    }
}
