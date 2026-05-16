//! Text-input e2e — drives the modal-overlay `TextEditor`-backed
//! surfaces (command palette + search overlay) through the
//! production [`Shell::dispatch_event`] path and verifies the
//! rendered tree reflects the live caret + selection.
//!
//! Sister test to `code_editor_e2e.rs`; that file covers the
//! `shell.code-editor` panel exhaustively, while this one focuses on
//! the modal surfaces that were wired through the shared
//! `services::text_input::dispatch_text_input` helper in the
//! unify-editors pass.

use prism_shell::headless::BuiltinScene;
use prism_shell::Shell;
use prism_ui_runtime::event::{Event, Modifiers};

fn ctrl() -> Modifiers {
    Modifiers {
        ctrl: true,
        ..Default::default()
    }
}

fn key(code: &str, modifiers: Modifiers) -> Event {
    Event::Key {
        code: code.into(),
        pressed: true,
        modifiers,
    }
}

fn text(s: &str) -> Event {
    Event::Text { text: s.into() }
}

// ── command palette ──────────────────────────────────────────────────

#[test]
fn palette_typing_routes_through_shared_dispatch() {
    let shell = Shell::new().expect("shell boots");
    shell.with_inner_mut(|inner| {
        inner.state.overlay.command_palette.open = true;
    });
    shell.dispatch_event(&text("un"));
    shell.dispatch_event(&text("do"));
    shell.with_inner(|inner| {
        assert_eq!(inner.state.overlay.command_palette.query_text(), "undo");
        assert_eq!(inner.state.overlay.command_palette.query.caret_byte(), 4);
    });
}

#[test]
fn palette_arrow_left_moves_caret_via_text_editor() {
    let shell = Shell::new().expect("shell boots");
    shell.with_inner_mut(|inner| {
        inner.state.overlay.command_palette.open = true;
    });
    shell.dispatch_event(&text("hello"));
    shell.dispatch_event(&key("arrowleft", Modifiers::default()));
    shell.dispatch_event(&key("arrowleft", Modifiers::default()));
    shell.with_inner(|inner| {
        assert_eq!(inner.state.overlay.command_palette.query.caret_byte(), 3);
    });
}

#[test]
fn palette_ctrl_a_selects_then_type_replaces() {
    let shell = Shell::new().expect("shell boots");
    shell.with_inner_mut(|inner| {
        inner.state.overlay.command_palette.open = true;
    });
    shell.dispatch_event(&text("hello"));
    shell.dispatch_event(&key("a", ctrl()));
    shell.with_inner(|inner| {
        assert_eq!(
            inner.state.overlay.command_palette.query.selection(),
            Some((0, 5))
        );
    });
    shell.dispatch_event(&text("X"));
    shell.with_inner(|inner| {
        assert_eq!(inner.state.overlay.command_palette.query_text(), "X");
        assert!(inner
            .state
            .overlay
            .command_palette
            .query
            .selection()
            .is_none());
    });
}

#[test]
fn palette_ctrl_c_populates_clipboard_with_selection() {
    let shell = Shell::new().expect("shell boots");
    shell.with_inner_mut(|inner| {
        inner.state.overlay.command_palette.open = true;
    });
    shell.dispatch_event(&text("hello"));
    shell.dispatch_event(&key("a", ctrl()));
    shell.dispatch_event(&key("c", ctrl()));
    shell.with_inner(|inner| {
        assert_eq!(
            inner.clipboard.get_string().as_deref(),
            Some("hello"),
            "Ctrl+C must populate the shared clipboard"
        );
    });
}

#[test]
fn palette_ctrl_v_pastes_clipboard_at_caret() {
    let shell = Shell::new().expect("shell boots");
    shell.with_inner_mut(|inner| {
        inner.state.overlay.command_palette.open = true;
        // Pre-populate the clipboard so Ctrl+V has something to paste,
        // then start with an empty buffer (caret 0, no selection) so
        // there's nothing to overwrite.
        inner.clipboard.set_string("world");
    });
    shell.dispatch_event(&key("v", ctrl()));
    shell.with_inner(|inner| {
        assert_eq!(inner.state.overlay.command_palette.query_text(), "world");
    });
}

#[test]
fn palette_ctrl_x_cuts_selection_to_clipboard() {
    let shell = Shell::new().expect("shell boots");
    shell.with_inner_mut(|inner| {
        inner.state.overlay.command_palette.open = true;
    });
    shell.dispatch_event(&text("cutme"));
    shell.dispatch_event(&key("a", ctrl()));
    shell.dispatch_event(&key("x", ctrl()));
    shell.with_inner(|inner| {
        assert_eq!(
            inner.clipboard.get_string().as_deref(),
            Some("cutme"),
            "Ctrl+X must populate the clipboard"
        );
        assert_eq!(
            inner.state.overlay.command_palette.query_text(),
            "",
            "Ctrl+X must clear the selection from the buffer"
        );
    });
}

#[test]
fn palette_modal_capture_swallows_ctrl_s_so_save_does_not_fire() {
    // §24.8 keystone — the palette's modal-capture is preserved even
    // though the dispatch is now routing through a shared helper.
    let shell = Shell::new().expect("shell boots");
    shell.with_inner_mut(|inner| {
        inner.state.overlay.command_palette.open = true;
        inner.state.project.current_file = Some(std::path::PathBuf::from("/tmp/x.prism"));
        inner.state.project.dirty = true;
    });
    shell.dispatch_event(&key("s", ctrl()));
    shell.with_inner(|inner| {
        assert!(
            inner.state.project.dirty,
            "save must NOT run while palette open"
        );
    });
}

#[test]
fn palette_rendered_tree_carries_caret_byte() {
    let shell = Shell::new().expect("shell boots");
    shell.apply_scene(BuiltinScene::CommandPalette);
    let frame = shell.dump_frame();
    assert!(
        frame.contains("\"value\": \"undo\""),
        "rendered tree must contain the query: {frame:?}"
    );
    assert!(
        frame.contains("\"caret_byte\": 2"),
        "rendered tree must contain caret_byte=2"
    );
    assert!(
        frame.contains("\"focused\": true"),
        "the palette input must be focused so the caret paints"
    );
}

// ── search overlay ───────────────────────────────────────────────────

#[test]
fn search_typing_routes_through_shared_dispatch() {
    let shell = Shell::new().expect("shell boots");
    shell.with_inner_mut(|inner| {
        inner.state.search.open = true;
    });
    shell.dispatch_event(&text("demo"));
    shell.with_inner(|inner| {
        assert_eq!(inner.state.search.query_text(), "demo");
    });
}

#[test]
fn search_arrow_left_moves_caret() {
    let shell = Shell::new().expect("shell boots");
    shell.with_inner_mut(|inner| {
        inner.state.search.open = true;
    });
    shell.dispatch_event(&text("hi"));
    shell.dispatch_event(&key("arrowleft", Modifiers::default()));
    shell.with_inner(|inner| {
        assert_eq!(inner.state.search.query.caret_byte(), 1);
    });
}

#[test]
fn search_ctrl_a_then_type_replaces() {
    let shell = Shell::new().expect("shell boots");
    shell.with_inner_mut(|inner| {
        inner.state.search.open = true;
    });
    shell.dispatch_event(&text("stale"));
    shell.dispatch_event(&key("a", ctrl()));
    shell.dispatch_event(&text("fresh"));
    shell.with_inner(|inner| {
        assert_eq!(inner.state.search.query_text(), "fresh");
    });
}

#[test]
fn search_modal_capture_swallows_ctrl_s() {
    let shell = Shell::new().expect("shell boots");
    shell.with_inner_mut(|inner| {
        inner.state.search.open = true;
        inner.state.project.current_file = Some(std::path::PathBuf::from("/tmp/x.prism"));
        inner.state.project.dirty = true;
    });
    shell.dispatch_event(&key("s", ctrl()));
    shell.with_inner(|inner| {
        assert!(
            inner.state.project.dirty,
            "save must NOT run while search open"
        );
    });
}

#[test]
fn search_rendered_tree_carries_caret_and_selection() {
    let shell = Shell::new().expect("shell boots");
    shell.apply_scene(BuiltinScene::SearchOverlay);
    let frame = shell.dump_frame();
    assert!(
        frame.contains("\"value\": \"demo\""),
        "rendered tree must contain the query"
    );
    assert!(
        frame.contains("\"caret_byte\": 4"),
        "rendered tree must contain caret_byte=4 (head of the selection)"
    );
    // Scene selects bytes 1..4 ("emo"). The runtime serialises
    // `selection: (1, 4)` somewhere in the tree — either as an array
    // tuple or the pretty-printed multi-line form.
    assert!(
        frame.contains("\"selection\""),
        "rendered tree must surface the selection: {frame:?}"
    );
}

// ── cross-surface modal precedence ───────────────────────────────────

#[test]
fn palette_short_circuits_search_when_both_open() {
    // While the palette is open, the search overlay must NOT see
    // keystrokes — the palette is registered ahead of the search
    // service so its modal capture wins.
    let shell = Shell::new().expect("shell boots");
    shell.with_inner_mut(|inner| {
        inner.state.overlay.command_palette.open = true;
        inner.state.search.open = true;
    });
    shell.dispatch_event(&text("x"));
    shell.with_inner(|inner| {
        assert_eq!(inner.state.overlay.command_palette.query_text(), "x");
        assert_eq!(inner.state.search.query_text(), "");
    });
}
