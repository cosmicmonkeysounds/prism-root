//! `ProjectService` — open / close a folder-scoped project.
//!
//! Owns `state.project.root` (and clears `current_file` on close).
//! File-graph ingestion (the heavy lifting that `legacy::project::ProjectManager`
//! did) is *not* in the service body — it's a one-line call into a
//! collaborator the shell wires through `MutCtx::vfs`. The service
//! surface stays declarative; the heavy walker, when it lands,
//! mounts as a separate ingest pass against the same `Vfs`.

use std::path::PathBuf;

use crate::cmd;
use crate::services::Vfs;
use crate::services::{CommandSpec, ShellService};
use crate::state::{FileKind, FileNode, Toast, ToastKind};

#[derive(Default)]
pub struct ProjectService;

impl ShellService for ProjectService {
    fn id(&self) -> &'static str {
        "project"
    }

    fn commands(&self) -> Vec<CommandSpec> {
        vec![
            cmd!(
                "project.open-folder",
                "Open Folder…",
                "File",
                "Ctrl+Shift+O",
                |ctx| {
                    // Host workflow: pick a folder, set
                    // `state.project.root`, dispatch this command.
                    let Some(root) = ctx.state.project.root.clone() else {
                        ctx.state.overlay.toasts.push(Toast {
                            title: "Open Folder".into(),
                            body: "Set state.project.root then call this command".into(),
                            kind: ToastKind::Info,
                        });
                        return;
                    };
                    match ingest_folder(ctx.vfs, &root) {
                        Ok(files) => {
                            ctx.state.catalog.files = files;
                            ctx.state.project.dirty = false;
                            reindex_project_symbols(ctx);
                        }
                        Err(e) => ctx.state.overlay.toasts.push(Toast {
                            title: "Open Folder failed".into(),
                            body: e,
                            kind: ToastKind::Error,
                        }),
                    }
                }
            ),
            cmd!("project.close-folder", "Close Folder", "File", |ctx| {
                ctx.state.project.root = None;
                ctx.state.project.current_file = None;
                ctx.state.catalog.files.clear();
            }),
        ]
    }
}

/// IDE Phase 2 — rebuild the project-wide symbol index from every
/// `.luau` file the ingest just listed. Unreadable / non-Luau files
/// are skipped; a parse failure simply contributes no symbols (see
/// `prism_core::language::symbol_index`). Cleared first so a re-open
/// of a different folder doesn't accumulate stale symbols.
pub(crate) fn reindex_project_symbols(ctx: &mut crate::services::MutCtx<'_>) {
    // `state` and `vfs` are disjoint fields of `MutCtx`, so the
    // index rebuild can borrow the vfs as its read hook while it
    // mutates `state.index`.
    let vfs = &*ctx.vfs;
    ctx.state.reindex_luau_symbols(|p| vfs.read(p).ok());
}

/// Single-pass folder walk. Skips dotfiles, `target/`, `node_modules/`,
/// `.git/`, `data/` — same skip set as the legacy walker, but expressed
/// once, here, against `Vfs`.
fn ingest_folder(vfs: &dyn Vfs, root: &PathBuf) -> Result<Vec<FileNode>, String> {
    fn skip(p: &std::path::Path) -> bool {
        p.file_name()
            .and_then(|s| s.to_str())
            .map(|n| n.starts_with('.') || matches!(n, "target" | "node_modules" | "data"))
            .unwrap_or(false)
    }
    let mut out = Vec::new();
    walk(vfs, root, root, 0, &mut out, &skip);
    Ok(out)
}

fn walk(
    vfs: &dyn Vfs,
    root: &PathBuf,
    here: &std::path::Path,
    depth: u32,
    out: &mut Vec<FileNode>,
    skip: &impl Fn(&std::path::Path) -> bool,
) {
    let entries = match vfs.list_dir(here) {
        Ok(es) => es,
        Err(_) => return,
    };
    for path in entries {
        if skip(&path) {
            continue;
        }
        let rel = path.strip_prefix(root).unwrap_or(&path).to_path_buf();
        let label = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();
        let is_dir = vfs.list_dir(&path).is_ok() && !vfs.exists(&path);
        // `Vfs::list_dir` succeeding on a file is impl-defined; the
        // `OsVfs` returns `NotADirectory`, the `InMemVfs` returns
        // `Ok(empty)`. We treat any path with no listable children as
        // a file — `is_dir` collapses to "list returned non-empty."
        let listable = matches!(vfs.list_dir(&path), Ok(es) if !es.is_empty());
        let kind = if is_dir || listable {
            FileKind::Directory
        } else {
            FileKind::File
        };
        out.push(FileNode {
            id: rel.to_string_lossy().into_owned(),
            label,
            kind,
            depth,
            // IDE-mode Phase 1: carry the absolute path so the
            // explorer's click router can hand it to the editor's
            // open-by-path command.
            path: path.clone(),
        });
        if matches!(kind, FileKind::Directory) {
            walk(vfs, root, &path, depth + 1, out, skip);
        }
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
    fn close_folder_clears_root() {
        let mut state = AppState::default();
        state.project.root = Some(PathBuf::from("/x"));
        state.project.current_file = Some(PathBuf::from("/x/a.prism"));
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
            modifier_registry: None,
        };
        assert!(reg.commands().run("project.close-folder", &mut ctx));
        assert!(state.project.root.is_none());
        assert!(state.project.current_file.is_none());
    }
}
