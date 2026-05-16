//! `CodeEditorService` — owns keyboard routing for the in-shell code
//! editor (`shell.code-editor`).
//!
//! Sister to `FieldFocusService`: when `state.code_editor_focused`
//! is true and *no* property-row field is also focused, every
//! `Event::Text` and `Event::Key` runs through the shared
//! [`text_input::dispatch_text_input`] helper. That helper drives the
//! `state.canvas.code_buffer.editor` engine with the full multi-line
//! key set — arrows, home/end, ctrl+arrows, ctrl+a, ctrl+z/y, ctrl+d,
//! ctrl+k, tab/shift-tab, backspace/delete (incl. word variants),
//! enter (auto-indent), Ctrl+C/X/V — without any per-key arms here.
//!
//! The service registers *after* `FieldFocusService` so an open
//! property-row edit still wins the fan-out race (a focused property
//! field would shadow the panel-wide editor otherwise).
//!
//! Two responsibilities stay in this file because they need state the
//! shared dispatch doesn't see:
//!
//! 1. **Ctrl+/** — toggle line comment routed through
//!    `code_buffer.toggle_line_comment()` so the prefix matches the
//!    active language (Luau `--`, JS `//`, Python `#`).
//! 2. **Wheel** — viewport scroll on the `code_buffer.scroll_{x,y}`.

use prism_ui_runtime::event::Event;

use crate::services::text_input::{dispatch_text_input, TextInputBindings, TextInputOutcome};
use crate::services::{CommandTable, EventOutcome, MutCtx, ShellService};

/// Modifier-bearing key combos the dispatch should NOT consume — they
/// belong to the file / tab service one layer up so Ctrl+N / Ctrl+O /
/// Ctrl+S / Ctrl+W / Ctrl+Tab / Ctrl+Shift+Tab fire their file-management
/// commands instead of mutating the buffer.
const FILE_PASSTHROUGH_KEYS: &[&str] = &["n", "o", "s", "w", "tab"];

/// Plain (non-modifier) keys the dispatch should surface as Ignored
/// so this service can handle them — Escape drops keyboard focus.
const PLAIN_PASSTHROUGH_KEYS: &[&str] = &["escape"];

#[derive(Default)]
pub struct CodeEditorService;

impl ShellService for CodeEditorService {
    fn id(&self) -> &'static str {
        "code-editor"
    }

    fn on_event(&self, event: &Event, ctx: &mut MutCtx<'_>, _cmds: &CommandTable) -> EventOutcome {
        if !ctx.state.code_editor_focused {
            return EventOutcome::Pass;
        }
        // Property-row field-focus takes precedence — typing in a
        // string row over a code-editor panel shouldn't ALSO insert
        // into the code buffer.
        if ctx.state.field_focus.is_some() {
            return EventOutcome::Pass;
        }

        // Wheel events scroll the editor while it has keyboard focus.
        // Pre-empt the shared dispatch — the dispatch returns Ignored
        // for wheel, but each tick adjusts state outside the editor.
        if let Event::Wheel { dx, dy } = event {
            ctx.state.canvas.code_buffer.scroll_x =
                (ctx.state.canvas.code_buffer.scroll_x + *dx).max(0.0);
            ctx.state.canvas.code_buffer.scroll_y =
                (ctx.state.canvas.code_buffer.scroll_y + *dy).max(0.0);
            return EventOutcome::Handled;
        }

        // Language-aware Ctrl+/ — routed through `CodeBuffer` so the
        // prefix matches the active language. The shared dispatch's
        // own apply_key path would otherwise default to `--`.
        if let Event::Key {
            code,
            pressed: true,
            modifiers,
        } = event
        {
            if (modifiers.ctrl || modifiers.meta) && code == "/" {
                ctx.state.canvas.code_buffer.toggle_line_comment();
                ctx.state.canvas.mark_active_tab_dirty();
                ensure_caret_visible(ctx);
                return EventOutcome::Handled;
            }
        }

        let bindings = TextInputBindings {
            passthrough_modifier_keys: FILE_PASSTHROUGH_KEYS,
            passthrough_plain_keys: PLAIN_PASSTHROUGH_KEYS,
        };
        let outcome = dispatch_text_input(
            event,
            &mut ctx.state.canvas.code_buffer.editor,
            ctx.clipboard,
            &bindings,
        );
        match outcome {
            TextInputOutcome::BufferMutated => {
                ctx.state.canvas.mark_active_tab_dirty();
                ensure_caret_visible(ctx);
                EventOutcome::Handled
            }
            TextInputOutcome::DisplayMutated => {
                ensure_caret_visible(ctx);
                EventOutcome::Handled
            }
            TextInputOutcome::Inert => EventOutcome::Handled,
            TextInputOutcome::PassToGlobal => EventOutcome::Pass,
            // Surfaced for caller-owned keys (escape).
            TextInputOutcome::Ignored => {
                if let Event::Key {
                    code,
                    pressed: true,
                    ..
                } = event
                {
                    if code == "escape" {
                        ctx.state.code_editor_focused = false;
                        return EventOutcome::Handled;
                    }
                }
                EventOutcome::Pass
            }
        }
    }
}

/// Push the editor's viewport so the caret stays visible after a
/// mutation. The viewport dimensions are best-effort — we read the
/// current `ctx.viewport` (the shell's full window) and assume a
/// reasonable editor width of about 60% of the window. The exact
/// rect doesn't matter much; clip-on-overscroll keeps glyphs from
/// leaking outside the input regardless.
fn ensure_caret_visible(ctx: &mut MutCtx<'_>) {
    let editor_w = (ctx.viewport.width * 0.6).max(200.0);
    let editor_h = (ctx.viewport.height * 0.5).max(200.0);
    ctx.state
        .canvas
        .code_buffer
        .ensure_caret_visible(editor_w, editor_h, 13.0);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::vfs::test_support::InMemVfs;
    use crate::services::{Clipboard, NoopLuauHost, ServiceRegistry, UndoStack};
    use crate::AppState;
    use prism_ui_runtime::event::{Event, Modifiers};
    use prism_ui_runtime::layout::Viewport;

    fn fan_out(state: &mut AppState, event: &Event) -> EventOutcome {
        let reg = ServiceRegistry::with_builtins();
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

    fn focused_state_with_buffer(source: &str) -> AppState {
        let mut state = AppState {
            code_editor_focused: true,
            ..AppState::default()
        };
        state.canvas.code_buffer.load(source, "luau");
        state
    }

    #[test]
    fn typing_while_focused_inserts_into_code_buffer() {
        let mut state = focused_state_with_buffer("hi");
        let out = fan_out(&mut state, &Event::Text { text: "a".into() });
        assert!(matches!(out, EventOutcome::Handled));
        assert_eq!(state.canvas.code_buffer.source(), "hia");
    }

    #[test]
    fn enter_inserts_newline_for_code_editor() {
        let mut state = focused_state_with_buffer("hi");
        let enter = Event::Key {
            code: "enter".into(),
            pressed: true,
            modifiers: Modifiers::default(),
        };
        fan_out(&mut state, &enter);
        assert_eq!(state.canvas.code_buffer.source(), "hi\n");
    }

    #[test]
    fn escape_clears_code_editor_focus() {
        let mut state = AppState {
            code_editor_focused: true,
            ..AppState::default()
        };
        let esc = Event::Key {
            code: "escape".into(),
            pressed: true,
            modifiers: Modifiers::default(),
        };
        fan_out(&mut state, &esc);
        assert!(!state.code_editor_focused);
    }

    #[test]
    fn unfocused_passes_through() {
        let mut state = AppState::default();
        state.canvas.code_buffer.load("hi", "luau");
        // Not focused — typing should NOT land in the code buffer.
        let out = fan_out(&mut state, &Event::Text { text: "a".into() });
        assert!(matches!(out, EventOutcome::Pass));
        assert_eq!(state.canvas.code_buffer.source(), "hi");
    }

    #[test]
    fn property_field_focus_shadows_code_editor() {
        let mut state = focused_state_with_buffer("hi");
        // Open a property-row field-focus on top.
        state.canvas.document = prism_builder::BuilderDocument {
            root: Some(prism_builder::Node {
                id: "n".into(),
                component: "text".into(),
                props: serde_json::json!({ "body": "x" }),
                ..Default::default()
            }),
            ..Default::default()
        };
        state.canvas.selection = Some("n".into());
        state.begin_field_focus("n", "body", "text");
        // Type — should land in the field-focus draft, NOT the code
        // editor buffer.
        fan_out(&mut state, &Event::Text { text: "y".into() });
        assert_eq!(state.field_focus.as_ref().unwrap().draft(), "xy");
        assert_eq!(state.canvas.code_buffer.source(), "hi");
    }

    #[test]
    fn wheel_scrolls_code_editor_viewport() {
        let mut state = focused_state_with_buffer("a long line of text");
        fan_out(&mut state, &Event::Wheel { dx: 50.0, dy: 25.0 });
        assert_eq!(state.canvas.code_buffer.scroll_x, 50.0);
        assert_eq!(state.canvas.code_buffer.scroll_y, 25.0);
        // Negative wheel deltas reduce — but the scroll never goes
        // negative (the heuristic in the service clamps).
        fan_out(
            &mut state,
            &Event::Wheel {
                dx: -100.0,
                dy: -200.0,
            },
        );
        assert_eq!(state.canvas.code_buffer.scroll_x, 0.0);
        assert_eq!(state.canvas.code_buffer.scroll_y, 0.0);
    }

    #[test]
    fn typing_at_end_auto_scrolls_right() {
        let mut state = focused_state_with_buffer("");
        // Type enough characters to push past the viewport's right
        // edge (default test viewport is 0x0, so editor_w lands at
        // 200 — the min — and the gutter pushes scroll_x as soon as
        // the caret advances past ~150 px).
        for _ in 0..100 {
            fan_out(&mut state, &Event::Text { text: "x".into() });
        }
        assert!(
            state.canvas.code_buffer.scroll_x > 0.0,
            "scroll_x should advance past 0 after typing past the viewport"
        );
    }

    #[test]
    fn ime_preedit_shows_inline_without_mutating_buffer() {
        let mut state = focused_state_with_buffer("ab");
        // Position caret between "a" and "b".
        state.canvas.code_buffer.editor.place_caret_at(1, false);
        fan_out(
            &mut state,
            &Event::ImePreedit {
                text: "ん".into(),
                cursor_byte: None,
            },
        );
        // Real buffer untouched.
        assert_eq!(state.canvas.code_buffer.source(), "ab");
        // Display reflects the composition.
        let display = state.canvas.code_buffer.editor.display_text().into_owned();
        assert_eq!(display, "aんb");
    }

    #[test]
    fn ime_commit_finalises_into_buffer() {
        let mut state = focused_state_with_buffer("ab");
        state.canvas.code_buffer.editor.place_caret_at(1, false);
        fan_out(
            &mut state,
            &Event::ImePreedit {
                text: "ん".into(),
                cursor_byte: None,
            },
        );
        fan_out(&mut state, &Event::ImeCommit { text: "漢".into() });
        assert!(!state.canvas.code_buffer.editor.has_preedit());
        assert_eq!(state.canvas.code_buffer.source(), "a漢b");
    }

    #[test]
    fn ime_disabled_clears_preedit() {
        let mut state = focused_state_with_buffer("ab");
        fan_out(
            &mut state,
            &Event::ImePreedit {
                text: "X".into(),
                cursor_byte: None,
            },
        );
        assert!(state.canvas.code_buffer.editor.has_preedit());
        fan_out(&mut state, &Event::ImeDisabled);
        assert!(!state.canvas.code_buffer.editor.has_preedit());
    }

    #[test]
    fn ctrl_z_undoes_in_code_buffer() {
        let mut state = focused_state_with_buffer("hi");
        fan_out(&mut state, &Event::Text { text: "abc".into() });
        assert_eq!(state.canvas.code_buffer.source(), "hiabc");
        let ctrl_z = Event::Key {
            code: "z".into(),
            pressed: true,
            modifiers: Modifiers {
                ctrl: true,
                ..Default::default()
            },
        };
        fan_out(&mut state, &ctrl_z);
        assert_eq!(state.canvas.code_buffer.source(), "hi");
    }
}
