//! Shared text-input dispatch — the single point where keyboard /
//! IME / clipboard events drive a [`TextEditor`].
//!
//! Three callers feed through this helper:
//!
//! 1. [`FieldFocusService`](super::FieldFocusService) — property-row
//!    inline edits.
//! 2. [`CodeEditorService`](super::CodeEditorService) — the
//!    `shell.code-editor` panel.
//! 3. The modal-overlay query fields (command palette + search) once
//!    they're wired through their own `TextEditor`.
//!
//! Each caller has its own post-mutation work (flush to prop, mark
//! tab dirty, ensure caret visible, rebuild result list). The
//! dispatch returns a [`TextInputOutcome`] and the caller branches on
//! it — *what* mutated stays out of this module.
//!
//! ## What the dispatch handles
//!
//! * `Event::Text` → `editor.apply_text(text)`.
//! * `Event::ImePreedit` / `ImeCommit` / `ImeEnabled` / `ImeDisabled`.
//! * `Event::Key { Ctrl/Cmd + (c|x|v) }` → clipboard ops, including
//!   the `editor.insert("", true)` that follows Ctrl+X.
//! * `Event::Key { … }` → `editor.apply_key(code, mods)` for every
//!   key the editor knows about.
//!
//! ## What the dispatch does NOT handle
//!
//! * `Enter` / `Escape` — caller's commit/cancel semantics differ.
//! * `Event::Wheel` — only the multi-line code panel scrolls.
//! * Number-field arrow nudges — `FieldFocusService` short-circuits
//!   those before calling dispatch.
//! * Language-aware actions (`Ctrl+/` comment toggle on a
//!   `CodeBuffer`) — caller wraps with its language tag first, then
//!   skips dispatch for that combo.
//!
//! The caller pre-empts any of these by matching on the event itself
//! before delegating to [`dispatch_text_input`].

use prism_ui_runtime::editor::TextEditor;
use prism_ui_runtime::event::Event;

use crate::services::Clipboard;

/// Declarative behaviour knobs. Default = "the editor owns every
/// modifier-bearing key combo it claims; nothing passes through".
#[derive(Clone, Copy, Debug, Default)]
pub struct TextInputBindings<'a> {
    /// Modifier-bearing key codes (Ctrl/Cmd + …) that the dispatch
    /// should *not* route through the editor — they pass through so
    /// an outer service can claim them.
    ///
    /// `CodeEditorService` uses `["n", "o", "s", "w", "tab"]` so
    /// `EditorFilesService` (one layer up) owns Ctrl+N/O/S/W/Tab.
    pub passthrough_modifier_keys: &'a [&'a str],
    /// Plain (non-modifier) key codes the dispatch should treat as
    /// caller-owned. Returned as [`TextInputOutcome::Ignored`] so the
    /// caller can implement its own commit / cancel / dismiss logic.
    ///
    /// `FieldFocusService` uses `["enter", "return", "escape"]` —
    /// Enter commits the field, Escape cancels.
    /// `CodeEditorService` uses `["escape"]` only — Enter still flows
    /// through the editor and inserts a newline.
    /// The modal-overlay query fields use `["enter", "return", "escape",
    /// "arrowup", "arrowdown"]` — Enter executes the selection,
    /// Escape closes, arrows nav through results.
    pub passthrough_plain_keys: &'a [&'a str],
}

/// Result of one dispatch call. The caller drives all post-mutation
/// work — this enum is the only signal that crosses the seam.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextInputOutcome {
    /// Buffer text changed. Flush-to-prop / mark-tab-dirty /
    /// rebuild-search-results work belongs here.
    BufferMutated,
    /// Caret moved, selection changed, or an IME preedit composition
    /// updated — display invalidated but buffer text is identical.
    /// Caller may want a redraw without touching dirty-state.
    DisplayMutated,
    /// Editor saw the event and produced no change. The caller owns
    /// the focus surface, so it should still report `Handled` — no
    /// global shortcut should fire while typing.
    Inert,
    /// Modifier-bearing key the editor doesn't claim and the bindings
    /// don't whitelist. Caller should return `Pass` so an outer
    /// service (input scheme, persistence, …) can handle it.
    PassToGlobal,
    /// Event isn't text-input shaped (e.g. `PointerMove`, `Wheel`).
    /// Caller should fall through to its own arms.
    Ignored,
}

/// Route a UI event through a [`TextEditor`] + clipboard. Returns a
/// [`TextInputOutcome`] describing what happened; caller decides what
/// to do next.
///
/// `Enter` and `Escape` are returned as [`TextInputOutcome::Ignored`]
/// so the caller's own commit/cancel logic stays the single source of
/// truth. Every other text-input shaped event is consumed here.
pub fn dispatch_text_input(
    event: &Event,
    editor: &mut TextEditor,
    clipboard: &mut Clipboard,
    bindings: &TextInputBindings<'_>,
) -> TextInputOutcome {
    match event {
        Event::Text { text } => {
            editor.apply_text(text);
            TextInputOutcome::BufferMutated
        }
        // IME preedit paints inline via `editor.display_text()` but
        // never touches the buffer — display-only invalidation.
        Event::ImePreedit { text, cursor_byte } => {
            editor.apply_ime_preedit(text, *cursor_byte);
            TextInputOutcome::DisplayMutated
        }
        // Commit finalises the composition into the buffer — caller
        // flushes to prop / marks dirty.
        Event::ImeCommit { text } => {
            editor.apply_ime_commit(text);
            TextInputOutcome::BufferMutated
        }
        Event::ImeEnabled => TextInputOutcome::Inert,
        Event::ImeDisabled => {
            editor.clear_preedit();
            TextInputOutcome::DisplayMutated
        }
        Event::Key {
            code,
            pressed: true,
            modifiers,
        } => {
            let cmd_held = modifiers.ctrl || modifiers.meta;
            // Plain (non-modifier) keys the caller wants to own —
            // commit / cancel / result-nav semantics differ across the
            // three callers, so return Ignored to surface them.
            if !cmd_held
                && !modifiers.alt
                && bindings.passthrough_plain_keys.contains(&code.as_str())
            {
                return TextInputOutcome::Ignored;
            }
            if cmd_held {
                if bindings.passthrough_modifier_keys.contains(&code.as_str()) {
                    return TextInputOutcome::PassToGlobal;
                }
                match code.as_str() {
                    "c" => {
                        if let Some(s) = editor.selected_text() {
                            clipboard.set_string(s.to_string());
                        }
                        // Copy never mutates the buffer; report Inert
                        // so the caller still reports Handled.
                        return TextInputOutcome::Inert;
                    }
                    "x" => {
                        if let Some(s) = editor.selected_text() {
                            let s = s.to_string();
                            clipboard.set_string(s);
                            editor.insert("", true);
                            return TextInputOutcome::BufferMutated;
                        }
                        return TextInputOutcome::Inert;
                    }
                    "v" => {
                        if let Some(text) = clipboard.get_string() {
                            editor.insert(&text, true);
                            return TextInputOutcome::BufferMutated;
                        }
                        return TextInputOutcome::Inert;
                    }
                    _ => {}
                }
            }
            // Distinguish *text* mutations from pure caret nav by
            // comparing the buffer length before / after. Caret +
            // selection moves return `Mutated` from the editor too,
            // so a raw outcome check would mark dirty on every arrow.
            let before_len = editor.text().len();
            let outcome = editor.apply_key(code, *modifiers);
            let after_len = editor.text().len();
            if outcome.mutated() {
                if before_len != after_len {
                    TextInputOutcome::BufferMutated
                } else {
                    TextInputOutcome::DisplayMutated
                }
            } else if cmd_held || modifiers.alt {
                TextInputOutcome::PassToGlobal
            } else {
                TextInputOutcome::Inert
            }
        }
        // Unhandled event variants — caller falls through to its own
        // arms (Wheel, PointerMove, …).
        _ => TextInputOutcome::Ignored,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use prism_ui_runtime::event::Modifiers;

    fn key(code: &str, modifiers: Modifiers) -> Event {
        Event::Key {
            code: code.into(),
            pressed: true,
            modifiers,
        }
    }

    #[test]
    fn text_event_mutates_buffer() {
        let mut editor = TextEditor::with_text("hi");
        let mut clip = Clipboard::default();
        let out = dispatch_text_input(
            &Event::Text { text: "X".into() },
            &mut editor,
            &mut clip,
            &TextInputBindings::default(),
        );
        assert_eq!(out, TextInputOutcome::BufferMutated);
        assert_eq!(editor.text(), "hiX");
    }

    #[test]
    fn ime_preedit_is_display_only() {
        let mut editor = TextEditor::with_text("hi");
        let mut clip = Clipboard::default();
        let out = dispatch_text_input(
            &Event::ImePreedit {
                text: "ん".into(),
                cursor_byte: None,
            },
            &mut editor,
            &mut clip,
            &TextInputBindings::default(),
        );
        assert_eq!(out, TextInputOutcome::DisplayMutated);
        // Buffer unchanged.
        assert_eq!(editor.text(), "hi");
        assert!(editor.has_preedit());
    }

    #[test]
    fn ime_commit_mutates_buffer() {
        let mut editor = TextEditor::with_text("hi");
        let mut clip = Clipboard::default();
        let out = dispatch_text_input(
            &Event::ImeCommit { text: "漢".into() },
            &mut editor,
            &mut clip,
            &TextInputBindings::default(),
        );
        assert_eq!(out, TextInputOutcome::BufferMutated);
        assert_eq!(editor.text(), "hi漢");
    }

    #[test]
    fn passthrough_plain_keys_skip_the_editor() {
        let mut editor = TextEditor::with_text("hi");
        let mut clip = Clipboard::default();
        let bindings = TextInputBindings {
            passthrough_plain_keys: &["enter", "return", "escape"],
            ..Default::default()
        };
        for code in ["enter", "return", "escape"] {
            let out = dispatch_text_input(
                &key(code, Modifiers::default()),
                &mut editor,
                &mut clip,
                &bindings,
            );
            assert_eq!(
                out,
                TextInputOutcome::Ignored,
                "`{code}` must defer to caller"
            );
        }
        // Buffer untouched.
        assert_eq!(editor.text(), "hi");
    }

    #[test]
    fn enter_with_no_passthrough_reaches_the_editor() {
        // CodeEditorService's bindings — Enter flows through and the
        // multi-line editor inserts a newline.
        let mut editor = TextEditor::new_multi_line();
        editor.set_text("hi");
        editor.place_caret_at(2, false);
        let mut clip = Clipboard::default();
        let out = dispatch_text_input(
            &key("enter", Modifiers::default()),
            &mut editor,
            &mut clip,
            &TextInputBindings::default(),
        );
        assert_eq!(out, TextInputOutcome::BufferMutated);
        assert_eq!(editor.text(), "hi\n");
    }

    #[test]
    fn ctrl_c_copies_selected_text_without_mutating() {
        let mut editor = TextEditor::with_text("hello world");
        editor.select_all();
        let mut clip = Clipboard::default();
        let out = dispatch_text_input(
            &key(
                "c",
                Modifiers {
                    ctrl: true,
                    ..Default::default()
                },
            ),
            &mut editor,
            &mut clip,
            &TextInputBindings::default(),
        );
        assert_eq!(out, TextInputOutcome::Inert);
        assert_eq!(clip.get_string().as_deref(), Some("hello world"));
        assert_eq!(editor.text(), "hello world");
    }

    #[test]
    fn ctrl_x_cuts_selection_and_reports_buffer_mutated() {
        let mut editor = TextEditor::with_text("hello world");
        editor.select_all();
        let mut clip = Clipboard::default();
        let out = dispatch_text_input(
            &key(
                "x",
                Modifiers {
                    ctrl: true,
                    ..Default::default()
                },
            ),
            &mut editor,
            &mut clip,
            &TextInputBindings::default(),
        );
        assert_eq!(out, TextInputOutcome::BufferMutated);
        assert_eq!(clip.get_string().as_deref(), Some("hello world"));
        assert_eq!(editor.text(), "");
    }

    #[test]
    fn ctrl_v_pastes_clipboard() {
        let mut editor = TextEditor::with_text("");
        let mut clip = Clipboard::default();
        clip.set_string("pasted");
        let out = dispatch_text_input(
            &key(
                "v",
                Modifiers {
                    ctrl: true,
                    ..Default::default()
                },
            ),
            &mut editor,
            &mut clip,
            &TextInputBindings::default(),
        );
        assert_eq!(out, TextInputOutcome::BufferMutated);
        assert_eq!(editor.text(), "pasted");
    }

    #[test]
    fn passthrough_modifier_keys_skip_the_editor() {
        let mut editor = TextEditor::with_text("hi");
        let mut clip = Clipboard::default();
        let bindings = TextInputBindings {
            passthrough_modifier_keys: &["s"],
            ..Default::default()
        };
        let out = dispatch_text_input(
            &key(
                "s",
                Modifiers {
                    ctrl: true,
                    ..Default::default()
                },
            ),
            &mut editor,
            &mut clip,
            &bindings,
        );
        assert_eq!(out, TextInputOutcome::PassToGlobal);
        assert_eq!(editor.text(), "hi");
    }

    #[test]
    fn arrow_key_is_display_only() {
        let mut editor = TextEditor::with_text("hello");
        let mut clip = Clipboard::default();
        let out = dispatch_text_input(
            &key("arrowleft", Modifiers::default()),
            &mut editor,
            &mut clip,
            &TextInputBindings::default(),
        );
        assert_eq!(out, TextInputOutcome::DisplayMutated);
        assert_eq!(editor.caret_byte(), 4);
        assert_eq!(editor.text(), "hello");
    }

    #[test]
    fn backspace_is_buffer_mutated() {
        let mut editor = TextEditor::with_text("hello");
        let mut clip = Clipboard::default();
        let out = dispatch_text_input(
            &key("backspace", Modifiers::default()),
            &mut editor,
            &mut clip,
            &TextInputBindings::default(),
        );
        assert_eq!(out, TextInputOutcome::BufferMutated);
        assert_eq!(editor.text(), "hell");
    }

    #[test]
    fn modifier_key_editor_does_not_claim_passes_to_global() {
        let mut editor = TextEditor::with_text("hi");
        let mut clip = Clipboard::default();
        // Ctrl+Q isn't an editor command and isn't in the passthrough
        // list — return PassToGlobal so an outer service can claim it.
        let out = dispatch_text_input(
            &key(
                "q",
                Modifiers {
                    ctrl: true,
                    ..Default::default()
                },
            ),
            &mut editor,
            &mut clip,
            &TextInputBindings::default(),
        );
        assert_eq!(out, TextInputOutcome::PassToGlobal);
    }

    #[test]
    fn non_text_event_is_ignored() {
        let mut editor = TextEditor::default();
        let mut clip = Clipboard::default();
        let out = dispatch_text_input(
            &Event::Wheel { dx: 1.0, dy: 1.0 },
            &mut editor,
            &mut clip,
            &TextInputBindings::default(),
        );
        assert_eq!(out, TextInputOutcome::Ignored);
    }
}
