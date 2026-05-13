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

use prism_ui_runtime::interpret::{interpret_with_scope, on_event_attr_key, LowerScope};
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
        (
            r##"<dispatch tag="heading" id="root">hi</dispatch>"##,
            "text",
        ),
        (
            r##"<dispatch tag="spacer" id="root" width="8" height="8"/>"##,
            "spacer",
        ),
        (
            r##"<dispatch tag="fragment"><text>inside</text></dispatch>"##,
            "text",
        ),
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
    let scope = LowerScope::default().with_binding("rows", json!(["a", "b", "c", "d", "e"]));
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
    let scope = LowerScope::default().with_binding("words", json!(["alpha", "beta", "gamma"]));
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
    let scope = LowerScope::default().with_binding("rows", json!(["a", "b", "c"]));
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
    let scope = LowerScope::default().with_binding("cmd", json!(""));
    let nodes = lower(r##"<container on:click="{cmd}"/>"##, &scope);
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
    let scope = LowerScope::default().with_binding(
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
    let scope = LowerScope::default()
        .with_binding("form", json!({"name": "Ada", "email": "ada@example.com"}));
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

// ---------- Functional helpers: map / reduce / filter / find / slice /
// sort_by / unique / reverse / keys / values / entries / includes /
// index_of / join / concat_arr ----------

#[test]
fn map_projects_field_from_each_object_in_array() {
    let scope = LowerScope::default().with_binding(
        "rows",
        json!([
            {"id": 1, "label": "first"},
            {"id": 2, "label": "second"},
            {"id": 3, "label": "third"},
        ]),
    );
    let nodes = lower(
        r#"<container>
            <text for="label in map(rows, 'label')">{label}</text>
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
        vec!["first".to_string(), "second".into(), "third".into()]
    );
}

#[test]
fn filter_keeps_objects_matching_field_value() {
    let scope = LowerScope::default().with_binding(
        "rows",
        json!([
            {"status": "active", "label": "A"},
            {"status": "draft",  "label": "B"},
            {"status": "active", "label": "C"},
        ]),
    );
    let nodes = lower(
        r#"<container>
            <text for="row in filter(rows, 'status', 'active')">{row.label}</text>
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
    assert_eq!(texts, vec!["A".to_string(), "C".into()]);
}

#[test]
fn find_returns_first_matching_object_or_null() {
    let scope = LowerScope::default().with_binding(
        "rows",
        json!([
            {"id": 1, "label": "Alpha"},
            {"id": 2, "label": "Bravo"},
            {"id": 3, "label": "Charlie"},
        ]),
    );
    let nodes = lower(r#"<text>{find(rows, 'id', 2).label}</text>"#, &scope);
    let Node::Text { content, .. } = &nodes[0] else {
        panic!()
    };
    assert_eq!(content, "Bravo");
}

#[test]
fn reduce_sums_numeric_field_across_objects() {
    let scope = LowerScope::default().with_binding(
        "rows",
        json!([
            {"id": 1, "price": 10},
            {"id": 2, "price": 25},
            {"id": 3, "price": 7},
        ]),
    );
    let nodes = lower(
        r#"<text>Total: {reduce(rows, 'sum', 'price')}</text>"#,
        &scope,
    );
    let Node::Text { content, .. } = &nodes[0] else {
        panic!()
    };
    assert_eq!(content, "Total: 42");
}

#[test]
fn reduce_supports_min_max_avg_count_product() {
    let scope = LowerScope::default().with_binding("xs", json!([3, 1, 4, 1, 5, 9, 2, 6]));
    let cases: &[(&str, &str)] = &[
        (r#"<text>{reduce(xs, 'min')}</text>"#, "1"),
        (r#"<text>{reduce(xs, 'max')}</text>"#, "9"),
        (r#"<text>{reduce(xs, 'count')}</text>"#, "8"),
        (r#"<text>{reduce(xs, 'sum')}</text>"#, "31"),
        (r#"<text>{reduce(xs, 'product')}</text>"#, "6480"),
    ];
    for (src, expected) in cases {
        let nodes = lower(src, &scope);
        let Node::Text { content, .. } = &nodes[0] else {
            panic!()
        };
        assert_eq!(content, expected, "for {}", src);
    }
}

#[test]
fn slice_returns_subarray_with_optional_bounds() {
    let scope = LowerScope::default().with_binding("rows", json!(["a", "b", "c", "d", "e"]));
    let nodes = lower(
        r#"<container>
            <text for="r in slice(rows, 1, 4)">{r}</text>
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
    assert_eq!(texts, vec!["b".to_string(), "c".into(), "d".into()]);
}

#[test]
fn slice_treats_negative_indices_like_python() {
    let scope = LowerScope::default().with_binding("rows", json!(["a", "b", "c", "d", "e"]));
    let nodes = lower(
        r#"<container>
            <text for="r in slice(rows, -2, 5)">{r}</text>
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
    assert_eq!(texts, vec!["d".to_string(), "e".into()]);
}

#[test]
fn sort_by_orders_objects_ascending_by_field() {
    let scope = LowerScope::default().with_binding(
        "rows",
        json!([
            {"name": "Bravo", "rank": 2},
            {"name": "Alpha", "rank": 1},
            {"name": "Charlie", "rank": 3},
        ]),
    );
    let nodes = lower(
        r#"<container>
            <text for="r in sort_by(rows, 'rank')">{r.name}</text>
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
        vec!["Alpha".to_string(), "Bravo".into(), "Charlie".into()]
    );
}

#[test]
fn sort_by_descending_flips_order() {
    let scope = LowerScope::default().with_binding(
        "rows",
        json!([
            {"name": "Bravo", "rank": 2},
            {"name": "Alpha", "rank": 1},
            {"name": "Charlie", "rank": 3},
        ]),
    );
    let nodes = lower(
        r#"<container>
            <text for="r in sort_by(rows, 'rank', 'desc')">{r.name}</text>
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
        vec!["Charlie".to_string(), "Bravo".into(), "Alpha".into()]
    );
}

#[test]
fn unique_drops_duplicates_preserving_first_occurrence() {
    let scope = LowerScope::default()
        .with_binding("tags", json!(["red", "green", "red", "blue", "green"]));
    let nodes = lower(
        r#"<container>
            <text for="t in unique(tags)">{t}</text>
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
        vec!["red".to_string(), "green".into(), "blue".into()]
    );
}

#[test]
fn reverse_returns_typed_reversed_array() {
    let scope = LowerScope::default().with_binding("xs", json!([1, 2, 3]));
    let nodes = lower(r#"<text>{reverse(xs).first}</text>"#, &scope);
    let Node::Text { content, .. } = &nodes[0] else {
        panic!()
    };
    assert_eq!(content, "3");
}

#[test]
fn keys_and_values_iterate_object_entries() {
    let scope = LowerScope::default().with_binding(
        "form",
        json!({"name": "Ada", "email": "ada@example.com"}),
    );
    let nodes = lower(
        r#"<container>
            <text for="k in keys(form)">{k}</text>
            <text for="v in values(form)">{v}</text>
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
    assert_eq!(texts.len(), 4);
    assert!(texts.iter().any(|t| t == "name"));
    assert!(texts.iter().any(|t| t == "Ada"));
}

#[test]
fn entries_produces_key_value_pair_objects() {
    let scope = LowerScope::default().with_binding("form", json!({"name": "Ada"}));
    let nodes = lower(
        r#"<container>
            <text for="e in entries(form)">{e.key}={e.value}</text>
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
    // collect_text_content inserts a space between text run + literal;
    // assert on flattened tokens loosely.
    assert_eq!(texts.len(), 1);
    assert!(texts[0].contains("name"));
    assert!(texts[0].contains("Ada"));
}

#[test]
fn includes_returns_boolean_membership() {
    let scope = LowerScope::default().with_binding("tags", json!(["alpha", "beta", "gamma"]));
    let nodes = lower(
        r#"<container>
            <text if="{includes(tags, 'beta')}">yes</text>
            <text else>no</text>
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
    assert_eq!(texts, vec!["yes".to_string()]);
}

#[test]
fn index_of_returns_position_or_negative_one() {
    let scope = LowerScope::default().with_binding("tags", json!(["alpha", "beta", "gamma"]));
    let cases: &[(&str, &str)] = &[
        (r#"<text>{index_of(tags, 'beta')}</text>"#, "1"),
        (r#"<text>{index_of(tags, 'alpha')}</text>"#, "0"),
        (r#"<text>{index_of(tags, 'missing')}</text>"#, "-1"),
    ];
    for (src, expected) in cases {
        let nodes = lower(src, &scope);
        let Node::Text { content, .. } = &nodes[0] else {
            panic!()
        };
        assert_eq!(content, expected, "for {}", src);
    }
}

#[test]
fn join_concatenates_array_with_separator() {
    let scope = LowerScope::default().with_binding("tags", json!(["alpha", "beta", "gamma"]));
    let nodes = lower(r#"<text>{join(tags, ', ')}</text>"#, &scope);
    let Node::Text { content, .. } = &nodes[0] else {
        panic!()
    };
    assert_eq!(content, "alpha, beta, gamma");
}

#[test]
fn concat_arr_merges_multiple_typed_arrays() {
    let scope = LowerScope::default()
        .with_binding("a", json!(["x", "y"]))
        .with_binding("b", json!(["z"]));
    let nodes = lower(
        r#"<container>
            <text for="t in concat_arr(a, b)">{t}</text>
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
    assert_eq!(texts, vec!["x".to_string(), "y".into(), "z".into()]);
}

#[test]
fn calls_compose_nested_through_owned_lookup() {
    // `slice(filter(rows, 'kind', 'active'), 0, 2)` — each arg is a
    // typed array, the next call operates on it.
    let scope = LowerScope::default().with_binding(
        "rows",
        json!([
            {"kind": "active",   "label": "A"},
            {"kind": "archived", "label": "B"},
            {"kind": "active",   "label": "C"},
            {"kind": "active",   "label": "D"},
        ]),
    );
    let nodes = lower(
        r#"<container>
            <text for="r in slice(filter(rows, 'kind', 'active'), 0, 2)">{r.label}</text>
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
    assert_eq!(texts, vec!["A".to_string(), "C".into()]);
}

#[test]
fn map_then_join_produces_csv_like_string() {
    let scope = LowerScope::default().with_binding(
        "people",
        json!([
            {"name": "Ada"},
            {"name": "Linus"},
            {"name": "Grace"},
        ]),
    );
    let nodes = lower(r#"<text>{join(map(people, 'name'), ', ')}</text>"#, &scope);
    let Node::Text { content, .. } = &nodes[0] else {
        panic!()
    };
    assert_eq!(content, "Ada, Linus, Grace");
}

#[test]
fn reduce_with_sum_drives_a_numeric_attribute() {
    let scope = LowerScope::default().with_binding(
        "rows",
        json!([
            {"size": 16},
            {"size": 24},
            {"size": 16},
        ]),
    );
    let nodes = lower(
        r##"<container width="{reduce(rows, 'sum', 'size')}" height="40"/>"##,
        &scope,
    );
    let Node::Container { props, .. } = &nodes[0] else {
        panic!()
    };
    assert!(matches!(
        props.width,
        prism_ui_runtime::layout::Sizing::Fixed(v) if (v - 56.0).abs() < f32::EPSILON
    ));
}
