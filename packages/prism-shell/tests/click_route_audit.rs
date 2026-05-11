//! Audit every click-routable container's `data-target-id` in the
//! production tree. Any role from `POINTER_ROUTES` that ends up
//! without `data-target-id` (or whatever cursor key the handler
//! reads) is a silent production click failure — the dispatch
//! handler short-circuits on the missing attr without an error.
//!
//! Run with `cargo test -p prism-shell --test click_route_audit -- --nocapture`.

use prism_shell::Shell;
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
fn every_clickable_role_carries_its_routing_key() {
    let shell = Shell::new().expect("boot");
    let nodes = shell.render();
    let viewport = shell.inner.borrow().viewport;
    let mut surface = Surface::new(wrap_root(nodes), viewport);
    let _ = surface.commands();

    // Every role in POINTER_ROUTES that the handler reads `data-target-id`
    // for. Roles that don't require a target id (`toolbar-zoom-reset`
    // is a stateless command, `app-card` reads `data-app` instead)
    // are omitted.
    let required_target_id_roles = [
        "inspector-row",
        "field-edit",
        "workflow-page-button",
        "dock-tab",
        "palette-item",
        "nav-page-row",
        "schema-row",
        "signal-connection-row",
    ];

    let hits = surface.hit_rects();
    let mut failures: Vec<String> = Vec::new();
    for role in required_target_id_roles {
        let role_hits: Vec<_> = hits
            .iter()
            .filter(|h| h.attrs.iter().any(|(k, v)| k == "data-role" && v == role))
            .collect();
        if role_hits.is_empty() {
            // Some roles only surface on workflow pages we're not on.
            // That's fine — the audit is about correctness, not coverage.
            println!("  {role}: 0 instances in tree (skipped)");
            continue;
        }
        for h in &role_hits {
            let target = h
                .attrs
                .iter()
                .find(|(k, _)| k == "data-target-id")
                .map(|(_, v)| v.as_str());
            let has_target = target.map(|s| !s.is_empty()).unwrap_or(false);
            if !has_target {
                failures.push(format!(
                    "role={role} id={id} attrs={attrs:?}",
                    id = h.id,
                    attrs = h.attrs
                ));
            }
        }
        println!(
            "  {role}: {} instances, {} with target-id",
            role_hits.len(),
            role_hits
                .iter()
                .filter(|h| h.attrs.iter().any(|(k, _)| k == "data-target-id"))
                .count(),
        );
    }
    assert!(
        failures.is_empty(),
        "{} clickable container(s) lack data-target-id and will silently \
         fail to route in production:\n{}",
        failures.len(),
        failures.join("\n"),
    );
}
