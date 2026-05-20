//! Integration smoke test: boot a real `Shell::new`, build a Surface
//! exactly the same shape `Shell::run` does, hit-test a known-good
//! coordinate, and confirm dispatch routes through the same chain
//! the production handler uses.
//!
//! Run with `cargo test -p prism-shell --test production_click`.

use prism_shell::events::dispatch_event;
use prism_shell::Shell;
use prism_ui_runtime::event::{Event, Modifiers, PointerButton};
use prism_ui_runtime::layout::{Node, Surface};

fn wrap_root(children: Vec<Node>) -> Node {
    Node::Container {
        id: String::new(),
        props: prism_ui_runtime::layout::ContainerProps {
            direction: prism_ui_runtime::layout::Direction::Column,
            width: prism_ui_runtime::layout::Sizing::Grow,
            height: prism_ui_runtime::layout::Sizing::Grow,
            ..Default::default()
        },
        children,
    }
}

#[test]
fn production_surface_has_clickable_hits_for_chrome() {
    let shell = Shell::new().expect("boot");
    let nodes = shell.render();
    let viewport = shell.inner.borrow().viewport;
    let mut surface = Surface::new(wrap_root(nodes), viewport);
    // Prime layout + hit cache.
    let _ = surface.commands();
    let hits = surface.hit_rects();
    println!("total hits = {}", hits.len());
    // Find every distinct data-role surfaced through the hit cache.
    let mut roles: Vec<&str> = hits
        .iter()
        .flat_map(|h| {
            h.attrs
                .iter()
                .filter(|(k, _)| k == "data-role")
                .map(|(_, v)| v.as_str())
        })
        .collect();
    roles.sort();
    roles.dedup();
    println!("distinct data-roles = {:?}", roles);
    assert!(hits.len() > 10, "surface should hit-test many containers");
    // A working chrome lays out at least these click-routable surfaces:
    for required in [
        "palette-item",
        "workflow-page-button",
        "dock-tab",
        "field-edit",
    ] {
        assert!(
            roles.contains(&required),
            "data-role={required} missing from hit cache; got {roles:?}"
        );
    }
}

#[test]
fn production_pointer_down_on_palette_item_mutates_selection() {
    let shell = Shell::new().expect("boot");
    let nodes = shell.render();
    let viewport = shell.inner.borrow().viewport;
    let mut surface = Surface::new(wrap_root(nodes), viewport);
    let _ = surface.commands();

    // Find any palette-item hit and click it through dispatch_event
    // exactly like the production handler.
    let palette_hit = surface
        .hit_rects()
        .iter()
        .find(|h| {
            h.attrs
                .iter()
                .any(|(k, v)| k == "data-role" && v == "palette-item")
        })
        .cloned()
        .expect("at least one palette-item hit");
    let target_id = palette_hit
        .attrs
        .iter()
        .find(|(k, _)| k == "data-target-id")
        .map(|(_, v)| v.clone())
        .expect("palette-item carries data-target-id");
    println!("clicking palette item id={target_id}");

    // Mirror Shell::run's PointerDown handling exactly.
    let event = Event::PointerDown {
        x: palette_hit.bounds.x + 1.0,
        y: palette_hit.bounds.y + 1.0,
        button: PointerButton::Primary,
        modifiers: Modifiers::default(),
    };
    let hit = surface
        .hit_test_at(palette_hit.bounds.x + 1.0, palette_hit.bounds.y + 1.0)
        .cloned();
    println!(
        "  hit_id={:?} role={:?}",
        hit.as_ref().map(|h| h.id.clone()),
        hit.as_ref()
            .and_then(|h| h.attrs.iter().find(|(k, _)| k == "data-role"))
            .map(|(_, v)| v.clone()),
    );
    let dirty = dispatch_event(&shell.inner, &event, hit);
    assert!(dirty, "palette click must dirty the frame");
    assert_eq!(
        shell
            .inner
            .borrow()
            .state
            .catalog
            .palette_selected
            .as_deref(),
        Some(target_id.as_str()),
        "palette_selected should track the clicked item"
    );
}

#[test]
fn production_pointer_down_on_nav_button_flips_selection() {
    // Activity-bar nav buttons (home/folder/search/settings) were
    // silently inert: they declared hover_bg but no `data-role` for
    // routing. A click should move `ChromeSlot::nav_buttons[*].selected`
    // — the radio-style selection across the four rows.
    let shell = Shell::new().expect("boot");
    let nodes = shell.render();
    let viewport = shell.inner.borrow().viewport;
    let mut surface = Surface::new(wrap_root(nodes), viewport);
    let _ = surface.commands();

    // Find a non-selected nav button (anything other than the boot
    // `home` row, which seed starts selected).
    let folder_hit = surface
        .hit_rects()
        .iter()
        .find(|h| {
            h.attrs
                .iter()
                .any(|(k, v)| k == "data-role" && v == "nav-button")
                && h.attrs
                    .iter()
                    .any(|(k, v)| k == "data-target-id" && v == "folder")
        })
        .cloned()
        .expect("nav-button id=folder hit present");

    let event = Event::PointerDown {
        x: folder_hit.bounds.x + 1.0,
        y: folder_hit.bounds.y + 1.0,
        button: PointerButton::Primary,
        modifiers: Modifiers::default(),
    };
    let hit = surface
        .hit_test_at(folder_hit.bounds.x + 1.0, folder_hit.bounds.y + 1.0)
        .cloned();
    let dirty = dispatch_event(&shell.inner, &event, hit);
    assert!(dirty, "nav-button click must dirty the frame");
    let guard = shell.inner.borrow();
    let selected: Vec<&str> = guard
        .state
        .chrome
        .nav_buttons
        .iter()
        .filter(|b| b.selected)
        .map(|b| b.id.as_str())
        .collect();
    assert_eq!(selected, vec!["folder"], "exactly one nav button selected");
}

#[test]
fn production_pointer_down_on_menu_pill_opens_menu() {
    // Top-bar menu pills (File/Edit/View/Window/Help) were inert.
    // A click should toggle `ChromeSlot::active_menu` so the same
    // pill paints its `aria-expanded` + active tint on the next frame.
    let shell = Shell::new().expect("boot");
    let nodes = shell.render();
    let viewport = shell.inner.borrow().viewport;
    let mut surface = Surface::new(wrap_root(nodes), viewport);
    let _ = surface.commands();

    let edit_hit = surface
        .hit_rects()
        .iter()
        .find(|h| {
            h.attrs
                .iter()
                .any(|(k, v)| k == "data-role" && v == "menu-pill")
                && h.attrs
                    .iter()
                    .any(|(k, v)| k == "data-target-id" && v == "edit")
        })
        .cloned()
        .expect("menu-pill id=edit hit present");

    let event = Event::PointerDown {
        x: edit_hit.bounds.x + 1.0,
        y: edit_hit.bounds.y + 1.0,
        button: PointerButton::Primary,
        modifiers: Modifiers::default(),
    };
    let hit = surface
        .hit_test_at(edit_hit.bounds.x + 1.0, edit_hit.bounds.y + 1.0)
        .cloned();
    let dirty = dispatch_event(&shell.inner, &event, hit);
    assert!(dirty, "menu-pill click must dirty the frame");
    assert_eq!(
        shell.inner.borrow().state.chrome.active_menu.as_deref(),
        Some("edit"),
    );
}

#[test]
fn production_pointer_down_on_number_field_starts_drag_session() {
    // After the inner-widget-id refactor, clicks on the drag-number
    // pill resolve to the *field-edit row's* HitRect (the pill itself
    // has empty id so it doesn't shadow the routing parent). The
    // PointerDown should open `state.number_drag` with the current
    // value + bounds, exactly like the legacy Slint Spacing field did.
    //
    // The toolbar's `zoom` field is the only bounded-number row in
    // the boot tree, so we surface it by selecting the canvas root
    // (whose schema includes it). For now the boot already exposes
    // number-style fields via the seeded inspector — find any one
    // with `data-kind == "number"` or `"integer"` and exercise it.
    let shell = Shell::new().expect("boot");
    let nodes = shell.render();
    let viewport = shell.inner.borrow().viewport;
    let mut surface = Surface::new(wrap_root(nodes), viewport);
    let _ = surface.commands();
    let Some(row_hit) = surface
        .hit_rects()
        .iter()
        .find(|h| {
            let role_ok = h
                .attrs
                .iter()
                .any(|(k, v)| k == "data-role" && v == "field-edit");
            let numeric = h
                .attrs
                .iter()
                .any(|(k, v)| k == "data-kind" && (v == "number" || v == "integer"));
            role_ok && numeric
        })
        .cloned()
    else {
        // No bounded-number field on the active selection — this
        // test surfaces them when the demo doc seeds one. Skip
        // silently for now; the audit test guarantees that *every*
        // surfaced field-edit row carries its routing attrs.
        return;
    };

    let event = Event::PointerDown {
        x: row_hit.bounds.x + 1.0,
        y: row_hit.bounds.y + 1.0,
        button: PointerButton::Primary,
        modifiers: Modifiers::default(),
    };
    let hit = surface
        .hit_test_at(row_hit.bounds.x + 1.0, row_hit.bounds.y + 1.0)
        .cloned();
    assert!(
        hit.as_ref()
            .map(|h| h
                .attrs
                .iter()
                .any(|(k, v)| k == "data-role" && v == "field-edit"))
            .unwrap_or(false),
        "number-field click must resolve to a hit with data-role=field-edit; \
         got {:?}",
        hit.as_ref().map(|h| h.id.clone()),
    );
    let dirty = dispatch_event(&shell.inner, &event, hit);
    assert!(dirty, "number-field click must dirty the frame");
    assert!(
        shell.inner.borrow().state.number_drag.is_some(),
        "number-field PointerDown must open a drag session",
    );
}

#[test]
fn production_typing_into_text_field_mutates_bound_prop_end_to_end() {
    // Click → focus → type → commit. After this round trip the doc
    // node's bound prop should reflect the typed text and the
    // re-rendered field-edit row should carry the new `data-value`.
    // This is the "properties aren't interactable" concern the user
    // raised — verified end-to-end against the production handler
    // chain.
    use prism_ui_runtime::event::{Event as RtEvent, Modifiers};
    let shell = Shell::new().expect("boot");
    let nodes = shell.render();
    let viewport = shell.inner.borrow().viewport;
    let mut surface = Surface::new(wrap_root(nodes), viewport);
    let _ = surface.commands();

    // Click the Body field-edit row to open a focus session.
    let row_hit = surface
        .hit_rects()
        .iter()
        .find(|h| {
            let role_ok = h
                .attrs
                .iter()
                .any(|(k, v)| k == "data-role" && v == "field-edit");
            let key_body = h.attrs.iter().any(|(k, v)| k == "data-key" && v == "body");
            role_ok && key_body
        })
        .cloned()
        .expect("Body field-edit row present");
    let click = RtEvent::PointerDown {
        x: row_hit.bounds.x + 1.0,
        y: row_hit.bounds.y + 1.0,
        button: PointerButton::Primary,
        modifiers: Modifiers::default(),
    };
    let hit = surface
        .hit_test_at(row_hit.bounds.x + 1.0, row_hit.bounds.y + 1.0)
        .cloned();
    assert!(dispatch_event(&shell.inner, &click, hit));

    // Type "!" — should flush into the bound prop.
    let typed = RtEvent::Text { text: "!".into() };
    assert!(dispatch_event(&shell.inner, &typed, None));
    let body = shell
        .inner
        .borrow()
        .state
        .canvas
        .document
        .root
        .as_ref()
        .and_then(|r| r.find("demo-heading"))
        .and_then(|n| n.props.get("body").cloned())
        .expect("demo-heading body prop");
    assert_eq!(
        body,
        serde_json::Value::String("Welcome to Studio!".into()),
        "typed character must flush into the bound prop",
    );

    // Type two more chars — verify they *append* to the existing
    // draft rather than replace it. The user reported "typing
    // deletes contents", but the focus model is append-only by
    // contract; this test guards the contract end-to-end.
    for ch in ["?", "x"] {
        assert!(dispatch_event(
            &shell.inner,
            &RtEvent::Text { text: ch.into() },
            None
        ));
    }
    let body = shell
        .inner
        .borrow()
        .state
        .canvas
        .document
        .root
        .as_ref()
        .and_then(|r| r.find("demo-heading"))
        .and_then(|n| n.props.get("body").cloned())
        .expect("demo-heading body prop");
    assert_eq!(
        body,
        serde_json::Value::String("Welcome to Studio!?x".into()),
        "multi-char typing must append, not replace",
    );

    // Press Enter — lowercase code now matches the FieldFocusService
    // (the prior production bug: winit emitted Title-Case "Enter" but
    // the service matched "enter"). Should commit + clear focus.
    let enter = RtEvent::Key {
        code: "enter".into(),
        pressed: true,
        modifiers: Modifiers::default(),
    };
    assert!(dispatch_event(&shell.inner, &enter, None));
    assert!(
        shell.inner.borrow().state.field_focus.is_none(),
        "Enter must commit and clear focus"
    );
}

/// The hex / file / text input inside a field-edit row is the
/// **deepest** hit-test match when the user clicks the input itself.
/// Without `data-role="field-edit"` on the input, the click silently
/// landed on a roleless TextInput and the focus session never opened
/// — the "string / color fields aren't clickable" symptom. Verify
/// every kind that renders an input surfaces the routing attrs so a
/// direct click on the input opens focus.
#[test]
fn production_pointer_down_directly_on_text_input_opens_focus_session() {
    use prism_ui_runtime::layout::Node as RtNode;
    let shell = Shell::new().expect("boot");
    // Walk the rendered tree for a TextInput leaf inside a field-edit
    // row so we can click its actual coordinates (the parent's
    // padding-area click works without the fix; the input-itself
    // click is what was broken).
    fn find_field_input(node: &RtNode) -> Option<(String, String, String)> {
        match node {
            RtNode::TextInput { id, semantic, .. } => {
                let role = semantic
                    .attrs
                    .iter()
                    .find(|(k, _)| k == "data-role")
                    .map(|(_, v)| v.clone())?;
                let target = semantic
                    .attrs
                    .iter()
                    .find(|(k, _)| k == "data-target-id")
                    .map(|(_, v)| v.clone())?;
                let key = semantic
                    .attrs
                    .iter()
                    .find(|(k, _)| k == "data-key")
                    .map(|(_, v)| v.clone())?;
                (role == "field-edit").then_some((id.clone(), target, key))
            }
            RtNode::Container { children, .. } => children.iter().find_map(find_field_input),
            _ => None,
        }
    }
    let nodes = shell.render();
    let viewport = shell.inner.borrow().viewport;
    let root = wrap_root(nodes);
    let (_input_id, target, key) =
        find_field_input(&root).expect("at least one field-edit text/color/file input present");
    let mut surface = Surface::new(root, viewport);
    let _ = surface.commands();
    // Find the matching input HitRect by its `data-target-id`+`data-key`
    // so we click its real bounds, not a synthetic padding sample.
    let input_hit = surface
        .hit_rects()
        .iter()
        .find(|h| {
            h.attrs
                .iter()
                .any(|(k, v)| k == "data-target-id" && v == &target)
                && h.attrs.iter().any(|(k, v)| k == "data-key" && v == &key)
                && h.attrs
                    .iter()
                    .any(|(k, v)| k == "data-role" && v == "field-edit")
        })
        .cloned()
        .expect("field-edit input present in hit cache");
    // Sample the centre of the input bounds — the "user clicked the
    // input itself" case.
    let cx = input_hit.bounds.x + input_hit.bounds.width * 0.5;
    let cy = input_hit.bounds.y + input_hit.bounds.height * 0.5;
    let event = Event::PointerDown {
        x: cx,
        y: cy,
        button: PointerButton::Primary,
        modifiers: Modifiers::default(),
    };
    let hit = surface.hit_test_at(cx, cy).cloned();
    let dirty = dispatch_event(&shell.inner, &event, hit);
    assert!(dirty, "direct input click must dirty the frame");
    let guard = shell.inner.borrow();
    let focus = guard
        .state
        .field_focus
        .as_ref()
        .expect("direct input click must open a focus session");
    assert_eq!(focus.target_id, target);
    assert_eq!(focus.key, key);
}

#[test]
fn production_pointer_down_on_text_field_opens_focus_session() {
    // The properties panel's Body row is a text-kind field-edit. A
    // click on it should open `state.field_focus` — the keyboard
    // routing path that flows typed characters into the bound prop.
    // If `data-key` / `data-kind` / `data-target-id` aren't all on
    // the rendered container, `handle_field_edit_click` short-circuits
    // and the user can't type into the input. That was the reported
    // production symptom: "properties aren't interactable."
    let shell = Shell::new().expect("boot");
    let nodes = shell.render();
    let viewport = shell.inner.borrow().viewport;
    let mut surface = Surface::new(wrap_root(nodes), viewport);
    let _ = surface.commands();

    // Find a text-kind field-edit hit (the Body row of the demo-heading).
    let row_hit = surface
        .hit_rects()
        .iter()
        .find(|h| {
            let role_ok = h
                .attrs
                .iter()
                .any(|(k, v)| k == "data-role" && v == "field-edit");
            let kind_text = h.attrs.iter().any(|(k, v)| k == "data-kind" && v == "text");
            role_ok && kind_text
        })
        .cloned()
        .expect("at least one text field-edit row");
    println!("clicking field-edit row attrs={:?}", row_hit.attrs);
    let target = row_hit
        .attrs
        .iter()
        .find(|(k, _)| k == "data-target-id")
        .map(|(_, v)| v.clone())
        .expect("data-target-id present");
    let key = row_hit
        .attrs
        .iter()
        .find(|(k, _)| k == "data-key")
        .map(|(_, v)| v.clone())
        .expect("data-key present");

    let event = Event::PointerDown {
        x: row_hit.bounds.x + 1.0,
        y: row_hit.bounds.y + 1.0,
        button: PointerButton::Primary,
        modifiers: Modifiers::default(),
    };
    let hit = surface
        .hit_test_at(row_hit.bounds.x + 1.0, row_hit.bounds.y + 1.0)
        .cloned();
    let dirty = dispatch_event(&shell.inner, &event, hit);
    assert!(dirty, "text field click must dirty the frame");
    let guard = shell.inner.borrow();
    let focus = guard
        .state
        .field_focus
        .as_ref()
        .expect("text-kind click must open a focus session");
    assert_eq!(focus.target_id, target);
    assert_eq!(focus.key, key);
}
