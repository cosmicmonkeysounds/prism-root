use super::*;
use crate::command::Color;
use crate::layout::{compute, ContainerProps, Sizing, TextProps, Viewport};
use serde_json::json;

use super::style::{parse_f32, parse_sizing};

const FIVE_ELEMENT_SOURCE: &str = r##"<container direction="column" gap="8" padding="16" width="grow" height="grow" style:background="#f0f0f0">
  <text id="title" font-size="24" style:color="#141414">Prism</text>
  <container id="row" direction="row" gap="8" width="grow" height="40" style:background="#ffffff">
<text id="a">A</text>
<spacer id="gap" width="16" height="0"/>
<text id="b">B</text>
  </container>
</container>"##;

#[test]
fn parses_minimal_container() {
    let nodes = interpret("<container/>").unwrap();
    assert_eq!(nodes.len(), 1);
    assert!(matches!(nodes[0], Node::Container { .. }));
}

#[test]
fn parses_text_content() {
    let nodes = interpret(r#"<text>Hello</text>"#).unwrap();
    let Node::Text { content, .. } = &nodes[0] else {
        panic!("expected text");
    };
    assert_eq!(content, "Hello");
}

#[test]
fn parses_color_and_sizing() {
    let nodes =
        interpret(r##"<container width="grow" height="40" style:background="#ff8800"/>"##).unwrap();
    let Node::Container { props, .. } = &nodes[0] else {
        panic!()
    };
    assert!(matches!(props.width, Sizing::Grow));
    assert!(matches!(props.height, Sizing::Fixed(v) if (v - 40.0).abs() < f32::EPSILON));
    let bg = props.background.expect("background");
    assert_eq!((bg.r, bg.g, bg.b, bg.a), (0xff, 0x88, 0x00, 0xff));
}

#[test]
fn heading_level_drives_font_size() {
    let nodes = interpret(r#"<heading level="3">Hi</heading>"#).unwrap();
    let Node::Text { props, .. } = &nodes[0] else {
        panic!()
    };
    assert_eq!(props.font_size, 20.0);
}

#[test]
fn five_element_source_round_trips_to_layout() {
    let nodes = interpret(FIVE_ELEMENT_SOURCE).unwrap();
    assert_eq!(nodes.len(), 1);
    let cmds = compute(
        &nodes[0],
        Viewport {
            width: 800.0,
            height: 600.0,
        },
    );
    assert_eq!(cmds.len(), 5);
}

#[test]
fn parse_errors_propagate() {
    let err = interpret("<container>").unwrap_err();
    assert!(!err.is_empty());
}

// ---------- Slots ----------

#[test]
fn slot_falls_back_to_default_children_when_no_binding() {
    let nodes = interpret(r#"<container><slot><text>fallback</text></slot></container>"#).unwrap();
    let Node::Container { children, .. } = &nodes[0] else {
        panic!()
    };
    assert_eq!(children.len(), 1);
    let Node::Text { content, .. } = &children[0] else {
        panic!()
    };
    assert_eq!(content, "fallback");
}

#[test]
fn slot_resolves_default_binding_from_scope() {
    let (doc, errs) = parse(r#"<container><slot/></container>"#);
    assert!(errs.is_empty());
    // Caller injects a `<text>injected</text>` AST node as the
    // default slot — same shape a Phase-3 component-instantiation
    // pass would feed in.
    let (injected, errs) = parse(r#"<text>injected</text>"#);
    assert!(errs.is_empty());
    let scope =
        LowerScope::default().with_slots(SlotBindings::default().with_default(injected.nodes));
    let nodes = lower_document_with_scope(&doc, &scope);
    let Node::Container { children, .. } = &nodes[0] else {
        panic!()
    };
    let Node::Text { content, .. } = &children[0] else {
        panic!()
    };
    assert_eq!(content, "injected");
}

#[test]
fn slot_named_falls_through_to_host_children_by_slot_map() {
    // Wave 13.1 — when no AST-level SlotBindings carries `header`,
    // the runtime falls through to `host_children_by_slot["header"]`.
    // The resolver populates this from a dispatched element's
    // `slot="X"` AST children; here we set it directly.
    let (doc, errs) = parse(r#"<container><slot name="header"/></container>"#);
    assert!(errs.is_empty());
    let mut map: HashMap<String, Vec<Node>> = HashMap::new();
    map.insert(
        "header".into(),
        vec![Node::Text {
            id: "hdr".into(),
            content: "FROM-HOST".into(),
            props: TextProps::default(),
        }],
    );
    let scope = LowerScope::default().with_host_children_by_slot(Arc::new(map));
    let nodes = lower_document_with_scope(&doc, &scope);
    let Node::Container { children, .. } = &nodes[0] else {
        panic!()
    };
    assert_eq!(children.len(), 1);
    let Node::Text { content, .. } = &children[0] else {
        panic!()
    };
    assert_eq!(content, "FROM-HOST");
}

#[test]
fn slot_unknown_name_falls_back_to_fallback_children() {
    // No binding for `nonexistent` — the element's own children
    // (the fallback) render instead.
    let (doc, _) =
        parse(r#"<container><slot name="nonexistent"><text>FALLBACK</text></slot></container>"#);
    let scope = LowerScope::default();
    let nodes = lower_document_with_scope(&doc, &scope);
    let Node::Container { children, .. } = &nodes[0] else {
        panic!()
    };
    assert_eq!(children.len(), 1);
    let Node::Text { content, .. } = &children[0] else {
        panic!()
    };
    assert_eq!(content, "FALLBACK");
}

#[test]
fn slot_named_binding_isolates_from_default() {
    let (doc, _) = parse(
        r#"<container>
            <slot name="header"/>
            <slot/>
        </container>"#,
    );
    let (header, _) = parse(r#"<text>HEAD</text>"#);
    let (default, _) = parse(r#"<text>BODY</text>"#);
    let scope = LowerScope::default().with_slots(
        SlotBindings::default()
            .with_named("header", header.nodes)
            .with_default(default.nodes),
    );
    let nodes = lower_document_with_scope(&doc, &scope);
    let Node::Container { children, .. } = &nodes[0] else {
        panic!()
    };
    assert_eq!(children.len(), 2);
    if let (Node::Text { content: a, .. }, Node::Text { content: b, .. }) =
        (&children[0], &children[1])
    {
        assert_eq!(a, "HEAD");
        assert_eq!(b, "BODY");
    } else {
        panic!("expected two text children")
    }
}

// ---------- Control flow ----------

#[test]
fn if_drops_subtree_when_binding_falsy() {
    let (doc, _) = parse(r#"<container><text if="show">visible</text></container>"#);
    let scope = LowerScope::default().with_binding("show", json!(false));
    let nodes = lower_document_with_scope(&doc, &scope);
    let Node::Container { children, .. } = &nodes[0] else {
        panic!()
    };
    assert!(children.is_empty(), "if=false drops the element");
}

#[test]
fn if_keeps_subtree_when_binding_truthy() {
    let (doc, _) = parse(r#"<container><text if="show">visible</text></container>"#);
    let scope = LowerScope::default().with_binding("show", json!(true));
    let nodes = lower_document_with_scope(&doc, &scope);
    let Node::Container { children, .. } = &nodes[0] else {
        panic!()
    };
    assert_eq!(children.len(), 1);
}

#[test]
fn else_if_chain_picks_first_truthy_branch() {
    let (doc, _) = parse(
        r#"<container>
            <text if="a">A</text>
            <text else-if="b">B</text>
            <text else>fallback</text>
        </container>"#,
    );
    let scope = LowerScope::default()
        .with_binding("a", json!(false))
        .with_binding("b", json!(true));
    let nodes = lower_document_with_scope(&doc, &scope);
    let Node::Container { children, .. } = &nodes[0] else {
        panic!()
    };
    assert_eq!(children.len(), 1);
    let Node::Text { content, .. } = &children[0] else {
        panic!()
    };
    assert_eq!(content, "B");
}

#[test]
fn else_falls_through_when_all_predicates_false() {
    let (doc, _) = parse(
        r#"<container>
            <text if="a">A</text>
            <text else>fallback</text>
        </container>"#,
    );
    let scope = LowerScope::default().with_binding("a", json!(false));
    let nodes = lower_document_with_scope(&doc, &scope);
    let Node::Container { children, .. } = &nodes[0] else {
        panic!()
    };
    let Node::Text { content, .. } = &children[0] else {
        panic!()
    };
    assert_eq!(content, "fallback");
}

#[test]
fn else_if_chain_survives_interleaved_comments() {
    // Regression: `field-editor.prui` documents every branch
    // with a leading `<!-- … -->`. Comments are never rendered
    // output, so they must not reset the if/else-if/else chain —
    // otherwise only the standalone `<text if>` survives and every
    // editor kind body silently vanishes (the "Properties panel
    // shows only labels" bug).
    let (doc, _) = parse(
        r#"<container>
            <!-- label always shows -->
            <text if="show_label">Label</text>
            <!-- ── boolean branch ── -->
            <text if="is_bool">BOOL</text>
            <!-- ── select branch ── -->
            <text else-if="is_select">SELECT</text>
            <!-- ── text fallback ── -->
            <text else>TEXT</text>
        </container>"#,
    );
    let scope = LowerScope::default()
        .with_binding("show_label", json!(true))
        .with_binding("is_bool", json!(false))
        .with_binding("is_select", json!(false));
    let nodes = lower_document_with_scope(&doc, &scope);
    let Node::Container { children, .. } = &nodes[0] else {
        panic!()
    };
    // Label + the `else` fallback — the chain must reach `else`
    // even though a comment precedes every branch.
    let texts: Vec<&str> = children
        .iter()
        .filter_map(|c| match c {
            Node::Text { content, .. } => Some(content.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(texts, vec!["Label", "TEXT"]);
}

#[test]
fn for_loop_supports_dotted_field_access_on_object_items() {
    let scope = LowerScope::default().with_binding(
        "items",
        serde_json::json!([
            { "label": "Alpha", "depth": 0 },
            { "label": "Beta", "depth": 2 },
        ]),
    );
    let (doc, errs) = parse(
        r#"<container><text for="item in items">{item.label}={item.depth}</text></container>"#,
    );
    assert!(errs.is_empty());
    let nodes = lower_document_with_scope(&doc, &scope);
    let Node::Container { children, .. } = &nodes[0] else {
        panic!()
    };
    assert_eq!(children.len(), 2);
    let Node::Text { content: a, .. } = &children[0] else {
        panic!()
    };
    let Node::Text { content: b, .. } = &children[1] else {
        panic!()
    };
    assert!(a.contains("Alpha") && a.contains('0'));
    assert!(b.contains("Beta") && b.contains('2'));
}

#[test]
fn for_clones_children_with_iteration_binding() {
    let (doc, _) = parse(
        r#"<container>
            <text for="post in posts">{post}</text>
        </container>"#,
    );
    let scope = LowerScope::default().with_binding("posts", json!(["alpha", "beta", "gamma"]));
    let nodes = lower_document_with_scope(&doc, &scope);
    let Node::Container { children, .. } = &nodes[0] else {
        panic!()
    };
    assert_eq!(children.len(), 3);
    let contents: Vec<&str> = children
        .iter()
        .map(|c| {
            if let Node::Text { content, .. } = c {
                content.as_str()
            } else {
                ""
            }
        })
        .collect();
    assert_eq!(contents, vec!["alpha", "beta", "gamma"]);
}

#[test]
fn for_with_missing_binding_yields_no_children() {
    let (doc, _) = parse(r#"<container><text for="x in missing">{x}</text></container>"#);
    let nodes = lower_document_with_scope(&doc, &LowerScope::default());
    let Node::Container { children, .. } = &nodes[0] else {
        panic!()
    };
    assert!(children.is_empty());
}

// ---------- Teleport (Wave 14.3) ----------

/// `<teleport to="overlay-root">` moves its children to the
/// container with `id="overlay-root"`. At the source position
/// the teleport emits nothing.
#[test]
fn teleport_routes_children_to_target_id() {
    let nodes = interpret(
        r#"<container>
             <container id="overlay-root"/>
             <container>
               <teleport to="overlay-root">
                 <text>routed</text>
               </teleport>
             </container>
           </container>"#,
    )
    .unwrap();
    let Node::Container { children, .. } = &nodes[0] else {
        panic!()
    };
    // Two children: the overlay root + the sibling that hosted
    // the teleport. The teleport itself emits nothing — its
    // child container has zero children.
    assert_eq!(children.len(), 2);
    let Node::Container {
        id: overlay_id,
        children: overlay_children,
        ..
    } = &children[0]
    else {
        panic!("expected overlay-root container")
    };
    assert_eq!(overlay_id, "overlay-root");
    assert_eq!(
        overlay_children.len(),
        1,
        "teleport payload landed at the target"
    );
    let Node::Text { content, .. } = &overlay_children[0] else {
        panic!("expected text payload")
    };
    assert_eq!(content, "routed");

    // The teleport's source sibling has no children — its
    // `<teleport>` body materialised at the target, not here.
    let Node::Container {
        children: source_children,
        ..
    } = &children[1]
    else {
        panic!("expected source-side container")
    };
    assert!(source_children.is_empty());
}

/// A teleport with no matching target id silently drops its
/// payload — matches Vue's behaviour.
#[test]
fn teleport_with_missing_target_drops_payload() {
    let nodes = interpret(
        r#"<container>
             <teleport to="nowhere"><text>lost</text></teleport>
           </container>"#,
    )
    .unwrap();
    let Node::Container { children, .. } = &nodes[0] else {
        panic!()
    };
    assert!(children.is_empty());
}

/// Multiple teleports targeting the same id stack their payloads
/// in source order.
#[test]
fn teleport_multiple_targets_stack_in_source_order() {
    let nodes = interpret(
        r#"<container>
             <container id="stack"/>
             <teleport to="stack"><text>first</text></teleport>
             <teleport to="stack"><text>second</text></teleport>
           </container>"#,
    )
    .unwrap();
    let Node::Container { children, .. } = &nodes[0] else {
        panic!()
    };
    let Node::Container {
        children: stack_children,
        ..
    } = &children[0]
    else {
        panic!("expected stack target")
    };
    let labels: Vec<&str> = stack_children
        .iter()
        .filter_map(|n| match n {
            Node::Text { content, .. } => Some(content.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(labels, vec!["first", "second"]);
}

// ---------- Memo (Wave 14.3) ----------

/// `memo="…"` is a no-op when no cache is installed — the
/// element lowers normally (proves the round-trip-through-cache
/// path doesn't depend on host wiring for default behaviour).
#[test]
fn memo_without_cache_lowers_element_normally() {
    let (doc, _) = parse(r#"<container id="x" memo="count"><text>hi</text></container>"#);
    let scope = LowerScope::default().with_binding("count", json!(1));
    let nodes = lower_document_with_scope(&doc, &scope);
    assert_eq!(nodes.len(), 1);
    if let Node::Container { children, .. } = &nodes[0] {
        assert_eq!(children.len(), 1);
    } else {
        panic!("expected container")
    }
}

/// With a cache installed, the second lowering call with the
/// same dep tuple returns the cached subtree verbatim and does
/// **not** re-evaluate the element body — proven here by
/// flipping the underlying binding the *body* reads. The cached
/// subtree shows the old text because the memo gate keeps the
/// re-evaluation from happening.
#[test]
fn memo_with_cache_returns_cached_subtree_when_deps_unchanged() {
    use std::cell::RefCell;
    use std::rc::Rc;

    let (doc, _) = parse(r#"<container id="x" memo="count"><text>{label}</text></container>"#);
    let cache = Rc::new(RefCell::new(MemoCache::new()));

    let first = lower_document_with_scope(
        &doc,
        &LowerScope::default()
            .with_binding("count", json!(1))
            .with_binding("label", json!("first"))
            .with_memo_cache(Rc::clone(&cache)),
    );
    let Node::Container { children, .. } = &first[0] else {
        panic!()
    };
    let Node::Text { content, .. } = &children[0] else {
        panic!()
    };
    assert_eq!(content, "first");
    assert_eq!(cache.borrow().len(), 1);

    // Same dep, different label: memo gate should bypass the
    // re-lower, so the rendered text stays "first".
    let second = lower_document_with_scope(
        &doc,
        &LowerScope::default()
            .with_binding("count", json!(1))
            .with_binding("label", json!("second"))
            .with_memo_cache(Rc::clone(&cache)),
    );
    let Node::Container { children, .. } = &second[0] else {
        panic!()
    };
    let Node::Text { content, .. } = &children[0] else {
        panic!()
    };
    assert_eq!(content, "first", "memo cache hit preserved old subtree");
}

/// When any dep moves, the cache invalidates and the body
/// re-evaluates against the fresh scope.
#[test]
fn memo_with_cache_re_evaluates_when_a_dep_changes() {
    use std::cell::RefCell;
    use std::rc::Rc;

    let (doc, _) = parse(r#"<container id="x" memo="count"><text>{label}</text></container>"#);
    let cache = Rc::new(RefCell::new(MemoCache::new()));

    let _ = lower_document_with_scope(
        &doc,
        &LowerScope::default()
            .with_binding("count", json!(1))
            .with_binding("label", json!("first"))
            .with_memo_cache(Rc::clone(&cache)),
    );

    let second = lower_document_with_scope(
        &doc,
        &LowerScope::default()
            .with_binding("count", json!(2))
            .with_binding("label", json!("second"))
            .with_memo_cache(Rc::clone(&cache)),
    );
    let Node::Container { children, .. } = &second[0] else {
        panic!()
    };
    let Node::Text { content, .. } = &children[0] else {
        panic!()
    };
    assert_eq!(content, "second", "dep change invalidated the cache");
}

/// **Phase 3** — with a dirty NodeId set installed, a clean
/// id'd subtree is spliced verbatim from cache (its body is not
/// re-evaluated against the fresh scope) while a dirty one
/// re-lowers. Mirrors the shell's reactive redraw: only the
/// blocks whose signals fired re-render.
#[test]
fn phase3_dirty_set_splices_clean_subtree_relowers_dirty() {
    use std::cell::RefCell;
    use std::collections::HashSet;
    use std::rc::Rc;

    let (doc, _) = parse(
        r#"<container id="root">
             <container id="a"><text>{av}</text></container>
             <container id="b"><text>{bv}</text></container>
           </container>"#,
    );
    let cache = Rc::new(RefCell::new(MemoCache::new()));

    // Full pass (no dirty set) warms the per-id cache.
    let _ = lower_document_with_scope(
        &doc,
        &LowerScope::default()
            .with_binding("av", json!("a1"))
            .with_binding("bv", json!("b1"))
            .with_memo_cache(Rc::clone(&cache)),
    );

    // Reactive pass: only `b` is dirty. `av`/`bv` both change in
    // scope, but `a` (clean) must splice its OLD subtree while
    // `b` (dirty) re-lowers against the fresh binding.
    let dirty: Rc<HashSet<String>> = Rc::new(["b".to_string()].into_iter().collect());
    cache.borrow_mut().begin_pass();
    let second = lower_document_with_scope(
        &doc,
        &LowerScope::default()
            .with_binding("av", json!("a2"))
            .with_binding("bv", json!("b2"))
            .with_memo_cache(Rc::clone(&cache))
            .with_dirty_nodes(Rc::clone(&dirty)),
    );
    let Node::Container { children: root, .. } = &second[0] else {
        panic!()
    };
    let text_of = |n: &Node| -> String {
        let Node::Container { children, .. } = n else {
            panic!()
        };
        let Node::Text { content, .. } = &children[0] else {
            panic!()
        };
        content.clone()
    };
    assert_eq!(text_of(&root[0]), "a1", "clean subtree spliced from cache");
    assert_eq!(text_of(&root[1]), "b2", "dirty subtree re-lowered");
    // The non-lossy guard: the dirty id `b` was re-lowered, so it
    // is recorded as touched (the shell would present this frame
    // without a full-walk fallback).
    assert!(cache.borrow().untouched(dirty.iter()).is_empty());
}

/// `memo=` without an id never enters the cache — there'd be
/// no stable key.
#[test]
fn memo_without_id_is_a_noop() {
    use std::cell::RefCell;
    use std::rc::Rc;

    let (doc, _) = parse(r#"<container memo="count"><text>{label}</text></container>"#);
    let cache = Rc::new(RefCell::new(MemoCache::new()));
    let _ = lower_document_with_scope(
        &doc,
        &LowerScope::default()
            .with_binding("count", json!(1))
            .with_binding("label", json!("first"))
            .with_memo_cache(Rc::clone(&cache)),
    );
    assert!(cache.borrow().is_empty());
}

// ---------- TextInput ----------

#[test]
fn input_lowers_to_text_input_node_with_value() {
    let nodes = interpret(r#"<input value="hello" placeholder="search..."/>"#).unwrap();
    assert_eq!(nodes.len(), 1);
    let Node::TextInput {
        value, placeholder, ..
    } = &nodes[0]
    else {
        panic!("expected TextInput, got {:?}", nodes[0])
    };
    assert_eq!(value, "hello");
    assert_eq!(placeholder, "search...");
}

#[test]
fn image_lowers_to_image_node_with_source_and_sizing() {
    let nodes = interpret(
        r##"<image src="icons/chevron-down.svg" width="10" height="10"
                   style:radius="2" style:tint="#cc000000" aria-label="open"/>"##,
    )
    .unwrap();
    assert_eq!(nodes.len(), 1);
    let Node::Image {
        source,
        width,
        height,
        radius,
        tint,
        semantic,
        ..
    } = &nodes[0]
    else {
        panic!("expected Image, got {:?}", nodes[0])
    };
    assert_eq!(source, "icons/chevron-down.svg");
    assert!(matches!(width, Sizing::Fixed(v) if (v - 10.0).abs() < f32::EPSILON));
    assert!(matches!(height, Sizing::Fixed(v) if (v - 10.0).abs() < f32::EPSILON));
    assert!((radius.tl - 2.0).abs() < f32::EPSILON);
    assert!(tint.is_some());
    assert_eq!(semantic.aria_label.as_deref(), Some("open"));
}

#[test]
fn image_data_and_aria_namespaces_round_trip_on_semantic() {
    let nodes =
        interpret(r##"<image src="icons/x.svg" data:role="close-icon" aria:hidden="true"/>"##)
            .unwrap();
    let Node::Image { semantic, .. } = &nodes[0] else {
        panic!("expected Image")
    };
    assert!(semantic
        .attrs
        .iter()
        .any(|(k, v)| k == "data-role" && v == "close-icon"));
    assert!(semantic
        .attrs
        .iter()
        .any(|(k, v)| k == "aria-hidden" && v == "true"));
}

#[test]
fn input_kind_string_is_stable() {
    let nodes = interpret(r#"<input value="x"/>"#).unwrap();
    assert_eq!(nodes[0].kind(), "text-input");
}

/// Wave 14.3 — `bind:value="<node-id>.<key>"` on `<input>` lowers
/// to a `data-bind-value` semantic attr on the input's node. The
/// shell event router reads it back at pointer-down time to open
/// a field-focus session against the bound source.
#[test]
fn input_bind_value_lowers_to_data_bind_value_attr() {
    let nodes = interpret(r#"<input bind:value="form.email"/>"#).unwrap();
    let Node::TextInput { semantic, .. } = &nodes[0] else {
        panic!("expected TextInput")
    };
    assert!(
        semantic
            .attrs
            .iter()
            .any(|(k, v)| k == "data-bind-value" && v == "form.email"),
        "bind:value should round-trip onto the input's semantic.attrs"
    );
}

/// `aria:*` / `data:*` on inputs round-trip onto `semantic.attrs`
/// so SSR + hit-test routing work the same as they do on
/// containers.
#[test]
fn input_data_and_aria_namespaces_round_trip_on_semantic() {
    let nodes =
        interpret(r#"<input value="x" data:role="email-field" aria:label="Email"/>"#).unwrap();
    let Node::TextInput { semantic, .. } = &nodes[0] else {
        panic!("expected TextInput")
    };
    assert!(semantic
        .attrs
        .iter()
        .any(|(k, v)| k == "data-role" && v == "email-field"));
    assert!(semantic
        .attrs
        .iter()
        .any(|(k, v)| k == "aria-label" && v == "Email"));
}

#[test]
fn input_round_trips_through_layout_and_emits_three_commands() {
    let nodes = interpret(r#"<input value="hi" width="200" height="32"/>"#).unwrap();
    let cmds = compute(
        &nodes[0],
        Viewport {
            width: 800.0,
            height: 600.0,
        },
    );
    // Background rectangle + border + text = 3 commands.
    assert_eq!(cmds.len(), 3);
}

// ---------- TagResolver ----------

/// Stand-in resolver for the unit tests — turns `<my.box>` into a
/// fixed-size container, leaves every other tag untouched. The
/// real resolver lives in `prism-builder` and dispatches through
/// `ComponentRegistry`.
struct FakeResolver;
impl TagResolver for FakeResolver {
    fn resolve(&self, element: &Element, _scope: &LowerScope) -> Option<Vec<Node>> {
        if element.tag != "my.box" {
            return None;
        }
        Some(vec![Node::Container {
            id: "from-resolver".into(),
            props: ContainerProps {
                width: Sizing::Fixed(40.0),
                height: Sizing::Fixed(40.0),
                ..Default::default()
            },
            children: vec![],
        }])
    }
}

#[test]
fn resolver_handles_unknown_tag_when_returning_some() {
    let (doc, errs) = parse(r#"<container><my.box/></container>"#);
    assert!(errs.is_empty());
    let scope = LowerScope::default().with_resolver(Arc::new(FakeResolver));
    let nodes = lower_document_with_scope(&doc, &scope);
    let Node::Container { children, .. } = &nodes[0] else {
        panic!()
    };
    assert_eq!(children.len(), 1);
    let Node::Container {
        id, props: cprops, ..
    } = &children[0]
    else {
        panic!("resolver did not produce container")
    };
    assert_eq!(id, "from-resolver");
    assert_eq!(cprops.width, Sizing::Fixed(40.0));
}

#[test]
fn resolver_returning_none_falls_back_to_default_unknown_tag() {
    // `<scene>` is not handled by FakeResolver, so it falls through
    // to the runtime's default "drop the wrapper, keep children"
    // behaviour — same shape as the no-resolver case.
    let (doc, _) = parse(r#"<scene><text>kept</text></scene>"#);
    let scope = LowerScope::default().with_resolver(Arc::new(FakeResolver));
    let nodes = lower_document_with_scope(&doc, &scope);
    assert_eq!(nodes.len(), 1);
    let Node::Text { content, .. } = &nodes[0] else {
        panic!()
    };
    assert_eq!(content, "kept");
}

#[test]
fn host_children_by_tag_round_trips_through_scope_getter() {
    // Pin the new injection seam: a host-supplied map keyed by tag
    // surfaces through `host_children_for(tag)` and clones cheaply
    // through scope forks (the `Arc` discipline).
    let mut map: HashMap<String, Vec<Node>> = HashMap::new();
    map.insert(
        "my.canvas".into(),
        vec![Node::Text {
            id: "leaf".into(),
            content: "from-host".into(),
            props: TextProps::default(),
        }],
    );
    let scope = LowerScope::default().with_host_children_by_tag(map);
    let supplied = scope.host_children_for("my.canvas").expect("entry");
    assert_eq!(supplied.len(), 1);
    assert!(scope.host_children_for("absent").is_none());
    // Clone propagates the Arc — every fork sees the same entries
    // without re-cloning the underlying Vec<Node>.
    let forked = scope.clone();
    assert!(forked.host_children_for("my.canvas").is_some());
}

#[test]
fn resolver_propagates_through_for_loop_child_scopes() {
    let (doc, _) = parse(r#"<container><my.box for="x in items"/></container>"#);
    let scope = LowerScope::default()
        .with_binding("items", json!([1, 2, 3]))
        .with_resolver(Arc::new(FakeResolver));
    let nodes = lower_document_with_scope(&doc, &scope);
    let Node::Container { children, .. } = &nodes[0] else {
        panic!()
    };
    assert_eq!(children.len(), 3, "resolver fired once per iteration");
}

/// §43 A1: `on:<event>="<action>"` on a bare `<container>`
/// lowers to a `data-on-<event>` semantic attribute that the
/// shell event router reads at pointer-down time. The runtime
/// is intentionally action-grammar-agnostic — it preserves the
/// raw string and lets the host parse it via
/// `prism_builder::signal::parse_action`.
#[test]
fn on_event_attribute_lowers_to_data_on_attr_on_container() {
    let nodes = interpret(r#"<container id="btn" on:click="emit save" on:hover="cmd help.show"/>"#)
        .unwrap();
    let crate::layout::Node::Container { id, props, .. } = &nodes[0] else {
        panic!("expected container, got {:?}", nodes[0])
    };
    assert_eq!(id, "btn");
    let attrs: std::collections::HashMap<_, _> = props
        .semantic
        .attrs
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    assert_eq!(
        attrs.get("data-on-click").map(String::as_str),
        Some("emit save"),
    );
    assert_eq!(
        attrs.get("data-on-hover").map(String::as_str),
        Some("cmd help.show"),
    );
}

/// Wave 9.1 — `route:<key>="<value>"` lowers to a `data-<key>`
/// semantic attribute the hit-test cache reads off. The
/// namespace is sugar — `route:role="x"` and `data:role="x"`
/// emit the same `data-role="x"`.
#[test]
fn route_namespace_lowers_to_data_dash_attr_on_container() {
    let nodes =
        interpret(r#"<container id="btn" route:role="resize-handle" route:direction="br"/>"#)
            .unwrap();
    let crate::layout::Node::Container { props, .. } = &nodes[0] else {
        panic!("expected container, got {:?}", nodes[0])
    };
    let attrs: std::collections::HashMap<_, _> = props
        .semantic
        .attrs
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    assert_eq!(
        attrs.get("data-role").map(String::as_str),
        Some("resize-handle")
    );
    assert_eq!(attrs.get("data-direction").map(String::as_str), Some("br"));
}

/// `data:<key>="<value>"` pass-through stays equivalent to the
/// `route:` namespace — same lowered shape, different
/// authoring vocabulary (data: is the bare pass-through,
/// route: is sugar for the hit-test conventions). Either form
/// reaches the hit cache.
#[test]
fn data_namespace_lowers_to_data_dash_attr_on_container() {
    let nodes =
        interpret(r#"<container data:role="palette-item" data:target-id="text"/>"#).unwrap();
    let crate::layout::Node::Container { props, .. } = &nodes[0] else {
        panic!()
    };
    let attrs: std::collections::HashMap<_, _> = props
        .semantic
        .attrs
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    assert_eq!(
        attrs.get("data-role").map(String::as_str),
        Some("palette-item")
    );
    assert_eq!(
        attrs.get("data-target-id").map(String::as_str),
        Some("text")
    );
}

/// `aria:<role>="<value>"` lowers to an `aria-<role>` semantic
/// attribute — the SSR + HTML backends pick it up verbatim,
/// the runtime hit-test cache does not gate on aria-* attrs
/// today but the data round-trips so the convention stays
/// addressable.
#[test]
fn aria_namespace_lowers_to_aria_dash_attr_on_container() {
    let nodes = interpret(r#"<container aria:label="Resize" aria:hidden="false"/>"#).unwrap();
    let crate::layout::Node::Container { props, .. } = &nodes[0] else {
        panic!()
    };
    let attrs: std::collections::HashMap<_, _> = props
        .semantic
        .attrs
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    assert_eq!(attrs.get("aria-label").map(String::as_str), Some("Resize"));
    assert_eq!(attrs.get("aria-hidden").map(String::as_str), Some("false"));
}

/// Wave 11.2 — bare `tag` / `role` / `aria-label` attrs on
/// `<container>` set the dedicated `Semantic` fields directly.
/// Hand-rolled Rust shell components build these via
/// `Semantic::tag(..).with_role(..).with_aria_label(..)`; the
/// DSL needs the same vocabulary for the `.prui`-authored
/// shell-component migration. The `attrs` vec used by `aria:` /
/// `data:` namespaces is independent — these three set the
/// typed fields the HTML emitter reads at the same seam it
/// always did.
#[test]
fn bare_semantic_attrs_set_dedicated_fields_on_container() {
    let nodes =
        interpret(r#"<container tag="section" role="separator" aria-label="Toolbar divider"/>"#)
            .unwrap();
    let crate::layout::Node::Container { props, .. } = &nodes[0] else {
        panic!()
    };
    assert_eq!(props.semantic.tag.as_deref(), Some("section"));
    assert_eq!(props.semantic.role.as_deref(), Some("separator"));
    assert_eq!(
        props.semantic.aria_label.as_deref(),
        Some("Toolbar divider")
    );
    // The dedicated fields don't double-write into `attrs`.
    assert!(props.semantic.attrs.is_empty());
}

/// Wave 14.6 — `animate:<prop>="<from> <duration>"` lowers to
/// a `data-animate-in-<prop>` semantic attribute the runtime
/// animator consumes on first observe to start an entry
/// transition.
#[test]
fn animate_namespace_lowers_to_data_animate_in_attr() {
    let nodes =
        interpret(r#"<container animate:opacity="0 200ms" animate:gap="0 120ms"/>"#).unwrap();
    let crate::layout::Node::Container { props, .. } = &nodes[0] else {
        panic!()
    };
    let attrs: std::collections::HashMap<_, _> = props
        .semantic
        .attrs
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    assert_eq!(
        attrs.get("data-animate-in-opacity").map(String::as_str),
        Some("0 200ms")
    );
    assert_eq!(
        attrs.get("data-animate-in-gap").map(String::as_str),
        Some("0 120ms")
    );
}

/// Wave 14.8 — `animate:in-<prop>` is the explicit spelling for
/// the entry-transition hint; it lowers to the same
/// `data-animate-in-<prop>` attr the bare `animate:<prop>`
/// shorthand uses. Mixing the two forms on one element is
/// supported (the shell uses the bare form on most blocks but
/// `animate:in-opacity` is what authors will reach for once the
/// `animate:out-*` sister exists).
#[test]
fn animate_in_prefix_lowers_to_data_animate_in_attr() {
    let nodes = interpret(r#"<container animate:in-opacity="0 200ms"/>"#).unwrap();
    let crate::layout::Node::Container { props, .. } = &nodes[0] else {
        panic!()
    };
    let attrs: std::collections::HashMap<_, _> = props
        .semantic
        .attrs
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    assert_eq!(
        attrs.get("data-animate-in-opacity").map(String::as_str),
        Some("0 200ms")
    );
    // No accidental `data-animate-out-*` emission.
    assert!(!attrs.keys().any(|k| k.starts_with("data-animate-out-")));
}

/// Wave 14.8 — `animate:out-<prop>` lowers to
/// `data-animate-out-<prop>` for the runtime animator's pending
/// retention path. Substrate-only today: the data round-trip
/// lands, the painter-side node retention is the next step.
#[test]
fn animate_out_prefix_lowers_to_data_animate_out_attr() {
    let nodes =
        interpret(r#"<container animate:out-opacity="0 250ms" animate:out-padding="0 150ms"/>"#)
            .unwrap();
    let crate::layout::Node::Container { props, .. } = &nodes[0] else {
        panic!()
    };
    let attrs: std::collections::HashMap<_, _> = props
        .semantic
        .attrs
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    assert_eq!(
        attrs.get("data-animate-out-opacity").map(String::as_str),
        Some("0 250ms")
    );
    assert_eq!(
        attrs.get("data-animate-out-padding").map(String::as_str),
        Some("0 150ms")
    );
    assert!(!attrs.keys().any(|k| k.starts_with("data-animate-in-")));
}

/// Wave 14.6 — `style:opacity="0.5"` lowers to
/// `ContainerProps::opacity`. Default `None` means fully
/// opaque; bare values clamp to `[0, 1]`.
#[test]
fn style_opacity_lowers_to_container_opacity() {
    let nodes = interpret(r#"<container style:opacity="0.4"/>"#).unwrap();
    let crate::layout::Node::Container { props, .. } = &nodes[0] else {
        panic!()
    };
    assert!((props.opacity.unwrap() - 0.4).abs() < 1e-5);
}

/// Wave 14.6 — out-of-range opacity values clamp into the
/// `[0, 1]` band so the painter never multiplies by negative
/// or super-unity factors.
#[test]
fn style_opacity_clamps_out_of_range_values() {
    let too_high = interpret(r#"<container style:opacity="1.7"/>"#).unwrap();
    let crate::layout::Node::Container { props, .. } = &too_high[0] else {
        panic!()
    };
    assert_eq!(props.opacity, Some(1.0));
    let too_low = interpret(r#"<container style:opacity="-0.3"/>"#).unwrap();
    let crate::layout::Node::Container { props, .. } = &too_low[0] else {
        panic!()
    };
    assert_eq!(props.opacity, Some(0.0));
}

/// Wave 9.4 — `transition:<prop>="<duration>"` lowers to a
/// `data-transition-<prop>` semantic attribute the
/// `Effect`-driven animator (follow-up) consumes.
#[test]
fn transition_namespace_lowers_to_data_transition_attr() {
    let nodes =
        interpret(r#"<container transition:opacity="200ms" transition:transform="120ms"/>"#)
            .unwrap();
    let crate::layout::Node::Container { props, .. } = &nodes[0] else {
        panic!()
    };
    let attrs: std::collections::HashMap<_, _> = props
        .semantic
        .attrs
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    assert_eq!(
        attrs.get("data-transition-opacity").map(String::as_str),
        Some("200ms")
    );
    assert_eq!(
        attrs.get("data-transition-transform").map(String::as_str),
        Some("120ms")
    );
}

/// §4.1 — `transition:easing="ease-in"` round-trips the keyword
/// verbatim into `data-transition-easing`; the animator's
/// `parse_easing` maps it to the builtin cubic curve.
#[test]
fn transition_easing_keyword_round_trips() {
    let nodes = interpret(r#"<container transition:easing="ease-in"/>"#).unwrap();
    let crate::layout::Node::Container { props, .. } = &nodes[0] else {
        panic!()
    };
    let v = props
        .semantic
        .attrs
        .iter()
        .find(|(k, _)| k == "data-transition-easing")
        .map(|(_, v)| v.as_str());
    assert_eq!(v, Some("ease-in"));
    assert_eq!(
        crate::animator::parse_easing(v.unwrap()),
        crate::animator::Easing::EaseIn
    );
}

/// §4.1 — a Luau easing closure on `transition:easing` is sampled
/// at lowering time into a numeric LUT the animator interpolates
/// with zero per-frame Lua calls. `\fn(t) return t*t end` → the
/// decoded curve must satisfy `ease(0)=0`, `ease(1)=1`,
/// `ease(0.5)≈0.25` (the quadratic at its midpoint).
#[cfg(feature = "luau")]
#[test]
fn transition_easing_closure_samples_to_lut() {
    let nodes = interpret(r#"<container transition:easing={\fn(t) return t * t end}/>"#).unwrap();
    let crate::layout::Node::Container { props, .. } = &nodes[0] else {
        panic!()
    };
    let encoded = props
        .semantic
        .attrs
        .iter()
        .find(|(k, _)| k == "data-transition-easing")
        .map(|(_, v)| v.clone())
        .expect("closure lowered to data-transition-easing LUT");
    // It's a comma-joined float list, not a keyword.
    assert!(encoded.contains(','), "expected LUT, got {encoded:?}");
    let easing = crate::animator::parse_easing(&encoded);
    assert!(
        matches!(easing, crate::animator::Easing::Lut(_)),
        "expected Lut, got {easing:?}"
    );
    assert!((easing.ease(0.0) - 0.0).abs() < 1e-3);
    assert!((easing.ease(1.0) - 1.0).abs() < 1e-3);
    assert!(
        (easing.ease(0.5) - 0.25).abs() < 2e-2,
        "quadratic midpoint ~0.25, got {}",
        easing.ease(0.5)
    );
}

/// Wave 13.3 — `use:<modifier-id>[="<value>"]` directive lowers
/// to a `data-use-<id>` semantic attribute. Authors write
/// `<container use:hover use:tooltip="Click to save"/>` instead
/// of hand-emitting the `data-use-` ladder; runtime modifier-fold
/// integration is a follow-up.
#[test]
fn use_namespace_lowers_to_data_use_attr() {
    let nodes = interpret(r#"<container use:hover use:tooltip="Click to save"/>"#).unwrap();
    let crate::layout::Node::Container { props, .. } = &nodes[0] else {
        panic!()
    };
    let attrs: std::collections::HashMap<_, _> = props
        .semantic
        .attrs
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    // Empty-bodied `use:hover` materialises as `data-use-hover="true"`
    // so SSR / hit-test caches see a non-empty value to dispatch on.
    assert_eq!(
        attrs.get("data-use-hover").map(String::as_str),
        Some("true")
    );
    assert_eq!(
        attrs.get("data-use-tooltip").map(String::as_str),
        Some("Click to save")
    );
}

/// `bind:<key>="<source>"` lowers to a `data-bind-<key>`
/// semantic attribute carrying the source path verbatim. The
/// reactive-binding installer (Phase 4 of the dioxus plan)
/// reads these off when the document loads.
#[test]
fn bind_namespace_lowers_to_data_bind_attr() {
    let nodes = interpret(r#"<container bind:title="$selection.name"/>"#).unwrap();
    let crate::layout::Node::Container { props, .. } = &nodes[0] else {
        panic!()
    };
    let attrs: std::collections::HashMap<_, _> = props
        .semantic
        .attrs
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    assert_eq!(
        attrs.get("data-bind-title").map(String::as_str),
        Some("$selection.name")
    );
}

/// `fct:<key>="<source>"` and `sig:<key>="<source>"` follow the
/// same carry-through pattern as `bind:` — they surface as
/// `data-fct-<key>` / `data-sig-<key>` semantic attrs so the
/// host's facet expander / signal-scope installer can act on
/// them post-interpret.
#[test]
fn fct_and_sig_namespaces_lower_to_data_attrs() {
    let nodes =
        interpret(r#"<container fct:source="resource:posts" sig:emit="clicked"/>"#).unwrap();
    let crate::layout::Node::Container { props, .. } = &nodes[0] else {
        panic!()
    };
    let attrs: std::collections::HashMap<_, _> = props
        .semantic
        .attrs
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    assert_eq!(
        attrs.get("data-fct-source").map(String::as_str),
        Some("resource:posts")
    );
    assert_eq!(
        attrs.get("data-sig-emit").map(String::as_str),
        Some("clicked")
    );
}

/// A4 — `<facet name="row" from="<source>">…</facet>` lowers its
/// children once per item resolved from `from`, binding each
/// item under `name` in the per-iteration scope. Same vocabulary
/// as `for="row in source"`, dedicated tag for declarative
/// authorship.
#[test]
fn facet_element_repeats_children_once_per_item() {
    let mut scope = LowerScope::default();
    scope.bindings.insert(
        "posts".to_string(),
        serde_json::json!([
            {"title": "First"},
            {"title": "Second"},
            {"title": "Third"},
        ]),
    );
    let nodes = interpret_with_scope(
        r#"<facet name="post" from="posts"><text>{post.title}</text></facet>"#,
        &scope,
    )
    .unwrap();
    // One Text node per post.
    assert_eq!(nodes.len(), 3);
    let titles: Vec<String> = nodes
        .iter()
        .map(|n| match n {
            crate::layout::Node::Text { content, .. } => content.clone(),
            other => panic!("expected text, got {other:?}"),
        })
        .collect();
    assert_eq!(titles, vec!["First", "Second", "Third"]);
}

/// Default item-binding name is `"item"` when `name=` is omitted.
/// `<facet from="rows">{item.foo}</facet>` reads the same as
/// `<facet name="item" from="rows">{item.foo}</facet>`.
#[test]
fn facet_element_defaults_item_name_to_item() {
    let mut scope = LowerScope::default();
    scope.bindings.insert(
        "rows".to_string(),
        serde_json::json!([{"value": "alpha"}, {"value": "beta"}]),
    );
    let nodes = interpret_with_scope(
        r#"<facet from="rows"><text>{item.value}</text></facet>"#,
        &scope,
    )
    .unwrap();
    assert_eq!(nodes.len(), 2);
    let values: Vec<String> = nodes
        .iter()
        .map(|n| match n {
            crate::layout::Node::Text { content, .. } => content.clone(),
            other => panic!("expected text, got {other:?}"),
        })
        .collect();
    assert_eq!(values, vec!["alpha", "beta"]);
}

/// A `<facet>` with no resolvable `from` source lowers to empty
/// — matches the `for=` behaviour and keeps headless / boot
/// paths panic-free.
#[test]
fn facet_element_with_missing_source_lowers_to_empty() {
    let nodes = interpret(r#"<facet name="row" from="nope.does.not.exist"/>"#).unwrap();
    assert!(nodes.is_empty());
}

/// A `<facet>` accepts inline range sources (same vocabulary as
/// `for="i in 0..3"`).
#[test]
fn facet_element_supports_range_sources() {
    let nodes = interpret(r#"<facet name="i" from="0..3"><text>row {i}</text></facet>"#).unwrap();
    assert_eq!(nodes.len(), 3);
    let labels: Vec<String> = nodes
        .iter()
        .map(|n| match n {
            crate::layout::Node::Text { content, .. } => content.clone(),
            other => panic!("expected text, got {other:?}"),
        })
        .collect();
    assert_eq!(labels, vec!["row 0", "row 1", "row 2"]);
}

/// Same carry-through on `<input>` so text-input authors can
/// bind facets / signals at the field boundary too.
#[test]
fn fct_and_sig_namespaces_lower_on_text_input() {
    let nodes = interpret(
        r#"<input fct:option="resource:options" sig:on:change="emit form.email-changed"/>"#,
    )
    .unwrap();
    let crate::layout::Node::TextInput { semantic, .. } = &nodes[0] else {
        panic!()
    };
    let attrs: std::collections::HashMap<_, _> = semantic
        .attrs
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    assert_eq!(
        attrs.get("data-fct-option").map(String::as_str),
        Some("resource:options")
    );
    assert_eq!(
        attrs.get("data-sig-on:change").map(String::as_str),
        Some("emit form.email-changed")
    );
}

/// Wave 9.2 — `style:background:hovered="<color>"` folds into
/// the container's `hover` override bundle. The plain
/// `style:background` still lands on `props.background`; the
/// `:hovered` variant only contributes to `HoverOverrides`.
#[test]
fn style_state_namespace_hovered_lowers_into_hover_overrides() {
    let nodes = interpret(
        r##"<container style:background="#000000" style:background:hovered="#3366ff"/>"##,
    )
    .unwrap();
    let crate::layout::Node::Container { props, .. } = &nodes[0] else {
        panic!()
    };
    let bg = props.background.expect("resting background set");
    assert_eq!((bg.r, bg.g, bg.b), (0x00, 0x00, 0x00));
    let hover = props.hover.as_ref().expect("hover bundle populated");
    let hbg = hover.background.expect("hover background set");
    assert_eq!((hbg.r, hbg.g, hbg.b), (0x33, 0x66, 0xff));
}

/// Wave 9.2 — `style:radius:hovered="<px>"` rounds all four
/// corners on hover, mirroring the resting-radius shape.
#[test]
fn style_state_namespace_hovered_lowers_radius_to_hover_overrides() {
    let nodes = interpret(r#"<container style:radius:hovered="8"/>"#).unwrap();
    let crate::layout::Node::Container { props, .. } = &nodes[0] else {
        panic!()
    };
    let hover = props.hover.as_ref().expect("hover bundle populated");
    let r = hover.radius.expect("hover radius set");
    assert!((r.tl - 8.0).abs() < f32::EPSILON);
    assert!((r.br - 8.0).abs() < f32::EPSILON);
}

/// Wave 9.2 — `:selected` / `:focused` states have no
/// container-level runtime infra yet, so the override
/// round-trips as `data-style-<key>-<state>` semantic attrs.
/// Same shape as Wave 9.4 transitions — data carries author
/// intent, runtime hookup is the follow-up.
#[test]
fn style_state_namespace_selected_and_focused_round_trip_as_data_attrs() {
    let nodes = interpret(
        r##"<container style:background:selected="#ff0000" style:background:focused="#00ff00"/>"##,
    )
    .unwrap();
    let crate::layout::Node::Container { props, .. } = &nodes[0] else {
        panic!()
    };
    let attrs: std::collections::HashMap<_, _> = props
        .semantic
        .attrs
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    assert_eq!(
        attrs
            .get("data-style-background-selected")
            .map(String::as_str),
        Some("#ff0000")
    );
    assert_eq!(
        attrs
            .get("data-style-background-focused")
            .map(String::as_str),
        Some("#00ff00")
    );
    assert!(
        props.hover.is_none(),
        "non-hover states should not populate hover"
    );
}

/// Wave 9.2 — an unrecognized state suffix (`:active`) is
/// treated as part of the key (no split), so the lookup
/// against the bare key/state pair falls through cleanly
/// without touching `props.background` / `props.hover` /
/// `props.semantic.attrs`.
#[test]
fn style_state_namespace_unknown_suffix_falls_through_cleanly() {
    let nodes = interpret(r##"<container style:background:active="#abcdef"/>"##).unwrap();
    let crate::layout::Node::Container { props, .. } = &nodes[0] else {
        panic!()
    };
    assert!(props.background.is_none());
    assert!(props.hover.is_none());
    assert!(props.semantic.attrs.is_empty());
}

#[test]
fn input_paints_placeholder_when_value_empty() {
    let nodes = interpret(r#"<input placeholder="type here" width="200" height="32"/>"#).unwrap();
    let cmds = compute(
        &nodes[0],
        Viewport {
            width: 800.0,
            height: 600.0,
        },
    );
    let text_cmd = cmds
        .iter()
        .find_map(|c| {
            if let crate::command::RenderCommand::Text { content, .. } = c {
                Some(content.as_str())
            } else {
                None
            }
        })
        .expect("text command emitted");
    assert_eq!(text_cmd, "type here");
}

// ─── Wave 11.2 substrate — expression evaluator across attrs ───

/// Templated attribute with a ternary head picks the matching
/// branch and stringifies — exactly what the toast / row-variant
/// migrations need to map a discriminant (`kind`, `selected`) to
/// a colour or label without a Rust seam.
#[test]
fn templated_attribute_resolves_ternary_against_scope() {
    use serde_json::json;
    let scope = LowerScope::default().with_binding("kind", json!("error"));
    let doc =
        parse(r##"<container style:background="{kind == 'error' ? '#cf222e' : '#0969da'}"/>"##).0;
    let nodes = lower_document_with_scope(&doc, &scope);
    let crate::layout::Node::Container { props, .. } = &nodes[0] else {
        panic!()
    };
    // Container's `style:background` lowered into `props.background`;
    // assert the resolved hex landed on the typed slot.
    let bg = props.background.expect("background set");
    assert_eq!(bg.r, 0xcf);
    assert_eq!(bg.g, 0x22);
    assert_eq!(bg.b, 0x2e);
}

/// `if=` on a control-flow attribute runs through the full
/// expression evaluator so authors can compose boolean expressions
/// (`enabled && !disabled`) without a Rust normalisation step.
#[test]
fn if_attribute_evaluates_boolean_expression() {
    use serde_json::json;
    let scope = LowerScope::default()
        .with_binding("enabled", json!(true))
        .with_binding("disabled", json!(false));
    let doc = parse(
        r#"<container>
            <text if="{enabled && !disabled}">visible</text>
            <text if="{!enabled || disabled}">hidden</text>
        </container>"#,
    )
    .0;
    let nodes = lower_document_with_scope(&doc, &scope);
    let crate::layout::Node::Container { children, .. } = &nodes[0] else {
        panic!()
    };
    // Exactly one of the two text nodes survives the conditional.
    assert_eq!(children.len(), 1, "if= evaluator should drop the false arm");
}

/// `if=` with a comparison against a string literal — the row-variant
/// migration shape (`if="{kind == 'error'}"`) used across the toast,
/// signal-connection-row, schema-row, etc.
#[test]
fn if_attribute_supports_string_equality_comparison() {
    use serde_json::json;
    let render_with_kind = |k: &str| {
        let scope = LowerScope::default().with_binding("kind", json!(k));
        let doc = parse(
            r#"<container>
                <text if="{kind == 'error'}">oops</text>
                <text if="{kind == 'success'}">ok</text>
            </container>"#,
        )
        .0;
        let nodes = lower_document_with_scope(&doc, &scope);
        let crate::layout::Node::Container { children, .. } = &nodes[0] else {
            panic!()
        };
        children.len()
    };
    assert_eq!(render_with_kind("error"), 1);
    assert_eq!(render_with_kind("success"), 1);
    assert_eq!(render_with_kind("info"), 0);
}

/// `for="row, idx in rows"` exposes the iteration index as a typed
/// number binding so the DSL author can synthesise stable per-row
/// ids — critical for hit-testing dispatched containers, which
/// only enter the hit cache when their `id` is non-empty.
#[test]
fn for_loop_exposes_optional_iteration_index() {
    use serde_json::json;
    let scope = LowerScope::default().with_binding(
        "rows",
        json!([
            { "label": "Body" },
            { "label": "Level" },
            { "label": "Link URL" },
        ]),
    );
    // `<container id="row-{idx}">` produces a unique id per item.
    let doc = parse(
        r#"<container>
            <container for="row, idx in rows" id="row-{idx}">
                <text>{row.label}</text>
            </container>
        </container>"#,
    )
    .0;
    let nodes = lower_document_with_scope(&doc, &scope);
    let crate::layout::Node::Container { children, .. } = &nodes[0] else {
        panic!()
    };
    let ids: Vec<&str> = children
        .iter()
        .filter_map(|c| match c {
            crate::layout::Node::Container { id, .. } => Some(id.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(ids, vec!["row-0", "row-1", "row-2"]);
}

/// Dotted-path lookups in expression position still resolve to the
/// underlying JSON value, so a `for="row in rows"` loop addressing
/// `{row.label}` continues to work after the parser switch.
#[test]
fn expression_evaluator_walks_dotted_path_into_object() {
    use serde_json::json;
    let scope =
        LowerScope::default().with_binding("row", json!({ "label": "Hi", "kind": "error" }));
    // Ternary referencing dotted field — exercises both substrate
    // additions in one expression.
    assert_eq!(
        evaluate_expression("row.kind == 'error' ? row.label : 'fallback'", &scope),
        Some(serde_json::Value::String("Hi".to_string()))
    );
}

// ── Wave 14 — substrate from HTMX / CSS / SwiftUI / Compose ──

/// **Wave 14.1** — the `tokens` scope binding round-trips
/// the design-token table through the existing dotted-path
/// resolver, so an author can write
/// `style:background="{tokens.colors.accent}"` and get the same
/// `ContainerProps.background` the equivalent hex literal would
/// produce. The colour serialisation uses the standard `#rrggbbaa`
/// shape `parse_color` consumes.
#[test]
fn tokens_binding_resolves_color_in_style_namespace() {
    let scope =
        LowerScope::default().with_design_tokens(&prism_core::design_tokens::DEFAULT_TOKENS);
    let nodes = interpret_with_scope(
        r#"<container id="t" style:background="{tokens.colors.accent}"/>"#,
        &scope,
    )
    .unwrap();
    let crate::layout::Node::Container { props, .. } = &nodes[0] else {
        panic!("expected container, got {:?}", nodes[0]);
    };
    let accent = &prism_core::design_tokens::DEFAULT_TOKENS.colors.accent;
    let bg = props.background.expect("background should resolve");
    assert_eq!(bg.r, accent.r);
    assert_eq!(bg.g, accent.g);
    assert_eq!(bg.b, accent.b);
    assert_eq!(bg.a, accent.a);
}

/// **Wave 14.1** — numeric tokens (spacing / radius / typography)
/// resolve through the same dotted-path lookup; `parse_f32` reads
/// the number verbatim through the JSON-number value.
#[test]
fn tokens_binding_resolves_nested_path_through_style() {
    let scope =
        LowerScope::default().with_design_tokens(&prism_core::design_tokens::DEFAULT_TOKENS);
    let nodes = interpret_with_scope(
        r#"<container id="t" padding="{tokens.spacing.md}" style:radius="{tokens.radius.lg}"/>"#,
        &scope,
    )
    .unwrap();
    let crate::layout::Node::Container { props, .. } = &nodes[0] else {
        panic!("expected container, got {:?}", nodes[0]);
    };
    let md = prism_core::design_tokens::DEFAULT_TOKENS.spacing.md as f32;
    let lg = prism_core::design_tokens::DEFAULT_TOKENS.radius.lg as f32;
    assert!((props.padding.left - md).abs() < f32::EPSILON);
    assert!((props.padding.top - md).abs() < f32::EPSILON);
    assert!((props.radius.tl - lg).abs() < f32::EPSILON);
    assert!((props.radius.br - lg).abs() < f32::EPSILON);
}

/// **Wave 14.2** — a sibling `<let name="total" value="{items.length}"/>`
/// seeds a `total` binding into every subsequent sibling's scope
/// without consuming a render slot. Today no `items.length`
/// built-in exists, so the test uses a primitive value pulled
/// from the parent scope to exercise the propagation rule.
#[test]
fn let_binding_propagates_to_subsequent_siblings() {
    use serde_json::json;
    let scope = LowerScope::default().with_binding("base", json!(8));
    let source = r#"
        <let name="doubled" value="{base * 2}"/>
        <container id="t" padding="{doubled}"/>
    "#;
    let nodes = interpret_with_scope(source, &scope).unwrap();
    // Single rendered sibling — the `<let/>` should not emit.
    assert_eq!(nodes.len(), 1, "let should not render: {nodes:?}");
    let crate::layout::Node::Container { props, .. } = &nodes[0] else {
        panic!("expected container, got {:?}", nodes[0]);
    };
    assert!((props.padding.left - 16.0).abs() < f32::EPSILON);
}

/// **Wave 14.2** — a `<let/>` binding overrides a same-named
/// parent-scope binding in subsequent siblings (lexical
/// shadowing); the parent-scope value remains untouched outside
/// the let-scope. Matches Svelte `{@const}` semantics.
#[test]
fn let_binding_shadows_parent_scope_for_subsequent_siblings() {
    use serde_json::json;
    let scope = LowerScope::default().with_binding("size", json!(4));
    let source = r#"
        <container id="a" padding="{size}"/>
        <let name="size" value="{20}"/>
        <container id="b" padding="{size}"/>
    "#;
    let nodes = interpret_with_scope(source, &scope).unwrap();
    assert_eq!(nodes.len(), 2);
    let crate::layout::Node::Container {
        id: id_a, props: a, ..
    } = &nodes[0]
    else {
        panic!()
    };
    let crate::layout::Node::Container {
        id: id_b, props: b, ..
    } = &nodes[1]
    else {
        panic!()
    };
    assert_eq!(id_a, "a");
    assert_eq!(id_b, "b");
    assert!((a.padding.left - 4.0).abs() < f32::EPSILON);
    assert!((b.padding.left - 20.0).abs() < f32::EPSILON);
}

// ── Wave 15 — iteration / loop patterns ──

/// **Wave 15.1** — Svelte-style `{:each}{:else}` shape: a `for`
/// that iterates zero items lets a following `else` sibling
/// render as the empty-state fallback. Non-empty iteration
/// suppresses the branch.
#[test]
fn else_after_empty_for_renders_fallback() {
    use serde_json::json;
    let scope = LowerScope::default().with_binding("items", json!([]));
    let source = r#"
        <container id="row" for="item in items"/>
        <text else>No items.</text>
    "#;
    let nodes = interpret_with_scope(source, &scope).unwrap();
    // For-loop emitted zero containers; the else-text replaces it.
    assert_eq!(nodes.len(), 1);
    let crate::layout::Node::Text { content, .. } = &nodes[0] else {
        panic!("expected fallback text, got {:?}", nodes[0]);
    };
    assert_eq!(content, "No items.");
}

#[test]
fn else_after_non_empty_for_is_suppressed() {
    use serde_json::json;
    let scope = LowerScope::default().with_binding("items", json!(["a", "b"]));
    let source = r#"
        <container id="row" for="item in items"/>
        <text else>No items.</text>
    "#;
    let nodes = interpret_with_scope(source, &scope).unwrap();
    // Two for-rows emitted; the else-text is suppressed.
    assert_eq!(nodes.len(), 2);
    for n in &nodes {
        assert!(matches!(n, crate::layout::Node::Container { .. }));
    }
}

#[test]
fn else_if_after_empty_for_evaluates_predicate() {
    use serde_json::json;
    let scope = LowerScope::default()
        .with_binding("items", json!([]))
        .with_binding("show", json!(true));
    let source = r#"
        <container id="row" for="item in items"/>
        <text else-if="{show}">Else-if branch.</text>
        <text else>Fallback.</text>
    "#;
    let nodes = interpret_with_scope(source, &scope).unwrap();
    assert_eq!(nodes.len(), 1);
    let crate::layout::Node::Text { content, .. } = &nodes[0] else {
        panic!("expected else-if branch text, got {:?}", nodes[0]);
    };
    assert_eq!(content, "Else-if branch.");
}

/// **Wave 15.2** — `for="i in 0..3"` iterates `i ∈ [0, 3)`,
/// matching Rust's `Range` / Python's `range(3)` shape.
#[test]
fn range_exclusive_iterates_start_through_end_minus_one() {
    let source = r#"<container id="row-{i}" for="i in 0..3" padding="{i}"/>"#;
    let nodes = interpret(source).unwrap();
    assert_eq!(nodes.len(), 3);
    for (idx, n) in nodes.iter().enumerate() {
        let crate::layout::Node::Container { id, props, .. } = n else {
            panic!()
        };
        assert_eq!(id, &format!("row-{idx}"));
        assert!((props.padding.left - idx as f32).abs() < f32::EPSILON);
    }
}

/// **Wave 15.2** — `..=` inclusive variant matches Rust's
/// `RangeInclusive` shape.
#[test]
fn range_inclusive_iterates_start_through_end() {
    let source = r#"<container id="row-{i}" for="i in 0..=3"/>"#;
    let nodes = interpret(source).unwrap();
    assert_eq!(nodes.len(), 4, "0..=3 includes 0,1,2,3");
}

/// **Wave 15.2** — start > end yields zero items, no panic.
#[test]
fn range_with_descending_endpoints_yields_zero_items() {
    let source = r#"<container id="row-{i}" for="i in 5..3"/>"#;
    let nodes = interpret(source).unwrap();
    assert!(nodes.is_empty());
}

/// **Wave 15.2** — range endpoints resolve through the scope
/// binding map. `for="i in 0..n"` with `n` bound to 4 iterates
/// `0..4`.
#[test]
fn range_endpoints_resolve_through_scope_bindings() {
    use serde_json::json;
    let scope = LowerScope::default().with_binding("n", json!(4));
    let nodes = interpret_with_scope(r#"<container id="r-{i}" for="i in 0..n"/>"#, &scope).unwrap();
    assert_eq!(nodes.len(), 4);
}

/// **Wave 15.3** — iterating over a JSON object yields one entry
/// per `(key, value)` pair in insertion order. The primary LHS
/// variable binds the value; the optional second LHS variable
/// binds the string key (mirroring the index-variable shape for
/// arrays).
#[test]
fn for_loop_over_object_iterates_entries_in_insertion_order() {
    use serde_json::json;
    let scope = LowerScope::default().with_binding(
        "props",
        json!({ "alpha": "first", "beta": "second", "gamma": "third" }),
    );
    let source = r#"<text for="value in props">{value}</text>"#;
    let nodes = interpret_with_scope(source, &scope).unwrap();
    let contents: Vec<String> = nodes
        .iter()
        .map(|n| match n {
            crate::layout::Node::Text { content, .. } => content.clone(),
            _ => panic!("expected text node"),
        })
        .collect();
    assert_eq!(contents, vec!["first", "second", "third"]);
}

#[test]
fn for_loop_over_object_binds_key_to_second_lhs_variable() {
    use serde_json::json;
    let scope =
        LowerScope::default().with_binding("props", json!({ "alpha": "first", "beta": "second" }));
    // `<text>{a} {b}</text>` interpolations are space-joined by
    // `collect_text_content` — keep the assertion on the joined
    // shape rather than fighting the runtime convention.
    let source = r#"<text for="value, key in props">{key} {value}</text>"#;
    let nodes = interpret_with_scope(source, &scope).unwrap();
    let contents: Vec<String> = nodes
        .iter()
        .map(|n| match n {
            crate::layout::Node::Text { content, .. } => content.clone(),
            _ => panic!(),
        })
        .collect();
    assert_eq!(contents, vec!["alpha first", "beta second"]);
}

/// **Regression pin** — the Wave 15.3 dispatch on source shape
/// must not break the existing array + index path. `for="x, idx
/// in arr"` still binds `idx` to the integer index.
#[test]
fn for_loop_index_variable_still_works_on_arrays() {
    use serde_json::json;
    let scope = LowerScope::default().with_binding("xs", json!(["a", "b"]));
    let source = r#"<text for="x, i in xs">{i} {x}</text>"#;
    let nodes = interpret_with_scope(source, &scope).unwrap();
    let contents: Vec<String> = nodes
        .iter()
        .map(|n| match n {
            crate::layout::Node::Text { content, .. } => content.clone(),
            _ => panic!(),
        })
        .collect();
    assert_eq!(contents, vec!["0 a", "1 b"]);
}

/// **Wave 14.3** — `on:click.once` lowers to `data-on-click-once`
/// so the modifier suffix round-trips through the same `data-on-*`
/// hit-test cache. Today no consumer reads the suffix; data
/// carries author intent for future runtime wiring.
#[test]
fn on_namespace_modifier_suffix_round_trips_as_data_attr() {
    let nodes =
        interpret(r#"<container id="b" on:click.once="cmd confirm" on:click.stop="cmd noop"/>"#)
            .unwrap();
    let crate::layout::Node::Container { props, .. } = &nodes[0] else {
        panic!("expected container, got {:?}", nodes[0]);
    };
    let attrs: std::collections::HashMap<_, _> = props
        .semantic
        .attrs
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    assert_eq!(
        attrs.get("data-on-click-once").map(String::as_str),
        Some("cmd confirm")
    );
    assert_eq!(
        attrs.get("data-on-click-stop").map(String::as_str),
        Some("cmd noop")
    );
}

/// **Wave 15.4 (step)** — `for="i in 0..10 step 2"` iterates by
/// `2` between endpoints. Matches Python `range(0, 10, 2)` /
/// Rust `(0..10).step_by(2)` / SwiftUI `stride(from:to:by:)`.
#[test]
fn range_with_step_iterates_by_increment() {
    let source = r#"<container id="r-{i}" for="i in 0..10 step 2"/>"#;
    let nodes = interpret(source).unwrap();
    let ids: Vec<String> = nodes
        .iter()
        .map(|n| match n {
            crate::layout::Node::Container { id, .. } => id.clone(),
            _ => panic!(),
        })
        .collect();
    assert_eq!(ids, vec!["r-0", "r-2", "r-4", "r-6", "r-8"]);
}

/// **Wave 15.4 (step)** — `step` composes with `..=` inclusive
/// ranges.
#[test]
fn inclusive_range_with_step_includes_upper_endpoint_when_aligned() {
    let source = r#"<container id="r-{i}" for="i in 0..=10 step 5"/>"#;
    let nodes = interpret(source).unwrap();
    let ids: Vec<String> = nodes
        .iter()
        .map(|n| match n {
            crate::layout::Node::Container { id, .. } => id.clone(),
            _ => panic!(),
        })
        .collect();
    assert_eq!(ids, vec!["r-0", "r-5", "r-10"]);
}

/// **Wave 15.4 (step)** — step ≤ 0 rejects the whole `for=`
/// (treated as `if false`), so the element drops cleanly.
#[test]
fn range_with_zero_or_negative_step_drops_element() {
    let zero = interpret(r#"<container id="r-{i}" for="i in 0..10 step 0"/>"#).unwrap();
    assert!(zero.is_empty());
    let neg = interpret(r#"<container id="r-{i}" for="i in 0..10 step -2"/>"#).unwrap();
    assert!(neg.is_empty());
}

/// **Wave 15.4 (reverse)** — `for="i in 0..5 reverse"` iterates
/// `4, 3, 2, 1, 0`. The endpoint shape is identical to forward
/// iteration; only the emitted order flips.
#[test]
fn range_with_reverse_iterates_descending() {
    let source = r#"<container id="r-{i}" for="i in 0..5 reverse"/>"#;
    let nodes = interpret(source).unwrap();
    let ids: Vec<String> = nodes
        .iter()
        .map(|n| match n {
            crate::layout::Node::Container { id, .. } => id.clone(),
            _ => panic!(),
        })
        .collect();
    assert_eq!(ids, vec!["r-4", "r-3", "r-2", "r-1", "r-0"]);
}

/// **Wave 15.4 (reverse)** — arrays reverse to last-first order.
#[test]
fn array_with_reverse_iterates_last_first() {
    use serde_json::json;
    let scope = LowerScope::default().with_binding("xs", json!(["a", "b", "c"]));
    let nodes = interpret_with_scope(r#"<text for="x in xs reverse">{x}</text>"#, &scope).unwrap();
    let contents: Vec<String> = nodes
        .iter()
        .map(|n| match n {
            crate::layout::Node::Text { content, .. } => content.clone(),
            _ => panic!(),
        })
        .collect();
    assert_eq!(contents, vec!["c", "b", "a"]);
}

/// **Wave 15.4 (reverse)** — objects reverse insertion order too.
/// The `<text>` body's joined shape inherits the runtime's
/// space-between-runs convention from `collect_text_content`.
#[test]
fn object_with_reverse_iterates_last_entry_first() {
    use serde_json::json;
    let scope = LowerScope::default().with_binding("o", json!({ "x": "1", "y": "2", "z": "3" }));
    let nodes =
        interpret_with_scope(r#"<text for="v, k in o reverse">{k} {v}</text>"#, &scope).unwrap();
    let contents: Vec<String> = nodes
        .iter()
        .map(|n| match n {
            crate::layout::Node::Text { content, .. } => content.clone(),
            _ => panic!(),
        })
        .collect();
    assert_eq!(contents, vec!["z 3", "y 2", "x 1"]);
}

/// **Wave 15.4 (step + reverse)** — modifiers compose. `step`
/// applies first (filters the range to every Nth element), then
/// `reverse` flips the filtered sequence.
#[test]
fn range_step_and_reverse_compose() {
    let source = r#"<container id="r-{i}" for="i in 0..10 step 2 reverse"/>"#;
    let nodes = interpret(source).unwrap();
    let ids: Vec<String> = nodes
        .iter()
        .map(|n| match n {
            crate::layout::Node::Container { id, .. } => id.clone(),
            _ => panic!(),
        })
        .collect();
    assert_eq!(ids, vec!["r-8", "r-6", "r-4", "r-2", "r-0"]);
}

/// **Wave 15.4** — modifiers tolerate either order on the `for`
/// clause: `reverse step N` parses identically to `step N reverse`.
#[test]
fn for_modifiers_accept_either_order() {
    let a = interpret(r#"<container id="r-{i}" for="i in 0..10 reverse step 2"/>"#).unwrap();
    let b = interpret(r#"<container id="r-{i}" for="i in 0..10 step 2 reverse"/>"#).unwrap();
    assert_eq!(a.len(), b.len());
    for (na, nb) in a.iter().zip(b.iter()) {
        match (na, nb) {
            (
                crate::layout::Node::Container { id: ida, .. },
                crate::layout::Node::Container { id: idb, .. },
            ) => assert_eq!(ida, idb),
            _ => panic!(),
        }
    }
}

/// **Wave 15.4** — repeating a modifier is rejected (drops the
/// element) so an author who writes `reverse reverse` sees the
/// missing output and fixes the typo.
#[test]
fn for_modifier_duplicate_drops_element() {
    let nodes = interpret(r#"<container for="i in 0..3 reverse reverse"/>"#).unwrap();
    assert!(nodes.is_empty());
    let nodes = interpret(r#"<container for="i in 0..3 step 1 step 2"/>"#).unwrap();
    assert!(nodes.is_empty());
}

/// **Wave 15.5** — `key="X"` on a `<container>` lowers to a
/// `data-key="X"` semantic attr. Reconciliation hint round-trip;
/// the runtime tree-diff that would consume it is the unblock.
#[test]
fn key_attr_lowers_to_data_key_semantic_attr() {
    let nodes = interpret(r#"<container id="row" key="row-42"/>"#).unwrap();
    let crate::layout::Node::Container { props, .. } = &nodes[0] else {
        panic!()
    };
    let attrs: std::collections::HashMap<_, _> = props
        .semantic
        .attrs
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    assert_eq!(attrs.get("data-key").map(String::as_str), Some("row-42"));
}

/// **Wave 15.5** — `key=""` drops cleanly so a ternary that
/// resolves to the empty string omits the attr (Wave 13 `data:` /
/// `aria:` empty-string filter pattern).
#[test]
fn key_attr_empty_string_drops_cleanly() {
    let nodes = interpret(r#"<container id="row" key=""/>"#).unwrap();
    let crate::layout::Node::Container { props, .. } = &nodes[0] else {
        panic!()
    };
    assert!(props.semantic.attrs.iter().all(|(k, _)| k != "data-key"));
}

/// **Numeric units** — `parse_f32` accepts CSS-style length
/// suffixes (`px`, `rem`, `em`). Bare numbers continue to mean
/// pixels (matches CSS bare-number-is-px convention).
#[test]
fn parse_f32_handles_px_rem_em_suffixes() {
    assert_eq!(parse_f32("14"), Some(14.0));
    assert_eq!(parse_f32("14px"), Some(14.0));
    assert_eq!(parse_f32("1rem"), Some(16.0));
    assert_eq!(parse_f32("0.5rem"), Some(8.0));
    assert_eq!(parse_f32("0.875rem"), Some(14.0));
    assert_eq!(parse_f32("1em"), Some(16.0));
    // Trailing whitespace inside the value is tolerated so a
    // ternary that produces `"14 px"` doesn't silently drop.
    assert_eq!(parse_f32("14 px"), Some(14.0));
}

#[test]
fn parse_f32_rejects_unknown_suffixes() {
    assert_eq!(parse_f32("14pt"), None);
    assert_eq!(parse_f32("14vh"), None);
    assert_eq!(parse_f32("nonsense"), None);
}

/// **Numeric units** — `parse_sizing` adds `%` and `auto` to
/// the vocabulary `parse_f32` accepts.
#[test]
fn parse_sizing_handles_percent_grow_fit_auto() {
    use crate::layout::Sizing;
    assert!(matches!(parse_sizing("grow"), Some(Sizing::Grow)));
    assert!(matches!(parse_sizing("fit"), Some(Sizing::Fit)));
    assert!(matches!(parse_sizing("auto"), Some(Sizing::Fit)));
    let Some(Sizing::Percent(p)) = parse_sizing("50%") else {
        panic!()
    };
    assert!((p - 0.5).abs() < f32::EPSILON);
    let Some(Sizing::Percent(p)) = parse_sizing("100%") else {
        panic!()
    };
    assert!((p - 1.0).abs() < f32::EPSILON);
}

/// **Numeric units (integration)** — `padding="1rem"` lowers
/// to 16px on a container. Same code path every other
/// length-valued attribute uses.
#[test]
fn padding_in_rem_resolves_to_pixels() {
    let nodes = interpret(r#"<container padding="1rem"/>"#).unwrap();
    let crate::layout::Node::Container { props, .. } = &nodes[0] else {
        panic!()
    };
    assert!((props.padding.left - 16.0).abs() < f32::EPSILON);
    assert!((props.padding.right - 16.0).abs() < f32::EPSILON);
}

/// **Numeric units (integration)** — `width="50%"` lowers to
/// `Sizing::Percent(0.5)`.
#[test]
fn width_50_percent_lowers_to_sizing_percent() {
    use crate::layout::Sizing;
    let nodes = interpret(r#"<container width="50%" height="100%"/>"#).unwrap();
    let crate::layout::Node::Container { props, .. } = &nodes[0] else {
        panic!()
    };
    let Sizing::Percent(w) = props.width else {
        panic!("expected percent width, got {:?}", props.width)
    };
    let Sizing::Percent(h) = props.height else {
        panic!("expected percent height, got {:?}", props.height)
    };
    assert!((w - 0.5).abs() < f32::EPSILON);
    assert!((h - 1.0).abs() < f32::EPSILON);
}

/// **Numeric units (integration)** — `font-size="0.875rem"`
/// on a text element lowers to 14px (the typography token
/// shape PRSS exposes by default).
#[test]
fn font_size_in_rem_resolves_to_pixels() {
    let nodes = interpret(r#"<text font-size="0.875rem">Hi</text>"#).unwrap();
    let crate::layout::Node::Text { props, .. } = &nodes[0] else {
        panic!()
    };
    assert!((props.font_size - 14.0).abs() < f32::EPSILON);
}

/// **Wave 15.5** — `key="{expr}"` interpolates through scope
/// so `for="row in rows"` + `key="{row.id}"` works.
#[test]
fn key_attr_interpolates_through_scope() {
    use serde_json::json;
    let scope =
        LowerScope::default().with_binding("rows", json!([{"id": "alpha"}, {"id": "beta"}]));
    let nodes = interpret_with_scope(
        r#"<container for="row in rows" id="r-{row.id}" key="{row.id}"/>"#,
        &scope,
    )
    .unwrap();
    let keys: Vec<String> = nodes
        .iter()
        .filter_map(|n| match n {
            crate::layout::Node::Container { props, .. } => props
                .semantic
                .attrs
                .iter()
                .find(|(k, _)| k == "data-key")
                .map(|(_, v)| v.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(keys, vec!["alpha", "beta"]);
}

/// **PRSS integration** — a `class="…"` attribute on a
/// `<container>` resolves each named class through the
/// installed stylesheet and applies its properties via
/// `apply_style_override`.
#[test]
fn prss_class_applies_background_and_radius() {
    use std::sync::Arc;
    let (sheet, _) = prism_core::language::prss::parse(
        r##"
        [class.btn]
        background = "#0060c0"
        radius = 8
        "##,
    );
    let scope = LowerScope::default().with_stylesheet(Arc::new(sheet));
    let nodes = interpret_with_scope(r#"<container class="btn"/>"#, &scope).unwrap();
    let crate::layout::Node::Container { props, .. } = &nodes[0] else {
        panic!()
    };
    let bg = props.background.expect("class supplied background");
    assert_eq!((bg.r, bg.g, bg.b, bg.a), (0x00, 0x60, 0xc0, 0xff));
    assert!((props.radius.tl - 8.0).abs() < f32::EPSILON);
}

/// **Fusion F.3** — when a `ClassUsage` collector is installed,
/// lowering records every `(class → resolvable NodeId)` pair so
/// the shell's `.prss` hot-reload consumer can mark just those
/// NodeIds dirty. Anonymous containers (no id) aren't recorded —
/// they can't be targeted selectively and fall back to the broad
/// invalidation path.
#[test]
fn class_usage_records_class_to_nodeid_when_collector_installed() {
    use std::cell::RefCell;
    use std::rc::Rc;
    use std::sync::Arc;
    let (sheet, _) = prism_core::language::prss::parse(
        r##"
        [class.panel]
        background = "#101010"
        [class.title]
        color = "#ffffff"
        "##,
    );
    let deps = Rc::new(RefCell::new(ClassUsage::new()));
    let scope = LowerScope::default()
        .with_stylesheet(Arc::new(sheet))
        .with_class_deps(Rc::clone(&deps));
    let _ = interpret_with_scope(
        r#"<container id="card" class="panel">
             <container id="hdr" class="title"/>
             <container class="panel"/>
           </container>"#,
        &scope,
    )
    .unwrap();
    let d = deps.borrow();
    assert_eq!(d.nodes_for("panel"), vec!["card".to_string()]);
    assert_eq!(d.nodes_for("title"), vec!["hdr".to_string()]);
    // The anonymous `.panel` child has no id → not recorded, so
    // `panel` maps to exactly the one id'd user.
    assert_eq!(d.nodes_for("panel").len(), 1);
    assert!(d.nodes_for("missing").is_empty());
}

/// **PRSS integration** — `extends` flattens parent properties
/// first; child overrides win.
#[test]
fn prss_extends_inherits_then_overrides() {
    use std::sync::Arc;
    let (sheet, _) = prism_core::language::prss::parse(
        r##"
        [class.btn]
        background = "#ffffff"
        radius = 8
        padding = 12

        [class.btn-primary]
        extends = "btn"
        background = "#0060c0"
        "##,
    );
    let scope = LowerScope::default().with_stylesheet(Arc::new(sheet));
    let nodes = interpret_with_scope(r#"<container class="btn-primary"/>"#, &scope).unwrap();
    let crate::layout::Node::Container { props, .. } = &nodes[0] else {
        panic!()
    };
    let bg = props.background.expect("class supplied background");
    assert_eq!((bg.r, bg.g, bg.b), (0x00, 0x60, 0xc0));
    assert!((props.radius.tl - 8.0).abs() < f32::EPSILON);
    assert!((props.padding.left - 12.0).abs() < f32::EPSILON);
}

/// **PRSS integration** — multiple classes apply left to
/// right; the rightmost wins on conflict.
#[test]
fn prss_multiple_classes_apply_left_to_right() {
    use std::sync::Arc;
    let (sheet, _) = prism_core::language::prss::parse(
        r##"
        [class.btn]
        background = "#ffffff"

        [class.accent]
        background = "#0060c0"
        "##,
    );
    let scope = LowerScope::default().with_stylesheet(Arc::new(sheet));
    let nodes = interpret_with_scope(r#"<container class="btn accent"/>"#, &scope).unwrap();
    let crate::layout::Node::Container { props, .. } = &nodes[0] else {
        panic!()
    };
    let bg = props.background.expect("class supplied background");
    assert_eq!((bg.r, bg.g, bg.b), (0x00, 0x60, 0xc0));
}

/// **PRSS integration** — inline `style:` always wins over
/// classes. Application-order rule from §4.6 of the PRSS
/// reference.
#[test]
fn prss_inline_style_overrides_class_property() {
    use std::sync::Arc;
    let (sheet, _) = prism_core::language::prss::parse(
        r##"
        [class.btn]
        background = "#0060c0"
        "##,
    );
    let scope = LowerScope::default().with_stylesheet(Arc::new(sheet));
    let nodes = interpret_with_scope(
        r##"<container class="btn" style:background="#ff0000"/>"##,
        &scope,
    )
    .unwrap();
    let crate::layout::Node::Container { props, .. } = &nodes[0] else {
        panic!()
    };
    let bg = props.background.expect("inline wins");
    assert_eq!((bg.r, bg.g, bg.b), (0xff, 0x00, 0x00));
}

/// **PRSS integration** — `[class.btn.hovered]` lands in
/// `ContainerProps.hover` through the existing state-suffix
/// pathway.
#[test]
fn prss_state_variant_lowers_into_hover_overrides() {
    use std::sync::Arc;
    let (sheet, _) = prism_core::language::prss::parse(
        r##"
        [class.btn]
        background = "#ffffff"

        [class.btn.hovered]
        background = "#0060c0"
        "##,
    );
    let scope = LowerScope::default().with_stylesheet(Arc::new(sheet));
    let nodes = interpret_with_scope(r#"<container class="btn"/>"#, &scope).unwrap();
    let crate::layout::Node::Container { props, .. } = &nodes[0] else {
        panic!()
    };
    let hover = props.hover.as_ref().expect("hover override installed");
    let bg = hover.background.expect("hover background");
    assert_eq!((bg.r, bg.g, bg.b), (0x00, 0x60, 0xc0));
}

/// **PRSS integration** — token overrides merge into the
/// `tokens` binding so `{tokens.colors.X}` in PRUI resolves
/// through the override.
#[test]
fn prss_tokens_merge_into_tokens_binding() {
    use std::sync::Arc;
    let (sheet, _) = prism_core::language::prss::parse(
        r##"
        [tokens.colors]
        accent = "#7c3aed"

        [tokens.spacing]
        md = 16
        "##,
    );
    let scope = LowerScope::default()
        .with_design_tokens(&prism_core::design_tokens::DEFAULT_TOKENS)
        .with_stylesheet(Arc::new(sheet));
    let nodes = interpret_with_scope(
        r##"<container padding="{tokens.spacing.md}" style:background="{tokens.colors.accent}"/>"##,
        &scope,
    )
    .unwrap();
    let crate::layout::Node::Container { props, .. } = &nodes[0] else {
        panic!()
    };
    let bg = props.background.expect("token reads through");
    assert_eq!((bg.r, bg.g, bg.b), (0x7c, 0x3a, 0xed));
    assert!((props.padding.left - 16.0).abs() < f32::EPSILON);
}

/// **PRSS integration** — class property values may reference
/// tokens via `{expr}` interpolation. Same expression evaluator
/// inline `style:` uses.
#[test]
fn prss_class_value_interpolates_token_reference() {
    use std::sync::Arc;
    let (sheet, _) = prism_core::language::prss::parse(
        r##"
        [tokens.colors]
        brand = "#5b21b6"

        [class.btn]
        background = "{tokens.colors.brand}"
        "##,
    );
    let scope = LowerScope::default()
        .with_design_tokens(&prism_core::design_tokens::DEFAULT_TOKENS)
        .with_stylesheet(Arc::new(sheet));
    let nodes = interpret_with_scope(r#"<container class="btn"/>"#, &scope).unwrap();
    let crate::layout::Node::Container { props, .. } = &nodes[0] else {
        panic!()
    };
    let bg = props.background.expect("interpolated token");
    assert_eq!((bg.r, bg.g, bg.b), (0x5b, 0x21, 0xb6));
}

/// **PRSS integration** — without a stylesheet installed,
/// `class="…"` is a styling no-op. The inline `style:` still
/// applies.
#[test]
fn prss_no_stylesheet_means_class_is_a_noop() {
    let nodes = interpret(r##"<container class="btn" style:background="#abcdef"/>"##).unwrap();
    let crate::layout::Node::Container { props, .. } = &nodes[0] else {
        panic!()
    };
    let bg = props.background.expect("inline style: applied");
    assert_eq!((bg.r, bg.g, bg.b), (0xab, 0xcd, 0xef));
}

/// **Sugar (`@event`)** — `@click="cmd save"` parses
/// identically to `on:click="cmd save"` (Vue shorthand). Both
/// lower to a `data-on-click` semantic attr.
#[test]
fn at_prefix_is_alias_for_on_namespace() {
    let nodes = interpret(r#"<container @click="cmd save"/>"#).unwrap();
    let crate::layout::Node::Container { props, .. } = &nodes[0] else {
        panic!()
    };
    let attr = props
        .semantic
        .attrs
        .iter()
        .find(|(k, _)| k == "data-on-click")
        .map(|(_, v)| v.as_str());
    assert_eq!(attr, Some("cmd save"));
}

/// **Sugar (`:prop`)** — `:value="form.email"` parses
/// identically to `bind:value="form.email"` (Vue shorthand).
#[test]
fn colon_prefix_is_alias_for_bind_namespace() {
    let nodes = interpret(r#"<container :value="form.email"/>"#).unwrap();
    let crate::layout::Node::Container { props, .. } = &nodes[0] else {
        panic!()
    };
    let attr = props
        .semantic
        .attrs
        .iter()
        .find(|(k, _)| k == "data-bind-value")
        .map(|(_, v)| v.as_str());
    assert_eq!(attr, Some("form.email"));
}

/// **Sugar (`<fragment>`)** — emits children verbatim with no
/// wrapping container.
#[test]
fn fragment_element_emits_children_unwrapped() {
    let nodes = interpret(r#"<fragment><text>A</text><text>B</text></fragment>"#).unwrap();
    assert_eq!(nodes.len(), 2);
    for n in &nodes {
        assert!(matches!(n, crate::layout::Node::Text { .. }));
    }
}

/// **Sugar (`<fragment>`)** — `if=` on the fragment gates its
/// whole body. Sibling-level control-flow applies before
/// element lowering.
#[test]
fn fragment_with_if_attribute_gates_children() {
    let nodes = interpret(r#"<fragment if="{false}"><text>hidden</text></fragment>"#).unwrap();
    assert!(nodes.is_empty());
    let nodes = interpret(r#"<fragment if="{true}"><text>shown</text></fragment>"#).unwrap();
    assert_eq!(nodes.len(), 1);
}

/// **Sugar (padding shorthand)** — `padding="8 16"` is
/// vertical/horizontal.
#[test]
fn padding_shorthand_two_values_is_vertical_horizontal() {
    let nodes = interpret(r#"<container padding="8 16"/>"#).unwrap();
    let crate::layout::Node::Container { props, .. } = &nodes[0] else {
        panic!()
    };
    assert!((props.padding.top - 8.0).abs() < f32::EPSILON);
    assert!((props.padding.bottom - 8.0).abs() < f32::EPSILON);
    assert!((props.padding.left - 16.0).abs() < f32::EPSILON);
    assert!((props.padding.right - 16.0).abs() < f32::EPSILON);
}

/// **Sugar (padding shorthand)** — three values: top, H, bottom.
#[test]
fn padding_shorthand_three_values_is_top_horizontal_bottom() {
    let nodes = interpret(r#"<container padding="4 8 12"/>"#).unwrap();
    let crate::layout::Node::Container { props, .. } = &nodes[0] else {
        panic!()
    };
    assert!((props.padding.top - 4.0).abs() < f32::EPSILON);
    assert!((props.padding.left - 8.0).abs() < f32::EPSILON);
    assert!((props.padding.right - 8.0).abs() < f32::EPSILON);
    assert!((props.padding.bottom - 12.0).abs() < f32::EPSILON);
}

/// **Sugar (padding shorthand)** — four values: CSS TRBL order.
#[test]
fn padding_shorthand_four_values_is_css_trbl() {
    let nodes = interpret(r#"<container padding="1 2 3 4"/>"#).unwrap();
    let crate::layout::Node::Container { props, .. } = &nodes[0] else {
        panic!()
    };
    assert!((props.padding.top - 1.0).abs() < f32::EPSILON);
    assert!((props.padding.right - 2.0).abs() < f32::EPSILON);
    assert!((props.padding.bottom - 3.0).abs() < f32::EPSILON);
    assert!((props.padding.left - 4.0).abs() < f32::EPSILON);
}

/// **Sugar (padding shorthand + units)** — each token in the
/// shorthand goes through `parse_f32`, so `rem` / `px` /
/// `em` suffixes all work per-token.
#[test]
fn padding_shorthand_with_units() {
    let nodes = interpret(r#"<container padding="1rem 8px"/>"#).unwrap();
    let crate::layout::Node::Container { props, .. } = &nodes[0] else {
        panic!()
    };
    assert!((props.padding.top - 16.0).abs() < f32::EPSILON); // 1rem
    assert!((props.padding.left - 8.0).abs() < f32::EPSILON); // 8px
}

// ─── class:foo="{cond}" reactive class toggle ──────────────

/// Truthy `class:active` applies the named PRSS class as if it
/// were part of `class="…"`.
#[test]
fn class_toggle_truthy_applies_prss_class() {
    let (sheet, _) = prism_core::language::prss::parse(
        r##"[class.active]
        background = "#0060c0"
        "##,
    );
    let scope = LowerScope::default()
        .with_binding("on", serde_json::json!(true))
        .with_stylesheet(Arc::new(sheet));
    let nodes = interpret_with_scope(r#"<container class:active="{on}"/>"#, &scope).unwrap();
    let Node::Container { props, .. } = &nodes[0] else {
        panic!()
    };
    let bg = props.background.expect("background");
    assert_eq!((bg.r, bg.g, bg.b), (0x00, 0x60, 0xc0));
}

/// Falsy `class:active` does not apply the class — the runtime
/// renders as if `class:active` were absent.
#[test]
fn class_toggle_falsy_skips_prss_class() {
    let (sheet, _) = prism_core::language::prss::parse(
        r##"[class.active]
        background = "#0060c0"
        "##,
    );
    let scope = LowerScope::default()
        .with_binding("on", serde_json::json!(false))
        .with_stylesheet(Arc::new(sheet));
    let nodes = interpret_with_scope(r#"<container class:active="{on}"/>"#, &scope).unwrap();
    let Node::Container { props, .. } = &nodes[0] else {
        panic!()
    };
    assert!(props.background.is_none());
}

/// Boolean attribute form (`class:active` with no `=value`)
/// reads as truthy — Vue / Svelte parity.
#[test]
fn class_toggle_boolean_form_is_truthy() {
    let (sheet, _) = prism_core::language::prss::parse(
        r##"[class.active]
        background = "#0060c0"
        "##,
    );
    let scope = LowerScope::default().with_stylesheet(Arc::new(sheet));
    let nodes = interpret_with_scope(r#"<container class:active/>"#, &scope).unwrap();
    let Node::Container { props, .. } = &nodes[0] else {
        panic!()
    };
    assert!(props.background.is_some());
}

/// Class round-trip — even with no stylesheet loaded, the active
/// class set lands on `Semantic::class` so the SSR / semantic-HTML
/// emitter prints `<div class="btn icon">` verbatim. Author intent
/// survives independently of PRSS lookup.
#[test]
fn class_attribute_round_trips_through_semantic_class() {
    let nodes = interpret(r#"<container class="btn icon"/>"#).unwrap();
    let Node::Container { props, .. } = &nodes[0] else {
        panic!()
    };
    assert_eq!(props.semantic.class.as_deref(), Some("btn icon"));
}

/// Class toggles append to the static class list and round-trip
/// through `Semantic::class` together. No-stylesheet path; the
/// HTML backend would emit `<div class="btn primary">`.
#[test]
fn class_toggle_extends_semantic_class_list() {
    let scope = LowerScope::default().with_binding("on", serde_json::json!(true));
    let nodes =
        interpret_with_scope(r#"<container class="btn" class:primary="{on}"/>"#, &scope).unwrap();
    let Node::Container { props, .. } = &nodes[0] else {
        panic!()
    };
    assert_eq!(props.semantic.class.as_deref(), Some("btn primary"));
}

/// Falsy `class:foo` keeps the static list intact in
/// `Semantic::class` — only truthy toggles contribute.
#[test]
fn falsy_class_toggle_does_not_appear_in_semantic_class() {
    let scope = LowerScope::default().with_binding("on", serde_json::json!(false));
    let nodes =
        interpret_with_scope(r#"<container class="btn" class:primary="{on}"/>"#, &scope).unwrap();
    let Node::Container { props, .. } = &nodes[0] else {
        panic!()
    };
    assert_eq!(props.semantic.class.as_deref(), Some("btn"));
}

/// SSR backend reads `Semantic::class` — verify a class attribute
/// authored on a PRUI container actually emits in the HTML.
#[test]
fn ssr_backend_emits_class_attribute_for_authored_class() {
    let nodes = interpret(r#"<container class="btn primary"/>"#).unwrap();
    let html = crate::backends::semantic_html::lower(&nodes[0]);
    assert!(
        html.contains("class=\"btn primary\""),
        "SSR HTML missing class attr: {html}"
    );
}

/// `class:foo` layers on top of static `class="…"` — both lists
/// participate in PRSS application; later toggles win on key
/// conflicts.
#[test]
fn class_toggle_layers_on_static_class_attribute() {
    let (sheet, _) = prism_core::language::prss::parse(
        r##"
        [class.btn]
        background = "#ffffff"

        [class.primary]
        background = "#0060c0"
        "##,
    );
    let scope = LowerScope::default()
        .with_binding("primary", serde_json::json!(true))
        .with_stylesheet(Arc::new(sheet));
    let nodes = interpret_with_scope(
        r#"<container class="btn" class:primary="{primary}"/>"#,
        &scope,
    )
    .unwrap();
    let Node::Container { props, .. } = &nodes[0] else {
        panic!()
    };
    let bg = props.background.expect("background");
    // `primary` declared after `btn` in attribute order; later wins.
    assert_eq!((bg.r, bg.g, bg.b), (0x00, 0x60, 0xc0));
}

// ─── PRSS descendant selectors ─────────────────────────────

/// `.btn .icon` matches an `<icon>` (well, container with
/// class="icon") nested under a container with class="btn",
/// even with intermediate ancestors.
#[test]
fn prss_descendant_selector_matches_through_ancestor_chain() {
    let (sheet, _) = prism_core::language::prss::parse(
        r##"
        [class.btn]
        background = "#fff"

        [class.icon]
        background = "#aaa"

        [class.".btn .icon"]
        background = "#0060c0"
        "##,
    );
    let scope = LowerScope::default().with_stylesheet(Arc::new(sheet));
    let nodes = interpret_with_scope(
        r#"<container class="btn">
            <container class="wrap">
                <container id="leaf" class="icon"/>
            </container>
        </container>"#,
        &scope,
    )
    .unwrap();
    // Walk to the deepest container.
    let leaf = find_container_by_id(&nodes, "leaf").expect("leaf");
    let Node::Container { props, .. } = leaf else {
        panic!()
    };
    let bg = props.background.expect("background");
    assert_eq!((bg.r, bg.g, bg.b), (0x00, 0x60, 0xc0));
}

/// Without a matching ancestor `.btn`, the descendant selector
/// does not match — the leaf falls back to its flat `.icon`
/// styling.
#[test]
fn prss_descendant_selector_misses_without_matching_ancestor() {
    let (sheet, _) = prism_core::language::prss::parse(
        r##"
        [class.icon]
        background = "#aaa"

        [class.".btn .icon"]
        background = "#0060c0"
        "##,
    );
    let scope = LowerScope::default().with_stylesheet(Arc::new(sheet));
    let nodes = interpret_with_scope(
        r#"<container>
            <container id="leaf" class="icon"/>
        </container>"#,
        &scope,
    )
    .unwrap();
    let leaf = find_container_by_id(&nodes, "leaf").expect("leaf");
    let Node::Container { props, .. } = leaf else {
        panic!()
    };
    let bg = props.background.expect("background");
    assert_eq!((bg.r, bg.g, bg.b), (0xaa, 0xaa, 0xaa));
}

fn find_container_by_id<'a>(nodes: &'a [Node], id: &str) -> Option<&'a Node> {
    for n in nodes {
        if let Node::Container {
            id: cid, children, ..
        } = n
        {
            if cid == id {
                return Some(n);
            }
            if let Some(found) = find_container_by_id(children, id) {
                return Some(found);
            }
        }
    }
    None
}

/// Three-segment descendant chain (`.a .b .c`) walks two
/// ancestors before landing on the current element.
#[test]
fn prss_three_segment_descendant_selector_matches() {
    let (sheet, _) = prism_core::language::prss::parse(
        r##"
        [class.".a .b .c"]
        background = "#0060c0"
        "##,
    );
    let scope = LowerScope::default().with_stylesheet(Arc::new(sheet));
    let nodes = interpret_with_scope(
        r#"<container class="a">
            <container class="b">
                <container id="leaf" class="c"/>
            </container>
        </container>"#,
        &scope,
    )
    .unwrap();
    let leaf = find_container_by_id(&nodes, "leaf").expect("leaf");
    let Node::Container { props, .. } = leaf else {
        panic!()
    };
    let bg = props.background.expect("background");
    assert_eq!((bg.r, bg.g, bg.b), (0x00, 0x60, 0xc0));
}

/// Three-segment chain skips intermediate ancestors that don't
/// match — `.a .c` finds `c` even if there's a non-matching `.x`
/// between `a` and `c`.
#[test]
fn prss_descendant_skips_intermediate_non_matching_ancestors() {
    let (sheet, _) = prism_core::language::prss::parse(
        r##"
        [class.".a .c"]
        background = "#0060c0"
        "##,
    );
    let scope = LowerScope::default().with_stylesheet(Arc::new(sheet));
    let nodes = interpret_with_scope(
        r#"<container class="a">
            <container class="x">
                <container class="y">
                    <container id="leaf" class="c"/>
                </container>
            </container>
        </container>"#,
        &scope,
    )
    .unwrap();
    let leaf = find_container_by_id(&nodes, "leaf").expect("leaf");
    let Node::Container { props, .. } = leaf else {
        panic!()
    };
    let bg = props.background.expect("background");
    assert_eq!((bg.r, bg.g, bg.b), (0x00, 0x60, 0xc0));
}

/// Reverse-order ancestors do not match — `.a .b` requires `a`
/// strictly outside `b`.
#[test]
fn prss_descendant_selector_requires_outer_to_inner_order() {
    let (sheet, _) = prism_core::language::prss::parse(
        r##"
        [class.".btn .icon"]
        background = "#0060c0"
        "##,
    );
    let scope = LowerScope::default().with_stylesheet(Arc::new(sheet));
    // `.icon` outside, `.btn` inside — selector doesn't match.
    let nodes = interpret_with_scope(
        r#"<container class="icon">
            <container id="leaf" class="btn"/>
        </container>"#,
        &scope,
    )
    .unwrap();
    let leaf = find_container_by_id(&nodes, "leaf").expect("leaf");
    let Node::Container { props, .. } = leaf else {
        panic!()
    };
    assert!(props.background.is_none());
}

/// Descendant selector with state variant: `[class.".btn .icon".hovered]`
/// applies a hover override on the matched leaf. The state lands
/// on `ContainerProps::hover` (the same path inline
/// `style:background:hovered` takes).
#[test]
fn prss_descendant_selector_with_state_variant_lowers_into_hover_overrides() {
    let (sheet, _) = prism_core::language::prss::parse(
        r##"
        [class.".btn .icon"]
        background = "#0060c0"

        [class.".btn .icon".hovered]
        background = "#a78bfa"
        "##,
    );
    let scope = LowerScope::default().with_stylesheet(Arc::new(sheet));
    let nodes = interpret_with_scope(
        r#"<container class="btn">
            <container id="leaf" class="icon"/>
        </container>"#,
        &scope,
    )
    .unwrap();
    let leaf = find_container_by_id(&nodes, "leaf").expect("leaf");
    let Node::Container { props, .. } = leaf else {
        panic!()
    };
    let hover = props.hover.as_ref().expect("hover overrides");
    let bg = hover.background.expect("hover background");
    assert_eq!((bg.r, bg.g, bg.b), (0xa7, 0x8b, 0xfa));
}

/// More-specific descendant selector overrides flat-class on the
/// same key — CSS specificity ordering: `.btn .icon` (specificity
/// 0,0,2,0) wins over `.icon` (0,0,1,0).
#[test]
fn prss_descendant_selector_overrides_flat_class_for_same_key() {
    let (sheet, _) = prism_core::language::prss::parse(
        r##"
        [class.icon]
        background = "#aaaaaa"

        [class.".btn .icon"]
        background = "#0060c0"
        "##,
    );
    let scope = LowerScope::default().with_stylesheet(Arc::new(sheet));
    let nodes = interpret_with_scope(
        r#"<container class="btn">
            <container id="leaf" class="icon"/>
        </container>"#,
        &scope,
    )
    .unwrap();
    let leaf = find_container_by_id(&nodes, "leaf").expect("leaf");
    let Node::Container { props, .. } = leaf else {
        panic!()
    };
    let bg = props.background.expect("background");
    // Descendant selector wins.
    assert_eq!((bg.r, bg.g, bg.b), (0x00, 0x60, 0xc0));
}

/// Multiple descendant selectors that all match contribute their
/// disjoint properties — `.a .x` sets background, `.b .x` sets
/// radius; both apply on a leaf nested under both ancestors.
#[test]
fn prss_multiple_descendant_selectors_apply_disjoint_keys() {
    let (sheet, _) = prism_core::language::prss::parse(
        r##"
        [class.".a .x"]
        background = "#0060c0"

        [class.".b .x"]
        radius = 8
        "##,
    );
    let scope = LowerScope::default().with_stylesheet(Arc::new(sheet));
    let nodes = interpret_with_scope(
        r#"<container class="a">
            <container class="b">
                <container id="leaf" class="x"/>
            </container>
        </container>"#,
        &scope,
    )
    .unwrap();
    let leaf = find_container_by_id(&nodes, "leaf").expect("leaf");
    let Node::Container { props, .. } = leaf else {
        panic!()
    };
    let bg = props.background.expect("background");
    assert_eq!((bg.r, bg.g, bg.b), (0x00, 0x60, 0xc0));
    assert!((props.radius.tl - 8.0).abs() < f32::EPSILON);
}

/// Conflicting descendant selectors resolve by declaration
/// order — later wins, mirroring `extends` chain ordering.
#[test]
fn prss_conflicting_descendant_selectors_resolve_by_declaration_order() {
    let (sheet, _) = prism_core::language::prss::parse(
        r##"
        [class.".a .x"]
        background = "#aaaaaa"

        [class.".b .x"]
        background = "#0060c0"
        "##,
    );
    let scope = LowerScope::default().with_stylesheet(Arc::new(sheet));
    let nodes = interpret_with_scope(
        r#"<container class="a">
            <container class="b">
                <container id="leaf" class="x"/>
            </container>
        </container>"#,
        &scope,
    )
    .unwrap();
    let leaf = find_container_by_id(&nodes, "leaf").expect("leaf");
    let Node::Container { props, .. } = leaf else {
        panic!()
    };
    let bg = props.background.expect("background");
    // `.b .x` declared second → wins.
    assert_eq!((bg.r, bg.g, bg.b), (0x00, 0x60, 0xc0));
}

/// Descendant selector inside a sibling subtree must not "leak"
/// — once we leave the matching ancestor's subtree, the
/// chain unwinds.
#[test]
fn prss_descendant_selector_does_not_leak_into_sibling_subtree() {
    let (sheet, _) = prism_core::language::prss::parse(
        r##"
        [class.".btn .icon"]
        background = "#0060c0"
        "##,
    );
    let scope = LowerScope::default().with_stylesheet(Arc::new(sheet));
    let nodes = interpret_with_scope(
        r#"<container>
            <container class="btn">
                <container id="inner" class="icon"/>
            </container>
            <container id="sibling" class="icon"/>
        </container>"#,
        &scope,
    )
    .unwrap();
    let inner = find_container_by_id(&nodes, "inner").expect("inner");
    let sibling = find_container_by_id(&nodes, "sibling").expect("sibling");
    let Node::Container { props: ip, .. } = inner else {
        panic!()
    };
    let Node::Container { props: sp, .. } = sibling else {
        panic!()
    };
    let inner_bg = ip.background.expect("inner background");
    assert_eq!((inner_bg.r, inner_bg.g, inner_bg.b), (0x00, 0x60, 0xc0));
    // Sibling has no `.btn` ancestor — selector misses.
    assert!(sp.background.is_none());
}

/// Descendant selector with class toggle on the leaf — the
/// truthy `class:icon="{cond}"` toggle still feeds the
/// rightmost-segment match.
#[test]
fn prss_descendant_selector_with_class_toggle_on_leaf() {
    let (sheet, _) = prism_core::language::prss::parse(
        r##"
        [class.".btn .icon"]
        background = "#0060c0"
        "##,
    );
    let scope = LowerScope::default()
        .with_binding("show", serde_json::json!(true))
        .with_stylesheet(Arc::new(sheet));
    let nodes = interpret_with_scope(
        r#"<container class="btn">
            <container id="leaf" class:icon="{show}"/>
        </container>"#,
        &scope,
    )
    .unwrap();
    let leaf = find_container_by_id(&nodes, "leaf").expect("leaf");
    let Node::Container { props, .. } = leaf else {
        panic!()
    };
    let bg = props.background.expect("background");
    assert_eq!((bg.r, bg.g, bg.b), (0x00, 0x60, 0xc0));
}

/// Descendant selector with class toggle on the *ancestor* — the
/// truthy `class:btn="{cond}"` toggle on an outer container
/// participates in the chain match.
#[test]
fn prss_descendant_selector_with_class_toggle_on_ancestor() {
    let (sheet, _) = prism_core::language::prss::parse(
        r##"
        [class.".btn .icon"]
        background = "#0060c0"
        "##,
    );
    let scope = LowerScope::default()
        .with_binding("primary", serde_json::json!(true))
        .with_stylesheet(Arc::new(sheet));
    let nodes = interpret_with_scope(
        r#"<container class:btn="{primary}">
            <container id="leaf" class="icon"/>
        </container>"#,
        &scope,
    )
    .unwrap();
    let leaf = find_container_by_id(&nodes, "leaf").expect("leaf");
    let Node::Container { props, .. } = leaf else {
        panic!()
    };
    let bg = props.background.expect("background");
    assert_eq!((bg.r, bg.g, bg.b), (0x00, 0x60, 0xc0));
}

/// Same class on multiple ancestors doesn't break the matcher —
/// the inner ancestor consumes the segment, the outer one is
/// available for further matches if needed.
#[test]
fn prss_descendant_selector_handles_repeated_class_in_chain() {
    let (sheet, _) = prism_core::language::prss::parse(
        r##"
        [class.".btn .btn .icon"]
        background = "#0060c0"
        "##,
    );
    let scope = LowerScope::default().with_stylesheet(Arc::new(sheet));
    let nodes = interpret_with_scope(
        r#"<container class="btn">
            <container class="btn">
                <container id="leaf" class="icon"/>
            </container>
        </container>"#,
        &scope,
    )
    .unwrap();
    let leaf = find_container_by_id(&nodes, "leaf").expect("leaf");
    let Node::Container { props, .. } = leaf else {
        panic!()
    };
    let bg = props.background.expect("background");
    assert_eq!((bg.r, bg.g, bg.b), (0x00, 0x60, 0xc0));
}

/// Bare-key (unquoted) descendant selector also parses through
/// `selector_segments` — `[class."btn icon"]` matches
/// `[class.".btn .icon"]` because both decompose to `["btn", "icon"]`.
#[test]
fn prss_descendant_selector_accepts_bare_segments_without_dots() {
    let (sheet, _) = prism_core::language::prss::parse(
        r##"
        [class."btn icon"]
        background = "#0060c0"
        "##,
    );
    let scope = LowerScope::default().with_stylesheet(Arc::new(sheet));
    let nodes = interpret_with_scope(
        r#"<container class="btn">
            <container id="leaf" class="icon"/>
        </container>"#,
        &scope,
    )
    .unwrap();
    let leaf = find_container_by_id(&nodes, "leaf").expect("leaf");
    let Node::Container { props, .. } = leaf else {
        panic!()
    };
    let bg = props.background.expect("background");
    assert_eq!((bg.r, bg.g, bg.b), (0x00, 0x60, 0xc0));
}

// ─── Short-name token references ───────────────────────────

/// PRSS `radius = "md"` resolves through the active token table
/// to the `tokens.radius.md` value.
#[test]
fn prss_short_name_radius_resolves_through_tokens() {
    let (sheet, _) = prism_core::language::prss::parse(
        r##"[class.btn]
        radius = "md"
        "##,
    );
    let scope = LowerScope::default()
        .with_design_tokens(&prism_core::design_tokens::DEFAULT_TOKENS)
        .with_stylesheet(Arc::new(sheet));
    let nodes = interpret_with_scope(r#"<container class="btn"/>"#, &scope).unwrap();
    let Node::Container { props, .. } = &nodes[0] else {
        panic!()
    };
    let expected = prism_core::design_tokens::DEFAULT_TOKENS.radius.md as f32;
    assert!((props.radius.tl - expected).abs() < f32::EPSILON);
}

/// Inline `style:background="accent"` resolves through
/// `tokens.colors.accent`.
#[test]
fn inline_style_short_name_color_resolves_through_tokens() {
    let scope =
        LowerScope::default().with_design_tokens(&prism_core::design_tokens::DEFAULT_TOKENS);
    let nodes = interpret_with_scope(r#"<container style:background="accent"/>"#, &scope).unwrap();
    let Node::Container { props, .. } = &nodes[0] else {
        panic!()
    };
    let expected = &prism_core::design_tokens::DEFAULT_TOKENS.colors.accent;
    let bg = props.background.expect("background");
    assert_eq!(bg.r, expected.r);
    assert_eq!(bg.g, expected.g);
    assert_eq!(bg.b, expected.b);
}

/// Bare `padding="md"` resolves through `tokens.spacing.md`.
#[test]
fn bare_padding_short_name_resolves_through_spacing_tokens() {
    let scope =
        LowerScope::default().with_design_tokens(&prism_core::design_tokens::DEFAULT_TOKENS);
    let nodes = interpret_with_scope(r#"<container padding="md"/>"#, &scope).unwrap();
    let Node::Container { props, .. } = &nodes[0] else {
        panic!()
    };
    let expected = prism_core::design_tokens::DEFAULT_TOKENS.spacing.md as f32;
    assert!((props.padding.left - expected).abs() < f32::EPSILON);
}

/// Unknown short names drop silently — the value passes through
/// to the parser, which fails to interpret and leaves the prop
/// at its default. Matches PRSS's "unknown drops cleanly" rule.
#[test]
fn short_name_lookup_miss_falls_through_to_parser() {
    let scope =
        LowerScope::default().with_design_tokens(&prism_core::design_tokens::DEFAULT_TOKENS);
    let nodes = interpret_with_scope(r#"<container padding="not-a-token-name"/>"#, &scope).unwrap();
    let Node::Container { props, .. } = &nodes[0] else {
        panic!()
    };
    // Default Padding::all(0) survives.
    assert!((props.padding.left - 0.0).abs() < f32::EPSILON);
}

/// Custom token (PRSS-defined `tokens.colors.brand-purple`)
/// resolves through the merged token table on the scope.
#[test]
fn short_name_resolves_through_custom_prss_token() {
    let (sheet, _) = prism_core::language::prss::parse(
        r##"
        [tokens.colors]
        brand-purple = "#5b21b6"

        [class.btn]
        background = "brand-purple"
        "##,
    );
    let scope = LowerScope::default()
        .with_design_tokens(&prism_core::design_tokens::DEFAULT_TOKENS)
        .with_stylesheet(Arc::new(sheet));
    let nodes = interpret_with_scope(r#"<container class="btn"/>"#, &scope).unwrap();
    let Node::Container { props, .. } = &nodes[0] else {
        panic!()
    };
    let bg = props.background.expect("background");
    assert_eq!((bg.r, bg.g, bg.b), (0x5b, 0x21, 0xb6));
}

/// `font-size = "lg"` resolves to `tokens.typography.font-size-lg`
/// — the typography bucket's `font-size-<short>` key shape.
#[test]
fn short_name_resolves_font_size_lg_through_typography_table() {
    let scope =
        LowerScope::default().with_design_tokens(&prism_core::design_tokens::DEFAULT_TOKENS);
    let nodes = interpret_with_scope(r#"<text font-size="lg">Hi</text>"#, &scope).unwrap();
    let Node::Text { props, .. } = &nodes[0] else {
        panic!()
    };
    let expected = prism_core::design_tokens::DEFAULT_TOKENS
        .typography
        .font_size_lg as f32;
    assert!((props.font_size - expected).abs() < f32::EPSILON);
}

/// Per-side padding short names also resolve — `padding-left = "md"`
/// reads through `tokens.spacing.md`.
#[test]
fn short_name_resolves_per_side_padding_through_spacing() {
    let scope =
        LowerScope::default().with_design_tokens(&prism_core::design_tokens::DEFAULT_TOKENS);
    let nodes = interpret_with_scope(
        r#"<container padding-left="md" padding-right="lg"/>"#,
        &scope,
    )
    .unwrap();
    let Node::Container { props, .. } = &nodes[0] else {
        panic!()
    };
    let expected_md = prism_core::design_tokens::DEFAULT_TOKENS.spacing.md as f32;
    let expected_lg = prism_core::design_tokens::DEFAULT_TOKENS.spacing.lg as f32;
    assert!((props.padding.left - expected_md).abs() < f32::EPSILON);
    assert!((props.padding.right - expected_lg).abs() < f32::EPSILON);
}

/// `gap = "sm"` resolves through `tokens.spacing.sm`.
#[test]
fn short_name_resolves_gap_through_spacing() {
    let scope =
        LowerScope::default().with_design_tokens(&prism_core::design_tokens::DEFAULT_TOKENS);
    let nodes = interpret_with_scope(r#"<container gap="sm"/>"#, &scope).unwrap();
    let Node::Container { props, .. } = &nodes[0] else {
        panic!()
    };
    let expected = prism_core::design_tokens::DEFAULT_TOKENS.spacing.sm as f32;
    assert!((props.gap - expected).abs() < f32::EPSILON);
}

/// Sizing keywords (`grow`, `fit`, `auto`) on `width`/`height`
/// must NOT be resolved as short tokens — these keys aren't in
/// the bucket table, so the keyword passes through to
/// `parse_sizing` unchanged.
#[test]
fn short_name_resolution_does_not_eat_sizing_keywords() {
    use crate::layout::Sizing;
    let scope =
        LowerScope::default().with_design_tokens(&prism_core::design_tokens::DEFAULT_TOKENS);
    let nodes = interpret_with_scope(r#"<container width="grow" height="fit"/>"#, &scope).unwrap();
    let Node::Container { props, .. } = &nodes[0] else {
        panic!()
    };
    assert!(matches!(props.width, Sizing::Grow));
    assert!(matches!(props.height, Sizing::Fit));
}

/// `direction = "row"` is a known direction keyword — even
/// though "row" looks like a token name, the `direction` key
/// isn't in the short-name bucket map so resolution is skipped
/// and the keyword reaches `parse_direction` intact.
#[test]
fn short_name_resolution_does_not_eat_direction_keyword() {
    use crate::layout::Direction;
    let scope =
        LowerScope::default().with_design_tokens(&prism_core::design_tokens::DEFAULT_TOKENS);
    let nodes = interpret_with_scope(r#"<container direction="row"/>"#, &scope).unwrap();
    let Node::Container { props, .. } = &nodes[0] else {
        panic!()
    };
    assert!(matches!(props.direction, Direction::Row));
}

/// Hex colors (`#…`) must NOT be resolved as short tokens —
/// the leading `#` rejects them at `is_bare_token_name`.
#[test]
fn short_name_resolution_skips_hex_color_values() {
    let scope =
        LowerScope::default().with_design_tokens(&prism_core::design_tokens::DEFAULT_TOKENS);
    let nodes =
        interpret_with_scope(r##"<container style:background="#7c3aed"/>"##, &scope).unwrap();
    let Node::Container { props, .. } = &nodes[0] else {
        panic!()
    };
    let bg = props.background.expect("background");
    assert_eq!((bg.r, bg.g, bg.b), (0x7c, 0x3a, 0xed));
}

/// Numeric values with units (e.g. `"1rem"`, `"50%"`,
/// `"12px"`, `"8"`) must NOT be resolved — they have a leading
/// digit so `is_bare_token_name` rejects them. The `1rem` here
/// resolves through rem-expansion using the scope's
/// `tokens.typography.font-size-md` base (14 in DEFAULT_TOKENS),
/// not through `tokens.spacing.1rem` lookup.
#[test]
fn short_name_resolution_skips_numeric_and_unit_values() {
    let scope =
        LowerScope::default().with_design_tokens(&prism_core::design_tokens::DEFAULT_TOKENS);
    let nodes = interpret_with_scope(r#"<container padding="1rem"/>"#, &scope).unwrap();
    let Node::Container { props, .. } = &nodes[0] else {
        panic!()
    };
    let expected = prism_core::design_tokens::DEFAULT_TOKENS
        .typography
        .font_size_md as f32;
    assert!((props.padding.left - expected).abs() < f32::EPSILON);
}

/// `data:` namespace attrs are NOT eligible for short-name
/// resolution — `data:role="md"` round-trips verbatim.
#[test]
fn short_name_resolution_skips_data_namespace() {
    let scope =
        LowerScope::default().with_design_tokens(&prism_core::design_tokens::DEFAULT_TOKENS);
    let nodes = interpret_with_scope(r#"<container data:role="md"/>"#, &scope).unwrap();
    let Node::Container { props, .. } = &nodes[0] else {
        panic!()
    };
    // The data attribute carries through as a literal "md",
    // not the resolved `tokens.spacing.md` value.
    assert!(
        props
            .semantic
            .attrs
            .iter()
            .any(|(k, v)| k == "data-role" && v == "md"),
        "data:role should round-trip verbatim, got {:?}",
        props.semantic.attrs
    );
}

/// `aria:` namespace attrs are NOT eligible for short-name
/// resolution — `aria:level="md"` round-trips verbatim.
#[test]
fn short_name_resolution_skips_aria_namespace() {
    let scope =
        LowerScope::default().with_design_tokens(&prism_core::design_tokens::DEFAULT_TOKENS);
    let nodes = interpret_with_scope(r#"<container aria:level="md"/>"#, &scope).unwrap();
    let Node::Container { props, .. } = &nodes[0] else {
        panic!()
    };
    assert!(
        props
            .semantic
            .attrs
            .iter()
            .any(|(k, v)| k == "aria-level" && v == "md"),
        "aria:level should round-trip verbatim, got {:?}",
        props.semantic.attrs
    );
}

/// Short-name resolution still fires inside a state-suffixed
/// style override — `style:background:hovered="accent"` resolves
/// through `tokens.colors.accent` then writes to the hover bundle.
#[test]
fn short_name_resolves_inside_style_state_override() {
    let scope =
        LowerScope::default().with_design_tokens(&prism_core::design_tokens::DEFAULT_TOKENS);
    let nodes =
        interpret_with_scope(r#"<container style:background:hovered="accent"/>"#, &scope).unwrap();
    let Node::Container { props, .. } = &nodes[0] else {
        panic!()
    };
    let hover = props.hover.as_ref().expect("hover overrides");
    let bg = hover.background.expect("hover background");
    let expected = &prism_core::design_tokens::DEFAULT_TOKENS.colors.accent;
    assert_eq!(bg.r, expected.r);
}

/// Compound state on a PRSS class — `[class.btn.hovered]` with
/// short-name `background = "accent-muted"` resolves through
/// the typography path correctly.
#[test]
fn short_name_resolves_inside_prss_class_state_variant() {
    let (sheet, _) = prism_core::language::prss::parse(
        r##"
        [class.btn]
        background = "surface"

        [class.btn.hovered]
        background = "accent-muted"
        "##,
    );
    let scope = LowerScope::default()
        .with_design_tokens(&prism_core::design_tokens::DEFAULT_TOKENS)
        .with_stylesheet(Arc::new(sheet));
    let nodes = interpret_with_scope(r#"<container class="btn"/>"#, &scope).unwrap();
    let Node::Container { props, .. } = &nodes[0] else {
        panic!()
    };
    // Base: background = surface
    let surface = &prism_core::design_tokens::DEFAULT_TOKENS.colors.surface;
    let bg = props.background.expect("background");
    assert_eq!(bg.r, surface.r);
    // Hover: accent-muted
    let hover = props.hover.as_ref().expect("hover");
    let muted = &prism_core::design_tokens::DEFAULT_TOKENS
        .colors
        .accent_muted;
    let hover_bg = hover.background.expect("hover background");
    assert_eq!(hover_bg.r, muted.r);
}

/// Without a `tokens` binding installed, short-name resolution
/// is a no-op — the literal value passes through to the parser.
#[test]
fn short_name_resolution_no_op_without_tokens() {
    let scope = LowerScope::default(); // no tokens
    let nodes = interpret_with_scope(r#"<container padding="md"/>"#, &scope).unwrap();
    let Node::Container { props, .. } = &nodes[0] else {
        panic!()
    };
    // No resolution happened; "md" failed `parse_padding_shorthand`
    // and the default Padding::all(0) survived.
    assert!((props.padding.left - 0.0).abs() < f32::EPSILON);
}

// ─── Token-driven rem base ─────────────────────────────────

/// `1rem` reads through `tokens.typography.font-size-md` when
/// the scope has a token table installed. Double the base
/// → double the resolved padding.
#[test]
fn rem_base_reads_from_typography_font_size_md() {
    let mut tokens = prism_core::design_tokens::DEFAULT_TOKENS;
    tokens.typography.font_size_md = 32; // 32px base instead of 16
    let scope = LowerScope::default().with_design_tokens(&tokens);
    let nodes = interpret_with_scope(r#"<container padding="1rem"/>"#, &scope).unwrap();
    let Node::Container { props, .. } = &nodes[0] else {
        panic!()
    };
    assert!((props.padding.left - 32.0).abs() < f32::EPSILON);
}

/// Without a token binding, `rem_px` falls back to the canonical
/// 16px so `1rem` reads as 16 — preserving existing behaviour.
#[test]
fn rem_base_defaults_to_sixteen_without_tokens() {
    let scope = LowerScope::default();
    let nodes = interpret_with_scope(r#"<container padding="1rem"/>"#, &scope).unwrap();
    let Node::Container { props, .. } = &nodes[0] else {
        panic!()
    };
    assert!((props.padding.left - 16.0).abs() < f32::EPSILON);
}

/// Multi-segment shorthand expansion (`padding="1rem 2rem"`)
/// honours the scope-driven rem base across every segment.
#[test]
fn rem_base_applies_to_each_padding_shorthand_token() {
    let mut tokens = prism_core::design_tokens::DEFAULT_TOKENS;
    tokens.typography.font_size_md = 20;
    let scope = LowerScope::default().with_design_tokens(&tokens);
    let nodes = interpret_with_scope(r#"<container padding="1rem 2rem"/>"#, &scope).unwrap();
    let Node::Container { props, .. } = &nodes[0] else {
        panic!()
    };
    // top = 1rem = 20, left = 2rem = 40
    assert!((props.padding.top - 20.0).abs() < f32::EPSILON);
    assert!((props.padding.left - 40.0).abs() < f32::EPSILON);
}

// ---------- `<dispatch tag="{expr}"/>` runtime-tag dispatch ----------

/// A literal `tag="container"` rewrites the dispatch element into
/// a closed-set primitive and lowers it through the container arm
/// — every container attribute (`gap`, `style:*`, …) flows through
/// the synthesised element verbatim.
#[test]
fn dispatch_tag_literal_routes_to_primitive_container() {
    let nodes =
        interpret(r##"<dispatch tag="container" gap="12" style:background="#aabbcc"/>"##).unwrap();
    assert_eq!(nodes.len(), 1);
    let Node::Container { props, .. } = &nodes[0] else {
        panic!("expected container, got {:?}", nodes[0]);
    };
    assert!((props.gap - 12.0).abs() < f32::EPSILON);
    let bg = props.background.expect("bg");
    assert_eq!((bg.r, bg.g, bg.b), (0xaa, 0xbb, 0xcc));
}

/// `tag="{binding}"` resolves against scope and re-dispatches.
/// Closes the §15 "data-driven tag" authoring gap for the
/// primitive vocabulary.
#[test]
fn dispatch_tag_resolves_through_scope_binding() {
    let scope = LowerScope::default().with_binding("kind", json!("text"));
    let nodes = interpret_with_scope(r#"<dispatch tag="{kind}">hello</dispatch>"#, &scope).unwrap();
    let Node::Text { content, .. } = &nodes[0] else {
        panic!("expected text");
    };
    assert_eq!(content, "hello");
}

/// Unknown resolved tags fall through to the resolver — no resolver
/// means the "drop wrapper, keep children" default fires, exactly
/// as for any author-written unknown tag.
#[test]
fn dispatch_tag_unknown_falls_through_to_default_unknown_tag() {
    let nodes = interpret(r#"<dispatch tag="my.widget"><text>inner</text></dispatch>"#).unwrap();
    assert_eq!(nodes.len(), 1);
    let Node::Text { content, .. } = &nodes[0] else {
        panic!()
    };
    assert_eq!(content, "inner");
}

/// Empty/missing `tag=` leaves the element as bare `<dispatch>`,
/// which falls through to the resolver path (legacy `component=`
/// form). Without a resolver, the unknown-tag default fires.
#[test]
fn dispatch_with_empty_tag_attribute_falls_through() {
    let nodes = interpret(r#"<dispatch tag=""><text>fallback</text></dispatch>"#).unwrap();
    let Node::Text { content, .. } = &nodes[0] else {
        panic!()
    };
    assert_eq!(content, "fallback");
}

/// The synthesised element retains every non-`tag` attribute so a
/// dispatched primitive sees the same prop shape an authored
/// element would.
#[test]
fn dispatch_tag_drops_tag_attribute_but_keeps_others() {
    let nodes =
        interpret(r##"<dispatch tag="text" font-size="22" id="dyn-title">Title</dispatch>"##)
            .unwrap();
    let Node::Text { id, content, props } = &nodes[0] else {
        panic!()
    };
    assert_eq!(id, "dyn-title");
    assert_eq!(content, "Title");
    assert!((props.font_size - 22.0).abs() < f32::EPSILON);
}

/// Dispatch composes with `for=` — typical use case is rendering
/// a row whose primitive varies by data.
#[test]
fn dispatch_tag_inside_for_loop_emits_one_node_per_item() {
    let scope = LowerScope::default().with_binding(
        "rows",
        json!([
            {"kind": "text", "body": "A"},
            {"kind": "text", "body": "B"},
            {"kind": "spacer"},
        ]),
    );
    let nodes = interpret_with_scope(
        r#"<container>
            <dispatch for="row in rows" tag="{row.kind}">{row.body}</dispatch>
           </container>"#,
        &scope,
    )
    .unwrap();
    let Node::Container { children, .. } = &nodes[0] else {
        panic!()
    };
    assert_eq!(children.len(), 3);
    assert!(matches!(children[0], Node::Text { .. }));
    assert!(matches!(children[1], Node::Text { .. }));
    assert!(matches!(children[2], Node::Spacer { .. }));
}

// ---------- Virtual `.length` / `.first` / `.last` segments ----------

/// `items.length` reads as the array length from a bare-path
/// lookup — closes the `for="i in 0..items.length"` authoring
/// gap the PRUI reference promises.
#[test]
fn array_length_virtual_segment_reads_as_count() {
    let scope = LowerScope::default().with_binding("rows", json!(["a", "b", "c", "d"]));
    let nodes = interpret_with_scope(
        r#"<container>
            <text for="i in 0..rows.length">{i}</text>
           </container>"#,
        &scope,
    )
    .unwrap();
    let Node::Container { children, .. } = &nodes[0] else {
        panic!()
    };
    assert_eq!(children.len(), 4);
}

/// `obj.length` reads as the number of keys.
#[test]
fn object_length_virtual_segment_reads_as_key_count() {
    let scope =
        LowerScope::default().with_binding("form", json!({"name": "x", "email": "y", "age": 1}));
    let nodes = interpret_with_scope(r#"<text>{form.length}</text>"#, &scope).unwrap();
    let Node::Text { content, .. } = &nodes[0] else {
        panic!()
    };
    assert_eq!(content, "3");
}

/// `items.first` / `items.last` resolve to the first/last
/// element value.
#[test]
fn array_first_and_last_virtual_segments_resolve_to_endpoints() {
    let scope = LowerScope::default().with_binding("rows", json!(["alpha", "beta", "gamma"]));
    let nodes = interpret_with_scope(
        r#"<container>
            <text>{rows.first}</text>
            <text>{rows.last}</text>
           </container>"#,
        &scope,
    )
    .unwrap();
    let Node::Container { children, .. } = &nodes[0] else {
        panic!()
    };
    let Node::Text { content: first, .. } = &children[0] else {
        panic!()
    };
    let Node::Text { content: last, .. } = &children[1] else {
        panic!()
    };
    assert_eq!(first, "alpha");
    assert_eq!(last, "gamma");
}

/// Empty array → first/last return Null which stringifies to ""
/// (the empty-string filter elsewhere already absorbs this).
#[test]
fn empty_array_first_returns_empty_string() {
    let scope = LowerScope::default().with_binding("rows", json!([]));
    let nodes = interpret_with_scope(r#"<text>{rows.first}</text>"#, &scope).unwrap();
    let Node::Text { content, .. } = &nodes[0] else {
        panic!()
    };
    assert_eq!(content, "");
}

/// `if="{items.length}"` is truthy when non-empty, falsy when
/// empty — matches the JS array-truthiness rule authors expect.
#[test]
fn if_with_length_is_falsy_on_empty_array() {
    let scope = LowerScope::default().with_binding("rows", json!([]));
    let nodes = interpret_with_scope(r#"<text if="{rows.length}">visible</text>"#, &scope).unwrap();
    assert!(nodes.is_empty());
}

/// `if="{items.length > 0}"` flows through the full evaluator and
/// sees the same synthesized length value.
#[test]
fn comparison_against_length_in_evaluator_works() {
    let scope = LowerScope::default().with_binding("rows", json!(["a", "b", "c"]));
    let nodes = interpret_with_scope(
        r#"<container>
            <text if="{rows.length > 2}">many</text>
            <text else>few</text>
           </container>"#,
        &scope,
    )
    .unwrap();
    let Node::Container { children, .. } = &nodes[0] else {
        panic!()
    };
    let Node::Text { content, .. } = &children[0] else {
        panic!()
    };
    assert_eq!(content, "many");
}

/// Strings also expose `.length` / `.first` / `.last`, matching
/// the surface promise that the same virtual segments work on
/// every container-shape.
#[test]
fn string_length_first_last_resolve_through_virtual_segments() {
    let scope = LowerScope::default().with_binding("word", json!("hello"));
    let nodes = interpret_with_scope(
        r#"<container>
            <text>{word.length}</text>
            <text>{word.first}</text>
            <text>{word.last}</text>
           </container>"#,
        &scope,
    )
    .unwrap();
    let Node::Container { children, .. } = &nodes[0] else {
        panic!()
    };
    let texts: Vec<String> = children
        .iter()
        .map(|c| match c {
            Node::Text { content, .. } => content.clone(),
            _ => String::new(),
        })
        .collect();
    assert_eq!(
        texts,
        vec!["5".to_string(), "h".to_string(), "o".to_string()]
    );
}

// ---------- on:event empty-string filter + modifier flattening ----------

/// `on:click=""` drops cleanly so a ternary that resolves to ""
/// omits the handler, matching the `data:`/`aria:` rule.
#[test]
fn on_event_empty_string_drops_the_attribute() {
    let nodes = interpret(r#"<container on:click=""/>"#).unwrap();
    let Node::Container { props, .. } = &nodes[0] else {
        panic!()
    };
    assert!(!props
        .semantic
        .attrs
        .iter()
        .any(|(k, _)| k == "data-on-click"));
}

/// `on:click.once.stop` flattens to `data-on-click-once-stop` —
/// the dotted suffix is the author surface; consumers read the
/// dash-joined wire form.
#[test]
fn on_event_modifier_dot_suffix_flattens_to_dash_in_data_attr() {
    let nodes = interpret(r#"<container on:click.once.stop="cmd save"/>"#).unwrap();
    let Node::Container { props, .. } = &nodes[0] else {
        panic!()
    };
    let attr = props
        .semantic
        .attrs
        .iter()
        .find(|(k, _)| k == "data-on-click-once-stop");
    assert!(attr.is_some(), "{:?}", props.semantic.attrs);
    assert_eq!(attr.unwrap().1, "cmd save");
}

/// `on:event_key` is the shared canonical-key helper — sanity
/// check it directly so a future refactor doesn't drift the wire
/// shape silently.
#[test]
fn on_event_attr_key_replaces_dots_with_dashes() {
    assert_eq!(on_event_attr_key("click"), "click");
    assert_eq!(on_event_attr_key("click.once"), "click-once");
    assert_eq!(on_event_attr_key("click.once.stop"), "click-once-stop");
}

// ---------- Wave B: pipe operator rewrite ----------

#[test]
fn pipe_rewrites_single_call() {
    assert_eq!(
        rewrite_pipes("tasks | take(5)").as_deref(),
        Some("take(tasks, 5)")
    );
}

#[test]
fn pipe_rewrites_bare_name() {
    assert_eq!(
        rewrite_pipes("xs | reverse").as_deref(),
        Some("reverse(xs)")
    );
}

#[test]
fn pipe_is_left_associative() {
    assert_eq!(
        rewrite_pipes("t | filter('s','open') | take(5)").as_deref(),
        Some("take(filter(t, 's','open'), 5)")
    );
}

#[test]
fn pipe_alias_arrow_form() {
    assert_eq!(
        rewrite_pipes("rows |> slice(0, 3)").as_deref(),
        Some("slice(rows, 0, 3)")
    );
}

#[test]
fn logical_or_is_not_a_pipe() {
    assert_eq!(rewrite_pipes("a || b"), None);
}

#[test]
fn closure_bars_are_not_pipes() {
    // A standalone closure literal has no top-level pipe.
    assert_eq!(rewrite_pipes("|t| t.priority == 'high'"), None);
    // Pipe whose RHS call carries a closure arg: only the
    // top-level `|` rewrites; the closure's bars stay intact.
    assert_eq!(
        rewrite_pipes("tasks | filter(|t| t.x == 'high')").as_deref(),
        Some("filter(tasks, |t| t.x == 'high')")
    );
}

#[test]
fn pipe_inside_parens_is_untouched() {
    // The only `|` is depth-1 (inside the call) → not a pipe.
    assert_eq!(rewrite_pipes("f(a | b)"), None);
}

// ---------- Wave B: closure builtins + pipelines (e2e) ----------

/// §7.2 headline: a `for=` source built from a pipe + closure
/// filter, with no `<script>` block (frame auto-provisioned).
#[cfg(feature = "luau")]
#[test]
fn for_source_pipe_closure_filter() {
    let src = r#"
<script>
  local tasks = prism.state {
{ title = "A", priority = "high" },
{ title = "B", priority = "low" },
{ title = "C", priority = "high" },
  }
</script>
<container for="t in tasks | filter(|t| t.priority == 'high')">
  <text>{t.title}</text>
</container>
"#;
    let nodes = interpret(src).unwrap();
    let titles: Vec<String> = nodes
        .iter()
        .filter_map(|n| match n {
            Node::Container { children, .. } => match children.first() {
                Some(Node::Text { content, .. }) => Some(content.clone()),
                _ => None,
            },
            _ => None,
        })
        .collect();
    assert_eq!(titles, vec!["A".to_string(), "C".to_string()]);
}

/// Multi-stage pipeline: filter (closure) → sort_by (closure,
/// desc via negation) → take. Mirrors the §7.2 example shape.
#[cfg(feature = "luau")]
#[test]
fn pipeline_filter_sort_take() {
    let src = r#"
<script>
  local tasks = prism.state {
{ title = "lo",  prio = 1, open = true },
{ title = "hi",  prio = 9, open = true },
{ title = "mid", prio = 5, open = true },
{ title = "done",prio = 7, open = false },
  }
</script>
<container for="t in tasks
| filter(|t| t.open)
| sort_by(\fn(t) return -t.prio end)
| take(2)">
  <text>{t.title}</text>
</container>
"#;
    let nodes = interpret(src).unwrap();
    let titles: Vec<String> = nodes
        .iter()
        .filter_map(|n| match n {
            Node::Container { children, .. } => match children.first() {
                Some(Node::Text { content, .. }) => Some(content.clone()),
                _ => None,
            },
            _ => None,
        })
        .collect();
    // open ones by prio desc: hi(9), mid(5), lo(1) → take 2.
    assert_eq!(titles, vec!["hi".to_string(), "mid".to_string()]);
}

/// Non-closure field-name builtin form is untouched by the
/// closure path (regression guard for the dispatch order).
#[cfg(feature = "luau")]
#[test]
fn field_name_builtin_still_works_alongside_closures() {
    let src = r#"
<script>
  local rows = prism.state {
{ k = "x", on = true }, { k = "y", on = false },
  }
</script>
<container for="r in filter(rows, 'on', true)">
  <text>{r.k}</text>
</container>
"#;
    let nodes = interpret(src).unwrap();
    let ks: Vec<String> = nodes
        .iter()
        .filter_map(|n| match n {
            Node::Container { children, .. } => match children.first() {
                Some(Node::Text { content, .. }) => Some(content.clone()),
                _ => None,
            },
            _ => None,
        })
        .collect();
    assert_eq!(ks, vec!["x".to_string()]);
}

/// Closure `map` inside a text interpolation, script-less doc
/// (frame auto-provisioned purely from the `|x|` sigil).
#[cfg(feature = "luau")]
#[test]
fn scriptless_closure_autoprovisions_frame() {
    // `range(1,4)` → [1,2,3]; map squares; join.
    let src = r#"<text>{join(map(range(1, 4), |n| n * n), ",")}</text>"#;
    let nodes = interpret(src).unwrap();
    let Node::Text { content, .. } = &nodes[0] else {
        panic!("expected text, got {:?}", nodes[0]);
    };
    assert_eq!(content, "1,4,9");
}

/// The §7.2 grouping example verbatim:
/// `entries(group_by(tasks, \fn(t) return t.assignee end))`,
/// then iterate with `{grp.key}`. Exercises closure-builtin
/// composition nested inside a non-closure builtin (`entries`).
#[cfg(feature = "luau")]
#[test]
fn entries_of_group_by_closure() {
    let src = r#"
<script>
  local tasks = prism.state {
{ title = "a", assignee = "ann" },
{ title = "b", assignee = "bo" },
{ title = "c", assignee = "ann" },
  }
</script>
<container for="grp in entries(group_by(tasks, \fn(t) return t.assignee end))">
  <text>{grp.key}</text>
</container>
"#;
    let nodes = interpret(src).unwrap();
    let mut keys: Vec<String> = nodes
        .iter()
        .filter_map(|n| match n {
            Node::Container { children, .. } => match children.first() {
                Some(Node::Text { content, .. }) => Some(content.clone()),
                _ => None,
            },
            _ => None,
        })
        .collect();
    keys.sort();
    assert_eq!(keys, vec!["ann".to_string(), "bo".to_string()]);
}

/// A closure closes over a harvested script `local`
/// (`threshold`) — strip-to-globals + same-Lua compile means the
/// closure body resolves it like any document binding.
#[cfg(feature = "luau")]
#[test]
fn closure_closes_over_script_local() {
    let src = r#"
<script>
  local threshold = "high"
  local tasks = prism.state {
{ title = "A", priority = "high" },
{ title = "B", priority = "low" },
  }
</script>
<container for="t in tasks | filter(|t| t.priority == threshold)">
  <text>{t.title}</text>
</container>
"#;
    let nodes = interpret(src).unwrap();
    let titles: Vec<String> = nodes
        .iter()
        .filter_map(|n| match n {
            Node::Container { children, .. } => match children.first() {
                Some(Node::Text { content, .. }) => Some(content.clone()),
                _ => None,
            },
            _ => None,
        })
        .collect();
    assert_eq!(titles, vec!["A".to_string()]);
}

// ---------- Wave C: prui[[…]] quasi-quote + macros (e2e) ----------

/// §7.7 headline: a `prism.macro` returning `prui[[…]]` with
/// `{attrs.title}` interpolation and `{children}` splice, used
/// as a custom `<empty-state>` tag with a child button.
#[cfg(feature = "luau")]
#[test]
fn macro_empty_state_with_attrs_and_children() {
    let src = r##"
<script>
  prism.macro("empty-state", function(attrs, children)
return prui [[
  <container direction="column" gap="8">
    <text>{attrs.title}</text>
    <text>{attrs.subtitle}</text>
    <fragment>{children}</fragment>
  </container>
]]
  end)
</script>
<empty-state title="No tasks" subtitle="Create one">
  <button>Create</button>
</empty-state>
"##;
    let nodes = interpret(src).unwrap();
    // The macro expanded to a single container.
    assert_eq!(nodes.len(), 1, "got {nodes:?}");
    let Node::Container { children, .. } = &nodes[0] else {
        panic!("expected container, got {:?}", nodes[0]);
    };
    let texts: Vec<String> = children
        .iter()
        .filter_map(|c| match c {
            Node::Text { content, .. } => Some(content.clone()),
            _ => None,
        })
        .collect();
    // `{attrs.title}` / `{attrs.subtitle}` resolved, and the
    // caller's `<button>Create</button>` spliced in via
    // `<fragment>{children}</fragment>` (button → text "Create").
    assert!(texts.contains(&"No tasks".to_string()), "{texts:?}");
    assert!(texts.contains(&"Create one".to_string()), "{texts:?}");
    assert!(
        texts.contains(&"Create".to_string()),
        "children not spliced: {texts:?}"
    );
}

/// Macro body resolves `{tokens.*}` (document context carried
/// into the hygienic macro scope) but NOT a caller PRUI binding
/// (`{leak}`) — Racket-style hygiene (§7.7).
#[cfg(feature = "luau")]
#[test]
fn macro_sees_tokens_but_not_caller_scope() {
    let src = r##"
<script>
  local rows = prism.state { "SECRET" }
  prism.macro("chip", function(attrs, children)
return prui [[
  <container style:background="{tokens.colors.accent}">
    <text>{attrs.label}</text>
    <text>{leak}</text>
  </container>
]]
  end)
</script>
<container for="leak in rows">
  <chip label="ok"/>
</container>
"##;
    let nodes = interpret(src).unwrap();
    // Find every text content in the tree.
    fn texts(n: &Node, out: &mut Vec<String>) {
        match n {
            Node::Text { content, .. } => out.push(content.clone()),
            Node::Container { children, .. } => {
                for c in children {
                    texts(c, out);
                }
            }
            _ => {}
        }
    }
    let mut all = Vec::new();
    for n in &nodes {
        texts(n, &mut all);
    }
    assert!(
        all.contains(&"ok".to_string()),
        "attrs.label resolved: {all:?}"
    );
    assert!(
        !all.contains(&"SECRET".to_string()),
        "caller binding leaked into macro body: {all:?}"
    );
}

/// A macro that emits another macro tag — nested expansion
/// works because the hygienic scope carries the Luau frame.
#[cfg(feature = "luau")]
#[test]
fn nested_macro_expansion() {
    let src = r##"
<script>
  prism.macro("inner", function(attrs, children)
return prui [[ <text>{attrs.v}</text> ]]
  end)
  prism.macro("outer", function(attrs, children)
return prui [[ <container><inner v="deep"/></container> ]]
  end)
</script>
<outer/>
"##;
    let nodes = interpret(src).unwrap();
    fn first_text(n: &Node) -> Option<String> {
        match n {
            Node::Text { content, .. } => Some(content.clone()),
            Node::Container { children, .. } => children.iter().find_map(first_text),
            _ => None,
        }
    }
    assert_eq!(
        nodes.iter().find_map(first_text),
        Some("deep".to_string()),
        "nested macro <inner> did not expand inside <outer>"
    );
}

/// A macro body's `class="…"` resolves the PRSS sheet that was
/// installed at the call site — the hygienic scope re-threads
/// the sheet `Arc` (Wave C gap closure).
#[cfg(feature = "luau")]
#[test]
fn macro_body_class_resolves_caller_stylesheet() {
    let (sheet, _) = prism_core::language::prss::parse(
        r##"
        [class.card]
        background = "#0060c0"
        "##,
    );
    let scope = LowerScope::default().with_stylesheet(Arc::new(sheet));
    let src = r##"
<script>
  prism.macro("boxed", function(attrs, children)
return prui [[ <container id="m" class="card"/> ]]
  end)
</script>
<boxed/>
"##;
    let nodes = interpret_with_scope(src, &scope).unwrap();
    let m = find_container_by_id(&nodes, "m").expect("macro container");
    let Node::Container { props, .. } = m else {
        panic!()
    };
    let bg = props.background.expect("PRSS class applied in macro body");
    assert_eq!((bg.r, bg.g, bg.b), (0x00, 0x60, 0xc0));
}

// ---------- Wave D: <match> / <suspense> ----------

fn text_contents(nodes: &[Node]) -> Vec<String> {
    fn walk(n: &Node, out: &mut Vec<String>) {
        match n {
            Node::Text { content, .. } => out.push(content.clone()),
            Node::Container { children, .. } => {
                for c in children {
                    walk(c, out);
                }
            }
            _ => {}
        }
    }
    let mut v = Vec::new();
    for n in nodes {
        walk(n, &mut v);
    }
    v
}

#[test]
fn match_selects_literal_case() {
    let src = r#"
<let name="kind" value="hover"/>
<match on="{kind}">
  <case is="click"><text>was click</text></case>
  <case is="hover"><text>was hover</text></case>
  <case default><text>unknown</text></case>
</match>
"#;
    let nodes = interpret(src).unwrap();
    assert_eq!(text_contents(&nodes), vec!["was hover".to_string()]);
}

#[test]
fn match_default_when_no_case_matches() {
    let src = r#"
<let name="kind" value="scroll"/>
<match on="{kind}">
  <case is="click"><text>c</text></case>
  <case default><text>fallthrough</text></case>
</match>
"#;
    let nodes = interpret(src).unwrap();
    assert_eq!(text_contents(&nodes), vec!["fallthrough".to_string()]);
}

#[test]
fn match_case_if_clause_narrows() {
    // `is="key"` matches but the `if=` clause fails → falls to
    // default.
    let src = r#"
<let name="kind" value="key"/>
<let name="key" value="Enter"/>
<match on="{kind}">
  <case is="key" if="{key == 'Escape'}"><text>esc</text></case>
  <case default><text>other key</text></case>
</match>
"#;
    let nodes = interpret(src).unwrap();
    assert_eq!(text_contents(&nodes), vec!["other key".to_string()]);
}

#[test]
fn match_first_match_wins_default_terminates() {
    // A second matching case after an earlier match never fires;
    // content after `<case default>` is unreachable.
    let src = r#"
<let name="k" value="a"/>
<match on="{k}">
  <case is="a"><text>first</text></case>
  <case is="a"><text>second</text></case>
  <case default><text>def</text></case>
</match>
"#;
    let nodes = interpret(src).unwrap();
    assert_eq!(text_contents(&nodes), vec!["first".to_string()]);
}

#[test]
fn suspense_shows_primary_when_not_pending() {
    let src = r#"
<let name="status" value="ready"/>
<suspense>
  <fallback><text>loading</text></fallback>
  <container><text>{status}</text></container>
</suspense>
"#;
    let nodes = interpret(src).unwrap();
    let texts = text_contents(&nodes);
    assert_eq!(texts, vec!["ready".to_string()], "primary should render");
}

#[cfg(feature = "luau")]
#[test]
fn suspense_shows_fallback_on_pending_marker() {
    // A script binding shaped like an unresolved async query.
    let src = r##"
<script>
  local async_tasks = prism.state { tag = "Pending" }
</script>
<suspense>
  <fallback><text>loading…</text></fallback>
  <container><text>{async_tasks.tag}</text></container>
</suspense>
"##;
    let nodes = interpret(src).unwrap();
    assert_eq!(text_contents(&nodes), vec!["loading…".to_string()]);
}

/// §4.2 — `prism.objects:query_async` returns the `{ tag =
/// "Pending" }` marker synchronously, so a `<suspense>` reading
/// the query renders the `<fallback>` on the first frame (before
/// the scheduler resolves it). End-to-end proof the new seam is
/// wired through `subtree_has_pending` with no extra glue — the
/// fallback→primary swap on resolve is covered by the
/// `luau_scope::query_async_*` reactive tests.
#[cfg(feature = "luau")]
#[test]
fn suspense_shows_fallback_for_unresolved_query_async() {
    let src = r##"
<script>
  local data = prism.objects:query_async(function()
return { rows = 3 }
  end)
</script>
<suspense>
  <fallback><text>loading…</text></fallback>
  <container><text>{data.rows}</text></container>
</suspense>
"##;
    let nodes = interpret(src).unwrap();
    assert_eq!(text_contents(&nodes), vec!["loading…".to_string()]);
}

// ---------- Wave E: sub-dialects + sigils ----------

/// §7.8 headline: `prism.dialect{name,parse}` + a
/// `<language name="upper">` block whose body the dialect
/// transforms into `prui[[…]]` source.
#[cfg(feature = "luau")]
#[test]
fn language_block_dispatches_to_dialect() {
    let src = r##"
<script>
  prism.dialect {
name = "upper",
parse = function(source)
  return "<text>" .. string.upper(source) .. "</text>"
end,
  }
</script>
<language name="upper">hello world</language>
"##;
    let nodes = interpret(src).unwrap();
    assert_eq!(text_contents(&nodes), vec!["HELLO WORLD".to_string()]);
}

/// The `~name{ … }` sigil is sugar for the same dialect path.
#[cfg(feature = "luau")]
#[test]
fn dialect_sigil_inline() {
    let src = r##"
<script>
  prism.dialect {
name = "shout",
parse = function(s) return "<text>" .. s .. "!!!</text>" end,
  }
</script>
<container>~shout{go}</container>
"##;
    let nodes = interpret(src).unwrap();
    assert_eq!(text_contents(&nodes), vec!["go!!!".to_string()]);
}

/// A small markdown-ish dialect (demonstrates the §7.8 / E.3
/// "every dialect is a Luau file" path): bullet lines → `<text>`
/// rows. Proves the dialect can emit a multi-node tree.
#[cfg(feature = "luau")]
#[test]
fn markdown_style_dialect_emits_node_tree() {
    let src = r##"
<script>
  prism.dialect {
name = "md",
parse = function(source)
  local out = "<container direction=\"column\">"
  for line in (source .. "\n"):gmatch("([^\n]*)\n") do
    local t = line:match("^%s*(.-)%s*$")
    if t ~= "" then
      out = out .. "<text>" .. t .. "</text>"
    end
  end
  return out .. "</container>"
end,
  }
</script>
<language name="md">
- first
- second
</language>
"##;
    let nodes = interpret(src).unwrap();
    let texts = text_contents(&nodes);
    assert!(texts.contains(&"- first".to_string()), "{texts:?}");
    assert!(texts.contains(&"- second".to_string()), "{texts:?}");
}

/// **Wave E.3 (§7.8)** — a document with *no* `<script>` block
/// still resolves `<language>` when the host installs a builtin
/// dialect via `LowerScope::with_builtin_scripts`. Also exercises
/// the enriched `prui_ast.column` / `prui_ast.text` constructors.
#[cfg(feature = "luau")]
#[test]
fn builtin_dialect_scripts_resolve_without_a_document_script() {
    let builtin = r#"
prism.dialect {
  name = "md",
  parse = function(source)
local kids = {}
for line in (tostring(source) .. "\n"):gmatch("([^\n]*)\n") do
  local t = line:gsub("^%s+", ""):gsub("%s+$", "")
  if t ~= "" then kids[#kids + 1] = prui_ast.text(t) end
end
return prui_ast.column { gap = 8, children = kids }
  end,
}
"#;
    let scope = LowerScope::default().with_builtin_scripts(vec![builtin.to_string()]);
    // No `<script>` anywhere — only the host-supplied builtin.
    let src = "<language name=\"md\">\nalpha\nbeta\n</language>";
    let nodes = interpret_with_scope(src, &scope).unwrap();
    let texts = text_contents(&nodes);
    assert!(texts.contains(&"alpha".to_string()), "{texts:?}");
    assert!(texts.contains(&"beta".to_string()), "{texts:?}");

    // A plain document (no `<language>`) must NOT pay for a Lua
    // state just because builtins are installed.
    let plain = interpret_with_scope("<container><text>x</text></container>", &scope).unwrap();
    assert_eq!(text_contents(&plain), vec!["x".to_string()]);
}

/// An unregistered dialect renders nothing (graceful) rather
/// than leaking its raw body.
#[cfg(feature = "luau")]
#[test]
fn unknown_dialect_renders_nothing() {
    let src = r#"<container>~nope{secret body}</container>"#;
    let nodes = interpret(src).unwrap();
    assert!(
        !text_contents(&nodes).iter().any(|t| t.contains("secret")),
        "raw dialect body leaked"
    );
}

/// `prui_ast.*` builds the same source string `prui[[…]]`
/// would, so a dialect can emit a tree programmatically.
#[cfg(feature = "luau")]
#[test]
fn prui_ast_constructors_build_tree() {
    let src = r##"
<script>
  prism.dialect {
name = "card",
parse = function(s)
  return prui_ast.container({
    direction = "column",
    children = {
      prui_ast.heading(s, { level = 2 }),
      prui_ast.text("body"),
    },
  })
end,
  }
</script>
<language name="card">Title</language>
"##;
    let nodes = interpret(src).unwrap();
    let texts = text_contents(&nodes);
    assert!(texts.contains(&"Title".to_string()), "{texts:?}");
    assert!(texts.contains(&"body".to_string()), "{texts:?}");
}

// ---------- Wave F: computed PRSS { lua = "…" } ----------

#[test]
fn prss_lua_value_resolves_token() {
    let (sheet, errs) = prism_core::language::prss::parse(
        r##"
        [class.card]
        background = { lua = "tokens.colors.accent" }
        "##,
    );
    assert!(errs.is_empty(), "{errs:?}");
    let scope = LowerScope::default()
        .with_design_tokens(&prism_core::design_tokens::DEFAULT_TOKENS)
        .with_stylesheet(Arc::new(sheet));
    let nodes = interpret_with_scope(r#"<container id="c" class="card"/>"#, &scope).unwrap();
    let Node::Container { props, .. } = find_container_by_id(&nodes, "c").unwrap() else {
        panic!()
    };
    let accent = prism_core::design_tokens::DEFAULT_TOKENS.colors.accent;
    let bg = props.background.expect("computed background");
    assert_eq!((bg.r, bg.g, bg.b), (accent.r, accent.g, accent.b));
}

#[test]
fn prss_lua_darken_helper() {
    let (sheet, errs) = prism_core::language::prss::parse(
        r##"
        [class.btn]
        background = { lua = "darken('#808080', 0.5)" }
        "##,
    );
    assert!(errs.is_empty(), "{errs:?}");
    let scope = LowerScope::default().with_stylesheet(Arc::new(sheet));
    let nodes = interpret_with_scope(r#"<container id="b" class="btn"/>"#, &scope).unwrap();
    let Node::Container { props, .. } = find_container_by_id(&nodes, "b").unwrap() else {
        panic!()
    };
    // 0x80 * (1 - 0.5) = 64 = 0x40.
    let bg = props.background.expect("darkened background");
    assert_eq!((bg.r, bg.g, bg.b), (0x40, 0x40, 0x40));
}

#[test]
fn prss_lua_computed_state_override() {
    let (sheet, errs) = prism_core::language::prss::parse(
        r##"
        [class.btn]
        background = "#000000"
        [class.btn.hovered]
        background = { lua = "lighten('#000000', 1.0)" }
        "##,
    );
    assert!(errs.is_empty(), "{errs:?}");
    let scope = LowerScope::default().with_stylesheet(Arc::new(sheet));
    let nodes = interpret_with_scope(r#"<container id="b" class="btn"/>"#, &scope).unwrap();
    let Node::Container { props, .. } = find_container_by_id(&nodes, "b").unwrap() else {
        panic!()
    };
    // base black; hovered → lighten(black,1.0) = white.
    let hov = props.hover.as_ref().expect("hover override");
    let bg = hov.background.expect("hovered background");
    assert_eq!((bg.r, bg.g, bg.b), (0xff, 0xff, 0xff));
}

#[cfg(feature = "luau")]
#[test]
fn prss_lua_value_calls_script_helper() {
    // A `<script>`-defined helper is reachable from a computed
    // PRSS value (the §7.9 "PRSS → Luau" edge).
    let (sheet, _) = prism_core::language::prss::parse(
        r##"
        [class.tag]
        background = { lua = "brand()" }
        "##,
    );
    let scope = LowerScope::default().with_stylesheet(Arc::new(sheet));
    let src = r##"
<script>
  local function brand() return "#123456" end
</script>
<container id="t" class="tag"/>
"##;
    let nodes = interpret_with_scope(src, &scope).unwrap();
    let Node::Container { props, .. } = find_container_by_id(&nodes, "t").unwrap() else {
        panic!()
    };
    let bg = props.background.expect("script-computed background");
    assert_eq!((bg.r, bg.g, bg.b), (0x12, 0x34, 0x56));
}

// ---------- Wave H: inline <style> + <import> ----------

#[test]
fn inline_style_block_applies_classes() {
    let src = r##"
<style>
[class.card]
background = "#0060c0"
</style>
<container id="c" class="card"/>
"##;
    let nodes = interpret(src).unwrap();
    let Node::Container { props, .. } = find_container_by_id(&nodes, "c").unwrap() else {
        panic!()
    };
    let bg = props.background.expect("inline-style class applied");
    assert_eq!((bg.r, bg.g, bg.b), (0x00, 0x60, 0xc0));
}

#[test]
fn multiple_style_blocks_layer_later_wins() {
    let src = r##"
<style>
[class.x]
background = "#111111"
</style>
<style>
[class.x]
background = "#222222"
</style>
<container id="c" class="x"/>
"##;
    let nodes = interpret(src).unwrap();
    let Node::Container { props, .. } = find_container_by_id(&nodes, "c").unwrap() else {
        panic!()
    };
    let bg = props.background.unwrap();
    assert_eq!((bg.r, bg.g, bg.b), (0x22, 0x22, 0x22));
}

#[cfg(feature = "luau")]
#[test]
fn inline_style_computed_value_uses_frame() {
    // Inline <style> with a Wave-F `{lua=…}` value referencing a
    // <script> helper — proves the style merge runs before the
    // Luau frame is built.
    let src = r##"
<style>
[class.tag]
background = { lua = "brand()" }
</style>
<script>
  local function brand() return "#abcdef" end
</script>
<container id="t" class="tag"/>
"##;
    let nodes = interpret(src).unwrap();
    let Node::Container { props, .. } = find_container_by_id(&nodes, "t").unwrap() else {
        panic!()
    };
    let bg = props.background.expect("computed inline-style value");
    assert_eq!((bg.r, bg.g, bg.b), (0xab, 0xcd, 0xef));
}

#[test]
fn import_stylesheet_via_resolver() {
    struct Res;
    impl ImportResolver for Res {
        fn resolve_import(&self, kind: &str, path: &str) -> Option<String> {
            if kind == "stylesheet" && path == "./theme.prss" {
                Some("[class.themed]\nbackground = \"#00ff00\"".to_string())
            } else {
                None
            }
        }
    }
    let scope = LowerScope::default().with_import_resolver(Arc::new(Res));
    let src = r#"
<import stylesheet="./theme.prss"/>
<container id="c" class="themed"/>
"#;
    let nodes = interpret_with_scope(src, &scope).unwrap();
    let Node::Container { props, .. } = find_container_by_id(&nodes, "c").unwrap() else {
        panic!()
    };
    let bg = props.background.expect("imported sheet applied");
    assert_eq!((bg.r, bg.g, bg.b), (0x00, 0xff, 0x00));
}

#[cfg(feature = "luau")]
#[test]
fn import_script_feeds_luau_frame() {
    struct Res;
    impl ImportResolver for Res {
        fn resolve_import(&self, kind: &str, path: &str) -> Option<String> {
            (kind == "script" && path == "prism://lib/fmt.luau")
                .then(|| "local function shout(s) return s .. '!' end".to_string())
        }
    }
    let scope = LowerScope::default().with_import_resolver(Arc::new(Res));
    let src = r#"
<import script="prism://lib/fmt.luau"/>
<text>{shout('hi')}</text>
"#;
    let nodes = interpret_with_scope(src, &scope).unwrap();
    assert_eq!(text_contents(&nodes), vec!["hi!".to_string()]);
}

#[cfg(feature = "luau")]
#[test]
fn import_script_as_namespaced_module() {
    // §5.9 tier 2: two modules each define a private `currency`;
    // namespacing keeps them isolated and addressable as
    // `fmt.label` / `dates.label` with zero collision. A data
    // field on the returned table resolves through `{ns.value}`.
    struct Res;
    impl ImportResolver for Res {
        fn resolve_import(&self, kind: &str, path: &str) -> Option<String> {
            match (kind, path) {
                ("script", "./fmt.luau") => Some(
                    "local function currency(n) return ('$' .. tostring(n)) end\n\
                     return { label = currency, kind = 'money' }"
                        .to_string(),
                ),
                ("script", "./dates.luau") => Some(
                    "local function currency(n) return (tostring(n) .. 'd') end\n\
                     return { label = currency }"
                        .to_string(),
                ),
                _ => None,
            }
        }
    }
    let scope = LowerScope::default().with_import_resolver(Arc::new(Res));
    let src = r#"
<import script="./fmt.luau" as="fmt"/>
<import script="./dates.luau" as="dates"/>
<text>{fmt.label(5)}</text>
<text>{dates.label(5)}</text>
<text>{fmt.kind}</text>
"#;
    let nodes = interpret_with_scope(src, &scope).unwrap();
    assert_eq!(
        text_contents(&nodes),
        vec!["$5".to_string(), "5d".to_string(), "money".to_string()]
    );
}

#[cfg(feature = "luau")]
#[test]
fn unnamed_script_import_still_flat_merges() {
    // No `as=` → legacy flat-merge: the helper lands as a bare
    // document-scope binding (back-compat with Wave H.3).
    struct Res;
    impl ImportResolver for Res {
        fn resolve_import(&self, kind: &str, path: &str) -> Option<String> {
            (kind == "script" && path == "./h.luau")
                .then(|| "local function dbl(n) return n * 2 end".to_string())
        }
    }
    let scope = LowerScope::default().with_import_resolver(Arc::new(Res));
    let src = r#"
<import script="./h.luau"/>
<text>{dbl(21)}</text>
"#;
    let nodes = interpret_with_scope(src, &scope).unwrap();
    assert_eq!(text_contents(&nodes), vec!["42".to_string()]);
}

#[cfg(feature = "luau")]
#[test]
fn tier3_require_transitive_through_resolver() {
    // §5.9 tier 3: an imported module `require`s a sibling; the
    // host resolves the transitive graph through the *same*
    // `ImportResolver` as `<import>`, deps-first, and the nested
    // `require("./fmt.luau")` is a cache hit.
    struct Res;
    impl ImportResolver for Res {
        fn resolve_import(&self, kind: &str, path: &str) -> Option<String> {
            match (kind, path) {
                ("script", "./money.luau") => Some(
                    "local fmt = require(\"./fmt.luau\")\n\
                     local function total(a, b) return fmt.sum(a, b) end"
                        .to_string(),
                ),
                ("script", "./fmt.luau") => {
                    Some("return { sum = function(a, b) return a + b end }".to_string())
                }
                _ => None,
            }
        }
    }
    let scope = LowerScope::default().with_import_resolver(Arc::new(Res));
    let src = "<import script=\"./money.luau\"/>\n<text>{total(2, 3)}</text>";
    let nodes = interpret_with_scope(src, &scope).unwrap();
    assert_eq!(text_contents(&nodes), vec!["5".to_string()]);
}

// ---------- Wave G: probe: + at: ----------

#[test]
fn probe_namespace_lowers_to_data_attr() {
    let nodes = interpret(r#"<container probe:render="card-shown" at:0="{opacity: 0}"/>"#).unwrap();
    let Node::Container { props, .. } = &nodes[0] else {
        panic!()
    };
    let attrs = &props.semantic.attrs;
    assert!(
        attrs
            .iter()
            .any(|(k, v)| k == "data-probe-render" && v == "card-shown"),
        "{attrs:?}"
    );
    assert!(
        attrs.iter().any(|(k, _)| k == "data-at-0"),
        "at: keyframe attr missing: {attrs:?}"
    );
}

#[cfg(feature = "luau")]
#[test]
fn prism_probes_on_subscribes_and_fires() {
    // Register a probe handler that records into a script local;
    // fire it from Rust (the host event-router seam) and read
    // the local back through the binding surface.
    let src = r##"
<script>
  local hits = prism.state { count = 0, last = "" }
  prism.probes:on("clicked", function(ev)
hits.count = hits.count + 1
hits.last = ev.id
  end)
</script>
<container probe:click="clicked"/>
"##;
    let (doc, errs) = prism_core::language::prism_ui::parse(src);
    assert!(errs.is_empty(), "{errs:?}");
    // Build the frame the way the loader does.
    let bodies: Vec<String> = doc
        .nodes
        .iter()
        .filter_map(|n| match n {
            prism_core::language::prism_ui::Node::Element(e) if e.tag == "script" => {
                e.children.iter().find_map(|c| match c {
                    prism_core::language::prism_ui::Node::Text { value, .. } => Some(value.clone()),
                    _ => None,
                })
            }
            _ => None,
        })
        .collect();
    let srcs: Vec<&str> = bodies.iter().map(String::as_str).collect();
    let frame = crate::luau_scope::LuauScopeFrame::from_scripts(&srcs, None).expect("frame");
    assert!(frame.has_probe("clicked"));
    frame
        .fire_probe("clicked", &serde_json::json!({ "id": "btn-1" }))
        .expect("subscribed")
        .expect("handler ok");
    // The handler mutated the live `hits` state table (read
    // live, not from the frozen load-time snapshot).
    assert_eq!(frame.read_global("hits.count"), Some(serde_json::json!(1)));
    assert_eq!(
        frame.read_global("hits.last"),
        Some(serde_json::json!("btn-1"))
    );
}

// ---------- Wave I: prism.scope host-binding bridge ----------

#[cfg(feature = "luau")]
#[test]
fn prism_scope_reads_host_binding() {
    // The host (resolver) provides `task`; a <script> reads it
    // via `prism.scope.task` and a helper projects a field that
    // an {expr} slot then renders.
    let scope = LowerScope::default().with_binding(
        "task",
        serde_json::json!({ "title": "Ship it", "priority": "high" }),
    );
    let src = r##"
<script>
  local task = prism.scope.task
  local function label() return task.title .. " (" .. task.priority .. ")" end
</script>
<text>{label()}</text>
"##;
    let nodes = interpret_with_scope(src, &scope).unwrap();
    assert_eq!(text_contents(&nodes), vec!["Ship it (high)".to_string()]);
}

#[cfg(feature = "luau")]
#[test]
fn prism_scope_absent_binding_is_nil() {
    // Reading an unprovided host binding is nil-safe (no panic,
    // no leak) — the script guards with `or`.
    let src = r##"
<script>
  local who = prism.scope.user or "anon"
</script>
<text>{who}</text>
"##;
    let nodes = interpret(src).unwrap();
    assert_eq!(text_contents(&nodes), vec!["anon".to_string()]);
}

// ---------- Wave A: <script> colocated module ----------

/// End-to-end §7.1: a `<script>` block's top-level `local`s
/// (helper fn + state table) resolve in `{expr}` slots, and the
/// `<script>` element itself renders nothing.
#[cfg(feature = "luau")]
#[test]
fn script_block_locals_resolve_in_expression_slots() {
    let src = r##"
<script>
  local function priority_color(p)
if p == "high" then return "#ff0000" end
return "#888888"
  end
  local state = prism.state { label = "Ready" }
</script>
<container>
  <text style:color="{priority_color('high')}">{state.label}</text>
</container>
"##;
    let nodes = interpret(src).unwrap();
    // Only the <container> lowers — the <script> is inert.
    assert_eq!(nodes.len(), 1, "script must not render a node");
    let Node::Container { children, .. } = &nodes[0] else {
        panic!("expected container, got {:?}", nodes[0]);
    };
    let Node::Text { content, props, .. } = &children[0] else {
        panic!("expected text child");
    };
    assert_eq!(content, "Ready", "state.label binding resolved");
    assert_eq!(
        props.color,
        Color {
            r: 0xff,
            g: 0,
            b: 0,
            a: 0xff
        },
        "priority_color('high') resolved through the call seam"
    );
}

/// Without the script block the same slots resolve to nothing —
/// guards that the Luau path is additive, not load-bearing for
/// plain documents.
#[cfg(feature = "luau")]
#[test]
fn document_without_script_is_unaffected() {
    let nodes = interpret(r#"<container><text>Hi</text></container>"#).unwrap();
    let Node::Container { children, .. } = &nodes[0] else {
        panic!()
    };
    let Node::Text { content, .. } = &children[0] else {
        panic!()
    };
    assert_eq!(content, "Hi");
}
