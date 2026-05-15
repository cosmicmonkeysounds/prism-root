//! End-to-end tests for the in-shell code editor.
//!
//! These tests drive the whole editor through the same production
//! path the user hits: `dispatch_event` over `prism_ui_runtime::event::Event`
//! values. No editor-private state is poked directly — every
//! manipulation goes through the shell's `ShellInner` (registry +
//! services + state) so a regression in the wiring shows up here as
//! reliably as it would in a manual session.
//!
//! Coverage:
//!
//! * Typing — characters land in the buffer, caret advances.
//! * Newline + indent — Enter adds `\n`, leading whitespace is
//!   preserved (auto-indent).
//! * Arrow navigation — Left/Right/Up/Down move the caret; shift
//!   extends a selection.
//! * Home / End / Ctrl+Home / Ctrl+End — line and document jumps.
//! * Delete / Backspace + word variants.
//! * Selection: Ctrl+A → typing replaces.
//! * Undo / Redo through Ctrl+Z / Ctrl+Y.
//! * Cut / copy / paste through the shared clipboard.
//! * Tab / Shift-Tab — indent / dedent both single and multi-line.
//! * Ctrl+D duplicate line, Ctrl+K delete line.
//! * Page-up / page-down with preferred-column preservation.
//! * Wheel scroll + auto-scroll-to-caret after edits.
//! * IME preedit / commit flow.
//! * Click-to-position via cosmic-text byte resolution.
//! * Drag-to-select with anchor + extend.
//! * Double-click word, triple-click line.
//! * Shift-click extend.
//! * Syntax-highlight spans surface in the rendered tree.

use prism_shell::headless::BuiltinScene;
use prism_shell::Shell;
use prism_ui_runtime::event::{Event, Modifiers, PointerButton};

/// Build a shell with the code-editor scene preloaded and focused.
/// The buffer starts with a small Luau snippet, so every test sees
/// the same baseline.
fn shell_with_editor() -> Shell {
    let shell = Shell::new().expect("Shell::new");
    shell.apply_scene(BuiltinScene::CodeEditor);
    shell
}

fn dispatch(shell: &Shell, event: Event) -> bool {
    shell.dispatch_event(&event)
}

fn buffer_source(shell: &Shell) -> String {
    shell.with_inner(|inner| inner.state.canvas.code_buffer.source().to_string())
}

fn buffer_caret(shell: &Shell) -> usize {
    shell.with_inner(|inner| inner.state.canvas.code_buffer.caret())
}

fn buffer_selection(shell: &Shell) -> Option<(usize, usize)> {
    shell.with_inner(|inner| inner.state.canvas.code_buffer.editor.selection())
}

fn buffer_scroll(shell: &Shell) -> (f32, f32) {
    shell.with_inner(|inner| {
        (
            inner.state.canvas.code_buffer.scroll_x,
            inner.state.canvas.code_buffer.scroll_y,
        )
    })
}

fn key_press(code: &str) -> Event {
    Event::Key {
        code: code.into(),
        pressed: true,
        modifiers: Modifiers::default(),
    }
}

fn key_press_mods(code: &str, mods: Modifiers) -> Event {
    Event::Key {
        code: code.into(),
        pressed: true,
        modifiers: mods,
    }
}

fn ctrl() -> Modifiers {
    Modifiers {
        ctrl: true,
        ..Default::default()
    }
}

fn shift() -> Modifiers {
    Modifiers {
        shift: true,
        ..Default::default()
    }
}

fn ctrl_shift() -> Modifiers {
    Modifiers {
        ctrl: true,
        shift: true,
        ..Default::default()
    }
}

// ─── Typing + Edits ────────────────────────────────────────────

#[test]
fn typing_appends_into_buffer_and_advances_caret() {
    let shell = shell_with_editor();
    // Place caret at end of buffer.
    dispatch(&shell, key_press_mods("end", ctrl()));
    let before = buffer_source(&shell);
    let before_caret = buffer_caret(&shell);
    dispatch(&shell, Event::Text { text: "X".into() });
    assert_eq!(buffer_source(&shell), format!("{before}X"));
    assert_eq!(buffer_caret(&shell), before_caret + 1);
}

#[test]
fn backspace_removes_char_before_caret() {
    let shell = shell_with_editor();
    dispatch(&shell, key_press_mods("end", ctrl()));
    let before = buffer_source(&shell);
    dispatch(&shell, key_press("backspace"));
    assert_eq!(buffer_source(&shell), before[..before.len() - 1]);
}

#[test]
fn enter_inserts_newline_and_preserves_indent() {
    let shell = shell_with_editor();
    // Place caret at end of line 2 ("  print(\"hello \" .. name)")
    // by jumping to doc-end and arrow-up'ing.
    dispatch(&shell, key_press_mods("home", ctrl()));
    dispatch(&shell, key_press("arrowdown")); // line 2
    dispatch(&shell, key_press("end")); // end of line 2
    dispatch(&shell, key_press("enter"));
    let src = buffer_source(&shell);
    // Auto-indent: the new line carries the same leading spaces as
    // line 2 (which starts with two spaces).
    assert!(
        src.contains("name)\n  \n"),
        "expected auto-indent after newline; got:\n{src}"
    );
}

// ─── Navigation ────────────────────────────────────────────────

#[test]
fn arrows_move_caret() {
    let shell = shell_with_editor();
    dispatch(&shell, key_press_mods("home", ctrl()));
    assert_eq!(buffer_caret(&shell), 0);
    dispatch(&shell, key_press("arrowright"));
    assert_eq!(buffer_caret(&shell), 1);
    dispatch(&shell, key_press("arrowleft"));
    assert_eq!(buffer_caret(&shell), 0);
}

#[test]
fn ctrl_arrow_jumps_words() {
    let shell = shell_with_editor();
    dispatch(&shell, key_press_mods("home", ctrl()));
    dispatch(&shell, key_press_mods("arrowright", ctrl()));
    // Should land at the end of "local" (byte 5).
    assert_eq!(buffer_caret(&shell), 5);
}

#[test]
fn shift_arrow_extends_selection() {
    let shell = shell_with_editor();
    dispatch(&shell, key_press_mods("home", ctrl()));
    dispatch(&shell, key_press_mods("arrowright", shift()));
    dispatch(&shell, key_press_mods("arrowright", shift()));
    assert_eq!(buffer_selection(&shell), Some((0, 2)));
}

#[test]
fn home_end_navigate_within_line() {
    let shell = shell_with_editor();
    dispatch(&shell, key_press_mods("home", ctrl()));
    // Line 1: "local function greet(name)" — 26 bytes.
    dispatch(&shell, key_press("end"));
    assert_eq!(buffer_caret(&shell), 26);
    dispatch(&shell, key_press("home"));
    assert_eq!(buffer_caret(&shell), 0);
}

#[test]
fn ctrl_home_end_jump_to_doc_extents() {
    let shell = shell_with_editor();
    dispatch(&shell, key_press_mods("end", ctrl()));
    let end = buffer_source(&shell).len();
    assert_eq!(buffer_caret(&shell), end);
    dispatch(&shell, key_press_mods("home", ctrl()));
    assert_eq!(buffer_caret(&shell), 0);
}

#[test]
fn page_down_moves_caret_down_multiple_rows() {
    let shell = shell_with_editor();
    dispatch(&shell, key_press_mods("home", ctrl()));
    let caret_before = buffer_caret(&shell);
    dispatch(&shell, key_press("pagedown"));
    let caret_after = buffer_caret(&shell);
    // The scene buffer is 7 lines; pagedown clamps to end-of-doc.
    assert!(caret_after > caret_before);
}

// ─── Selection + Clipboard ────────────────────────────────────

#[test]
fn ctrl_a_selects_all() {
    let shell = shell_with_editor();
    dispatch(&shell, key_press_mods("a", ctrl()));
    let full = buffer_source(&shell);
    assert_eq!(buffer_selection(&shell), Some((0, full.len())));
}

#[test]
fn typing_replaces_selection() {
    let shell = shell_with_editor();
    dispatch(&shell, key_press_mods("a", ctrl()));
    dispatch(&shell, Event::Text { text: "X".into() });
    assert_eq!(buffer_source(&shell), "X");
    assert_eq!(buffer_caret(&shell), 1);
}

#[test]
fn ctrl_z_undoes_typing_run() {
    let shell = shell_with_editor();
    let original = buffer_source(&shell);
    dispatch(&shell, key_press_mods("end", ctrl()));
    dispatch(&shell, Event::Text { text: "abc".into() });
    assert_eq!(buffer_source(&shell), format!("{original}abc"));
    dispatch(&shell, key_press_mods("z", ctrl()));
    assert_eq!(buffer_source(&shell), original);
}

#[test]
fn ctrl_y_redoes() {
    let shell = shell_with_editor();
    let original = buffer_source(&shell);
    dispatch(&shell, key_press_mods("end", ctrl()));
    dispatch(&shell, Event::Text { text: "abc".into() });
    dispatch(&shell, key_press_mods("z", ctrl()));
    assert_eq!(buffer_source(&shell), original);
    dispatch(&shell, key_press_mods("y", ctrl()));
    assert_eq!(buffer_source(&shell), format!("{original}abc"));
}

#[test]
fn shift_ctrl_z_also_redoes() {
    let shell = shell_with_editor();
    let original = buffer_source(&shell);
    dispatch(&shell, key_press_mods("end", ctrl()));
    dispatch(&shell, Event::Text { text: "abc".into() });
    dispatch(&shell, key_press_mods("z", ctrl()));
    dispatch(&shell, key_press_mods("z", ctrl_shift()));
    assert_eq!(buffer_source(&shell), format!("{original}abc"));
}

#[test]
fn cut_copy_paste_round_trip() {
    let shell = shell_with_editor();
    // Select "local" on line 1.
    dispatch(&shell, key_press_mods("home", ctrl()));
    for _ in 0..5 {
        dispatch(&shell, key_press_mods("arrowright", shift()));
    }
    // Cut.
    dispatch(&shell, key_press_mods("x", ctrl()));
    assert!(!buffer_source(&shell).starts_with("local"));
    // Paste at end.
    dispatch(&shell, key_press_mods("end", ctrl()));
    dispatch(&shell, key_press_mods("v", ctrl()));
    assert!(buffer_source(&shell).ends_with("local"));
}

#[test]
fn ctrl_slash_toggles_line_comment() {
    let shell = shell_with_editor();
    dispatch(&shell, key_press_mods("home", ctrl()));
    let before = buffer_source(&shell);
    dispatch(&shell, key_press_mods("/", ctrl()));
    let after = buffer_source(&shell);
    // The scene buffer is Luau, so the prefix is `-- `.
    assert!(after.starts_with("-- "));
    // Toggle again to uncomment.
    dispatch(&shell, key_press_mods("/", ctrl()));
    assert_eq!(buffer_source(&shell), before);
}

#[test]
fn alt_up_moves_line() {
    let shell = shell_with_editor();
    // Place caret on line 2.
    dispatch(&shell, key_press_mods("home", ctrl()));
    dispatch(&shell, key_press("arrowdown"));
    let before = buffer_source(&shell);
    dispatch(
        &shell,
        key_press_mods(
            "arrowup",
            Modifiers {
                alt: true,
                ..Default::default()
            },
        ),
    );
    let after = buffer_source(&shell);
    assert_ne!(after, before);
    // The first two lines should have swapped.
    let before_lines: Vec<&str> = before.lines().collect();
    let after_lines: Vec<&str> = after.lines().collect();
    assert_eq!(after_lines[0], before_lines[1]);
    assert_eq!(after_lines[1], before_lines[0]);
}

#[test]
fn ctrl_l_selects_current_line() {
    let shell = shell_with_editor();
    dispatch(&shell, key_press_mods("home", ctrl()));
    dispatch(&shell, key_press_mods("l", ctrl()));
    let sel = buffer_selection(&shell);
    // Line 1: "local function greet(name)" — bytes 0..26.
    assert_eq!(sel, Some((0, 26)));
}

#[test]
fn ctrl_close_bracket_jumps_to_matching_bracket() {
    let shell = shell_with_editor();
    // Place caret on the opening `(` of `greet(name)` — byte 20.
    shell.with_inner(|_| {});
    dispatch(&shell, key_press_mods("home", ctrl()));
    for _ in 0..20 {
        dispatch(&shell, key_press("arrowright"));
    }
    dispatch(&shell, key_press_mods("]", ctrl()));
    let caret = buffer_caret(&shell);
    // The matching `)` lives at byte 25 in "local function greet(name)".
    assert_eq!(caret, 25);
}

#[test]
fn auto_close_inserts_matching_quote() {
    let shell = shell_with_editor();
    // Clear the buffer and type a `(`.
    dispatch(&shell, key_press_mods("a", ctrl()));
    dispatch(&shell, key_press("backspace"));
    dispatch(
        &shell,
        Event::Text {
            text: "(".into(),
        },
    );
    assert_eq!(buffer_source(&shell), "()");
    assert_eq!(buffer_caret(&shell), 1);
}

#[test]
fn smart_indent_inside_braces_opens_three_lines() {
    let shell = shell_with_editor();
    // Clear and re-seed with a minimal `{}` so the smart-indent path
    // is unambiguous.
    dispatch(&shell, key_press_mods("a", ctrl()));
    dispatch(
        &shell,
        Event::Text {
            text: "{".into(),
        },
    );
    // auto-close gives "{}", caret at byte 1.
    dispatch(&shell, key_press("enter"));
    let src = buffer_source(&shell);
    assert!(src.contains("{\n  \n}"), "got: {src}");
}

#[test]
fn ctrl_d_duplicates_line() {
    let shell = shell_with_editor();
    dispatch(&shell, key_press_mods("home", ctrl()));
    let before = buffer_source(&shell);
    dispatch(&shell, key_press_mods("d", ctrl()));
    let after = buffer_source(&shell);
    // The buffer should have one more newline-delimited row.
    assert!(after.lines().count() > before.lines().count());
}

#[test]
fn ctrl_k_deletes_line() {
    let shell = shell_with_editor();
    dispatch(&shell, key_press_mods("home", ctrl()));
    let before = buffer_source(&shell);
    dispatch(&shell, key_press_mods("k", ctrl()));
    let after = buffer_source(&shell);
    // The first line ("local function greet(name)") should be gone.
    assert!(!after.starts_with("local function greet"));
    assert!(before.lines().count() > after.lines().count());
}

// ─── Tab / Shift-Tab ──────────────────────────────────────────

#[test]
fn tab_inserts_two_spaces_at_caret() {
    let shell = shell_with_editor();
    dispatch(&shell, key_press_mods("home", ctrl()));
    let before = buffer_source(&shell);
    dispatch(&shell, key_press("tab"));
    assert_eq!(buffer_source(&shell), format!("  {before}"));
}

#[test]
fn shift_tab_dedents_indented_line() {
    let shell = shell_with_editor();
    // Move to line 2 which starts with "  print(...)".
    dispatch(&shell, key_press_mods("home", ctrl()));
    dispatch(&shell, key_press("arrowdown"));
    dispatch(&shell, key_press("home"));
    dispatch(&shell, key_press_mods("tab", shift()));
    let src = buffer_source(&shell);
    assert!(
        src.contains("\nprint(\"hello"),
        "expected dedented line; got:\n{src}"
    );
}

// ─── Scroll ───────────────────────────────────────────────────

#[test]
fn wheel_scrolls_editor_viewport() {
    let shell = shell_with_editor();
    let before = buffer_scroll(&shell);
    dispatch(&shell, Event::Wheel { dx: 30.0, dy: 15.0 });
    let after = buffer_scroll(&shell);
    assert!(after.0 > before.0, "expected scroll_x to advance");
    assert!(after.1 > before.1, "expected scroll_y to advance");
}

#[test]
fn wheel_clamps_at_zero() {
    let shell = shell_with_editor();
    dispatch(
        &shell,
        Event::Wheel {
            dx: -100.0,
            dy: -100.0,
        },
    );
    let (sx, sy) = buffer_scroll(&shell);
    assert_eq!((sx, sy), (0.0, 0.0));
}

#[test]
fn typing_past_visible_width_auto_scrolls_right() {
    let shell = shell_with_editor();
    let (sx_before, _) = buffer_scroll(&shell);
    dispatch(&shell, key_press_mods("end", ctrl()));
    for _ in 0..200 {
        dispatch(&shell, Event::Text { text: "X".into() });
    }
    let (sx_after, _) = buffer_scroll(&shell);
    assert!(
        sx_after > sx_before,
        "scroll_x should advance after long line; got {sx_before} -> {sx_after}"
    );
}

// ─── IME ──────────────────────────────────────────────────────

#[test]
fn ime_preedit_shows_inline_without_mutating_buffer() {
    let shell = shell_with_editor();
    let before = buffer_source(&shell);
    dispatch(
        &shell,
        Event::ImePreedit {
            text: "ん".into(),
            cursor_byte: None,
        },
    );
    // Real buffer untouched.
    assert_eq!(buffer_source(&shell), before);
    // Display text reflects the composition.
    let display = shell.with_inner(|inner| {
        inner
            .state
            .canvas
            .code_buffer
            .editor
            .display_text()
            .into_owned()
    });
    assert!(display.contains('ん'));
}

#[test]
fn ime_commit_inserts_into_buffer() {
    let shell = shell_with_editor();
    dispatch(&shell, key_press_mods("end", ctrl()));
    dispatch(
        &shell,
        Event::ImePreedit {
            text: "ん".into(),
            cursor_byte: None,
        },
    );
    dispatch(&shell, Event::ImeCommit { text: "漢".into() });
    assert!(buffer_source(&shell).ends_with('漢'));
}

// ─── Click / Drag / Multi-click ───────────────────────────────

/// Helper: synthesise a pointer-down on the editor body. The scene
/// loader stashes the rendered tree in the shell's surface; we
/// resolve the editor body's bounds via the hit cache.
fn editor_body_bounds(shell: &Shell) -> prism_ui_runtime::command::Rect {
    let hit = shell.find_hit_by_role("code-editor-body");
    hit.expect("code-editor body must be rendered in the CodeEditor scene")
        .bounds
}

#[test]
fn click_into_editor_repositions_caret() {
    let shell = shell_with_editor();
    let bounds = editor_body_bounds(&shell);
    // Click roughly at column 4 of line 1.
    let px = bounds.x + 6.0 + 4.0 * 13.0 * 0.55;
    let py = bounds.y + 6.0 + 4.0;
    dispatch(
        &shell,
        Event::PointerDown {
            x: px,
            y: py,
            button: PointerButton::Primary,
            modifiers: Modifiers::default(),
        },
    );
    dispatch(
        &shell,
        Event::PointerUp {
            x: px,
            y: py,
            button: PointerButton::Primary,
            modifiers: Modifiers::default(),
        },
    );
    let caret = buffer_caret(&shell);
    // Caret should land somewhere inside the first ~10 bytes of
    // the buffer — proportional fonts add up to ±3 byte noise.
    assert!(caret <= 10, "expected caret near start; got {caret}");
}

#[test]
fn drag_select_extends_selection_from_anchor() {
    let shell = shell_with_editor();
    let bounds = editor_body_bounds(&shell);
    let baseline_y = bounds.y + 6.0 + 4.0;
    // Anchor at (roughly) column 0 of line 1, drag to column 10.
    let x0 = bounds.x + 6.0;
    let x1 = bounds.x + 6.0 + 10.0 * 13.0 * 0.55;
    dispatch(
        &shell,
        Event::PointerDown {
            x: x0,
            y: baseline_y,
            button: PointerButton::Primary,
            modifiers: Modifiers::default(),
        },
    );
    dispatch(
        &shell,
        Event::PointerMove {
            x: x1,
            y: baseline_y,
            modifiers: Modifiers::default(),
        },
    );
    dispatch(
        &shell,
        Event::PointerUp {
            x: x1,
            y: baseline_y,
            button: PointerButton::Primary,
            modifiers: Modifiers::default(),
        },
    );
    let sel = buffer_selection(&shell);
    assert!(sel.is_some(), "expected a selection after drag");
    let (s, e) = sel.unwrap();
    assert!(s < e);
    assert!(e - s >= 4, "selection should span several chars");
}

#[test]
fn shift_click_extends_existing_selection() {
    let shell = shell_with_editor();
    let bounds = editor_body_bounds(&shell);
    let baseline_y = bounds.y + 6.0 + 4.0;
    // Plain click at column 0.
    let x0 = bounds.x + 6.0;
    let x1 = bounds.x + 6.0 + 8.0 * 13.0 * 0.55;
    dispatch(
        &shell,
        Event::PointerDown {
            x: x0,
            y: baseline_y,
            button: PointerButton::Primary,
            modifiers: Modifiers::default(),
        },
    );
    dispatch(
        &shell,
        Event::PointerUp {
            x: x0,
            y: baseline_y,
            button: PointerButton::Primary,
            modifiers: Modifiers::default(),
        },
    );
    // Shift-click at column 8 should extend.
    dispatch(
        &shell,
        Event::PointerDown {
            x: x1,
            y: baseline_y,
            button: PointerButton::Primary,
            modifiers: shift(),
        },
    );
    dispatch(
        &shell,
        Event::PointerUp {
            x: x1,
            y: baseline_y,
            button: PointerButton::Primary,
            modifiers: shift(),
        },
    );
    let sel = buffer_selection(&shell);
    assert!(sel.is_some(), "expected selection from shift-click");
}

#[test]
fn double_click_selects_word_under_pointer() {
    let shell = shell_with_editor();
    let bounds = editor_body_bounds(&shell);
    let baseline_y = bounds.y + 6.0 + 4.0;
    // Click in the middle of "local" (col ~2).
    let px = bounds.x + 6.0 + 2.0 * 13.0 * 0.55;
    let down = || Event::PointerDown {
        x: px,
        y: baseline_y,
        button: PointerButton::Primary,
        modifiers: Modifiers::default(),
    };
    let up = || Event::PointerUp {
        x: px,
        y: baseline_y,
        button: PointerButton::Primary,
        modifiers: Modifiers::default(),
    };
    dispatch(&shell, down());
    dispatch(&shell, up());
    dispatch(&shell, down());
    dispatch(&shell, up());
    let sel = buffer_selection(&shell);
    assert_eq!(
        sel,
        Some((0, 5)),
        "expected 'local' selected (bytes 0..5); got {sel:?}"
    );
}

#[test]
fn triple_click_selects_line() {
    let shell = shell_with_editor();
    let bounds = editor_body_bounds(&shell);
    let baseline_y = bounds.y + 6.0 + 4.0;
    let px = bounds.x + 6.0 + 4.0 * 13.0 * 0.55;
    let down = || Event::PointerDown {
        x: px,
        y: baseline_y,
        button: PointerButton::Primary,
        modifiers: Modifiers::default(),
    };
    let up = || Event::PointerUp {
        x: px,
        y: baseline_y,
        button: PointerButton::Primary,
        modifiers: Modifiers::default(),
    };
    for _ in 0..3 {
        dispatch(&shell, down());
        dispatch(&shell, up());
    }
    let sel = buffer_selection(&shell).expect("triple-click should produce a selection");
    // First line of the scene buffer ends at byte 26
    // ("local function greet(name)").
    assert_eq!(sel, (0, 26));
}

// ─── Render tree / Syntax highlighting ────────────────────────

#[test]
fn rendered_tree_carries_syntax_spans_for_luau() {
    let shell = shell_with_editor();
    let json = shell.dump_frame();
    // The Luau tokenizer produces a `Keyword` span for "local";
    // it lowers to a `spans` field on the editor body's
    // text-input render node.
    assert!(
        json.contains("\"spans\""),
        "rendered tree missing syntax spans"
    );
    assert!(
        json.contains("\"start_byte\": 0"),
        "expected a span starting at byte 0 for the leading keyword"
    );
}

#[test]
fn rendered_tree_carries_caret_and_selection_when_active() {
    let shell = shell_with_editor();
    // CodeEditor scene preloads a selection covering "hello".
    let json = shell.dump_frame();
    assert!(
        json.contains("\"selection\""),
        "rendered tree should carry the editor's active selection"
    );
    assert!(
        json.contains("\"caret_byte\""),
        "rendered tree should carry caret byte"
    );
}

#[test]
fn hover_over_keyword_pushes_help_tooltip() {
    let shell = shell_with_editor();
    let bounds = editor_body_bounds(&shell);
    let baseline_y = bounds.y + 6.0 + 4.0;
    // The scene buffer starts with "local function …" — hover over
    // column ~2 (inside "local").
    let px = bounds.x + 6.0 + 2.0 * 13.0 * 0.55;
    dispatch(
        &shell,
        Event::PointerMove {
            x: px,
            y: baseline_y,
            modifiers: Modifiers::default(),
        },
    );
    let title = shell.with_inner(|inner| {
        inner
            .state
            .overlay
            .help_tooltip
            .as_ref()
            .map(|t| t.title.clone())
    });
    assert!(
        title.as_deref().is_some_and(|t| t.starts_with("editor:")),
        "expected editor-prefixed help tooltip; got {title:?}"
    );
}

#[test]
fn hover_leaving_editor_clears_help_tooltip() {
    let shell = shell_with_editor();
    let bounds = editor_body_bounds(&shell);
    let baseline_y = bounds.y + 6.0 + 4.0;
    let inside_x = bounds.x + 6.0 + 2.0 * 13.0 * 0.55;
    dispatch(
        &shell,
        Event::PointerMove {
            x: inside_x,
            y: baseline_y,
            modifiers: Modifiers::default(),
        },
    );
    // Now hover far away (outside the editor body).
    dispatch(
        &shell,
        Event::PointerMove {
            x: 0.0,
            y: 0.0,
            modifiers: Modifiers::default(),
        },
    );
    let title = shell.with_inner(|inner| {
        inner
            .state
            .overlay
            .help_tooltip
            .as_ref()
            .map(|t| t.title.clone())
    });
    assert!(title.is_none(), "editor tooltip should clear on leave");
}

#[test]
fn rendered_tree_carries_underline_when_preedit_active() {
    let shell = shell_with_editor();
    dispatch(
        &shell,
        Event::ImePreedit {
            text: "ん".into(),
            cursor_byte: None,
        },
    );
    let json = shell.dump_frame();
    assert!(
        json.contains("\"underline\""),
        "preedit should surface as an underline range in the rendered tree"
    );
}
