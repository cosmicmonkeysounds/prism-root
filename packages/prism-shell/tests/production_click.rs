//! Integration smoke test: boot a real `Shell::new`, build a Surface
//! exactly the same shape `Shell::run` does, hit-test a known-good
//! coordinate, and confirm dispatch routes through the same chain
//! the production handler uses.
//!
//! Run with `cargo test -p prism-shell --test production_click`.

use prism_shell::events::dispatch_event;
use prism_shell::Shell;
use prism_ui_runtime::event::{Event, PointerButton};
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
