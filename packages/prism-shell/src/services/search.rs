//! `SearchService` — owns the find overlay's command rows
//! (`search.{open, close, next, prev}`).
//!
//! Event handling — typing into the query, modal-capture for Ctrl+S,
//! result refilter, Enter/Esc — moved to a
//! [`TextInputDeclaration`](crate::services::text_input::TextInputDeclaration)
//! on `DeclarativeTextInputService` during the editor-unify pass. The
//! search overlay's modal-capture invariant is now expressed
//! declaratively (`TextInputDeclaration::modal_capture` + the
//! plain-key passthrough list), and the per-keystroke result rebuild
//! lives on the declaration's `on_buffer_change` hook.
//!
//! This service only contributes commands; it has no `on_event` impl.

use crate::cmd;
use crate::services::{CommandSpec, MutCtx, ShellService};
use crate::state::{SearchHit, Toast, ToastKind};

#[derive(Default)]
pub struct SearchService;

impl ShellService for SearchService {
    fn id(&self) -> &'static str {
        "search"
    }

    fn commands(&self) -> Vec<CommandSpec> {
        vec![
            cmd!("search.open", "Find…", "View", "Ctrl+F", |ctx| {
                ctx.state.search.open = true;
                ctx.state.search.selected_index = 0;
            }),
            cmd!("search.close", "Close Find", "View", |ctx| {
                ctx.state.search.open = false;
                ctx.state.search.query.set_text("");
                ctx.state.search.results.clear();
                ctx.state.search.selected_index = 0;
            }),
            cmd!("search.next", "Next Result", "View", |ctx| {
                let n = ctx.state.search.results.len();
                if n > 0 {
                    ctx.state.search.selected_index = (ctx.state.search.selected_index + 1) % n;
                }
            }),
            cmd!("search.prev", "Previous Result", "View", |ctx| {
                let n = ctx.state.search.results.len();
                if n > 0 {
                    ctx.state.search.selected_index = (ctx.state.search.selected_index + n - 1) % n;
                }
            }),
            // IDE Phase 6 — flip Document ⇄ Project scope. The query
            // buffer is preserved; results re-derive on the next
            // keystroke, so nudge the buffer's change hook by clearing
            // the now-stale list immediately.
            cmd!(
                "search.toggle-scope",
                "Toggle Search Scope",
                "View",
                |ctx| {
                    ctx.state.search.scope = ctx.state.search.scope.toggled();
                    ctx.state.search.results.clear();
                    ctx.state.search.selected_index = 0;
                }
            ),
            // Open the selected result: project hits jump to the file
            // (`open_at_offset`), document hits select the builder node.
            cmd!("search.activate", "Open Result", "View", |ctx| {
                let sel = ctx.state.search.selected_index;
                let Some(hit) = ctx.state.search.results.get(sel).cloned() else {
                    return;
                };
                activate_search_hit(ctx, &hit);
            }),
            // IDE Phase 6 — apply the replace string across every
            // project hit in the current result set.
            cmd!(
                "search.replace-all",
                "Replace All in Files",
                "View",
                |ctx| {
                    crate::services::text_input::search_replace_all(ctx);
                }
            ),
        ]
    }
}

/// Open a search result and dismiss the overlay. Project hits jump
/// to `path:offset` through the shared editor-jump seam; document
/// hits route the builder `NodeId` to canvas selection.
pub(crate) fn activate_search_hit(ctx: &mut MutCtx<'_>, hit: &SearchHit) {
    if let Some(path) = hit.path.clone() {
        let vfs = &*ctx.vfs;
        if let Err(e) =
            crate::services::editor_files::open_at_offset(ctx.state, vfs, path, hit.offset)
        {
            ctx.state.overlay.toasts.push(Toast {
                title: "Open result failed".into(),
                body: e,
                kind: ToastKind::Error,
            });
        }
    } else if !hit.node_id.is_empty() {
        ctx.state.select_node(&hit.node_id, ctx.registry);
    }
    let s = &mut ctx.state.search;
    s.open = false;
    s.query.set_text("");
    s.results.clear();
    s.selected_index = 0;
}
