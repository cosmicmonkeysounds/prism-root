use super::*;
use crate::shell::Shell;
use prism_ui_runtime::command::Rect;
use prism_ui_runtime::event::Modifiers;

fn hit_with(role: &str, target: &str, extra: &[(&str, &str)]) -> HitRect {
    let mut attrs = vec![
        ("data-role".to_string(), role.into()),
        ("data-target-id".to_string(), target.into()),
    ];
    for (k, v) in extra {
        attrs.push(((*k).into(), (*v).into()));
    }
    HitRect {
        id: format!("hit-{role}"),
        bounds: Rect {
            x: 0.0,
            y: 0.0,
            width: 10.0,
            height: 10.0,
        },
        attrs,
        disabled: false,
    }
}

#[test]
fn resize_updates_viewport_and_requests_redraw() {
    let shell = Shell::new().expect("boot");
    let dirty = dispatch_event(
        &shell.inner,
        &Event::Resize {
            width: 1024,
            height: 600,
        },
        None,
    );
    assert!(dirty, "resize must request a redraw");
    let vp = shell.inner.borrow().viewport;
    assert_eq!(vp.width, 1024.0);
    assert_eq!(vp.height, 600.0);
}

#[cfg(feature = "native")]
#[test]
fn data_probe_hit_records_into_devtools_probes_lens() {
    let shell = Shell::new().expect("boot");
    let before = shell.inner.borrow().state.devtools.probes.len();
    let hit = hit_with("button", "demo", &[("data-probe-click", "tap")]);
    assert!(route_probe(&shell.inner, &hit));
    let g = shell.inner.borrow();
    assert_eq!(g.state.devtools.probes.len(), before + 1);
    let ev = g.state.devtools.probes.back().unwrap();
    assert_eq!(ev.name, "click");
    // Payload carries the other data-* attrs (prefix stripped).
    assert_eq!(ev.payload["role"], serde_json::json!("button"));
    assert_eq!(ev.source_node_id.as_deref(), Some("hit-button"));
}

#[test]
fn unhandled_events_are_no_redraw() {
    let shell = Shell::new().expect("boot");
    let dirty = dispatch_event(&shell.inner, &Event::Wheel { dx: 0.0, dy: 1.0 }, None);
    assert!(!dirty);
}

#[test]
fn pointer_events_route_through_canvas_slot_under_active_tool() {
    // §22 keystone: the router knows pointer phases, the slot
    // knows the tool. A down/move/up trio against a populated
    // canvas mutates the document via `apply_gizmo_delta` without
    // the router ever growing tool-mode awareness.
    use prism_builder::{BuilderDocument, Node};
    use prism_core::foundation::spatial::Transform2D;
    use prism_ui_runtime::event::PointerButton;

    let shell = Shell::new().expect("boot");
    {
        let mut guard = shell.inner.borrow_mut();
        guard.state.canvas.document = BuilderDocument {
            root: Some(Node {
                id: "root".into(),
                component: "container".into(),
                transform: Transform2D {
                    position: [100.0, 100.0],
                    ..Default::default()
                },
                ..Default::default()
            }),
            ..Default::default()
        };
        guard.state.canvas.selection = Some("root".into());
        guard.state.canvas.tool = crate::state::ToolMode::Move;
    }
    let down = Event::PointerDown {
        x: 100.0,
        y: 100.0,
        button: PointerButton::Primary,
        modifiers: Modifiers::default(),
    };
    let mv = Event::PointerMove {
        x: 160.0,
        y: 140.0,
        modifiers: Modifiers::default(),
    };
    let up = Event::PointerUp {
        x: 160.0,
        y: 140.0,
        button: PointerButton::Primary,
        modifiers: Modifiers::default(),
    };
    assert!(
        !dispatch_event(&shell.inner, &down, None),
        "capture is silent"
    );
    assert!(
        dispatch_event(&shell.inner, &mv, None),
        "move triggers redraw"
    );
    assert!(
        dispatch_event(&shell.inner, &up, None),
        "up triggers redraw"
    );
    let pos = shell
        .inner
        .borrow()
        .state
        .canvas
        .document
        .root
        .as_ref()
        .unwrap()
        .transform
        .position;
    assert_eq!(pos, [160.0, 140.0]);
}

#[test]
fn pointer_down_on_inspector_row_selects_target_node() {
    // §43 C3: a hit on a `data-role="inspector-row"` container
    // moves the canvas selection to its `data-target-id` and
    // resyncs the builder slot. The boot seed pre-selects
    // `demo-heading`; this test asserts the selection moves to
    // `demo-paragraph` after the click.
    use prism_ui_runtime::event::PointerButton;
    let shell = Shell::new().expect("boot");
    assert_eq!(
        shell.inner.borrow().state.canvas.selection.as_deref(),
        Some("demo-heading"),
        "boot seed pre-selects the heading"
    );
    let hit = hit_with("inspector-row", "demo-paragraph", &[]);
    let dirty = dispatch_event(
        &shell.inner,
        &Event::PointerDown {
            x: 5.0,
            y: 5.0,
            button: PointerButton::Primary,
            modifiers: Modifiers::default(),
        },
        Some(hit),
    );
    assert!(dirty, "selection mutation requests a redraw");
    assert_eq!(
        shell.inner.borrow().state.canvas.selection.as_deref(),
        Some("demo-paragraph"),
    );
}

#[test]
fn pointer_down_on_boolean_field_edit_toggles_prop() {
    // §43 C2: a hit on a `data-role="field-edit"` boolean row
    // toggles the bound prop on the target doc node. The seed
    // document has a `demo-button` with no `visible` prop yet;
    // a click sets it to `false`. A second click flips it back
    // to `true`.
    use prism_ui_runtime::event::PointerButton;
    let shell = Shell::new().expect("boot");
    let hit = hit_with(
        "field-edit",
        "demo-button",
        &[
            ("data-key", "visible"),
            ("data-kind", "boolean"),
            ("data-value", "true"),
        ],
    );
    let dirty = dispatch_event(
        &shell.inner,
        &Event::PointerDown {
            x: 5.0,
            y: 5.0,
            button: PointerButton::Primary,
            modifiers: Modifiers::default(),
        },
        Some(hit),
    );
    assert!(dirty);
    let visible = shell
        .inner
        .borrow()
        .state
        .canvas
        .document
        .root
        .as_ref()
        .and_then(|r| r.find("demo-button"))
        .and_then(|n| n.props.get("visible").cloned())
        .expect("visible prop set");
    assert_eq!(visible, serde_json::Value::Bool(false));
}

/// Wave 2.3 — pointer-down on a `data-role="field-edit"` hit
/// whose kind is `select` opens the anchored dropdown overlay
/// (replaces the legacy click-to-cycle behaviour). The dropdown's
/// option rows route through `select-dropdown-option` for the
/// actual commit (see the next test for that contract).
#[test]
fn pointer_down_on_select_field_edit_opens_dropdown_overlay() {
    use prism_ui_runtime::event::PointerButton;
    let shell = Shell::new().expect("boot");
    let hit = hit_with(
        "field-edit",
        "demo-heading",
        &[
            ("data-key", "level"),
            ("data-kind", "select"),
            ("data-value", "h1"),
            ("data-options", "h1,h2,h3"),
        ],
    );
    let dirty = dispatch_event(
        &shell.inner,
        &Event::PointerDown {
            x: 5.0,
            y: 5.0,
            button: PointerButton::Primary,
            modifiers: Modifiers::default(),
        },
        Some(hit),
    );
    assert!(dirty);
    let dropdown = shell.inner.borrow().state.overlay.select_dropdown.clone();
    assert!(dropdown.open);
    assert_eq!(dropdown.target_id, "demo-heading");
    assert_eq!(dropdown.key, "level");
    assert_eq!(dropdown.value, "h1");
    assert_eq!(dropdown.options.len(), 3);
    // Verify the bare-value parsing branch (no `:label`) — when
    // `data-options` carries plain values they round-trip with
    // `value == label`.
    let first = dropdown.options[0]
        .get("value")
        .and_then(|v| v.as_str())
        .unwrap();
    assert_eq!(first, "h1");
}

/// Wave 2.3 — pointer-down on a `data-role="select-dropdown-option"`
/// commits the option's `data-value` through `set_node_prop` and
/// closes the dropdown.
#[test]
fn pointer_down_on_select_dropdown_option_commits_and_closes() {
    use prism_ui_runtime::event::PointerButton;
    let shell = Shell::new().expect("boot");
    shell.inner.borrow_mut().state.open_select_dropdown(
        "demo-heading",
        "level",
        "h1",
        vec![
            serde_json::json!({ "value": "h1", "label": "H1" }),
            serde_json::json!({ "value": "h2", "label": "H2" }),
        ],
    );
    let hit = hit_with(
        "select-dropdown-option",
        "ignored-target",
        &[("data-value", "h2")],
    );
    let dirty = dispatch_event(
        &shell.inner,
        &Event::PointerDown {
            x: 5.0,
            y: 5.0,
            button: PointerButton::Primary,
            modifiers: Modifiers::default(),
        },
        Some(hit),
    );
    assert!(dirty);
    let guard = shell.inner.borrow();
    assert!(!guard.state.overlay.select_dropdown.open);
    let level = guard
        .state
        .canvas
        .document
        .root
        .as_ref()
        .and_then(|r| r.find("demo-heading"))
        .and_then(|n| n.props.get("level").cloned())
        .expect("level prop set");
    assert_eq!(level, serde_json::Value::String("h2".into()));
}

/// Wave 2.3 — pointer-down on `data-role="select-dropdown-close"`
/// dismisses the overlay without committing.
#[test]
fn pointer_down_on_select_dropdown_close_dismisses_overlay() {
    use prism_ui_runtime::event::PointerButton;
    let shell = Shell::new().expect("boot");
    shell.inner.borrow_mut().state.open_select_dropdown(
        "demo-heading",
        "level",
        "h1",
        vec![serde_json::json!({ "value": "h1", "label": "H1" })],
    );
    let hit = hit_with("select-dropdown-close", "", &[]);
    let dirty = dispatch_event(
        &shell.inner,
        &Event::PointerDown {
            x: 5.0,
            y: 5.0,
            button: PointerButton::Primary,
            modifiers: Modifiers::default(),
        },
        Some(hit),
    );
    assert!(dirty);
    assert!(!shell.inner.borrow().state.overlay.select_dropdown.open);
}

/// Wave 2.4 — pointer-down on a `data-role="color-swatch"` hit
/// opens the color-picker overlay against the swatch's
/// (target-id, key, value) triple. The overlay is rendered by
/// the bound `shell.color-picker` block on the next frame.
#[test]
fn pointer_down_on_color_swatch_opens_color_picker() {
    use prism_ui_runtime::event::PointerButton;
    let shell = Shell::new().expect("boot");
    let hit = hit_with(
        "color-swatch",
        "demo-heading",
        &[("data-key", "color"), ("data-value", "#ff0000")],
    );
    let dirty = dispatch_event(
        &shell.inner,
        &Event::PointerDown {
            x: 5.0,
            y: 5.0,
            button: PointerButton::Primary,
            modifiers: Modifiers::default(),
        },
        Some(hit),
    );
    assert!(dirty);
    let picker = &shell.inner.borrow().state.overlay.color_picker;
    assert!(picker.open);
    assert_eq!(picker.target_id, "demo-heading");
    assert_eq!(picker.key, "color");
    assert_eq!(picker.value, "#ff0000");
}

/// Wave 2.4 — pointer-down on a `data-role="color-preset-select"`
/// hit commits the preset's `data-color` through `set_node_prop`,
/// leaving the picker open for further preview.
#[test]
fn pointer_down_on_color_preset_commits_through_set_node_prop() {
    use prism_ui_runtime::event::PointerButton;
    let shell = Shell::new().expect("boot");
    shell
        .inner
        .borrow_mut()
        .state
        .open_color_picker("demo-heading", "color", "#000000");
    let hit = hit_with(
        "color-preset-select",
        "ignored-target",
        &[("data-color", "#0060c0")],
    );
    let dirty = dispatch_event(
        &shell.inner,
        &Event::PointerDown {
            x: 5.0,
            y: 5.0,
            button: PointerButton::Primary,
            modifiers: Modifiers::default(),
        },
        Some(hit),
    );
    assert!(dirty);
    let guard = shell.inner.borrow();
    // Picker stays open so the user can preview multiple presets.
    assert!(guard.state.overlay.color_picker.open);
    assert_eq!(guard.state.overlay.color_picker.value, "#0060c0");
    // Doc node received the new color.
    let color = guard
        .state
        .canvas
        .document
        .root
        .as_ref()
        .and_then(|r| r.find("demo-heading"))
        .and_then(|n| n.props.get("color").cloned())
        .expect("color prop set");
    assert_eq!(color, serde_json::Value::String("#0060c0".into()));
}

/// Wave 2.4 — pointer-down on `data-role="color-picker-close"`
/// dismisses the overlay without committing.
#[test]
fn pointer_down_on_color_picker_close_dismisses_overlay() {
    use prism_ui_runtime::event::PointerButton;
    let shell = Shell::new().expect("boot");
    shell
        .inner
        .borrow_mut()
        .state
        .open_color_picker("demo-heading", "color", "#000000");
    let hit = hit_with("color-picker-close", "", &[]);
    let dirty = dispatch_event(
        &shell.inner,
        &Event::PointerDown {
            x: 5.0,
            y: 5.0,
            button: PointerButton::Primary,
            modifiers: Modifiers::default(),
        },
        Some(hit),
    );
    assert!(dirty);
    assert!(!shell.inner.borrow().state.overlay.color_picker.open);
}

/// Number field-edit shape under the B4 drag-scrubber: pointer-down
/// opens a scrub session (no prop write yet), pointer-up with no
/// pointer-move in between falls through to the legacy `+1` step.
/// The test sends the full {down, up} pair to exercise the click
/// path through to the prop mutation.
#[test]
fn click_on_number_field_edit_increments_clamped_to_max() {
    use prism_builder::Node;
    use prism_ui_runtime::event::PointerButton;
    let shell = Shell::new().expect("boot");
    {
        let mut guard = shell.inner.borrow_mut();
        let g = &mut *guard;
        let root = g.state.canvas.document.root.as_mut().expect("canvas root");
        root.children.push(Node {
            id: "num-target".into(),
            component: prism_builder::ComponentId::from("text"),
            props: serde_json::json!({ "count": 4.0 }),
            children: vec![],
            layout_mode: Default::default(),
            transform: Default::default(),
            modifiers: vec![],
            style: Default::default(),
        });
    }
    let hit = hit_with(
        "field-edit",
        "num-target",
        &[
            ("data-key", "count"),
            ("data-kind", "number"),
            ("data-value", "4"),
            ("data-max", "5"),
        ],
    );
    let _ = dispatch_event(
        &shell.inner,
        &Event::PointerDown {
            x: 5.0,
            y: 5.0,
            button: PointerButton::Primary,
            modifiers: Modifiers::default(),
        },
        Some(hit.clone()),
    );
    // Pointer-up at the same coordinates → no drag → click step.
    let _ = dispatch_event(
        &shell.inner,
        &Event::PointerUp {
            x: 5.0,
            y: 5.0,
            button: PointerButton::Primary,
            modifiers: Modifiers::default(),
        },
        Some(hit),
    );
    let count = shell
        .inner
        .borrow()
        .state
        .canvas
        .document
        .root
        .as_ref()
        .and_then(|r| r.find("num-target"))
        .and_then(|n| n.props.get("count").cloned())
        .expect("count prop set");
    assert_eq!(count.as_f64(), Some(5.0));
    // Click again: hit value attribute still reflects pre-state but
    // dispatch uses the attr's value — re-fire confirms the clamp
    // sticks (5+1 → max-clamped to 5).
    let hit2 = hit_with(
        "field-edit",
        "num-target",
        &[
            ("data-key", "count"),
            ("data-kind", "number"),
            ("data-value", "5"),
            ("data-max", "5"),
        ],
    );
    let _ = dispatch_event(
        &shell.inner,
        &Event::PointerDown {
            x: 5.0,
            y: 5.0,
            button: PointerButton::Primary,
            modifiers: Modifiers::default(),
        },
        Some(hit2.clone()),
    );
    let _ = dispatch_event(
        &shell.inner,
        &Event::PointerUp {
            x: 5.0,
            y: 5.0,
            button: PointerButton::Primary,
            modifiers: Modifiers::default(),
        },
        Some(hit2),
    );
    let count2 = shell
        .inner
        .borrow()
        .state
        .canvas
        .document
        .root
        .as_ref()
        .and_then(|r| r.find("num-target"))
        .and_then(|n| n.props.get("count").cloned())
        .expect("count prop set");
    assert_eq!(count2.as_f64(), Some(5.0));
}

#[test]
fn click_on_integer_field_edit_emits_integer_json() {
    use prism_builder::Node;
    use prism_ui_runtime::event::PointerButton;
    let shell = Shell::new().expect("boot");
    {
        let mut guard = shell.inner.borrow_mut();
        let g = &mut *guard;
        let root = g.state.canvas.document.root.as_mut().expect("canvas root");
        root.children.push(Node {
            id: "int-target".into(),
            component: prism_builder::ComponentId::from("text"),
            props: serde_json::json!({ "ord": 2 }),
            children: vec![],
            layout_mode: Default::default(),
            transform: Default::default(),
            modifiers: vec![],
            style: Default::default(),
        });
    }
    let hit = hit_with(
        "field-edit",
        "int-target",
        &[
            ("data-key", "ord"),
            ("data-kind", "integer"),
            ("data-value", "2"),
        ],
    );
    let _ = dispatch_event(
        &shell.inner,
        &Event::PointerDown {
            x: 5.0,
            y: 5.0,
            button: PointerButton::Primary,
            modifiers: Modifiers::default(),
        },
        Some(hit.clone()),
    );
    let _ = dispatch_event(
        &shell.inner,
        &Event::PointerUp {
            x: 5.0,
            y: 5.0,
            button: PointerButton::Primary,
            modifiers: Modifiers::default(),
        },
        Some(hit),
    );
    let ord = shell
        .inner
        .borrow()
        .state
        .canvas
        .document
        .root
        .as_ref()
        .and_then(|r| r.find("int-target"))
        .and_then(|n| n.props.get("ord").cloned())
        .expect("ord prop set");
    // Integer kind keeps it integral, not a float — important because
    // serde round-trips treat the two differently.
    assert_eq!(ord, serde_json::Value::from(3i64));
}

/// Drag-scrub variant: pointer-down opens a session, pointer-move
/// past the threshold mutates the prop, pointer-up ends the
/// session *without* dispatching the click-step (because `moved`
/// is true).
#[test]
fn drag_on_number_field_edit_scrubs_value_proportional_to_delta() {
    use prism_builder::Node;
    use prism_ui_runtime::event::PointerButton;
    let shell = Shell::new().expect("boot");
    {
        let mut guard = shell.inner.borrow_mut();
        let g = &mut *guard;
        let root = g.state.canvas.document.root.as_mut().expect("canvas root");
        root.children.push(Node {
            id: "scrub-target".into(),
            component: prism_builder::ComponentId::from("text"),
            props: serde_json::json!({ "count": 10.0 }),
            children: vec![],
            layout_mode: Default::default(),
            transform: Default::default(),
            modifiers: vec![],
            style: Default::default(),
        });
    }
    let hit = HitRect {
        id: "fe".into(),
        bounds: Rect {
            x: 100.0,
            y: 0.0,
            width: 80.0,
            height: 24.0,
        },
        attrs: vec![
            ("data-role".into(), "field-edit".into()),
            ("data-target-id".into(), "scrub-target".into()),
            ("data-key".into(), "count".into()),
            ("data-kind".into(), "number".into()),
            ("data-value".into(), "10".into()),
        ],
        disabled: false,
    };
    let _ = dispatch_event(
        &shell.inner,
        &Event::PointerDown {
            x: 140.0,
            y: 12.0,
            button: PointerButton::Primary,
            modifiers: Modifiers::default(),
        },
        Some(hit.clone()),
    );
    // Move +40px → 40/4 = +10 → value should be 20.0.
    let _ = dispatch_event(
        &shell.inner,
        &Event::PointerMove {
            x: 180.0,
            y: 12.0,
            modifiers: Modifiers::default(),
        },
        Some(hit.clone()),
    );
    let _ = dispatch_event(
        &shell.inner,
        &Event::PointerUp {
            x: 180.0,
            y: 12.0,
            button: PointerButton::Primary,
            modifiers: Modifiers::default(),
        },
        Some(hit),
    );
    let count = shell
        .inner
        .borrow()
        .state
        .canvas
        .document
        .root
        .as_ref()
        .and_then(|r| r.find("scrub-target"))
        .and_then(|n| n.props.get("count").cloned())
        .expect("count prop set");
    assert_eq!(count.as_f64(), Some(20.0));
    // Drag session must have ended.
    assert!(shell.inner.borrow().state.number_drag.is_none());
}

#[test]
fn click_on_text_field_edit_opens_focus_session() {
    use prism_ui_runtime::event::PointerButton;
    let shell = Shell::new().expect("boot");
    let hit = hit_with(
        "field-edit",
        "demo-heading",
        &[
            ("data-key", "body"),
            ("data-kind", "text"),
            ("data-value", "Welcome to Studio"),
        ],
    );
    let dirty = dispatch_event(
        &shell.inner,
        &Event::PointerDown {
            x: 5.0,
            y: 5.0,
            button: PointerButton::Primary,
            modifiers: Modifiers::default(),
        },
        Some(hit),
    );
    assert!(dirty, "opening focus requests a redraw");
    let focus = shell
        .inner
        .borrow()
        .state
        .field_focus
        .clone()
        .expect("focus opened");
    assert_eq!(focus.target_id, "demo-heading");
    assert_eq!(focus.key, "body");
    assert_eq!(focus.kind, "text");
    // Original is the current prop value — restored on Esc.
    assert_eq!(focus.original, "Welcome to Studio");
}

#[test]
fn click_elsewhere_commits_active_focus_session() {
    use prism_ui_runtime::event::PointerButton;
    let shell = Shell::new().expect("boot");
    // Open focus on the heading body.
    let open = hit_with(
        "field-edit",
        "demo-heading",
        &[
            ("data-key", "body"),
            ("data-kind", "text"),
            ("data-value", "Welcome to Studio"),
        ],
    );
    let _ = dispatch_event(
        &shell.inner,
        &Event::PointerDown {
            x: 5.0,
            y: 5.0,
            button: PointerButton::Primary,
            modifiers: Modifiers::default(),
        },
        Some(open),
    );
    assert!(shell.inner.borrow().state.field_focus.is_some());
    // Click on an unrelated chrome row.
    let blur_hit = hit_with("inspector-row", "demo-paragraph", &[]);
    let _ = dispatch_event(
        &shell.inner,
        &Event::PointerDown {
            x: 5.0,
            y: 5.0,
            button: PointerButton::Primary,
            modifiers: Modifiers::default(),
        },
        Some(blur_hit),
    );
    assert!(
        shell.inner.borrow().state.field_focus.is_none(),
        "clicking elsewhere commits + clears the focus"
    );
}

#[test]
fn pointer_down_with_unknown_role_falls_through_to_canvas() {
    // A hit with no recognised `data-role` doesn't mutate
    // selection / props; the §22 canvas path still gets to
    // attempt a drag capture.
    use prism_ui_runtime::event::PointerButton;
    let shell = Shell::new().expect("boot");
    let baseline = shell.inner.borrow().state.canvas.selection.clone();
    let hit = hit_with("toolbar-align-cluster", "noop", &[]);
    let _ = dispatch_event(
        &shell.inner,
        &Event::PointerDown {
            x: 5.0,
            y: 5.0,
            button: PointerButton::Primary,
            modifiers: Modifiers::default(),
        },
        Some(hit),
    );
    assert_eq!(shell.inner.borrow().state.canvas.selection, baseline);
}

/// §43 E3 — the named end-to-end verification script.
///
/// Mirrors the user-facing flow: "click Heading in palette, drop
/// in canvas, click it, edit text in properties → tree updates."
///
/// Drives a booted `Shell` through the same code paths the
/// femtovg backend hits at runtime:
///
/// 1. **Palette pick** — set `palette_selected` to the chosen
///    builtin id. The component-palette block reads this on the
///    next frame and paints the selected pill.
/// 2. **Drop in canvas** — call `insert_at_offset` to add a new
///    `text` heading node. This is the data mutation a future
///    palette→canvas drop router will call once the wiring lands;
///    the underlying mutator is the contract.
/// 3. **Click it** — dispatch a synthetic `PointerDown` with a
///    `data-role="inspector-row"` hit targeting the new node.
///    The §43 C3 router fans this through `select_node`, which
///    moves `canvas.selection` and re-derives the inspector +
///    properties.
/// 4. **Edit in properties** — dispatch a synthetic `PointerDown`
///    with a `data-role="field-edit"` boolean hit. The §43 C2
///    router toggles the bound prop on the selected doc node.
///    (Text-kind edit UX is deferred; the boolean toggle proves
///    the dispatch chain end-to-end.)
/// 5. **Tree updates** — the inspector tree carries the new node;
///    the properties form rebuilt against its schema; the edited
///    prop made it through to `doc.find(id).props`.
#[test]
fn e2e_palette_pick_drop_select_edit_updates_tree() {
    use prism_builder::Node;
    use prism_ui_runtime::event::PointerButton;
    use serde_json::json;

    let shell = Shell::new().expect("boot");

    // §43 D1 landed the registry merge in `Shell::new` itself,
    // so the boot resync already populates the right rail for
    // the pre-selected `demo-heading`. This used to require a
    // hand-rolled `for spec in BUILTINS` extension here.
    assert!(
        !shell.inner.borrow().state.builder.property_rows.is_empty(),
        "boot resync with builder builtins populates the right rail"
    );

    let initial_count = shell.inner.borrow().state.canvas.node_count();
    let new_id = "e2e-heading";

    // 1. Palette pick — the palette item "text" is the `Heading`
    //    family in the seed (heading-shaped `text` node with
    //    `level: h1`).
    {
        let mut guard = shell.inner.borrow_mut();
        guard.state.catalog.palette_selected = Some("text".into());
    }
    assert_eq!(
        shell
            .inner
            .borrow()
            .state
            .catalog
            .palette_selected
            .as_deref(),
        Some("text"),
        "palette pick records the selected builtin id"
    );

    // 2. Drop in canvas — insert a new text node into the root.
    //    The seed pre-selects `demo-heading` so the §43-style
    //    insert-after-selection lands the new node as a sibling.
    let drop_value = serde_json::to_value(Node {
        id: new_id.into(),
        component: "text".into(),
        props: json!({ "body": "Drop heading", "level": "h1" }),
        children: Vec::new(),
        layout_mode: Default::default(),
        transform: Default::default(),
        modifiers: Vec::new(),
        style: Default::default(),
    })
    .expect("serialize node");
    let inserted = {
        let mut guard = shell.inner.borrow_mut();
        guard.state.canvas.insert_at_offset(drop_value, 0)
    };
    assert!(inserted.is_some(), "drop must succeed");
    // The mutator may rewrite the id to dodge collisions; capture
    // whatever id it returned for the downstream steps.
    let landed_id = inserted.unwrap();
    assert_eq!(
        shell.inner.borrow().state.canvas.node_count(),
        initial_count + 1,
        "node-count rises by one after the drop"
    );

    // After the drop the right rail still reflects the
    // pre-selected `demo-heading`. Run one resync against the live
    // registry so the inspector tree picks up the freshly-inserted
    // row before the click finds it.
    {
        let mut guard = shell.inner.borrow_mut();
        let g = &mut *guard;
        let registry = g.registry.as_component_registry();
        g.state.resync_builder_for_selection(Some(registry));
    }
    assert!(
        shell
            .inner
            .borrow()
            .state
            .builder
            .inspector
            .iter()
            .any(|n| n.id == landed_id),
        "inspector tree must include the new node"
    );

    // 3. Click it — synthetic inspector-row PointerDown.
    let click_hit = hit_with("inspector-row", &landed_id, &[]);
    let dirty = dispatch_event(
        &shell.inner,
        &Event::PointerDown {
            x: 0.0,
            y: 0.0,
            button: PointerButton::Primary,
            modifiers: Modifiers::default(),
        },
        Some(click_hit),
    );
    assert!(dirty, "inspector-row click requests a redraw");
    assert_eq!(
        shell.inner.borrow().state.canvas.selection.as_deref(),
        Some(landed_id.as_str()),
        "selection moves to the newly-dropped node"
    );

    // Property rows now match the `text` schema for the new node.
    {
        let guard = shell.inner.borrow();
        let rows = &guard.state.builder.property_rows;
        assert!(
            !rows.is_empty(),
            "property rows must rebuild for the new selection"
        );
        assert_eq!(rows[0].component, "shell.section-header");
        let keys: Vec<&str> = rows
            .iter()
            .filter(|r| r.component == "shell.field-editor")
            .filter_map(|r| r.props.get("key").and_then(|v| v.as_str()))
            .collect();
        assert!(
            keys.contains(&"body"),
            "text schema must include `body`, got {keys:?}"
        );
    }

    // 4. Edit in properties — synthetic field-edit PointerDown.
    //    Boolean kind is the only kind wired through the event
    //    router today; the underlying `set_node_prop` is the same
    //    mutator that text/number/select edits will call when
    //    their UX lands. We target an arbitrary `visible` key —
    //    not on the `text` schema, so the write proves the
    //    mutator path through pure dispatch.
    let edit_hit = hit_with(
        "field-edit",
        &landed_id,
        &[
            ("data-key", "visible"),
            ("data-kind", "boolean"),
            ("data-value", "true"),
        ],
    );
    let dirty = dispatch_event(
        &shell.inner,
        &Event::PointerDown {
            x: 0.0,
            y: 0.0,
            button: PointerButton::Primary,
            modifiers: Modifiers::default(),
        },
        Some(edit_hit),
    );
    assert!(dirty, "field-edit click requests a redraw");

    // 5. Tree updates — the prop write reached the doc node, and
    //    the inspector still flags it as selected.
    let guard = shell.inner.borrow();
    let landed = guard
        .state
        .canvas
        .document
        .root
        .as_ref()
        .and_then(|r| r.find(&landed_id))
        .expect("new node still in tree");
    assert_eq!(
        landed.props.get("visible").cloned(),
        Some(serde_json::Value::Bool(false)),
        "field-edit toggle landed on the doc node"
    );
    let selected_row = guard
        .state
        .builder
        .inspector
        .iter()
        .find(|n| n.selected)
        .expect("inspector still flags a selection");
    assert_eq!(
        selected_row.id, landed_id,
        "selected inspector row is the edited node"
    );
}

/// §43 B5: a pointer-down on a `data-canvas-node`-tagged container
/// moves the canvas selection to that node and re-derives the
/// inspector tree. The hit is shaped like the runtime would emit
/// it for a button rendered inside the builder canvas.
#[test]
fn pointer_down_on_canvas_node_routes_to_select_node() {
    use prism_builder::Node;
    use prism_ui_runtime::event::PointerButton;
    use serde_json::json;

    let shell = Shell::new().expect("boot");

    // Extend the seeded canvas doc with a button sibling so we can
    // assert selection moves *between* nodes (the boot already
    // pre-selects `demo-heading`).
    let button_id = "canvas-click-target".to_string();
    {
        let mut guard = shell.inner.borrow_mut();
        let g = &mut *guard;
        let root = g.state.canvas.document.root.as_mut().expect("canvas root");
        root.children.push(Node {
            id: button_id.clone(),
            component: prism_builder::ComponentId::from("button"),
            props: json!({ "label": "click me" }),
            children: vec![],
            layout_mode: Default::default(),
            transform: Default::default(),
            modifiers: vec![],
            style: Default::default(),
        });
    }
    // Sanity-check the pre-condition: the boot selection is
    // `demo-heading`, not our new button.
    assert_ne!(
        shell.inner.borrow().state.canvas.selection.as_deref(),
        Some(button_id.as_str()),
    );

    let hit = HitRect {
        id: button_id.clone(),
        bounds: Rect {
            x: 0.0,
            y: 0.0,
            width: 80.0,
            height: 24.0,
        },
        attrs: vec![
            ("data-role".into(), "button".into()),
            ("data-canvas-node".into(), button_id.clone()),
        ],
        disabled: false,
    };
    let dirty = dispatch_event(
        &shell.inner,
        &Event::PointerDown {
            x: 10.0,
            y: 10.0,
            button: PointerButton::Primary,
            modifiers: Modifiers::default(),
        },
        Some(hit),
    );
    assert!(dirty, "canvas-node click moved the selection → redraw");
    assert_eq!(
        shell.inner.borrow().state.canvas.selection.as_deref(),
        Some(button_id.as_str()),
        "selection follows the clicked canvas node"
    );
}

/// §43 B5: a hit whose id matches a chrome container ("root" is
/// the canonical collision — both `<shell.app-window>` and the
/// canvas `BuilderDocument::page_shell()` use it) must NOT move
/// the canvas selection. The `data-canvas-node` attribute is the
/// disambiguator.
#[test]
fn pointer_down_on_chrome_container_does_not_select_canvas_node() {
    use prism_ui_runtime::event::PointerButton;

    let shell = Shell::new().expect("boot");
    let baseline = shell.inner.borrow().state.canvas.selection.clone();
    // The hit shape mirrors `<shell.app-window id="root">` — same id
    // as the canvas root, but no `data-canvas-node` attribute, so
    // routing must leave the selection alone.
    let hit = HitRect {
        id: "root".into(),
        bounds: Rect {
            x: 0.0,
            y: 0.0,
            width: 1280.0,
            height: 800.0,
        },
        attrs: vec![],
        disabled: false,
    };
    let _ = dispatch_event(
        &shell.inner,
        &Event::PointerDown {
            x: 100.0,
            y: 100.0,
            button: PointerButton::Primary,
            modifiers: Modifiers::default(),
        },
        Some(hit),
    );
    assert_eq!(
        shell.inner.borrow().state.canvas.selection,
        baseline,
        "chrome click left the canvas selection alone"
    );
}

/// §43 A1: a hit carrying `data-on-click="cmd <id>"` invokes the
/// matching shell command. Verifies the parse → execute chain
/// end-to-end through the same command table the palette uses.
#[test]
fn pointer_down_on_data_on_click_cmd_runs_the_shell_command() {
    use prism_ui_runtime::event::PointerButton;

    let shell = Shell::new().expect("boot");
    // `signals.fire-mounted` is a registered command (see
    // `SignalsService::commands`); pick it because it's
    // side-effect-light. Pre-state: command palette idle.
    // Post-condition: the command must be resolvable through
    // the shared CommandTable, which is what `route_on_click`
    // dispatches into. We don't observe state here — the
    // command's body fires `mounted` on the current selection,
    // which is fine.
    let hit = HitRect {
        id: "demo-heading".into(),
        bounds: Rect {
            x: 0.0,
            y: 0.0,
            width: 80.0,
            height: 24.0,
        },
        attrs: vec![("data-on-click".into(), "cmd signals.fire-mounted".into())],
        disabled: false,
    };
    let dirty = dispatch_event(
        &shell.inner,
        &Event::PointerDown {
            x: 10.0,
            y: 10.0,
            button: PointerButton::Primary,
            modifiers: Modifiers::default(),
        },
        Some(hit),
    );
    // The command ran (it requested a redraw via the standard
    // mutating path) — the precise downstream effects matter
    // less than proving the dispatch chain reached the command.
    assert!(
        dirty,
        "registered command dispatched through data-on-click should request a redraw"
    );
}

/// §43 A1: `data-on-click="emit <signal>"` fires the signal on
/// the hit's container id, which cascades through any matching
/// canvas-doc connections. We seed a single connection so the
/// dispatch chain has a concrete sink: `clicked` on `btn` →
/// `set-property visible=false` on `target`. After the click,
/// the target node's `visible` prop should flip.
#[test]
fn pointer_down_on_data_on_click_emit_cascades_through_canvas_connections() {
    use prism_builder::{ActionKind, Connection, Node};
    use prism_ui_runtime::event::PointerButton;
    use serde_json::{json, Value};

    let shell = Shell::new().expect("boot");
    // Seed a button + target node + the connection that wires
    // the click to a visibility toggle. The exact action shape
    // doesn't matter — we just need an observable mutation we
    // can probe after `dispatch_event`.
    {
        let mut guard = shell.inner.borrow_mut();
        let g = &mut *guard;
        let root = g.state.canvas.document.root.as_mut().expect("canvas root");
        root.children.push(Node {
            id: "target".into(),
            component: prism_builder::ComponentId::from("container"),
            props: json!({ "visible": true }),
            children: vec![],
            layout_mode: Default::default(),
            transform: Default::default(),
            modifiers: vec![],
            style: Default::default(),
        });
        g.state.canvas.document.connections.push(Connection {
            id: "c-toggle".into(),
            source_node: "btn".into(),
            signal: "clicked".into(),
            target_node: "target".into(),
            action: ActionKind::SetProperty {
                key: "visible".into(),
                value: Value::Bool(false),
            },
            params: json!({}),
        });
    }
    let hit = HitRect {
        id: "btn".into(),
        bounds: Rect {
            x: 0.0,
            y: 0.0,
            width: 80.0,
            height: 24.0,
        },
        attrs: vec![("data-on-click".into(), "emit clicked".into())],
        disabled: false,
    };
    let dirty = dispatch_event(
        &shell.inner,
        &Event::PointerDown {
            x: 10.0,
            y: 10.0,
            button: PointerButton::Primary,
            modifiers: Modifiers::default(),
        },
        Some(hit),
    );
    assert!(dirty, "an emit that matched a connection requests a redraw");
    let guard = shell.inner.borrow();
    let target = guard
        .state
        .canvas
        .document
        .root
        .as_ref()
        .and_then(|r| r.find("target"))
        .expect("target still in tree");
    assert_eq!(
        target.props.get("visible"),
        Some(&Value::Bool(false)),
        "emit cascade ran the connection's SetProperty action"
    );
}

/// Wave 14.3 — modifier suffix parser splits a dash-joined chain
/// into a typed bool set. Order doesn't matter; unknown segments
/// drop silently.
#[test]
fn event_modifier_parser_recognises_known_segments() {
    assert_eq!(EventModifiers::parse(""), EventModifiers::default());
    assert_eq!(
        EventModifiers::parse("once"),
        EventModifiers {
            once: true,
            ..Default::default()
        }
    );
    assert_eq!(
        EventModifiers::parse("once-stop-prevent"),
        EventModifiers {
            once: true,
            stop: true,
            prevent: true,
        }
    );
    assert_eq!(
        EventModifiers::parse("stop-once"),
        EventModifiers {
            once: true,
            stop: true,
            ..Default::default()
        },
        "modifier segments are commutative",
    );
    assert_eq!(
        EventModifiers::parse("once-bogus"),
        EventModifiers {
            once: true,
            ..Default::default()
        },
        "unknown segments are dropped without affecting recognised ones",
    );
}

/// `.once` fires the handler the first time, then no-ops on every
/// subsequent click against the same hit-id + attr-key pair. We
/// observe through the `signals.fire-mounted` command — its
/// per-frame redraw signal is the proxy for "did the handler run?"
#[test]
fn pointer_down_on_data_on_click_once_fires_only_first_time() {
    use prism_ui_runtime::event::PointerButton;

    let shell = Shell::new().expect("boot");
    let hit = || HitRect {
        id: "demo-once".into(),
        bounds: Rect {
            x: 0.0,
            y: 0.0,
            width: 80.0,
            height: 24.0,
        },
        attrs: vec![(
            "data-on-click-once".into(),
            "cmd signals.fire-mounted".into(),
        )],
        disabled: false,
    };
    let press = || Event::PointerDown {
        x: 10.0,
        y: 10.0,
        button: PointerButton::Primary,
        modifiers: Modifiers::default(),
    };
    let dirty_first = dispatch_event(&shell.inner, &press(), Some(hit()));
    assert!(
        dirty_first,
        "first dispatch should fire the once-gated command"
    );
    // Once-fired registry should now contain the (hit-id, attr-key).
    assert!(shell
        .inner
        .borrow()
        .state
        .once_fired
        .contains(&("demo-once".to_string(), "data-on-click-once".to_string())));
    let dirty_second = dispatch_event(&shell.inner, &press(), Some(hit()));
    assert!(
        !dirty_second,
        "second dispatch must be a no-op — `.once` removes the handler"
    );
}

/// `.stop` returns `true` from `route_on_click` regardless of
/// whether any connection fired — the rest of the pointer-down
/// fallback chain (canvas selection, palette drag) is suppressed.
/// We prove this by clicking a `data-on-click-stop` attr whose
/// action emits a signal nobody subscribed to: without `.stop`
/// that would fall through to the canvas. With `.stop`, the
/// router consumes the press and reports dirty.
#[test]
fn pointer_down_on_data_on_click_stop_consumes_even_without_fire() {
    use prism_ui_runtime::event::PointerButton;

    let shell = Shell::new().expect("boot");
    let hit = HitRect {
        id: "demo-stop".into(),
        bounds: Rect {
            x: 0.0,
            y: 0.0,
            width: 80.0,
            height: 24.0,
        },
        attrs: vec![(
            "data-on-click-stop".into(),
            // No connection wired — `fire_signal` returns 0.
            "emit nobody-listens".into(),
        )],
        disabled: false,
    };
    let dirty = dispatch_event(
        &shell.inner,
        &Event::PointerDown {
            x: 10.0,
            y: 10.0,
            button: PointerButton::Primary,
            modifiers: Modifiers::default(),
        },
        Some(hit),
    );
    assert!(
        dirty,
        "`.stop` consumes the press so the router reports redraw"
    );
}

/// Bare `on:click` (no modifier) preserves the legacy semantics:
/// when the action fires no observable mutation, the router
/// returns `false` so the rest of the pointer-down chain runs.
/// Pair with the `.stop` test above to prove the modifier is the
/// thing that flipped the consume bit.
#[test]
fn pointer_down_on_bare_data_on_click_with_no_subscriber_falls_through() {
    use prism_ui_runtime::event::PointerButton;

    let shell = Shell::new().expect("boot");
    let hit = HitRect {
        id: "demo-bare".into(),
        bounds: Rect {
            x: 0.0,
            y: 0.0,
            width: 80.0,
            height: 24.0,
        },
        attrs: vec![("data-on-click".into(), "emit nobody-listens".into())],
        disabled: false,
    };
    let dirty = dispatch_event(
        &shell.inner,
        &Event::PointerDown {
            x: 10.0,
            y: 10.0,
            button: PointerButton::Primary,
            modifiers: Modifiers::default(),
        },
        Some(hit),
    );
    // Falls through to canvas pointer_down — which on an empty
    // doc + no selection is itself a no-op, so `dirty` stays
    // false. The contract under test is `route_on_click` returning
    // false; observing dirty is the visible witness.
    assert!(
        !dirty,
        "bare on:click without a connection nor canvas hit must not consume"
    );
}

/// Wave 14.3 — clicking an `<input>` whose `bind:value`
/// resolves to `<node-id>.<key>` opens a field-focus session
/// against that doc node. The subsequent `Event::Text` then
/// flows through `FieldFocusService` and writes back to the
/// node's prop via `set_node_prop`.
#[test]
fn pointer_down_on_input_with_bind_value_opens_field_focus() {
    use prism_builder::Node;
    use prism_ui_runtime::event::PointerButton;
    use serde_json::{json, Value};

    let shell = Shell::new().expect("boot");
    {
        let mut guard = shell.inner.borrow_mut();
        let root = guard
            .state
            .canvas
            .document
            .root
            .as_mut()
            .expect("canvas root");
        root.children.push(Node {
            id: "form-email".into(),
            component: prism_builder::ComponentId::from("container"),
            props: json!({ "value": "" }),
            children: vec![],
            layout_mode: Default::default(),
            transform: Default::default(),
            modifiers: vec![],
            style: Default::default(),
        });
    }
    let hit = HitRect {
        id: "email-input".into(),
        bounds: Rect {
            x: 0.0,
            y: 0.0,
            width: 200.0,
            height: 24.0,
        },
        attrs: vec![("data-bind-value".into(), "form-email.value".into())],
        disabled: false,
    };
    let dirty = dispatch_event(
        &shell.inner,
        &Event::PointerDown {
            x: 10.0,
            y: 10.0,
            button: PointerButton::Primary,
            modifiers: Modifiers::default(),
        },
        Some(hit),
    );
    assert!(dirty, "bind:value click should request a redraw");
    let focus = shell
        .inner
        .borrow()
        .state
        .field_focus
        .clone()
        .expect("field focus session active");
    assert_eq!(focus.target_id, "form-email");
    assert_eq!(focus.key, "value");
    assert_eq!(focus.kind, "text");

    // Now simulate a keystroke landing on the focused input.
    let dirty = dispatch_event(&shell.inner, &Event::Text { text: "hi".into() }, None);
    assert!(dirty, "typed text writes back through field-focus");
    let val = shell
        .inner
        .borrow()
        .state
        .canvas
        .document
        .root
        .as_ref()
        .and_then(|r| r.find("form-email"))
        .and_then(|n| n.props.get("value").cloned())
        .unwrap_or(Value::Null);
    assert_eq!(val, Value::String("hi".into()));
}

/// Clicking an input whose bind path points at a non-existent
/// node is a clean no-op — the router falls through to the
/// canvas chain instead of getting wedged on a phantom focus.
#[test]
fn pointer_down_on_input_with_bogus_bind_path_falls_through() {
    use prism_ui_runtime::event::PointerButton;
    let shell = Shell::new().expect("boot");
    let hit = HitRect {
        id: "input-x".into(),
        bounds: Rect {
            x: 0.0,
            y: 0.0,
            width: 200.0,
            height: 24.0,
        },
        attrs: vec![("data-bind-value".into(), "missing-node.value".into())],
        disabled: false,
    };
    dispatch_event(
        &shell.inner,
        &Event::PointerDown {
            x: 10.0,
            y: 10.0,
            button: PointerButton::Primary,
            modifiers: Modifiers::default(),
        },
        Some(hit),
    );
    assert!(shell.inner.borrow().state.field_focus.is_none());
}

#[test]
fn pointer_down_on_workflow_page_button_switches_active_page() {
    use prism_ui_runtime::event::PointerButton;
    let shell = Shell::new().expect("boot");
    // Boot lands on the first page (index 0). Pick a non-active
    // page id so the click actually moves the state.
    let pages: Vec<String> = shell
        .inner
        .borrow()
        .state
        .workspace
        .workspace
        .pages()
        .iter()
        .map(|p| p.id.clone())
        .collect();
    let target = pages
        .get(1)
        .expect("workspace has at least two pages")
        .clone();
    let hit = hit_with("workflow-page-button", &target, &[]);
    let dirty = dispatch_event(
        &shell.inner,
        &Event::PointerDown {
            x: 5.0,
            y: 5.0,
            button: PointerButton::Primary,
            modifiers: Modifiers::default(),
        },
        Some(hit),
    );
    assert!(dirty, "switching pages requests a redraw");
    assert_eq!(
        shell
            .inner
            .borrow()
            .state
            .workspace
            .workspace
            .active_page()
            .id,
        target,
    );
}

#[test]
fn pointer_down_on_dock_tab_activates_panel_inside_its_tab_group() {
    use prism_ui_runtime::event::PointerButton;
    let shell = Shell::new().expect("boot");
    // Pick a panel id that exists somewhere in the workspace —
    // `inspector` ships in the default "edit" page.
    let target = "inspector".to_string();
    let hit = hit_with("dock-tab", &target, &[]);
    let dirty = dispatch_event(
        &shell.inner,
        &Event::PointerDown {
            x: 5.0,
            y: 5.0,
            button: PointerButton::Primary,
            modifiers: Modifiers::default(),
        },
        Some(hit),
    );
    // `navigate_to_panel` returns true when the panel exists
    // anywhere in the workspace, so any click on a real panel id
    // should request a redraw.
    assert!(dirty, "panel navigation requests a redraw");
}

#[test]
fn pointer_down_on_device_pill_switches_canvas_device() {
    use prism_ui_runtime::event::PointerButton;
    let shell = Shell::new().expect("boot");
    assert_eq!(
        shell.inner.borrow().state.canvas.device,
        crate::state::Device::Desktop,
        "boot defaults to Desktop"
    );
    let hit = HitRect {
        id: "pill".into(),
        bounds: prism_ui_runtime::command::Rect {
            x: 0.0,
            y: 0.0,
            width: 60.0,
            height: 24.0,
        },
        attrs: vec![
            ("data-role".into(), "toolbar-device-pill".into()),
            ("data-device".into(), "tablet".into()),
        ],
        disabled: false,
    };
    let dirty = dispatch_event(
        &shell.inner,
        &Event::PointerDown {
            x: 5.0,
            y: 5.0,
            button: PointerButton::Primary,
            modifiers: Modifiers::default(),
        },
        Some(hit),
    );
    assert!(dirty, "device switch requests a redraw");
    assert_eq!(
        shell.inner.borrow().state.canvas.device,
        crate::state::Device::Tablet,
    );
}

#[test]
fn pointer_down_on_zoom_reset_pill_resets_canvas_zoom() {
    use prism_ui_runtime::event::PointerButton;
    let shell = Shell::new().expect("boot");
    {
        let mut guard = shell.inner.borrow_mut();
        guard.state.canvas.viewport.zoom = 2.5;
    }
    let hit = HitRect {
        id: "pill".into(),
        bounds: prism_ui_runtime::command::Rect {
            x: 0.0,
            y: 0.0,
            width: 40.0,
            height: 24.0,
        },
        attrs: vec![("data-role".into(), "toolbar-zoom-reset".into())],
        disabled: false,
    };
    let dirty = dispatch_event(
        &shell.inner,
        &Event::PointerDown {
            x: 5.0,
            y: 5.0,
            button: PointerButton::Primary,
            modifiers: Modifiers::default(),
        },
        Some(hit),
    );
    assert!(dirty, "zoom reset requests a redraw");
    assert_eq!(shell.inner.borrow().state.canvas.viewport.zoom, 1.0);
}

#[test]
fn pointer_down_on_nav_page_row_moves_active_nav_page() {
    use crate::state::{NavPage, NavigationSlot};
    use prism_ui_runtime::event::PointerButton;
    let shell = Shell::new().expect("boot");
    {
        let mut guard = shell.inner.borrow_mut();
        guard.state.navigation = NavigationSlot {
            pages: vec![
                NavPage {
                    id: "home".into(),
                    title: "Home".into(),
                    route: "/".into(),
                    x: 0.0,
                    y: 0.0,
                    node_count: 0,
                    link_count: 0,
                    is_active: true,
                },
                NavPage {
                    id: "about".into(),
                    title: "About".into(),
                    route: "/about".into(),
                    x: 0.0,
                    y: 0.0,
                    node_count: 0,
                    link_count: 0,
                    is_active: false,
                },
            ],
            edges: vec![],
            ..Default::default()
        };
    }
    let hit = hit_with("nav-page-row", "about", &[]);
    let dirty = dispatch_event(
        &shell.inner,
        &Event::PointerDown {
            x: 5.0,
            y: 5.0,
            button: PointerButton::Primary,
            modifiers: Modifiers::default(),
        },
        Some(hit),
    );
    assert!(dirty, "active nav page switch requests a redraw");
    let nav = &shell.inner.borrow().state.navigation;
    assert!(nav.pages[1].is_active);
    assert!(!nav.pages[0].is_active);
}

#[test]
fn pointer_down_on_nav_page_row_also_moves_chevron_cursor() {
    use crate::state::{NavPage, NavigationSlot};
    use prism_ui_runtime::event::PointerButton;
    let shell = Shell::new().expect("boot");
    {
        let mut guard = shell.inner.borrow_mut();
        guard.state.navigation = NavigationSlot {
            pages: vec![NavPage {
                id: "home".into(),
                title: "Home".into(),
                route: "/".into(),
                x: 0.0,
                y: 0.0,
                node_count: 0,
                link_count: 0,
                is_active: true,
            }],
            edges: vec![],
            ..Default::default()
        };
    }
    let hit = hit_with("nav-page-row", "home", &[]);
    let _ = dispatch_event(
        &shell.inner,
        &Event::PointerDown {
            x: 1.0,
            y: 1.0,
            button: PointerButton::Primary,
            modifiers: Modifiers::default(),
        },
        Some(hit),
    );
    assert_eq!(
        shell
            .inner
            .borrow()
            .state
            .navigation
            .selected_page
            .as_deref(),
        Some("home"),
    );
}

#[test]
fn pointer_down_on_app_card_sets_workspace_active_app() {
    use prism_ui_runtime::event::PointerButton;
    let shell = Shell::new().expect("boot");
    let hit = HitRect {
        id: "card-lattice".into(),
        bounds: Rect {
            x: 0.0,
            y: 0.0,
            width: 160.0,
            height: 160.0,
        },
        attrs: vec![
            ("data-role".into(), "app-card".into()),
            ("data-app".into(), "lattice".into()),
        ],
        disabled: false,
    };
    let dirty = dispatch_event(
        &shell.inner,
        &Event::PointerDown {
            x: 5.0,
            y: 5.0,
            button: PointerButton::Primary,
            modifiers: Modifiers::default(),
        },
        Some(hit),
    );
    assert!(dirty, "app-card click requests a redraw");
    assert_eq!(
        shell.inner.borrow().state.workspace.active_app.as_deref(),
        Some("lattice"),
    );
}

#[test]
fn pointer_down_on_app_card_drives_full_swap_chain() {
    // ADR-009 + ADR-010 wiring: a launchpad click must flow
    // through the same `switch_active_app` path that the public
    // shell API uses — cursor update + service rebuild +
    // render scope dirty. Previously the handler only moved the
    // cursor (`WorkspaceSlot::set_active_app`), bypassing the
    // service rebuild.
    use prism_ui_runtime::event::PointerButton;
    let shell = Shell::new().expect("boot");
    // Drain any pre-existing dirty state from boot.
    let _ = shell.render();
    assert!(
        !shell.inner.borrow().render_scope.needs_redraw(),
        "render-scope should be clean after a successful render"
    );

    let hit = HitRect {
        id: "card-flux".into(),
        bounds: Rect {
            x: 0.0,
            y: 0.0,
            width: 160.0,
            height: 160.0,
        },
        attrs: vec![
            ("data-role".into(), "app-card".into()),
            ("data-app".into(), "flux".into()),
        ],
        disabled: false,
    };
    let dirty = dispatch_event(
        &shell.inner,
        &Event::PointerDown {
            x: 5.0,
            y: 5.0,
            button: PointerButton::Primary,
            modifiers: Modifiers::default(),
        },
        Some(hit),
    );
    assert!(dirty);
    // Cursor moved.
    assert_eq!(
        shell.inner.borrow().state.workspace.active_app.as_deref(),
        Some("flux"),
    );
    // Render scope was marked dirty by `switch_active_app`'s
    // FRAME_DIRTY_SENTINEL — proves the full chain fired, not
    // just the cursor write.
    assert!(
        shell.inner.borrow().render_scope.needs_redraw(),
        "click on app-card should mark the render scope dirty via switch_active_app"
    );
}

#[test]
fn pointer_down_on_already_active_app_card_is_idempotent() {
    // The full chain is idempotent: clicking the already-active
    // app's tile returns `false` from `switch_active_app` (no
    // rebuild, no dirty bump). The dispatcher still reports
    // `dirty` because clicking *something* counts as activity in
    // its conservative redraw model, but the underlying state
    // didn't move.
    use prism_ui_runtime::event::PointerButton;
    let shell = Shell::new().expect("boot");
    shell.switch_active_app(Some("musica"));
    let _ = shell.render();

    let hit = HitRect {
        id: "card-musica".into(),
        bounds: Rect {
            x: 0.0,
            y: 0.0,
            width: 160.0,
            height: 160.0,
        },
        attrs: vec![
            ("data-role".into(), "app-card".into()),
            ("data-app".into(), "musica".into()),
        ],
        disabled: false,
    };
    let _ = dispatch_event(
        &shell.inner,
        &Event::PointerDown {
            x: 5.0,
            y: 5.0,
            button: PointerButton::Primary,
            modifiers: Modifiers::default(),
        },
        Some(hit),
    );
    // Cursor stays put.
    assert_eq!(
        shell.inner.borrow().state.workspace.active_app.as_deref(),
        Some("musica"),
    );
}

#[test]
fn pointer_down_on_create_card_without_data_app_is_a_no_op() {
    use prism_ui_runtime::event::PointerButton;
    let shell = Shell::new().expect("boot");
    let hit = HitRect {
        id: "card-create".into(),
        bounds: Rect {
            x: 0.0,
            y: 0.0,
            width: 160.0,
            height: 160.0,
        },
        attrs: vec![
            ("data-role".into(), "app-card".into()),
            ("data-create".into(), "true".into()),
        ],
        disabled: false,
    };
    let _ = dispatch_event(
        &shell.inner,
        &Event::PointerDown {
            x: 5.0,
            y: 5.0,
            button: PointerButton::Primary,
            modifiers: Modifiers::default(),
        },
        Some(hit),
    );
    assert!(shell.inner.borrow().state.workspace.active_app.is_none());
}

#[test]
fn pointer_down_on_schema_row_moves_schema_field_cursor() {
    use crate::state::{SchemaDoc, SchemaField};
    use prism_ui_runtime::event::PointerButton;
    let shell = Shell::new().expect("boot");
    {
        let mut guard = shell.inner.borrow_mut();
        guard.state.builder.schema = SchemaDoc {
            fields: vec![
                SchemaField {
                    name: "title".into(),
                    kind: "text".into(),
                    required: false,
                },
                SchemaField {
                    name: "body".into(),
                    kind: "rich-text".into(),
                    required: false,
                },
            ],
            ..Default::default()
        };
    }
    let hit = hit_with("schema-row", "body", &[]);
    let dirty = dispatch_event(
        &shell.inner,
        &Event::PointerDown {
            x: 5.0,
            y: 5.0,
            button: PointerButton::Primary,
            modifiers: Modifiers::default(),
        },
        Some(hit),
    );
    assert!(dirty, "schema-row click moves the cursor → redraw");
    assert_eq!(
        shell
            .inner
            .borrow()
            .state
            .builder
            .schema
            .selected_field
            .as_deref(),
        Some("body"),
    );
}

#[test]
fn pointer_down_on_signal_connection_row_moves_connection_cursor() {
    use crate::state::SignalConnection;
    use prism_ui_runtime::event::PointerButton;
    let shell = Shell::new().expect("boot");
    {
        let mut guard = shell.inner.borrow_mut();
        guard
            .state
            .builder
            .signal_connections
            .push(SignalConnection {
                id: "c1".into(),
                source_signal: "clicked".into(),
                action_kind: "EmitSignal".into(),
                target_label: "x".into(),
            });
        guard
            .state
            .builder
            .signal_connections
            .push(SignalConnection {
                id: "c2".into(),
                source_signal: "hovered".into(),
                action_kind: "SetProperty".into(),
                target_label: "y".into(),
            });
    }
    let hit = hit_with("signal-connection-row", "c2", &[]);
    let dirty = dispatch_event(
        &shell.inner,
        &Event::PointerDown {
            x: 5.0,
            y: 5.0,
            button: PointerButton::Primary,
            modifiers: Modifiers::default(),
        },
        Some(hit),
    );
    assert!(dirty, "signal-connection-row click moves cursor → redraw");
    assert_eq!(
        shell
            .inner
            .borrow()
            .state
            .builder
            .selected_connection
            .as_deref(),
        Some("c2"),
    );
}

#[test]
fn pointer_down_on_palette_item_sets_palette_selected() {
    use prism_ui_runtime::event::PointerButton;
    let shell = Shell::new().expect("boot");
    let hit = hit_with("palette-item", "text", &[]);
    let dirty = dispatch_event(
        &shell.inner,
        &Event::PointerDown {
            x: 5.0,
            y: 5.0,
            button: PointerButton::Primary,
            modifiers: Modifiers::default(),
        },
        Some(hit),
    );
    assert!(dirty, "palette pick mutates state and requests a redraw");
    assert_eq!(
        shell
            .inner
            .borrow()
            .state
            .catalog
            .palette_selected
            .as_deref(),
        Some("text"),
    );
}

/// §43 A1: an unsupported / malformed action string falls
/// through cleanly — the rest of the pointer-down chain (canvas
/// selection, drag capture) still runs. The contract is "parsed
/// actions without a shipped executor are no-ops, never panics."
#[test]
fn pointer_down_on_unsupported_action_does_not_break_chain() {
    use prism_ui_runtime::event::PointerButton;

    let shell = Shell::new().expect("boot");
    let baseline_selection = shell.inner.borrow().state.canvas.selection.clone();
    let hit = HitRect {
        id: "demo-heading".into(),
        bounds: Rect {
            x: 0.0,
            y: 0.0,
            width: 80.0,
            height: 24.0,
        },
        attrs: vec![("data-on-click".into(), "yodel loud".into())],
        disabled: false,
    };
    let _ = dispatch_event(
        &shell.inner,
        &Event::PointerDown {
            x: 10.0,
            y: 10.0,
            button: PointerButton::Primary,
            modifiers: Modifiers::default(),
        },
        Some(hit),
    );
    // No actor fired — the canvas selection stayed exactly where
    // it was (the boot pre-selection on `demo-heading`).
    assert_eq!(
        shell.inner.borrow().state.canvas.selection,
        baseline_selection,
    );
}

// ── Wave 1.6 modifier route tests ───────────────────────────────

fn attach_tooltip_to(shell: &Shell, node_id: &str) {
    // Helper: attach a tooltip modifier to the named doc node so
    // the route tests have a target to toggle / remove / reorder.
    use prism_builder::{Modifier, ModifierKind};
    let mut guard = shell.inner.borrow_mut();
    let g = &mut *guard;
    if let Some(n) = g
        .state
        .canvas
        .document
        .root
        .as_mut()
        .and_then(|r| r.find_mut(node_id))
    {
        n.modifiers.push(Modifier::from_kind(ModifierKind::Tooltip));
    }
    let registry = g.registry.as_component_registry();
    g.state.resync_builder_for_selection(Some(registry));
}

#[test]
fn pointer_down_on_modifier_toggle_flips_enabled() {
    use prism_ui_runtime::event::PointerButton;
    let shell = Shell::new().expect("boot");
    attach_tooltip_to(&shell, "demo-button");
    // Select the button so the inspector sees its modifiers.
    {
        let mut guard = shell.inner.borrow_mut();
        let g = &mut *guard;
        let registry = g.registry.as_component_registry();
        g.state.select_node("demo-button", Some(registry));
    }

    let hit = hit_with(
        "modifier-toggle",
        "demo-button",
        &[("data-modifier-idx", "0")],
    );
    let dirty = dispatch_event(
        &shell.inner,
        &Event::PointerDown {
            x: 1.0,
            y: 1.0,
            button: PointerButton::Primary,
            modifiers: Modifiers::default(),
        },
        Some(hit),
    );
    assert!(dirty);
    let enabled = shell
        .inner
        .borrow()
        .state
        .canvas
        .document
        .root
        .as_ref()
        .unwrap()
        .find("demo-button")
        .unwrap()
        .modifiers[0]
        .enabled;
    assert!(!enabled, "first click disables the modifier");
}

#[test]
fn pointer_down_on_modifier_remove_detaches_entry() {
    use prism_ui_runtime::event::PointerButton;
    let shell = Shell::new().expect("boot");
    attach_tooltip_to(&shell, "demo-button");
    assert_eq!(
        shell
            .inner
            .borrow()
            .state
            .canvas
            .document
            .root
            .as_ref()
            .unwrap()
            .find("demo-button")
            .unwrap()
            .modifiers
            .len(),
        1
    );

    let hit = hit_with(
        "modifier-remove",
        "demo-button",
        &[("data-modifier-idx", "0")],
    );
    let _ = dispatch_event(
        &shell.inner,
        &Event::PointerDown {
            x: 1.0,
            y: 1.0,
            button: PointerButton::Primary,
            modifiers: Modifiers::default(),
        },
        Some(hit),
    );
    let len = shell
        .inner
        .borrow()
        .state
        .canvas
        .document
        .root
        .as_ref()
        .unwrap()
        .find("demo-button")
        .unwrap()
        .modifiers
        .len();
    assert_eq!(len, 0);
}

#[test]
fn pointer_down_on_add_modifier_open_seeds_picker_state() {
    use prism_ui_runtime::event::PointerButton;
    let shell = Shell::new().expect("boot");
    let hit = hit_with(
        "add-modifier-open",
        "demo-button",
        &[("data-attached", r#"["tooltip"]"#)],
    );
    let _ = dispatch_event(
        &shell.inner,
        &Event::PointerDown {
            x: 1.0,
            y: 1.0,
            button: PointerButton::Primary,
            modifiers: Modifiers::default(),
        },
        Some(hit),
    );
    let picker = shell.inner.borrow().state.overlay.modifier_picker.clone();
    assert!(picker.open);
    assert_eq!(picker.target_id, "demo-button");
    assert_eq!(picker.attached, vec!["tooltip".to_string()]);
}

#[test]
fn pointer_down_on_modifier_picker_select_attaches_and_closes_picker() {
    use prism_ui_runtime::event::PointerButton;
    let shell = Shell::new().expect("boot");
    // Open the picker first.
    {
        let mut guard = shell.inner.borrow_mut();
        guard.state.overlay.modifier_picker = crate::state::ModifierPicker {
            open: true,
            target_id: "demo-button".into(),
            attached: vec![],
        };
    }
    let hit = hit_with(
        "modifier-picker-select",
        "demo-button",
        &[("data-modifier-id", "tooltip")],
    );
    let _ = dispatch_event(
        &shell.inner,
        &Event::PointerDown {
            x: 1.0,
            y: 1.0,
            button: PointerButton::Primary,
            modifiers: Modifiers::default(),
        },
        Some(hit),
    );
    let inner = shell.inner.borrow();
    assert_eq!(
        inner
            .state
            .canvas
            .document
            .root
            .as_ref()
            .unwrap()
            .find("demo-button")
            .unwrap()
            .modifiers
            .len(),
        1
    );
    assert!(!inner.state.overlay.modifier_picker.open, "picker closes");
}

// ── Wave 3.2 palette drag → drop tests ──────────────────────────

/// A canvas hit while `palette_selected` is armed must capture
/// a palette drag (recording the cursor + drop target) without
/// also re-selecting the canvas node under the cursor.
#[test]
fn pointer_down_on_canvas_with_palette_armed_begins_palette_drag() {
    use prism_ui_runtime::event::PointerButton;
    let shell = Shell::new().expect("boot");
    {
        let mut guard = shell.inner.borrow_mut();
        guard.state.catalog.palette_selected = Some("text".into());
    }
    // A canvas hit shaped like the lowered `demo-heading` preview
    // node — `data-canvas-node="demo-heading"` is the disambiguator.
    let hit = HitRect {
        id: "demo-heading".into(),
        bounds: Rect {
            x: 0.0,
            y: 0.0,
            width: 80.0,
            height: 24.0,
        },
        attrs: vec![
            ("data-role".into(), "canvas-preview".into()),
            ("data-canvas-node".into(), "demo-heading".into()),
        ],
        disabled: false,
    };
    let dirty = dispatch_event(
        &shell.inner,
        &Event::PointerDown {
            x: 100.0,
            y: 200.0,
            button: PointerButton::Primary,
            modifiers: Modifiers::default(),
        },
        Some(hit),
    );
    assert!(dirty, "palette drag start requests redraw");
    let inner = shell.inner.borrow();
    let drag = inner
        .state
        .catalog
        .palette_drag
        .as_ref()
        .expect("drag captured");
    assert_eq!(drag.kind, "text");
    assert_eq!(drag.pointer, (100.0, 200.0));
    assert_eq!(drag.drop_target.as_deref(), Some("demo-heading"));
}

/// A palette drag released over a canvas node inserts the new
/// node under that target, clears `palette_selected`, and moves
/// the canvas selection onto the freshly-dropped node.
#[test]
fn pointer_up_with_active_palette_drag_inserts_node_under_target() {
    use prism_ui_runtime::event::PointerButton;
    let shell = Shell::new().expect("boot");
    let before = shell.inner.borrow().state.canvas.node_count();
    {
        let mut guard = shell.inner.borrow_mut();
        guard.state.catalog.palette_selected = Some("button".into());
    }
    // Step 1: PointerDown on a canvas-preview hit captures the
    // drag and records the drop target.
    let target_id = shell
        .inner
        .borrow()
        .state
        .canvas
        .document
        .root
        .as_ref()
        .unwrap()
        .id
        .clone();
    let hit = HitRect {
        id: target_id.clone(),
        bounds: Rect {
            x: 0.0,
            y: 0.0,
            width: 200.0,
            height: 200.0,
        },
        attrs: vec![
            ("data-role".into(), "canvas-preview".into()),
            ("data-canvas-node".into(), target_id.clone()),
        ],
        disabled: false,
    };
    let _ = dispatch_event(
        &shell.inner,
        &Event::PointerDown {
            x: 12.0,
            y: 12.0,
            button: PointerButton::Primary,
            modifiers: Modifiers::default(),
        },
        Some(hit.clone()),
    );
    // Step 2: PointerUp commits the insert.
    let dirty = dispatch_event(
        &shell.inner,
        &Event::PointerUp {
            x: 20.0,
            y: 20.0,
            button: PointerButton::Primary,
            modifiers: Modifiers::default(),
        },
        Some(hit),
    );
    assert!(dirty, "drop commits the insert → redraw");
    let after = shell.inner.borrow().state.canvas.node_count();
    assert_eq!(after, before + 1, "node-count rises by one");
    let inner = shell.inner.borrow();
    assert!(
        inner.state.catalog.palette_drag.is_none(),
        "drag state is consumed"
    );
    assert!(
        inner.state.catalog.palette_selected.is_none(),
        "palette pill clears after drop"
    );
    // Selection moves to the new node (its id is "button-new" +
    // a unique suffix because the target's children may already
    // hold a `button-new`).
    assert!(
        inner
            .state
            .canvas
            .selection
            .as_deref()
            .map(|s| s.starts_with("button-new"))
            .unwrap_or(false),
        "selection moves to the dropped node"
    );
}

/// With no palette item armed, a canvas click falls through to
/// the existing select-canvas-node path — the Wave 3.2 hook
/// must not steal clicks that aren't actually palette-driven.
#[test]
fn pointer_down_on_canvas_without_palette_armed_falls_through_to_select() {
    use prism_ui_runtime::event::PointerButton;
    let shell = Shell::new().expect("boot");
    // No palette pick.
    assert!(shell
        .inner
        .borrow()
        .state
        .catalog
        .palette_selected
        .is_none());
    let hit = HitRect {
        id: "demo-heading".into(),
        bounds: Rect {
            x: 0.0,
            y: 0.0,
            width: 80.0,
            height: 24.0,
        },
        attrs: vec![
            ("data-role".into(), "canvas-preview".into()),
            ("data-canvas-node".into(), "demo-heading".into()),
        ],
        disabled: false,
    };
    let _ = dispatch_event(
        &shell.inner,
        &Event::PointerDown {
            x: 10.0,
            y: 10.0,
            button: PointerButton::Primary,
            modifiers: Modifiers::default(),
        },
        Some(hit),
    );
    let inner = shell.inner.borrow();
    assert!(
        inner.state.catalog.palette_drag.is_none(),
        "no drag started"
    );
    assert_eq!(
        inner.state.canvas.selection.as_deref(),
        Some("demo-heading"),
        "ordinary canvas click still selects"
    );
}

// ── Wave 3.4 right-click context menu tests ─────────────────────

/// A right-click on a canvas-resident hit opens the context menu
/// with the node-mutation triad plus clipboard rows; the canvas
/// selection moves to the right-clicked node so command
/// activation operates against it.
#[test]
fn right_click_on_canvas_node_opens_context_menu_with_actions() {
    use prism_ui_runtime::event::PointerButton;
    let shell = Shell::new().expect("boot");
    let hit = HitRect {
        id: "demo-heading".into(),
        bounds: Rect {
            x: 0.0,
            y: 0.0,
            width: 80.0,
            height: 24.0,
        },
        attrs: vec![
            ("data-role".into(), "canvas-preview".into()),
            ("data-canvas-node".into(), "demo-heading".into()),
        ],
        disabled: false,
    };
    let dirty = dispatch_event(
        &shell.inner,
        &Event::PointerDown {
            x: 50.0,
            y: 50.0,
            button: PointerButton::Secondary,
            modifiers: Modifiers::default(),
        },
        Some(hit),
    );
    assert!(dirty, "right-click opens the menu → redraw");
    let inner = shell.inner.borrow();
    assert_eq!(
        inner.state.canvas.selection.as_deref(),
        Some("demo-heading"),
        "right-click moves the selection cursor",
    );
    let labels: Vec<&str> = inner
        .state
        .menus
        .context
        .iter()
        .filter(|m| !m.separator)
        .map(|m| m.label.as_str())
        .collect();
    assert!(
        labels.contains(&"Delete"),
        "menu carries the Delete row, got {labels:?}"
    );
    assert!(
        labels.contains(&"Move Up"),
        "menu carries the Move Up row, got {labels:?}"
    );
}

/// A right-click on the empty canvas (no `data-canvas-node`) still
/// opens the menu, falling back to document-level actions.
#[test]
fn right_click_on_empty_canvas_opens_paste_only_menu() {
    use prism_ui_runtime::event::PointerButton;
    let shell = Shell::new().expect("boot");
    // Clear selection so the menu reflects "no node selected."
    {
        let mut guard = shell.inner.borrow_mut();
        guard.state.canvas.selection = None;
    }
    let hit = HitRect {
        id: "canvas".into(),
        bounds: Rect {
            x: 0.0,
            y: 0.0,
            width: 800.0,
            height: 600.0,
        },
        attrs: vec![("data-role".into(), "canvas-page".into())],
        disabled: false,
    };
    let _ = dispatch_event(
        &shell.inner,
        &Event::PointerDown {
            x: 400.0,
            y: 300.0,
            button: PointerButton::Secondary,
            modifiers: Modifiers::default(),
        },
        Some(hit),
    );
    let inner = shell.inner.borrow();
    let labels: Vec<&str> = inner
        .state
        .menus
        .context
        .iter()
        .filter(|m| !m.separator)
        .map(|m| m.label.as_str())
        .collect();
    assert_eq!(labels, vec!["Paste"]);
}

/// A primary click outside the menu closes an open context menu.
/// The hit need not match any chrome — even an idle canvas
/// surface dismisses it.
#[test]
fn primary_click_outside_menu_dismisses_open_context_menu() {
    use prism_ui_runtime::event::PointerButton;
    let shell = Shell::new().expect("boot");
    // Seed an open menu.
    {
        let mut guard = shell.inner.borrow_mut();
        guard.state.menus.context.push(crate::state::MenuItem {
            label: "Foo".into(),
            shortcut: None,
            command: Some("noop".into()),
            separator: false,
            enabled: true,
        });
    }
    let hit = HitRect {
        id: "elsewhere".into(),
        bounds: Rect {
            x: 0.0,
            y: 0.0,
            width: 10.0,
            height: 10.0,
        },
        attrs: vec![],
        disabled: false,
    };
    let dirty = dispatch_event(
        &shell.inner,
        &Event::PointerDown {
            x: 0.0,
            y: 0.0,
            button: PointerButton::Primary,
            modifiers: Modifiers::default(),
        },
        Some(hit),
    );
    assert!(dirty, "dismiss requests a redraw");
    assert!(shell.inner.borrow().state.menus.context.is_empty());
}

// ── Wave 3.3 selection-gizmo + resize tests ─────────────────────

/// A canvas-node click captures the hit's bounding rect into
/// `state.canvas.selection_bbox` so the next frame paints the
/// selection outline + 8-handle ring at the real layout rect.
#[test]
fn pointer_down_on_canvas_node_captures_selection_bbox() {
    use prism_ui_runtime::event::PointerButton;
    let shell = Shell::new().expect("boot");
    let hit = HitRect {
        id: "demo-heading".into(),
        bounds: Rect {
            x: 24.0,
            y: 48.0,
            width: 160.0,
            height: 32.0,
        },
        attrs: vec![
            ("data-role".into(), "canvas-preview".into()),
            ("data-canvas-node".into(), "demo-heading".into()),
        ],
        disabled: false,
    };
    let _ = dispatch_event(
        &shell.inner,
        &Event::PointerDown {
            x: 25.0,
            y: 49.0,
            button: PointerButton::Primary,
            modifiers: Modifiers::default(),
        },
        Some(hit),
    );
    let bbox = shell
        .inner
        .borrow()
        .state
        .canvas
        .selection_bbox
        .expect("bbox captured");
    assert_eq!(bbox.x, 24.0);
    assert_eq!(bbox.y, 48.0);
    assert_eq!(bbox.width, 160.0);
    assert_eq!(bbox.height, 32.0);
}

/// A pointer-down on a resize-handle hit captures the direction
/// and snapshots the selection's transform; a follow-up
/// pointer-move translates the node along the handle's axes.
#[test]
fn resize_handle_press_then_move_translates_selection_transform() {
    use prism_builder::Node;
    use prism_core::foundation::spatial::Transform2D;
    use prism_ui_runtime::event::PointerButton;
    let shell = Shell::new().expect("boot");
    // Seed a doc with a known-position node and select it so
    // the handle handler has something to drag.
    {
        let mut guard = shell.inner.borrow_mut();
        let g = &mut *guard;
        g.state.canvas.document = prism_builder::BuilderDocument {
            root: Some(Node {
                id: "root".into(),
                component: "container".into(),
                transform: Transform2D {
                    position: [100.0, 100.0],
                    ..Default::default()
                },
                ..Default::default()
            }),
            ..Default::default()
        };
        g.state.canvas.selection = Some("root".into());
    }
    let handle_hit = hit_with("resize-handle", "", &[("data-direction", "br")]);
    let _ = dispatch_event(
        &shell.inner,
        &Event::PointerDown {
            x: 0.0,
            y: 0.0,
            button: PointerButton::Primary,
            modifiers: Modifiers::default(),
        },
        Some(handle_hit),
    );
    assert!(
        shell.inner.borrow().state.canvas.resize_drag.is_some(),
        "press captures the drag session"
    );
    // PointerMove with no hit (resize drag doesn't need one):
    // delta (50, 30) under bottom-right handle adds positively.
    let _ = dispatch_event(
        &shell.inner,
        &Event::PointerMove {
            x: 50.0,
            y: 30.0,
            modifiers: Modifiers::default(),
        },
        None,
    );
    let pos = shell
        .inner
        .borrow()
        .state
        .canvas
        .document
        .root
        .as_ref()
        .unwrap()
        .transform
        .position;
    // Origin was (handle bbox center = 5, 5). Delta = (45, 25)
    // against zoom 1.0. Snapshot = (100, 100). After drag:
    // (100 + 45, 100 + 25) = (145, 125).
    assert!((pos[0] - 145.0).abs() < 0.5, "x translated: {pos:?}");
    assert!((pos[1] - 125.0).abs() < 0.5, "y translated: {pos:?}");
    // PointerUp commits the drag (clears the session).
    let _ = dispatch_event(
        &shell.inner,
        &Event::PointerUp {
            x: 50.0,
            y: 30.0,
            button: PointerButton::Primary,
            modifiers: Modifiers::default(),
        },
        None,
    );
    assert!(
        shell.inner.borrow().state.canvas.resize_drag.is_none(),
        "release commits the drag"
    );
}

/// A resize-handle press without a selected canvas node is a
/// clean no-op — clicking a stale handle (e.g. one painted from
/// a prior selection that the user then deselected) doesn't
/// capture an empty drag.
#[test]
fn resize_handle_press_without_selection_is_a_noop() {
    use prism_ui_runtime::event::PointerButton;
    let shell = Shell::new().expect("boot");
    {
        let mut guard = shell.inner.borrow_mut();
        guard.state.canvas.selection = None;
    }
    let handle_hit = hit_with("resize-handle", "", &[("data-direction", "br")]);
    let _ = dispatch_event(
        &shell.inner,
        &Event::PointerDown {
            x: 0.0,
            y: 0.0,
            button: PointerButton::Primary,
            modifiers: Modifiers::default(),
        },
        Some(handle_hit),
    );
    assert!(shell.inner.borrow().state.canvas.resize_drag.is_none());
}

/// An unknown `data-direction` value falls through cleanly; the
/// 8-direction whitelist guards against malformed authoring.
#[test]
fn resize_handle_press_with_unknown_direction_falls_through() {
    use prism_builder::Node;
    use prism_ui_runtime::event::PointerButton;
    let shell = Shell::new().expect("boot");
    {
        let mut guard = shell.inner.borrow_mut();
        let g = &mut *guard;
        g.state.canvas.document = prism_builder::BuilderDocument {
            root: Some(Node {
                id: "root".into(),
                component: "container".into(),
                ..Default::default()
            }),
            ..Default::default()
        };
        g.state.canvas.selection = Some("root".into());
    }
    let handle_hit = hit_with("resize-handle", "", &[("data-direction", "???")]);
    let _ = dispatch_event(
        &shell.inner,
        &Event::PointerDown {
            x: 0.0,
            y: 0.0,
            button: PointerButton::Primary,
            modifiers: Modifiers::default(),
        },
        Some(handle_hit),
    );
    assert!(shell.inner.borrow().state.canvas.resize_drag.is_none());
}

// ── Wave 4.3 connection picker route tests ──────────────────────

/// Clicking the connection-picker's `action-kind` field cycles
/// through the declared `ActionKind` variant list. Each click
/// advances one step; the picker stays open so the user can
/// also edit source/target before confirming.
#[test]
fn pointer_down_on_connection_picker_action_kind_cycles() {
    use prism_ui_runtime::event::PointerButton;
    let shell = Shell::new().expect("boot");
    {
        let mut guard = shell.inner.borrow_mut();
        assert!(guard.state.open_connection_picker());
    }
    let baseline = shell
        .inner
        .borrow()
        .state
        .overlay
        .connection_picker
        .action_kind
        .clone();
    let hit = hit_with(
        "connection-picker-field",
        "",
        &[("data-field", "action-kind")],
    );
    let dirty = dispatch_event(
        &shell.inner,
        &Event::PointerDown {
            x: 0.0,
            y: 0.0,
            button: PointerButton::Primary,
            modifiers: Modifiers::default(),
        },
        Some(hit),
    );
    assert!(dirty, "cycle moves the field → redraw");
    let after = shell
        .inner
        .borrow()
        .state
        .overlay
        .connection_picker
        .action_kind
        .clone();
    assert_ne!(after, baseline, "action-kind advanced");
}

/// The Add button confirms the picker → a fresh
/// `SignalConnection` lands and the picker closes.
#[test]
fn pointer_down_on_connection_picker_add_inserts_and_closes() {
    use prism_ui_runtime::event::PointerButton;
    let shell = Shell::new().expect("boot");
    let before = shell.inner.borrow().state.builder.signal_connections.len();
    {
        let mut guard = shell.inner.borrow_mut();
        guard.state.open_connection_picker();
        guard.state.overlay.connection_picker.source_signal = "clicked".into();
        guard.state.overlay.connection_picker.target_label = "x".into();
    }
    let hit = hit_with("connection-picker-add", "", &[]);
    let dirty = dispatch_event(
        &shell.inner,
        &Event::PointerDown {
            x: 0.0,
            y: 0.0,
            button: PointerButton::Primary,
            modifiers: Modifiers::default(),
        },
        Some(hit),
    );
    assert!(dirty, "add → redraw");
    let inner = shell.inner.borrow();
    assert_eq!(
        inner.state.builder.signal_connections.len(),
        before + 1,
        "one connection added"
    );
    assert!(
        !inner.state.overlay.connection_picker.open,
        "picker closes after add"
    );
}

/// The Cancel button closes the picker without inserting.
#[test]
fn pointer_down_on_connection_picker_cancel_closes_without_insert() {
    use prism_ui_runtime::event::PointerButton;
    let shell = Shell::new().expect("boot");
    let before = shell.inner.borrow().state.builder.signal_connections.len();
    {
        let mut guard = shell.inner.borrow_mut();
        guard.state.open_connection_picker();
        guard.state.overlay.connection_picker.source_signal = "clicked".into();
    }
    let hit = hit_with("connection-picker-cancel", "", &[]);
    let dirty = dispatch_event(
        &shell.inner,
        &Event::PointerDown {
            x: 0.0,
            y: 0.0,
            button: PointerButton::Primary,
            modifiers: Modifiers::default(),
        },
        Some(hit),
    );
    assert!(dirty);
    assert_eq!(
        shell.inner.borrow().state.builder.signal_connections.len(),
        before,
        "no insert on cancel"
    );
    assert!(!shell.inner.borrow().state.overlay.connection_picker.open);
}

/// The "+ Add Connection" footer in the signals panel dispatches
/// `signals.open-connection-picker` via the existing
/// `data-on-click="cmd <id>"` path. Verify that the route fires
/// the command and the picker actually opens.
#[test]
fn data_on_click_open_picker_dispatches_through_command_table() {
    use prism_ui_runtime::event::PointerButton;
    let shell = Shell::new().expect("boot");
    assert!(!shell.inner.borrow().state.overlay.connection_picker.open);
    let hit = HitRect {
        id: "add-button".into(),
        bounds: Rect {
            x: 0.0,
            y: 0.0,
            width: 100.0,
            height: 30.0,
        },
        attrs: vec![(
            "data-on-click".into(),
            "cmd signals.open-connection-picker".into(),
        )],
        disabled: false,
    };
    let dirty = dispatch_event(
        &shell.inner,
        &Event::PointerDown {
            x: 1.0,
            y: 1.0,
            button: PointerButton::Primary,
            modifiers: Modifiers::default(),
        },
        Some(hit),
    );
    assert!(dirty);
    assert!(
        shell.inner.borrow().state.overlay.connection_picker.open,
        "picker opened via command dispatch"
    );
}

/// §7.7 Phase 1 (Q10) — `:disabled` click suppression. A pointer
/// press on a `HitRect` carrying `disabled: true` must NOT route
/// through the dispatcher: no `data-on-click` arm fires, no command
/// dispatches, no signal cascade. Repaint of `:disabled` overrides
/// happens at the surface layer via the same mechanism `:hovered`
/// uses; this test pins only the dispatcher-level gate.
#[test]
fn disabled_hit_suppresses_data_on_click_dispatch() {
    use prism_ui_runtime::event::PointerButton;
    let shell = Shell::new().expect("boot");
    assert!(!shell.inner.borrow().state.overlay.connection_picker.open);
    // Same `data-on-click` payload as the test above — proves the
    // route exists and would fire if not disabled.
    let hit = HitRect {
        id: "add-button-disabled".into(),
        bounds: Rect {
            x: 0.0,
            y: 0.0,
            width: 100.0,
            height: 30.0,
        },
        attrs: vec![(
            "data-on-click".into(),
            "cmd signals.open-connection-picker".into(),
        )],
        disabled: true,
    };
    let dirty = dispatch_event(
        &shell.inner,
        &Event::PointerDown {
            x: 1.0,
            y: 1.0,
            button: PointerButton::Primary,
            modifiers: Modifiers::default(),
        },
        Some(hit),
    );
    assert!(!dirty, "disabled hit must produce no redraw signal");
    assert!(
        !shell.inner.borrow().state.overlay.connection_picker.open,
        "disabled click must not open the picker",
    );
}

/// §7.7 Phase 1 — enabled hit on the same id (no `disabled` flag)
/// still routes normally. Sanity sibling to the suppression test so
/// a future regression that drops the gate doesn't silently keep
/// both tests passing.
#[test]
fn enabled_hit_routes_data_on_click_dispatch() {
    use prism_ui_runtime::event::PointerButton;
    let shell = Shell::new().expect("boot");
    let hit = HitRect {
        id: "add-button-enabled".into(),
        bounds: Rect {
            x: 0.0,
            y: 0.0,
            width: 100.0,
            height: 30.0,
        },
        attrs: vec![(
            "data-on-click".into(),
            "cmd signals.open-connection-picker".into(),
        )],
        disabled: false,
    };
    let dirty = dispatch_event(
        &shell.inner,
        &Event::PointerDown {
            x: 1.0,
            y: 1.0,
            button: PointerButton::Primary,
            modifiers: Modifiers::default(),
        },
        Some(hit),
    );
    assert!(dirty);
    assert!(
        shell.inner.borrow().state.overlay.connection_picker.open,
        "enabled click opens the picker",
    );
}
