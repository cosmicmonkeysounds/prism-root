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
