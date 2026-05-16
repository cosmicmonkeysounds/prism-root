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
use crate::services::{CommandSpec, ShellService};

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
        ]
    }
}
