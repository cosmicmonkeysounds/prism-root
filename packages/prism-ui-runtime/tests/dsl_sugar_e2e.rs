//! End-to-end tests for the recently-landed PRUI sugar:
//!
//! 1. `<dispatch tag="{expr}"/>` — runtime-tag dispatch for the
//!    closed-set primitive vocabulary plus the registered-tag
//!    resolver fallthrough.
//! 2. Virtual `.length` / `.first` / `.last` segments on array /
//!    object / string bindings.
//! 3. `on:event` empty-string filter + dotted modifier-suffix
//!    flattening (`on:click.once.stop` → `data-on-click-once-stop`).
//!
//! Drives the full parse → lower → semantic-HTML pipeline so any
//! regression that breaks an authoring shape from the PRUI reference
//! surfaces here rather than at the binding level.

use prism_ui_runtime::interpret::{
    interpret_with_scope, on_event_attr_key, LowerScope,
};
use prism_ui_runtime::layout::{compute, Node, Viewport};
use serde_json::json;

const VIEWPORT: Viewport = Viewport {
    width: 800.0,
    height: 600.0,
};

fn lower(source: &str, scope: &LowerScope) -> Vec<Node> {
    interpret_with_scope(source, scope).expect("parses cleanly")
}

fn flatten_text(node: &Node, out: &mut Vec<String>) {
    match node {
        Node::Text { content, .. } => out.push(content.clone()),
        Node::Container { children, .. } => {
            for c in children {
                flatten_text(c, out);
            }
        }
        _ => {}
    }
}

#[test]
fn dispatch_tag_literal_primitive_routes_to_correct_arm() {
    // Each `tag=` literal should rewrite to the matching closed-set
    // primitive arm. Walk one element per arm so a future regression
    // in the synthesised-element path lands on a specific failure.
    let scope = LowerScope::default();
    let cases: &[(&str, &str)] = &[
        (r##"<dispatch tag="container" id="root"/>"##, "container"),
        (r##"<dispatch tag="text" id="root">hi</dispatch>"##, "text"),
        (r##"<dispatch tag="heading" id="root">hi</dispatch>"##, "text"),
        (r##"<dispatch tag="spacer" id="root" width="8" height="8"/>"##, "spacer"),
        (r##"<dispatch tag="fragment"><text>inside</text></dispatch>"##, "text"),
    ];
    for (src, expected_kind) in cases {
        let nodes = lower(src, &scope);
        let n0 = nodes.first().expect("at least one node");
        let kind = match n0 {
            Node::Container { .. } => "container",
            Node::Text { .. } => "text",
            Node::Spacer { .. } => "spacer",
            Node::TextInput { .. } => "input",
            Node::Image { .. } => "image",
        };
        assert_eq!(kind, *expected_kind, "for source {}", src);
    }
}

#[test]
fn dispatch_tag_inside_for_loop_dispatches_per_item() {
    let scope = LowerScope::default().with_binding(
        "rows",
        json!([
            {"kind": "text", "body": "Alpha"},
            {"kind": "spacer"},
            {"kind": "text", "body": "Bravo"},
        ]),
    );
    let nodes = lower(
        r#"<container>
            <dispatch for="row in rows" tag="{row.kind}">{row.body}</dispatch>
           </container>"#,
        &scope,
    );
    let Node::Container { children, .. } = &nodes[0] else {
        panic!()
    };
    assert!(matches!(children[0], Node::Text { .. }));
    assert!(matches!(children[1], Node::Spacer { .. }));
    assert!(matches!(children[2], Node::Text { .. }));
    let mut texts = Vec::new();
    for c in children {
        flatten_text(c, &mut texts);
    }
    assert_eq!(texts, vec!["Alpha".to_string(), "Bravo".to_string()]);
}

#[test]
fn dispatch_tag_pipes_through_layout_and_paints_commands() {
    // End-to-end: a `<dispatch tag="container"/>` should layout +
    // paint exactly like an authored `<container/>`. One rectangle
    // command, viewport-sized, from the background colour.
    let scope = LowerScope::default();
    let nodes = lower(
        r##"<dispatch tag="container" width="grow" height="grow" style:background="#001122"/>"##,
        &scope,
    );
    let cmds = compute(&nodes[0], VIEWPORT);
    assert_eq!(cmds.len(), 1);
}

#[test]
fn virtual_length_segment_drives_range_for_loop() {
    // Author writes `for="i in 0..rows.length"`; runtime synthesises
    // `rows.length` as a Number through `lookup_path_owned`.
    let scope = LowerScope::default()
        .with_binding("rows", json!(["a", "b", "c", "d", "e"]));
    let nodes = lower(
        r#"<container>
            <text for="i in 0..rows.length">{i}</text>
           </container>"#,
        &scope,
    );
    let Node::Container { children, .. } = &nodes[0] else {
        panic!()
    };
    let mut texts = Vec::new();
    for c in children {
        flatten_text(c, &mut texts);
    }
    assert_eq!(texts, vec!["0", "1", "2", "3", "4"]);
}

#[test]
fn virtual_first_and_last_segments_resolve_in_text_content() {
    let scope = LowerScope::default()
        .with_binding("words", json!(["alpha", "beta", "gamma"]));
    let nodes = lower(
        r#"<container>
            <text>{words.first}</text>
            <text>{words.last}</text>
           </container>"#,
        &scope,
    );
    let mut texts = Vec::new();
    flatten_text(&nodes[0], &mut texts);
    assert_eq!(texts, vec!["alpha".to_string(), "gamma".to_string()]);
}

#[test]
fn virtual_length_compares_through_full_evaluator() {
    // Operator-bearing expression (`rows.length > 2`) routes through
    // the evaluator. The owned-lookup ScopeStore::resolve installs
    // virtual segments uniformly so the comparison sees the same
    // synthesised length.
    let scope = LowerScope::default()
        .with_binding("rows", json!(["a", "b", "c"]));
    let nodes = lower(
        r#"<container>
            <text if="{rows.length > 2}">many</text>
            <text else>few</text>
           </container>"#,
        &scope,
    );
    let mut texts = Vec::new();
    flatten_text(&nodes[0], &mut texts);
    assert_eq!(texts, vec!["many".to_string()]);
}

#[test]
fn empty_array_with_length_collapses_via_else_branch() {
    // Symmetry check — an `if="{rows.length}"` falsy branch should
    // route to the `else` sibling. This is the empty-state ladder
    // authors lean on across the shell.
    let scope = LowerScope::default().with_binding("rows", json!([]));
    let nodes = lower(
        r#"<container>
            <text if="{rows.length}">non-empty</text>
            <text else>empty</text>
           </container>"#,
        &scope,
    );
    let mut texts = Vec::new();
    flatten_text(&nodes[0], &mut texts);
    assert_eq!(texts, vec!["empty".to_string()]);
}

#[test]
fn on_event_empty_string_filter_drops_handler_attribute() {
    let scope = LowerScope::default()
        .with_binding("cmd", json!(""));
    let nodes = lower(
        r##"<container on:click="{cmd}"/>"##,
        &scope,
    );
    let Node::Container { props, .. } = &nodes[0] else {
        panic!()
    };
    assert!(!props
        .semantic
        .attrs
        .iter()
        .any(|(k, _)| k.starts_with("data-on-")));
}

#[test]
fn on_event_modifier_suffix_round_trips_to_dashed_data_attr() {
    let nodes = lower(
        r##"<container on:click.once.stop="cmd save"/>"##,
        &LowerScope::default(),
    );
    let Node::Container { props, .. } = &nodes[0] else {
        panic!()
    };
    let found = props
        .semantic
        .attrs
        .iter()
        .find(|(k, _)| k == "data-on-click-once-stop")
        .expect("data-on-click-once-stop attribute present");
    assert_eq!(found.1, "cmd save");
}

#[test]
fn on_event_attr_key_helper_round_trips() {
    // Pin the canonical wire form so the resolver-side path
    // (`prism-builder::ui_resolver::attach_on_handlers`) and the
    // runtime path (`apply_container_attributes`) can't drift apart.
    assert_eq!(on_event_attr_key("click"), "click");
    assert_eq!(on_event_attr_key("pointer.down"), "pointer-down");
    assert_eq!(on_event_attr_key("click.once.stop"), "click-once-stop");
}

#[test]
fn virtual_segments_compose_with_props_spread_on_dispatch() {
    // `<dispatch tag="text"/>` carrying spread'd `{row}` props +
    // a `body` interpolation that uses `.length` on a different
    // binding. Exercises the synthesised-element path AND the
    // virtual-segment resolver in one pass.
    let scope = LowerScope::default()
        .with_binding(
            "rows",
            json!([
                {"label": "first"},
                {"label": "second"},
            ]),
        );
    let nodes = lower(
        r#"<container>
            <dispatch for="row in rows" tag="text">{row.label} of {rows.length}</dispatch>
           </container>"#,
        &scope,
    );
    let Node::Container { children, .. } = &nodes[0] else {
        panic!()
    };
    let mut texts = Vec::new();
    for c in children {
        flatten_text(c, &mut texts);
    }
    assert_eq!(
        texts,
        vec!["first of 2".to_string(), "second of 2".to_string()]
    );
}

/// Object-shape iteration combined with `obj.length` produces a
/// counted-row UI — a common pattern for debug / properties views.
#[test]
fn object_length_with_for_loop_emits_one_row_per_entry() {
    let scope = LowerScope::default().with_binding(
        "form",
        json!({"name": "Ada", "email": "ada@example.com"}),
    );
    let nodes = lower(
        r#"<container>
            <text>fields: {form.length}</text>
            <container for="value, key in form">
              <text>{key}</text>
              <text>{value}</text>
            </container>
           </container>"#,
        &scope,
    );
    let Node::Container { children, .. } = &nodes[0] else {
        panic!()
    };
    let mut texts = Vec::new();
    for c in children {
        flatten_text(c, &mut texts);
    }
    // header + 2× (key, value) — six entries.
    assert_eq!(texts.len(), 5);
    assert_eq!(texts[0], "fields: 2");
}
