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
            // IDE Phase 2 — "Go to Symbol" palette (Ctrl+T, matching
            // VS Code's workspace-symbol shortcut; Ctrl+Shift+O is
            // already taken by project.open-folder).
            cmd!(
                "editor.go-to-symbol",
                "Go to Symbol…",
                "View",
                "Ctrl+T",
                |ctx| {
                    let idx = &mut ctx.state.index;
                    idx.palette_open = true;
                    idx.query.set_text("");
                    idx.selected_index = 0;
                    idx.refresh_results();
                    // Lose to no other modal — clear competitors so the
                    // symbol-palette text-input declaration wins.
                    ctx.state.overlay.command_palette.open = false;
                    ctx.state.search.open = false;
                }
            ),
            cmd!("editor.symbol-next", "Next Symbol", "View", |ctx| {
                let n = ctx.state.index.results.len();
                if n > 0 {
                    ctx.state.index.selected_index = (ctx.state.index.selected_index + 1) % n;
                }
            }),
            cmd!("editor.symbol-prev", "Previous Symbol", "View", |ctx| {
                let n = ctx.state.index.results.len();
                if n > 0 {
                    ctx.state.index.selected_index = (ctx.state.index.selected_index + n - 1) % n;
                }
            }),
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
    // IDE Phase 2 — keep the symbol index live: a save is the one
    // moment the on-disk source is known-fresh. Single-file rebuild
    // (cheap) so jump-to-symbol / the palette see the new defs.
    if path.extension().and_then(|e| e.to_str()) == Some("luau") {
        let src = ctx.state.canvas.code_buffer.source().to_string();
        ctx.state.index.symbols.rebuild_file(path.clone(), &src);
        ctx.state.index.refresh_results();
        // IDE Phase 3 — diagnostics ride the same save cadence.
        ctx.state.diagnostics.rebuild_file(path.clone(), &src);
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

/// IDE Phase 2 — open `path` in the code editor and drop the caret at
/// `offset` (a byte index into the file's current source). Shared by
/// the symbol palette's Enter-commit and its row-click handler so both
/// land on the exact same jump. Returns a human-readable failure for
/// the caller to surface as a toast.
pub(crate) fn open_at_offset(
    state: &mut crate::state::AppState,
    vfs: &dyn crate::services::Vfs,
    path: PathBuf,
    offset: usize,
) -> Result<(), String> {
    let bytes = vfs.read(&path).map_err(|e| format!("{e}"))?;
    let source =
        String::from_utf8(bytes).map_err(|_| "file contains non-UTF-8 bytes".to_string())?;
    let language = language_from_path(&path);
    state.canvas.open_editor_tab(path, source, language);
    state
        .canvas
        .code_buffer
        .editor
        .place_caret_at(offset, false);
    state.code_editor_focused = true;
    Ok(())
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
    use crate::services::{Clipboard, NoopLuauHost, ServiceRegistry, UndoStack, Vfs};
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
    fn go_to_symbol_opens_palette_with_results() {
        let mut state = focused_state();
        state
            .index
            .symbols
            .rebuild_file("m.luau", "local function greet() end\nlocal count = 0");
        let mut vfs = InMemVfs::default();
        assert!(run_cmd(&mut state, "editor.go-to-symbol", &mut vfs));
        assert!(state.index.palette_open);
        // Empty query lists every symbol.
        assert!(state.index.results.iter().any(|s| s.name == "greet"));
        assert!(state.index.results.iter().any(|s| s.name == "count"));
    }

    #[test]
    fn open_at_offset_opens_tab_and_places_caret() {
        let mut state = focused_state();
        let mut vfs = InMemVfs::default();
        let path = PathBuf::from("/proj/m.luau");
        let src = "local x = 1\nlocal function go() end";
        vfs.write(&path, src.as_bytes()).unwrap();
        let off = src.find("go").unwrap();
        open_at_offset(&mut state, &vfs, path.clone(), off).unwrap();
        assert_eq!(state.canvas.code_buffer_meta.path.as_ref(), Some(&path));
        assert_eq!(state.canvas.code_buffer.editor.caret_byte(), off);
        assert!(state.code_editor_focused);
    }

    #[test]
    fn saving_luau_rebuilds_the_symbol_index() {
        let mut state = focused_state();
        let mut vfs = InMemVfs::default();
        let path = PathBuf::from("/proj/s.luau");
        state
            .canvas
            .open_editor_tab(path.clone(), "local function alpha() end", "luau");
        run_cmd(&mut state, "editor.file.save", &mut vfs);
        assert!(state.index.symbols.lookup("alpha").len() == 1);
    }

    #[test]
    fn saving_luau_refreshes_diagnostics() {
        let mut state = focused_state();
        let mut vfs = InMemVfs::default();
        let path = PathBuf::from("/proj/bad.luau");
        // Broken source → at least one diagnostic on save.
        state
            .canvas
            .open_editor_tab(path.clone(), "local x = = =", "luau");
        run_cmd(&mut state, "editor.file.save", &mut vfs);
        assert!(state.diagnostics.total() >= 1);
        assert!(state.diagnostics.per_file.contains_key(&path));
        // Fix it → the file drops out of the problems table.
        state.canvas.code_buffer.editor.set_text("local x = 1");
        run_cmd(&mut state, "editor.file.save", &mut vfs);
        assert_eq!(state.diagnostics.total(), 0);
        assert!(!state.diagnostics.per_file.contains_key(&path));
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
