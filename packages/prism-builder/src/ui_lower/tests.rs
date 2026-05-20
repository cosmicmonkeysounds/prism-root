use super::*;

#[test]
fn lower_as_resolves_through_registry_when_attached() {
    use crate::block::{register_block, Block};
    use crate::registry::{ComponentRegistry, FieldSpec};
    use crate::ComponentId;
    use prism_ui_runtime::layout::{ContainerProps, Sizing};
    use std::sync::Arc;

    struct Tag {
        id: ComponentId,
    }
    impl Block for Tag {
        fn id(&self) -> &ComponentId {
            &self.id
        }
        fn schema(&self) -> Vec<FieldSpec> {
            vec![]
        }
        fn lower_ui(&self, _: &LowerCtx<'_>, node: &Node, _: &StyleProperties) -> UiNode {
            UiNode::Container {
                id: node.id.clone(),
                props: ContainerProps {
                    width: Sizing::Fixed(99.0),
                    ..Default::default()
                },
                children: vec![],
            }
        }
    }

    let mut reg = ComponentRegistry::new();
    register_block(
        &mut reg,
        Arc::new(Tag {
            id: "demo.tag".into(),
        }),
    )
    .unwrap();
    let cascade = StyleProperties::default();
    let ctx = LowerCtx::new(Some(&reg), &cascade);

    let out = ctx
        .lower_as("demo.tag", "derived", serde_json::json!({}))
        .expect("registered tag resolves");
    let UiNode::Container { id, props, .. } = out else {
        panic!()
    };
    assert_eq!(id, "derived");
    assert_eq!(props.width, Sizing::Fixed(99.0));
}

#[test]
fn lower_as_returns_none_when_no_registry() {
    let cascade = StyleProperties::default();
    let ctx = LowerCtx::new(None, &cascade);
    assert!(ctx
        .lower_as("anything", "x", serde_json::json!({}))
        .is_none());
}

#[test]
fn lower_as_returns_none_when_id_unregistered() {
    use crate::registry::ComponentRegistry;
    let reg = ComponentRegistry::new();
    let cascade = StyleProperties::default();
    let ctx = LowerCtx::new(Some(&reg), &cascade);
    assert!(ctx
        .lower_as("never.registered", "x", serde_json::json!({}))
        .is_none());
}

#[test]
fn block_invalidator_subscribes_signal_reads_inside_lower_ui() {
    // Phase 3b: a block's `lower_ui` body that reads a reactive
    // Signal auto-subscribes a per-NodeId reactive context;
    // a later signal write fires the invalidator's on_dirty
    // callback with that NodeId. The signal lives in a thread-
    // local so the Block struct stays Send+Sync as required by
    // the Block trait.
    use crate::block::{register_block, Block};
    use crate::registry::{ComponentRegistry, FieldSpec};
    use crate::ComponentId;
    use prism_core::reactive::{Owner, Signal};
    use prism_ui_runtime::layout::{ContainerProps, Sizing};
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;
    use std::sync::Arc;

    thread_local! {
        static SIG: Cell<Option<Signal<i32>>> = const { Cell::new(None) };
    }

    struct Reader {
        id: ComponentId,
    }
    impl Block for Reader {
        fn id(&self) -> &ComponentId {
            &self.id
        }
        fn schema(&self) -> Vec<FieldSpec> {
            vec![]
        }
        fn lower_ui(&self, _: &LowerCtx<'_>, node: &Node, _: &StyleProperties) -> UiNode {
            // Read the thread-local signal inside the lower body.
            SIG.with(|s| {
                if let Some(sig) = s.get() {
                    let _ = sig.get();
                }
            });
            UiNode::Container {
                id: node.id.clone(),
                props: ContainerProps {
                    width: Sizing::Fixed(1.0),
                    ..Default::default()
                },
                children: vec![],
            }
        }
    }

    let outer = Owner::new();
    let sig = outer.insert(0_i32);
    SIG.with(|s| s.set(Some(sig)));

    let mut reg = ComponentRegistry::new();
    register_block(
        &mut reg,
        Arc::new(Reader {
            id: "test.reader".into(),
        }),
    )
    .unwrap();

    let dirty: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
    let dirty_cb = Rc::clone(&dirty);
    let invalidator =
        BlockInvalidator::new(move |id: &str| dirty_cb.borrow_mut().push(id.to_string()));

    let cascade = StyleProperties::default();
    let ctx = LowerCtx::new(Some(&reg), &cascade).with_block_invalidator(invalidator.clone());
    let node = Node {
        id: "node-A".into(),
        component: "test.reader".into(),
        ..Default::default()
    };
    let _ = ctx.lower(&node);
    assert!(dirty.borrow().is_empty(), "initial subscribe did not fire");

    sig.set(1);
    assert_eq!(
        dirty.borrow().as_slice(),
        &["node-A".to_string()],
        "signal write fired the invalidator with the block's NodeId",
    );
    SIG.with(|s| s.set(None));
}

#[test]
fn block_invalidator_reuses_per_node_contexts_across_lowers() {
    // Phase 3b: lowering the same node twice must reuse the
    // cached per-NodeId reactive context (via reset_and_run_in
    // semantics) rather than allocate a fresh one each frame.
    use crate::block::{register_block, Block};
    use crate::registry::{ComponentRegistry, FieldSpec};
    use crate::ComponentId;
    use prism_ui_runtime::layout::ContainerProps;
    use std::sync::Arc;

    struct Pass {
        id: ComponentId,
    }
    impl Block for Pass {
        fn id(&self) -> &ComponentId {
            &self.id
        }
        fn schema(&self) -> Vec<FieldSpec> {
            vec![]
        }
        fn lower_ui(&self, _: &LowerCtx<'_>, node: &Node, _: &StyleProperties) -> UiNode {
            UiNode::Container {
                id: node.id.clone(),
                props: ContainerProps::default(),
                children: vec![],
            }
        }
    }

    let mut reg = ComponentRegistry::new();
    register_block(&mut reg, Arc::new(Pass { id: "p".into() })).unwrap();

    let invalidator = BlockInvalidator::new(|_id: &str| {});

    let cascade = StyleProperties::default();
    let node = Node {
        id: "stable-id".into(),
        component: "p".into(),
        ..Default::default()
    };

    for _ in 0..3 {
        let ctx = LowerCtx::new(Some(&reg), &cascade).with_block_invalidator(invalidator.clone());
        let _ = ctx.lower(&node);
    }
    // One context per distinct NodeId, regardless of how many
    // times we re-lowered.
    assert_eq!(invalidator.cached_len(), 1);
}

#[test]
fn block_invalidator_forget_node_disposes_context() {
    let invalidator = BlockInvalidator::new(|_id: &str| {});
    // Force a context to materialise via run_for_node.
    invalidator.run_for_node("ephemeral", || ());
    assert_eq!(invalidator.cached_len(), 1);
    invalidator.forget_node("ephemeral");
    assert_eq!(invalidator.cached_len(), 0);
}

#[test]
fn lower_without_invalidator_is_pass_through() {
    // Headless test path: no invalidator installed → blocks lower
    // exactly as before, no reactive wrapping.
    use crate::block::{register_block, Block};
    use crate::registry::{ComponentRegistry, FieldSpec};
    use crate::ComponentId;
    use prism_ui_runtime::layout::ContainerProps;
    use std::sync::Arc;

    struct Pass {
        id: ComponentId,
    }
    impl Block for Pass {
        fn id(&self) -> &ComponentId {
            &self.id
        }
        fn schema(&self) -> Vec<FieldSpec> {
            vec![]
        }
        fn lower_ui(&self, _: &LowerCtx<'_>, node: &Node, _: &StyleProperties) -> UiNode {
            UiNode::Container {
                id: node.id.clone(),
                props: ContainerProps::default(),
                children: vec![],
            }
        }
    }
    let mut reg = ComponentRegistry::new();
    register_block(&mut reg, Arc::new(Pass { id: "p".into() })).unwrap();
    let cascade = StyleProperties::default();
    let ctx = LowerCtx::new(Some(&reg), &cascade);
    let n = Node {
        id: "n1".into(),
        component: "p".into(),
        ..Default::default()
    };
    let out = ctx.lower(&n);
    let UiNode::Container { id, .. } = out else {
        panic!()
    };
    assert_eq!(id, "n1");
}

#[test]
fn hover_bg_returns_some_for_valid_color() {
    let h = hover_bg("#1f000000").expect("valid color");
    assert!(h.background.is_some());
    assert!(h.radius.is_none());
}

#[test]
fn hover_bg_returns_none_for_invalid_color() {
    assert!(hover_bg("not-a-color").is_none());
}

#[test]
fn prop_helpers_extract_with_sensible_defaults() {
    use crate::layout::LayoutMode;
    use prism_core::foundation::spatial::Transform2D;
    use serde_json::json;
    let node = Node {
        id: "n".into(),
        component: "x".into(),
        props: json!({ "label": "Hi", "selected": true }),
        children: vec![],
        layout_mode: LayoutMode::default(),
        transform: Transform2D::default(),
        modifiers: vec![],
        style: StyleProperties::default(),
    };
    let style = StyleProperties::default();
    let ctx = LowerCtx::new(None, &style);
    assert_eq!(ctx.prop_str(&node, "label"), "Hi");
    assert_eq!(ctx.prop_str(&node, "missing"), "");
    assert!(ctx.prop_bool(&node, "selected", false));
    assert!(!ctx.prop_bool(&node, "missing", false));
    assert!(ctx.prop_bool(&node, "missing", true));
}

#[test]
fn prop_helpers_subscribe_through_bindings() {
    // Phase 4b: when a `DocumentBindings` is wired and we read a
    // prop through `ctx.prop_str` inside a reactive context, the
    // context subscribes; a later write through `NodeMutator`
    // fires the dirty callback. This is the contract that makes
    // `lower_ui` bodies reactive without per-block plumbing.
    use crate::layout::LayoutMode;
    use crate::mutator::NodeMutator;
    use crate::reactive_props::DocumentBindings;
    use prism_core::foundation::spatial::Transform2D;
    use prism_core::reactive::ReactiveContext;
    use serde_json::json;
    use std::cell::Cell;
    let mut node = Node {
        id: "n".into(),
        component: "x".into(),
        props: json!({ "label": "hello" }),
        children: vec![],
        layout_mode: LayoutMode::default(),
        transform: Transform2D::default(),
        modifiers: vec![],
        style: StyleProperties::default(),
    };
    let bindings = DocumentBindings::new();
    let style = StyleProperties::default();
    let ctx = LowerCtx::new(None, &style).with_bindings(&bindings);

    let dirty = Rc::new(Cell::new(0_usize));
    let dirty_for_ctx = Rc::clone(&dirty);
    let rcx = ReactiveContext::new(move || {
        dirty_for_ctx.set(dirty_for_ctx.get() + 1);
    });
    let read = rcx.reset_and_run_in(|| ctx.prop_str(&node, "label"));
    assert_eq!(read, "hello");
    assert_eq!(dirty.get(), 0, "subscribe alone doesn't fire dirty");

    NodeMutator::with_bindings(&bindings).write(&mut node, "label", json!("world"));
    assert_eq!(
        dirty.get(),
        1,
        "reactive write wakes the subscribing context"
    );
    rcx.dispose();
}

#[test]
fn prop_memo_str_returns_typed_memo_with_reactive_recompute() {
    // Phase 4b typed-memo follow-up: `prop_memo_str` yields a
    // `Memo<String>` whose `get` re-reads from the underlying
    // `Signal<Value>`. Writes through `NodeMutator` flow through
    // the memo without the caller subscribing the inner signal
    // directly.
    use crate::layout::LayoutMode;
    use crate::mutator::NodeMutator;
    use crate::reactive_props::DocumentBindings;
    use prism_core::foundation::spatial::Transform2D;
    use serde_json::json;
    let mut node = Node {
        id: "n".into(),
        component: "x".into(),
        props: json!({ "label": "alpha", "selected": false }),
        children: vec![],
        layout_mode: LayoutMode::default(),
        transform: Transform2D::default(),
        modifiers: vec![],
        style: StyleProperties::default(),
    };
    let bindings = DocumentBindings::new();
    let style = StyleProperties::default();
    let ctx = LowerCtx::new(None, &style).with_bindings(&bindings);

    let memo = ctx
        .prop_memo_str(&node, "label")
        .expect("memo present with bindings");
    assert_eq!(memo.get(), "alpha");
    NodeMutator::with_bindings(&bindings).write(&mut node, "label", json!("beta"));
    assert_eq!(memo.get(), "beta", "memo re-derives after prop write");

    let bool_memo = ctx
        .prop_memo_bool(&node, "selected", false)
        .expect("bool memo present");
    assert!(!bool_memo.get());
    NodeMutator::with_bindings(&bindings).write(&mut node, "selected", json!(true));
    assert!(bool_memo.get());
}

#[test]
fn prop_memo_str_returns_none_without_bindings() {
    use crate::layout::LayoutMode;
    use prism_core::foundation::spatial::Transform2D;
    use serde_json::json;
    let node = Node {
        id: "n".into(),
        component: "x".into(),
        props: json!({ "label": "alpha" }),
        children: vec![],
        layout_mode: LayoutMode::default(),
        transform: Transform2D::default(),
        modifiers: vec![],
        style: StyleProperties::default(),
    };
    let style = StyleProperties::default();
    let ctx = LowerCtx::new(None, &style);
    assert!(ctx.prop_memo_str(&node, "label").is_none());
    assert!(ctx.prop_memo_bool(&node, "selected", false).is_none());
}

#[test]
fn colored_text_node_overrides_cascade_color() {
    let cascade = StyleProperties {
        color: Some("#000000".into()),
        ..Default::default()
    };
    let UiNode::Text { props, .. } =
        colored_text_node("t".into(), "hello".into(), &cascade, 14.0, "#ff0000")
    else {
        panic!("expected text")
    };
    assert_eq!(props.color.r, 0xff);
    assert_eq!(props.color.g, 0x00);
}

#[test]
fn hover_bg_round_trips_alpha() {
    // #RRGGBBAA — last byte is alpha.
    let h = hover_bg("#0000001f").unwrap();
    let c = h.background.unwrap();
    assert_eq!(c.a, 0x1f);
}

// ── Wave 1: modifier render fold ─────────────────────────────────

/// A behaviour that wraps the child in a tagged container so the
/// fold order is observable from the output tree.
struct TagWrapBehaviour {
    id: &'static str,
    marker: &'static str,
}
impl crate::modifier::ModifierBehaviour for TagWrapBehaviour {
    fn id(&self) -> crate::modifier::ModifierId {
        std::borrow::Cow::Borrowed(self.id)
    }
    fn label(&self) -> &str {
        self.id
    }
    fn schema(&self) -> Vec<crate::registry::FieldSpec> {
        Vec::new()
    }
    fn wrap(&self, _modifier: &crate::modifier::Modifier, child: UiNode) -> UiNode {
        // Wrap the child in a container whose semantic tag carries
        // the marker — the fold-order pin reads these back off the
        // output tree.
        UiNode::Container {
            id: format!("wrap-{}", self.marker),
            props: ContainerProps {
                semantic: prism_ui_runtime::layout::Semantic::tag(self.marker),
                ..ContainerProps::default()
            },
            children: vec![child],
        }
    }
}

fn make_test_node(id: &str) -> Node {
    Node {
        id: id.into(),
        component: "container".into(),
        props: serde_json::Value::Null,
        children: vec![],
        layout_mode: LayoutMode::default(),
        transform: prism_core::foundation::spatial::Transform2D::default(),
        modifiers: vec![],
        style: StyleProperties::default(),
    }
}

#[test]
fn modifier_fold_applies_innermost_first() {
    let mut reg = crate::modifier::ModifierRegistry::new();
    reg.register(std::sync::Arc::new(TagWrapBehaviour {
        id: "outer-beh",
        marker: "outer",
    }))
    .unwrap();
    reg.register(std::sync::Arc::new(TagWrapBehaviour {
        id: "inner-beh",
        marker: "inner",
    }))
    .unwrap();

    let mut node = make_test_node("n0");
    // Order on disk: [outer, inner]. Innermost-first means `inner`
    // wraps the bare child first, then `outer` wraps that.
    // Resulting tree: outer → inner → bare.
    node.modifiers
        .push(crate::modifier::Modifier::new("outer-beh"));
    node.modifiers
        .push(crate::modifier::Modifier::new("inner-beh"));

    let style = StyleProperties::default();
    let ctx = LowerCtx::new(None, &style).with_modifier_registry(&reg);
    let lowered = ctx.lower(&node);

    let UiNode::Container {
        props: outer_props,
        children: outer_children,
        ..
    } = &lowered
    else {
        panic!("expected outer wrap container, got {lowered:?}");
    };
    assert_eq!(outer_props.semantic.tag.as_deref(), Some("outer"));
    let UiNode::Container {
        props: inner_props, ..
    } = &outer_children[0]
    else {
        panic!("expected inner wrap container");
    };
    assert_eq!(inner_props.semantic.tag.as_deref(), Some("inner"));
}

#[test]
fn modifier_fold_skips_disabled_entries() {
    let mut reg = crate::modifier::ModifierRegistry::new();
    reg.register(std::sync::Arc::new(TagWrapBehaviour {
        id: "should-wrap",
        marker: "applied",
    }))
    .unwrap();
    reg.register(std::sync::Arc::new(TagWrapBehaviour {
        id: "skip-me",
        marker: "skipped",
    }))
    .unwrap();

    let mut node = make_test_node("n1");
    node.modifiers
        .push(crate::modifier::Modifier::new("should-wrap"));
    node.modifiers
        .push(crate::modifier::Modifier::new("skip-me").disabled());

    let style = StyleProperties::default();
    let ctx = LowerCtx::new(None, &style).with_modifier_registry(&reg);
    let lowered = ctx.lower(&node);

    let UiNode::Container { props, .. } = &lowered else {
        panic!()
    };
    // Only the enabled behaviour wrapped — the disabled one didn't.
    assert_eq!(props.semantic.tag.as_deref(), Some("applied"));
}

#[test]
fn modifier_fold_passes_through_unknown_ids() {
    let reg = crate::modifier::ModifierRegistry::new(); // empty
    let mut node = make_test_node("n2");
    node.modifiers
        .push(crate::modifier::Modifier::new("nonexistent"));

    let style = StyleProperties::default();
    let ctx = LowerCtx::new(None, &style).with_modifier_registry(&reg);
    let lowered = ctx.lower(&node);
    // Default container fallback — no wrap applied.
    let UiNode::Container { id, .. } = &lowered else {
        panic!()
    };
    assert_eq!(id, "n2");
}

#[test]
fn modifier_fold_no_op_without_registry() {
    // Without `with_modifier_registry`, the fold is skipped
    // entirely — headless / SSR paths preserve their current
    // output unchanged.
    let mut node = make_test_node("n3");
    node.modifiers.push(crate::modifier::Modifier::from_kind(
        crate::modifier::ModifierKind::Tooltip,
    ));

    let style = StyleProperties::default();
    let ctx = LowerCtx::new(None, &style);
    let lowered = ctx.lower(&node);
    let UiNode::Container { id, .. } = &lowered else {
        panic!()
    };
    assert_eq!(id, "n3");
}
