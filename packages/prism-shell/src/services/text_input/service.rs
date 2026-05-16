//! `DeclarativeTextInputService` — the single service that walks a
//! slice of [`TextInputDeclaration`]s and routes events through the
//! shared dispatch primitive.
//!
//! ## Dispatch policy
//!
//! For every event the service iterates declarations in registration
//! order, picks the **first** whose `is_active(&AppState)` returns
//! true, and dispatches the event through it. If no declaration is
//! active, the event passes to the next service in the registry.
//!
//! Within an active declaration:
//!
//! * `BufferMutated` → run `on_buffer_change` hook, return `Handled`.
//! * `DisplayMutated` → run `on_display_change` hook, return `Handled`.
//! * `Inert` → return `Handled` (we own the surface).
//! * `PassToGlobal` → `Handled` when `modal_capture`, `Pass` otherwise.
//! * `Ignored` (commit / cancel keys + non-text events):
//!   * `Enter` / `Return` → run `on_commit` if set, else `Handled`/`Pass`
//!     by modal-capture rule.
//!   * `Escape` → run `on_cancel` if set, same rule.
//!   * Other → `Handled` if modal, else `Pass`.
//!
//! ## Where it sits in the service registry
//!
//! Registered ahead of `InputService` so its modal-capture wins for
//! every declared overlay. The bespoke surfaces (`FieldFocusService`,
//! `CodeEditorService`) still register before this service — they
//! have shape-specific state the declarative path doesn't model yet.

use prism_ui_runtime::event::Event;

use crate::services::{CommandTable, EventOutcome, MutCtx, ShellService};

use super::declaration::TextInputDeclaration;
use super::dispatch::{dispatch_text_input, TextInputOutcome};

/// Service that dispatches every event through a registered
/// declaration's `TextEditor`. Built with [`Self::with_declarations`]
/// from a slice of `'static` declarations — typically
/// [`builtin_declarations`].
pub struct DeclarativeTextInputService {
    declarations: &'static [TextInputDeclaration],
}

impl DeclarativeTextInputService {
    pub const fn with_declarations(declarations: &'static [TextInputDeclaration]) -> Self {
        Self { declarations }
    }
}

impl ShellService for DeclarativeTextInputService {
    fn id(&self) -> &'static str {
        "text-input"
    }

    fn on_event(&self, event: &Event, ctx: &mut MutCtx<'_>, _cmds: &CommandTable) -> EventOutcome {
        // Find the first active declaration. Iteration is cheap —
        // we ship ~4 declarations and walking a `'static` slice is
        // a tight loop.
        let Some(decl) = self.declarations.iter().find(|d| (d.is_active)(ctx.state)) else {
            return EventOutcome::Pass;
        };

        let editor = (decl.write)(ctx.state);
        let outcome = dispatch_text_input(event, editor, ctx.clipboard, &decl.bindings);

        match outcome {
            TextInputOutcome::BufferMutated => {
                if let Some(hook) = decl.on_buffer_change {
                    hook(ctx);
                }
                EventOutcome::Handled
            }
            TextInputOutcome::DisplayMutated => {
                if let Some(hook) = decl.on_display_change {
                    hook(ctx);
                }
                EventOutcome::Handled
            }
            TextInputOutcome::Inert => EventOutcome::Handled,
            TextInputOutcome::PassToGlobal => {
                if decl.modal_capture {
                    EventOutcome::Handled
                } else {
                    EventOutcome::Pass
                }
            }
            TextInputOutcome::Ignored => match event {
                Event::Key {
                    code,
                    pressed: true,
                    ..
                } => match code.as_str() {
                    "enter" | "return" => {
                        if let Some(hook) = decl.on_commit {
                            hook(ctx);
                            EventOutcome::Handled
                        } else if decl.modal_capture {
                            EventOutcome::Handled
                        } else {
                            EventOutcome::Pass
                        }
                    }
                    "escape" => {
                        if let Some(hook) = decl.on_cancel {
                            hook(ctx);
                            EventOutcome::Handled
                        } else if decl.modal_capture {
                            EventOutcome::Handled
                        } else {
                            EventOutcome::Pass
                        }
                    }
                    _ => {
                        if decl.modal_capture {
                            EventOutcome::Handled
                        } else {
                            EventOutcome::Pass
                        }
                    }
                },
                Event::Wheel { .. } if decl.modal_capture => EventOutcome::Handled,
                _ => EventOutcome::Pass,
            },
        }
    }
}

// ── built-in declarations ────────────────────────────────────────────

/// The four shipping modal-overlay surfaces wired through the
/// declarative system: command palette query, search overlay query.
///
/// Adding a new surface is one row here — declare the slot getters,
/// the activation predicate, and the hooks; the rest of the system
/// (event routing, modal capture, clipboard, IME, caret/selection
/// rendering) follows from the shared primitive.
pub fn builtin_declarations() -> &'static [TextInputDeclaration] {
    &BUILTINS
}

static BUILTINS: [TextInputDeclaration; 3] = [
    palette_declaration(),
    search_declaration(),
    devtools_filter_declaration(),
];

const fn palette_declaration() -> TextInputDeclaration {
    TextInputDeclaration::builder(
        "palette",
        |s| &s.overlay.command_palette.query,
        |s| &mut s.overlay.command_palette.query,
    )
    .active_when(|s| s.overlay.command_palette.open)
    .on_buffer_change(palette_reset_selection)
    .on_commit(palette_exec_selected)
    .on_cancel(palette_close)
    .passthrough_plain(&[
        "enter",
        "return",
        "escape",
        "arrowup",
        "arrowdown",
        "up",
        "down",
    ])
    .modal()
    .build()
}

const fn search_declaration() -> TextInputDeclaration {
    TextInputDeclaration::builder("search", |s| &s.search.query, |s| &mut s.search.query)
        .active_when(|s| s.search.open)
        .on_buffer_change(search_rebuild_results)
        .on_cancel(search_close)
        .passthrough_plain(&[
            "enter",
            "return",
            "escape",
            "arrowup",
            "arrowdown",
            "up",
            "down",
        ])
        .modal()
        .build()
}

/// IDE-mode Phase 4 — the DevTools panel's filter field.
///
/// Sibling to the palette + search declarations, but with non-modal
/// semantics: when the filter has focus, typing flows into the
/// editor, but the panel doesn't swallow global shortcuts. Escape
/// drops focus (closing the filter loop); commit is a no-op (the
/// filter applies live, no Enter needed).
const fn devtools_filter_declaration() -> TextInputDeclaration {
    TextInputDeclaration::builder(
        "devtools-filter",
        |s| &s.devtools.filter,
        |s| &mut s.devtools.filter,
    )
    .active_when(|s| s.devtools.filter_focused)
    .on_cancel(devtools_filter_blur)
    .passthrough_plain(&["escape"])
    .build()
}

fn devtools_filter_blur(ctx: &mut MutCtx<'_>) {
    ctx.state.devtools.filter_focused = false;
}

// Palette hooks ------------------------------------------------------

fn palette_reset_selection(ctx: &mut MutCtx<'_>) {
    ctx.state.overlay.command_palette.selected_index = 0;
}

fn palette_exec_selected(_ctx: &mut MutCtx<'_>) {
    // Filled in by the host: read
    // `state.overlay.command_palette.results[selected_index].id`
    // and re-dispatch through the same command table before clearing
    // the palette. The service-side body is intentionally empty so
    // the dispatch path is one table lookup, never a reentrant
    // `run`-from-handler. Matches the prior `palette.exec-selected`
    // command-body shape.
}

fn palette_close(ctx: &mut MutCtx<'_>) {
    let p = &mut ctx.state.overlay.command_palette;
    p.open = false;
    p.query.set_text("");
    p.selected_index = 0;
}

// Search hooks -------------------------------------------------------

fn search_rebuild_results(ctx: &mut MutCtx<'_>) {
    use crate::state::SearchHit;
    let q = ctx.state.search.query.text().to_lowercase();
    ctx.state.search.results.clear();
    if q.is_empty() {
        ctx.state.search.selected_index = 0;
        return;
    }
    let mut hits: Vec<SearchHit> = Vec::new();
    if let Some(root) = ctx.state.canvas.document.root.as_ref() {
        walk_score(root, &q, &mut hits);
    }
    hits.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    hits.truncate(50);
    ctx.state.search.results = hits;
    ctx.state.search.selected_index = 0;
}

fn search_close(ctx: &mut MutCtx<'_>) {
    ctx.state.search.open = false;
    ctx.state.search.query.set_text("");
    ctx.state.search.results.clear();
    ctx.state.search.selected_index = 0;
}

fn walk_score(node: &prism_builder::Node, q: &str, out: &mut Vec<crate::state::SearchHit>) {
    if let Some(hit) = score_node(node, q) {
        out.push(hit);
    }
    for child in &node.children {
        walk_score(child, q, out);
    }
}

fn score_node(node: &prism_builder::Node, q: &str) -> Option<crate::state::SearchHit> {
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
        Some(crate::state::SearchHit {
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
    use crate::services::{
        register_shell_services, vfs::test_support::InMemVfs, Clipboard, NoopLuauHost,
        ServiceRegistry, UndoStack,
    };
    use crate::AppState;
    use prism_ui_runtime::event::{Event, Modifiers};
    use prism_ui_runtime::layout::Viewport;

    fn fan_out(state: &mut AppState, event: &Event) -> EventOutcome {
        let mut reg = ServiceRegistry::new();
        register_shell_services(&mut reg);
        let mut undo = UndoStack::default();
        let mut vfs = InMemVfs::default();
        let mut luau = NoopLuauHost::default();
        let mut clipboard = Clipboard::default();
        let mut ctx = MutCtx {
            state,
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
        reg.fan_out(event, &mut ctx)
    }

    #[test]
    fn declarations_list_is_non_empty() {
        // Smoke test: the registry has at least the two shipping
        // declarations.
        let decls = builtin_declarations();
        assert!(decls.len() >= 2);
        let ids: Vec<&str> = decls.iter().map(|d| d.id).collect();
        assert!(ids.contains(&"palette"));
        assert!(ids.contains(&"search"));
    }

    #[test]
    fn palette_typing_routes_through_declaration() {
        let mut state = AppState::default();
        state.overlay.command_palette.open = true;
        fan_out(&mut state, &Event::Text { text: "hi".into() });
        assert_eq!(state.overlay.command_palette.query_text(), "hi");
    }

    #[test]
    fn search_typing_routes_through_declaration() {
        let mut state = AppState::default();
        state.search.open = true;
        fan_out(
            &mut state,
            &Event::Text {
                text: "demo".into(),
            },
        );
        assert_eq!(state.search.query_text(), "demo");
    }

    #[test]
    fn modal_capture_swallows_ctrl_s_for_active_overlay() {
        let mut state = AppState::default();
        state.overlay.command_palette.open = true;
        state.project.current_file = Some(std::path::PathBuf::from("/tmp/x.prism"));
        state.project.dirty = true;
        fan_out(
            &mut state,
            &Event::Key {
                code: "s".into(),
                pressed: true,
                modifiers: Modifiers {
                    ctrl: true,
                    ..Default::default()
                },
            },
        );
        assert!(state.project.dirty, "Ctrl+S must NOT save");
    }

    #[test]
    fn escape_runs_on_cancel_hook_closing_the_overlay() {
        let mut state = AppState::default();
        state.overlay.command_palette.open = true;
        state.overlay.command_palette.query.set_text("stuck");
        fan_out(
            &mut state,
            &Event::Key {
                code: "escape".into(),
                pressed: true,
                modifiers: Modifiers::default(),
            },
        );
        assert!(!state.overlay.command_palette.open, "Esc must close");
        assert_eq!(
            state.overlay.command_palette.query_text(),
            "",
            "Esc must clear the query"
        );
    }
}
