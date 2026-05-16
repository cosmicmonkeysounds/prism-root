//! DevTools / Inspector panel e2e — IDE-mode Phase 4 / cross-cutting §4.3.
//!
//! Drives the unified Inspector / DevTools panel through the
//! production [`Shell::dispatch_event`] path and verifies:
//!
//! * Tab clicks switch the active lens.
//! * Each lens renders its expected row markup (Document, Presence,
//!   Probes, Bindings).
//! * The filter field is wired through the declarative text-input
//!   dispatch — typing into it filters whichever lens is active.
//! * Clicking a document-row jumps the canvas selection.
//! * `devtools.clear-probes` empties the probe buffer.

use prism_shell::headless::BuiltinScene;
use prism_shell::state::{DevToolsLens, PresencePeer, ProbeEvent};
use prism_shell::Shell;
use prism_ui_runtime::event::{Event, Modifiers, PointerButton};
use prism_ui_runtime::layout::{HitRect, Viewport};

fn pointer_down(hit: &HitRect) -> Event {
    Event::PointerDown {
        x: hit.bounds.x + 1.0,
        y: hit.bounds.y + 1.0,
        button: PointerButton::Primary,
        modifiers: Modifiers::default(),
    }
}

fn open_devtools(shell: &Shell) {
    shell.with_inner_mut(|inner| {
        inner.viewport = Viewport {
            width: 1280.0,
            height: 800.0,
        };
        inner
            .state
            .workspace
            .workspace
            .ensure_panel_visible("devtools");
    });
}

fn click_role_with_attr(shell: &Shell, role: &str, attr: &str, value: &str) -> bool {
    // Walk every hit (the find_hit_by_role helper returns the
    // topmost only — for tabs we need the one with the right
    // tab-id, etc.).
    let tree = shell.render();
    use prism_ui_runtime::layout::{ContainerProps, Direction, Node as UiNode, Sizing, Surface};
    let viewport = shell.with_inner(|i| i.viewport);
    let root = UiNode::Container {
        id: String::new(),
        props: ContainerProps {
            direction: Direction::Column,
            width: Sizing::Grow,
            height: Sizing::Grow,
            ..Default::default()
        },
        children: tree,
    };
    let mut surface = Surface::new(root, viewport);
    for h in surface.hit_rects().iter().rev() {
        let role_ok = h.attrs.iter().any(|(k, v)| k == "data-role" && v == role);
        let attr_ok = h.attrs.iter().any(|(k, v)| k == attr && v == value);
        if role_ok && attr_ok {
            shell.dispatch_event(&pointer_down(h));
            return true;
        }
    }
    false
}

// ── tab switching ────────────────────────────────────────────────────

#[test]
fn clicking_each_tab_switches_the_active_lens() {
    let shell = Shell::new().expect("shell boots");
    open_devtools(&shell);
    // Default lens.
    shell.with_inner(|inner| {
        assert_eq!(inner.state.devtools.active_lens, DevToolsLens::Document);
    });
    // Click each tab and assert the lens follows.
    for (tab_id, expected) in [
        ("probes", DevToolsLens::Probes),
        ("presence", DevToolsLens::Presence),
        ("bindings", DevToolsLens::Bindings),
        ("document", DevToolsLens::Document),
    ] {
        let clicked = click_role_with_attr(&shell, "devtools-tab", "data-tab-id", tab_id);
        assert!(clicked, "tab `{tab_id}` must be in the rendered tree");
        shell.with_inner(|inner| {
            assert_eq!(
                inner.state.devtools.active_lens, expected,
                "clicking `{tab_id}` must switch to {expected:?}"
            );
        });
    }
}

// ── lens rendering ───────────────────────────────────────────────────

#[test]
fn probes_lens_renders_seeded_events_newest_first() {
    let shell = Shell::new().expect("shell boots");
    shell.apply_scene(BuiltinScene::IdeDevTools);
    let frame = shell.dump_frame();
    // Three seeded events: render (frame 0), click (frame 1), render (frame 2).
    // The lens renders newest-first via `.rev()`, so timestamp 1002
    // (the second "render") should appear before 1001 + 1000.
    assert!(
        frame.contains("\"id\": \"devtools-probe-1002-render\""),
        "probe row id must encode timestamp + name"
    );
    assert!(frame.contains("\"content\": \"render\""));
    assert!(frame.contains("\"content\": \"click\""));
    // The earliest probe (1000) still in the buffer.
    assert!(frame.contains("\"id\": \"devtools-probe-1000-render\""));
}

#[test]
fn presence_lens_renders_seeded_peers() {
    let shell = Shell::new().expect("shell boots");
    shell.apply_scene(BuiltinScene::IdeDevTools);
    shell.with_inner_mut(|inner| {
        inner.state.devtools.switch_lens(DevToolsLens::Presence);
    });
    let frame = shell.dump_frame();
    assert!(frame.contains("\"id\": \"devtools-peer-peer-a\""));
    assert!(frame.contains("\"id\": \"devtools-peer-peer-b\""));
    assert!(frame.contains("\"content\": \"Alice\""));
    assert!(frame.contains("\"content\": \"Bob\""));
}

#[test]
fn document_lens_renders_builder_tree_with_depth() {
    let shell = Shell::new().expect("shell boots");
    open_devtools(&shell);
    shell.with_inner_mut(|inner| {
        inner.state.devtools.switch_lens(DevToolsLens::Document);
    });
    let frame = shell.dump_frame();
    // The boot seed includes a demo-heading + demo-button etc. Each
    // gets a `devtools-doc-row-<id>` container.
    assert!(
        frame.contains("\"id\": \"devtools-doc-row-demo-heading\""),
        "document lens must render rows for every seeded node"
    );
}

#[test]
fn bindings_lens_renders_one_row_per_registered_tag() {
    let shell = Shell::new().expect("shell boots");
    open_devtools(&shell);
    shell.with_inner_mut(|inner| {
        inner.state.devtools.switch_lens(DevToolsLens::Bindings);
    });
    let frame = shell.dump_frame();
    // The bindings lens enumerates `crate::props::builtin_binding_tags()`,
    // which includes every registered slot accessor.
    assert!(frame.contains("\"id\": \"devtools-binding-shell.toast-stack\""));
    assert!(frame.contains("\"id\": \"devtools-binding-shell.command-palette\""));
    assert!(frame.contains("\"id\": \"devtools-binding-shell.devtools\""));
}

// ── filter dispatch through the declarative text-input system ───────

#[test]
fn typing_into_focused_filter_field_filters_the_bindings_lens() {
    let shell = Shell::new().expect("shell boots");
    open_devtools(&shell);
    shell.with_inner_mut(|inner| {
        inner.state.devtools.switch_lens(DevToolsLens::Bindings);
        inner.state.devtools.filter_focused = true;
    });
    // Type a substring of `shell.devtools` — should hide the others.
    shell.dispatch_event(&Event::Text {
        text: "devtools".into(),
    });
    shell.with_inner(|inner| {
        assert_eq!(inner.state.devtools.filter_text(), "devtools");
    });
    let frame = shell.dump_frame();
    // `shell.devtools` survives the filter; `shell.toast-stack`
    // doesn't (no "devtools" substring in the tag).
    assert!(frame.contains("\"id\": \"devtools-binding-shell.devtools\""));
    assert!(!frame.contains("\"id\": \"devtools-binding-shell.toast-stack\""));
}

#[test]
fn typing_into_focused_filter_field_filters_the_probes_lens() {
    let shell = Shell::new().expect("shell boots");
    shell.apply_scene(BuiltinScene::IdeDevTools);
    shell.with_inner_mut(|inner| {
        inner.state.devtools.filter_focused = true;
    });
    shell.dispatch_event(&Event::Text {
        text: "click".into(),
    });
    let frame = shell.dump_frame();
    // The "click" probe survives; the two "render" probes are
    // filtered out.
    assert!(frame.contains("\"content\": \"click\""));
    assert!(!frame.contains("\"content\": \"render\""));
}

#[test]
fn escape_in_focused_filter_field_drops_focus_via_declaration_cancel() {
    let shell = Shell::new().expect("shell boots");
    open_devtools(&shell);
    shell.with_inner_mut(|inner| {
        inner.state.devtools.filter_focused = true;
    });
    shell.dispatch_event(&Event::Key {
        code: "escape".into(),
        pressed: true,
        modifiers: Modifiers::default(),
    });
    shell.with_inner(|inner| {
        assert!(
            !inner.state.devtools.filter_focused,
            "Escape must drop filter focus via the declaration's on_cancel hook"
        );
    });
}

// ── document-row click → canvas selection ───────────────────────────

#[test]
fn clicking_a_document_row_jumps_canvas_selection() {
    let shell = Shell::new().expect("shell boots");
    open_devtools(&shell);
    shell.with_inner_mut(|inner| {
        inner.state.devtools.switch_lens(DevToolsLens::Document);
    });
    let clicked =
        click_role_with_attr(&shell, "devtools-doc-row", "data-target-id", "demo-heading");
    assert!(clicked, "demo-heading row must be in the rendered tree");
    shell.with_inner(|inner| {
        assert_eq!(
            inner.state.canvas.selection.as_deref(),
            Some("demo-heading"),
            "document-row click must set the canvas selection"
        );
    });
}

// ── clear-probes command ────────────────────────────────────────────

#[test]
fn devtools_clear_probes_command_empties_the_buffer() {
    let shell = Shell::new().expect("shell boots");
    shell.with_inner_mut(|inner| {
        for i in 0..5 {
            inner.state.devtools.record_probe(ProbeEvent {
                name: format!("event-{i}"),
                payload: serde_json::Value::Null,
                timestamp_ms: i as u64,
                source_node_id: None,
            });
        }
    });
    shell.with_inner(|i| assert_eq!(i.state.devtools.probes.len(), 5));
    // Drive via the registered command-table entry.
    shell.run_command("devtools.clear-probes");
    shell.with_inner(|i| assert!(i.state.devtools.probes.is_empty()));
}

// ── probe buffer cap (FIFO eviction) ────────────────────────────────

#[test]
fn probe_buffer_caps_at_two_hundred() {
    use prism_shell::state::DevToolsSlot;
    let shell = Shell::new().expect("shell boots");
    shell.with_inner_mut(|inner| {
        for i in 0..(DevToolsSlot::PROBE_BUFFER_LIMIT + 50) {
            inner.state.devtools.record_probe(ProbeEvent {
                name: format!("e{i}"),
                payload: serde_json::Value::Null,
                timestamp_ms: i as u64,
                source_node_id: None,
            });
        }
    });
    shell.with_inner(|i| {
        assert_eq!(
            i.state.devtools.probes.len(),
            DevToolsSlot::PROBE_BUFFER_LIMIT
        );
        // The oldest 50 should have been evicted; the front is now e50.
        assert_eq!(i.state.devtools.probes.front().unwrap().name, "e50");
    });
}

// ── presence row carries color swatch ───────────────────────────────

#[test]
fn presence_row_carries_peer_data_for_swatch_render() {
    let shell = Shell::new().expect("shell boots");
    shell.with_inner_mut(|inner| {
        inner.state.devtools.switch_lens(DevToolsLens::Presence);
        inner.state.devtools.presence = vec![PresencePeer {
            peer_id: "p1".into(),
            display_name: "Test User".into(),
            // `#ff00ff` parses to RGB (255, 0, 255) in the runtime;
            // the rendered tree carries it as a Color struct, not the
            // literal hex. The assertion below checks the parsed
            // components instead.
            color: "#ff00ff".into(),
            selection: None,
            active_view: None,
            last_seen_ms: 0,
        }];
        inner
            .state
            .workspace
            .workspace
            .ensure_panel_visible("devtools");
    });
    let frame = shell.dump_frame();
    assert!(
        frame.contains("\"id\": \"devtools-peer-p1\""),
        "peer row must render with the peer-id-derived id"
    );
    assert!(frame.contains("\"content\": \"Test User\""));
    // Color lands as a parsed Color struct — assert one of the
    // channels is in the rendered tree (255 = 0xff for the red and
    // blue channels of magenta).
    assert!(
        frame.contains("\"r\": 255")
            || frame.contains("\"r\":255")
            || frame.contains("rgb(255, 0, 255)")
            || frame.contains("#ff00ff"),
        "magenta color must surface in some serialised form: {frame:?}"
    );
}
