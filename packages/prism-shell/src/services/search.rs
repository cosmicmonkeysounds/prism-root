//! `SearchService` — Ctrl+F overlay over the active document. Owns
//! mutators on `state.search`; reads `state.canvas.document` for
//! ranking. The matcher is a simple lowercase-substring scorer
//! (token overlap × inverse hit position) — TF-IDF rebuilds when it
//! starts to matter, but the *interface* (`build_index`, `query`)
//! stays exactly this shape.
//!
//! Modal capture follows the same §25 pattern as the command
//! palette: while `state.search.open` is true, `Text` events feed
//! the query and Esc closes — no other service sees them.

use prism_ui_runtime::event::Event;

use crate::cmd;
use crate::services::{CommandSpec, CommandTable, EventOutcome, MutCtx, ShellService};
use crate::state::{SearchHit, SearchSlot};

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
                ctx.state.search.query.clear();
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

    fn on_event(&self, event: &Event, ctx: &mut MutCtx<'_>, _cmds: &CommandTable) -> EventOutcome {
        if !ctx.state.search.open {
            return EventOutcome::Pass;
        }
        match event {
            Event::Text { text } => {
                ctx.state.search.query.push_str(text);
                rebuild_results(&mut ctx.state.search, &ctx.state.canvas.document);
                EventOutcome::Handled
            }
            Event::Key {
                code,
                pressed: true,
                ..
            } if code == "backspace" => {
                ctx.state.search.query.pop();
                rebuild_results(&mut ctx.state.search, &ctx.state.canvas.document);
                EventOutcome::Handled
            }
            // Modal capture: while the search overlay is open, every
            // other key event terminates here — except shortcuts
            // resolved earlier by `InputService` (Esc/Enter/arrows
            // fire `search.{close,next,prev}` before reaching here).
            Event::Key { .. } => EventOutcome::Handled,
            _ => EventOutcome::Pass,
        }
    }
}

fn rebuild_results(search: &mut SearchSlot, doc: &prism_builder::BuilderDocument) {
    let q = search.query.to_lowercase();
    if q.is_empty() {
        search.results.clear();
        search.selected_index = 0;
        return;
    }
    let mut hits: Vec<SearchHit> = Vec::new();
    if let Some(root) = doc.root.as_ref() {
        walk_score(root, &q, &mut hits);
    }
    hits.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    hits.truncate(50);
    search.results = hits;
    search.selected_index = 0;
}

fn walk_score(node: &prism_builder::Node, q: &str, out: &mut Vec<SearchHit>) {
    if let Some(hit) = score_node(node, q) {
        out.push(hit);
    }
    for child in &node.children {
        walk_score(child, q, out);
    }
}

fn score_node(node: &prism_builder::Node, q: &str) -> Option<SearchHit> {
    let label = node.component.clone();
    let label_lc = label.to_lowercase();
    let mut best_score = 0.0_f32;
    let mut best_snippet = String::new();
    if let Some(idx) = label_lc.find(q) {
        best_score = 2.0 / (1.0 + idx as f32);
        best_snippet = label.clone();
    }
    if let serde_json::Value::Object(map) = &node.props {
        for (k, v) in map {
            let s = match v {
                serde_json::Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            let lc = s.to_lowercase();
            if let Some(idx) = lc.find(q) {
                let score = 1.0 / (1.0 + idx as f32);
                if score > best_score {
                    best_score = score;
                    best_snippet = format!("{k}: {s}");
                }
            }
        }
    }
    if best_score > 0.0 {
        Some(SearchHit {
            node_id: node.id.clone(),
            label,
            snippet: best_snippet,
            score: best_score,
        })
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::vfs::test_support::InMemVfs;
    use crate::services::{Clipboard, NoopLuauHost, ServiceRegistry, UndoStack};
    use crate::AppState;
    use prism_ui_runtime::layout::Viewport;

    #[test]
    fn open_then_close_clears_query() {
        let mut state = AppState::default();
        state.search.query = "stale".into();
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
        assert!(reg.commands().run("search.open", &mut ctx));
        assert!(state.search.open);
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
        assert!(reg.commands().run("search.close", &mut ctx));
        assert!(!state.search.open);
        assert!(state.search.query.is_empty());
    }
}
