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

static BUILTINS: [TextInputDeclaration; 5] = [
    palette_declaration(),
    search_replace_declaration(),
    search_declaration(),
    symbol_palette_declaration(),
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
        .active_when(|s| s.search.open && !s.search.replace_focused)
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

/// IDE Phase 6 — the replace field. Active only while the replace
/// input holds focus (the query declaration yields via its
/// `!replace_focused` guard). Enter applies the replacement across
/// the current result set; Escape blurs back to the query.
const fn search_replace_declaration() -> TextInputDeclaration {
    TextInputDeclaration::builder(
        "search-replace",
        |s| &s.search.replace,
        |s| &mut s.search.replace,
    )
    .active_when(|s| s.search.open && s.search.replace_focused)
    .on_commit(search_replace_all)
    .on_cancel(search_replace_blur)
    .passthrough_plain(&["enter", "return", "escape"])
    .modal()
    .build()
}

fn search_replace_blur(ctx: &mut MutCtx<'_>) {
    ctx.state.search.replace_focused = false;
}

/// Apply the replace string to every project hit in the current
/// result set. Replacements are applied **back-to-front per file**
/// (so earlier offsets stay valid), the file is written back through
/// the `Vfs`, then results + the symbol index / diagnostics are
/// re-derived. The matched span length is the query's byte length —
/// the grep is literal, so for ASCII source (the common case) this
/// is exact; a non-ASCII case-fold shift is re-grepped immediately.
pub(crate) fn search_replace_all(ctx: &mut MutCtx<'_>) {
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    let qlen = ctx.state.search.query.text().len();
    if qlen == 0 {
        return;
    }
    let replacement = ctx.state.search.replace.text().to_string();

    let mut by_file: BTreeMap<PathBuf, Vec<usize>> = BTreeMap::new();
    for h in &ctx.state.search.results {
        if let Some(p) = &h.path {
            by_file.entry(p.clone()).or_default().push(h.offset);
        }
    }
    if by_file.is_empty() {
        return;
    }

    let mut files_changed = 0usize;
    let mut total = 0usize;
    for (path, mut offsets) in by_file {
        let Ok(bytes) = ctx.vfs.read(&path) else {
            continue;
        };
        let Ok(mut text) = String::from_utf8(bytes) else {
            continue;
        };
        offsets.sort_unstable();
        offsets.dedup();
        for &off in offsets.iter().rev() {
            let end = off + qlen;
            if end <= text.len() && text.is_char_boundary(off) && text.is_char_boundary(end) {
                text.replace_range(off..end, &replacement);
                total += 1;
            }
        }
        if ctx.vfs.write(&path, text.as_bytes()).is_ok() {
            files_changed += 1;
        }
    }

    ctx.state.overlay.toasts.push(crate::state::Toast {
        title: "Replace in Files".into(),
        body: format!("{total} replacement(s) across {files_changed} file(s)"),
        kind: crate::state::ToastKind::Info,
    });
    search_rebuild_results(ctx);
    let vfs = &*ctx.vfs;
    ctx.state.reindex_luau_symbols(|p| vfs.read(p).ok());
}

/// IDE Phase 2 — the "Go to Symbol" palette (Ctrl+T). Sister to the
/// command-palette / search declarations: modal, single-line query,
/// live fuzzy re-rank on every keystroke, Enter jumps to the selected
/// symbol, Escape closes.
const fn symbol_palette_declaration() -> TextInputDeclaration {
    TextInputDeclaration::builder("symbol-palette", |s| &s.index.query, |s| &mut s.index.query)
        .active_when(|s| s.index.palette_open)
        .on_buffer_change(symbol_palette_refresh)
        .on_commit(symbol_palette_commit)
        .on_cancel(symbol_palette_close)
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

fn symbol_palette_refresh(ctx: &mut MutCtx<'_>) {
    ctx.state.index.selected_index = 0;
    ctx.state.index.refresh_results();
}

fn symbol_palette_close(ctx: &mut MutCtx<'_>) {
    let idx = &mut ctx.state.index;
    idx.palette_open = false;
    idx.query.set_text("");
    idx.selected_index = 0;
}

fn symbol_palette_commit(ctx: &mut MutCtx<'_>) {
    let sel = ctx.state.index.selected_index;
    let Some(sym) = ctx.state.index.results.get(sel).cloned() else {
        symbol_palette_close(ctx);
        return;
    };
    let vfs = &*ctx.vfs;
    if let Err(e) =
        crate::services::editor_files::open_at_offset(ctx.state, vfs, sym.path.clone(), sym.offset)
    {
        ctx.state.overlay.toasts.push(crate::state::Toast {
            title: "Go to Symbol failed".into(),
            body: e,
            kind: crate::state::ToastKind::Error,
        });
    }
    symbol_palette_close(ctx);
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
    use crate::state::{SearchHit, SearchScope};
    let q = ctx.state.search.query.text().to_lowercase();
    ctx.state.search.results.clear();
    if q.is_empty() {
        ctx.state.search.selected_index = 0;
        return;
    }
    let mut hits: Vec<SearchHit> = Vec::new();
    match ctx.state.search.scope {
        SearchScope::Document => {
            if let Some(root) = ctx.state.canvas.document.root.as_ref() {
                walk_score(root, &q, &mut hits);
            }
            hits.sort_by(|a, b| {
                b.score
                    .partial_cmp(&a.score)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            hits.truncate(50);
        }
        SearchScope::Project => {
            // `state` + `vfs` are disjoint `MutCtx` fields, so the
            // grep can borrow the vfs while reading the file list.
            let vfs = &*ctx.vfs;
            project_grep(&ctx.state.catalog.files, vfs, &q, &mut hits);
        }
    }
    ctx.state.search.results = hits;
    ctx.state.search.selected_index = 0;
}

/// Largest single file the project grep will scan (bytes) and the
/// global hit cap — keeps a "find in files" over a big tree bounded.
const GREP_MAX_FILE_BYTES: usize = 1 << 20;
const GREP_MAX_HITS: usize = 200;

/// Case-insensitive substring grep over every readable, UTF-8 text
/// file in the explorer list. Each match becomes a `SearchHit`
/// carrying the file path + byte offset (activation jumps there via
/// `editor_files::open_at_offset`) and a `path:line` label with the
/// trimmed source line as the snippet.
fn project_grep(
    files: &[crate::state::FileNode],
    vfs: &dyn crate::services::Vfs,
    q: &str,
    out: &mut Vec<crate::state::SearchHit>,
) {
    use crate::state::{FileKind, SearchHit};
    for node in files {
        if out.len() >= GREP_MAX_HITS {
            break;
        }
        if !matches!(node.kind, FileKind::File) {
            continue;
        }
        let Ok(bytes) = vfs.read(&node.path) else {
            continue;
        };
        if bytes.len() > GREP_MAX_FILE_BYTES {
            continue;
        }
        let Ok(text) = String::from_utf8(bytes) else {
            continue;
        };
        let name = node
            .path
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| node.label.clone());
        let mut line_start = 0usize;
        for (lineno, line) in text.split_inclusive('\n').enumerate() {
            if out.len() >= GREP_MAX_HITS {
                break;
            }
            if let Some(col) = line.to_lowercase().find(q) {
                out.push(SearchHit {
                    node_id: String::new(),
                    label: format!("{name}:{}", lineno + 1),
                    snippet: line.trim().chars().take(120).collect(),
                    score: 1.0,
                    path: Some(node.path.clone()),
                    offset: line_start + col,
                });
            }
            line_start += line.len();
        }
    }
}

fn search_close(ctx: &mut MutCtx<'_>) {
    ctx.state.search.open = false;
    ctx.state.search.query.set_text("");
    ctx.state.search.replace.set_text("");
    ctx.state.search.replace_focused = false;
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
            ..Default::default()
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

    #[test]
    fn project_grep_finds_matches_with_path_and_offset() {
        use crate::services::Vfs;
        use crate::state::{FileKind, FileNode};
        use std::path::PathBuf;

        let mut vfs = InMemVfs::default();
        let p = PathBuf::from("/proj/a.luau");
        let src = "local x = 1\nlocal needle = 2\nreturn x\n";
        vfs.write(&p, src.as_bytes()).unwrap();
        let files = vec![FileNode {
            id: "a.luau".into(),
            label: "a.luau".into(),
            depth: 0,
            kind: FileKind::File,
            path: p.clone(),
        }];

        let mut out = Vec::new();
        project_grep(&files, &vfs, "needle", &mut out);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].path.as_ref(), Some(&p));
        assert_eq!(&src[out[0].offset..out[0].offset + 6], "needle");
        assert_eq!(out[0].label, "a.luau:2");
        assert!(out[0].snippet.contains("needle"));
    }

    #[test]
    fn replace_all_rewrites_files_and_regreps() {
        use crate::services::Vfs;
        use crate::state::{FileKind, FileNode, SearchScope};
        use std::path::PathBuf;

        let mut state = AppState::default();
        let mut vfs = InMemVfs::default();
        let p = PathBuf::from("/proj/a.luau");
        vfs.write(&p, b"local needle = 1\nreturn needle\n").unwrap();
        state.catalog.files = vec![FileNode {
            id: "a.luau".into(),
            label: "a.luau".into(),
            depth: 0,
            kind: FileKind::File,
            path: p.clone(),
        }];
        state.search.open = true;
        state.search.scope = SearchScope::Project;
        state.search.query.set_text("needle");
        state.search.replace.set_text("haystack");

        let mut undo = UndoStack::default();
        let mut luau = NoopLuauHost::default();
        let mut clipboard = Clipboard::default();
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
        // Populate results, then apply the replacement.
        search_rebuild_results(&mut ctx);
        assert_eq!(ctx.state.search.results.len(), 2);
        search_replace_all(&mut ctx);

        let after = String::from_utf8(vfs.read(&p).unwrap()).unwrap();
        assert_eq!(after, "local haystack = 1\nreturn haystack\n");
        // Re-grep cleared the (now non-matching) hits.
        assert!(state.search.results.is_empty());
    }

    #[test]
    fn toggle_scope_command_flips_and_clears() {
        use crate::state::SearchScope;
        let mut state = AppState::default();
        assert_eq!(state.search.scope, SearchScope::Document);
        let mut reg = ServiceRegistry::new();
        register_shell_services(&mut reg);
        let mut undo = UndoStack::default();
        let mut vfs = InMemVfs::default();
        let mut luau = NoopLuauHost::default();
        let mut clipboard = Clipboard::default();
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
        assert!(reg.commands().run("search.toggle-scope", &mut ctx));
        assert_eq!(state.search.scope, SearchScope::Project);
    }
}
