use super::*;
use crate::ComponentRegistry;

#[test]
fn primitive_count_is_sixteen() {
    // Wave 10 listed 14 primitives; Wave 11.4 added
    // `prism.builder-host` (15); the editor-views pass added
    // `prism.text-area` as an alias of `prism.text-buffer` (16).
    // The catalogue is a closed set; further changes need a
    // plan update.
    assert_eq!(PRIMITIVES.len(), 16);
}

#[test]
fn builder_host_primitive_is_registered_with_required_document_field() {
    let mut reg = ComponentRegistry::new();
    register_primitives(&mut reg).expect("register");
    let comp = reg
        .get("prism.builder-host")
        .expect("builder-host registered");
    let schema = comp.schema();
    let document_field = schema
        .iter()
        .find(|f| f.key == "document")
        .expect("document field present");
    assert!(
        document_field.required,
        "the `document` field must be required so the inspector flags missing bindings"
    );
}

#[test]
fn every_primitive_id_starts_with_prism_namespace() {
    for spec in PRIMITIVES {
        assert!(
            spec.id.starts_with("prism."),
            "primitive id `{}` must be in the `prism.` namespace",
            spec.id
        );
    }
}

#[test]
fn primitive_ids_are_unique() {
    let mut ids: Vec<&str> = PRIMITIVES.iter().map(|s| s.id).collect();
    let len = ids.len();
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), len, "duplicate primitive ids");
}

#[test]
fn register_primitives_lands_all_specs_in_the_registry() {
    let mut reg = ComponentRegistry::new();
    register_primitives(&mut reg).expect("register");
    for spec in PRIMITIVES {
        assert!(
            reg.get(spec.id).is_some(),
            "expected `{}` in registry after register_primitives",
            spec.id
        );
    }
}

#[test]
fn primitive_lower_emits_data_role_matching_tag_local_part() {
    let mut reg = ComponentRegistry::new();
    register_primitives(&mut reg).expect("register");
    let node = Node {
        id: "p1".into(),
        component: "prism.popover".into(),
        ..Default::default()
    };
    let cascade = StyleProperties::default();
    let ctx = LowerCtx::new(Some(&reg), &cascade);
    let UiNode::Container { props, .. } = popover_lower(&ctx, &node, &cascade) else {
        panic!()
    };
    assert!(props
        .semantic
        .attrs
        .iter()
        .any(|(k, v)| k == "data-role" && v == "popover"));
}

/// Wave 10.4: `prism.text-input` lowers to a real `TextInput`
/// node carrying every prop the schema declares. The `bind`
/// prop forwards as `data-bind-value` so the shell's
/// `route_bind_input_focus` opens a field-focus session on
/// pointer-down (closing the two-way `bind:value` deferral).
#[test]
fn text_input_lower_emits_typed_node_with_props_folded_in() {
    let node = Node {
        id: "in1".into(),
        component: "prism.text-input".into(),
        props: serde_json::json!({
            "value": "hello",
            "placeholder": "type…",
            "bind": "form.email",
            "disabled": true,
            "max-length": 80,
        }),
        ..Default::default()
    };
    let cascade = StyleProperties::default();
    let ctx = LowerCtx::new(None, &cascade);
    let lowered = text_input_lower(&ctx, &node, &cascade);
    let UiNode::TextInput {
        id,
        value,
        placeholder,
        semantic,
        ..
    } = lowered
    else {
        panic!("expected TextInput, got {lowered:?}");
    };
    assert_eq!(id, "in1");
    assert_eq!(value, "hello");
    assert_eq!(placeholder, "type…");
    let attr = |k: &str| {
        semantic
            .attrs
            .iter()
            .find_map(|(name, v)| (name == k).then(|| v.clone()))
    };
    assert_eq!(attr("data-role"), Some("text-input".into()));
    assert_eq!(attr("data-bind-value"), Some("form.email".into()));
    assert_eq!(attr("aria-disabled"), Some("true".into()));
    assert_eq!(attr("data-max-length"), Some("80".into()));
    assert_eq!(attr("placeholder"), Some("type…".into()));
}

#[test]
fn text_input_lower_skips_optional_attrs_when_unset() {
    let node = Node {
        id: "in1".into(),
        component: "prism.text-input".into(),
        props: serde_json::json!({ "value": "" }),
        ..Default::default()
    };
    let cascade = StyleProperties::default();
    let ctx = LowerCtx::new(None, &cascade);
    let UiNode::TextInput { semantic, .. } = text_input_lower(&ctx, &node, &cascade) else {
        panic!();
    };
    let has = |k: &str| semantic.attrs.iter().any(|(n, _)| n == k);
    assert!(has("data-role"));
    assert!(!has("data-bind-value"));
    assert!(!has("aria-disabled"));
    assert!(!has("data-max-length"));
    assert!(!has("placeholder"));
}

fn lower(
    component: &str,
    lower_fn: fn(&LowerCtx, &Node, &StyleProperties) -> UiNode,
    props: serde_json::Value,
) -> UiNode {
    let node = Node {
        id: "n1".into(),
        component: component.into(),
        props,
        ..Default::default()
    };
    let cascade = StyleProperties::default();
    let ctx = LowerCtx::new(None, &cascade);
    lower_fn(&ctx, &node, &cascade)
}

fn semantic_attr(node: &UiNode, key: &str) -> Option<String> {
    match node {
        UiNode::Container { props, .. } => props
            .semantic
            .attrs
            .iter()
            .find_map(|(k, v)| (k == key).then(|| v.clone())),
        UiNode::TextInput { semantic, .. } => semantic
            .attrs
            .iter()
            .find_map(|(k, v)| (k == key).then(|| v.clone())),
        _ => None,
    }
}

/// Wave 10.5 — drag-scrub fold of value / step / min / max + bind.
#[test]
fn drag_scrub_lower_emits_value_step_bounds_and_bind() {
    let n = lower(
        "prism.drag-scrub",
        drag_scrub_lower,
        serde_json::json!({
            "value": 5.5, "step": 0.25, "min": 0, "max": 10, "bind-value": "form.amount",
        }),
    );
    assert_eq!(
        semantic_attr(&n, "data-role").as_deref(),
        Some("drag-scrub")
    );
    assert_eq!(semantic_attr(&n, "data-value").as_deref(), Some("5.5"));
    assert_eq!(semantic_attr(&n, "data-step").as_deref(), Some("0.25"));
    assert_eq!(semantic_attr(&n, "data-min").as_deref(), Some("0"));
    assert_eq!(semantic_attr(&n, "data-max").as_deref(), Some("10"));
    assert_eq!(semantic_attr(&n, "aria-valuemin").as_deref(), Some("0"));
    assert_eq!(
        semantic_attr(&n, "data-bind-value").as_deref(),
        Some("form.amount")
    );
}

/// Wave 10.6 — popover collapses when closed, expands when open.
#[test]
fn popover_lower_open_emits_data_attrs() {
    let n = lower(
        "prism.popover",
        popover_lower,
        serde_json::json!({
            "open": true, "anchor-id": "swatch-1", "placement": "top",
        }),
    );
    assert_eq!(semantic_attr(&n, "data-role").as_deref(), Some("popover"));
    assert_eq!(semantic_attr(&n, "data-open").as_deref(), Some("true"));
    assert_eq!(semantic_attr(&n, "data-placement").as_deref(), Some("top"));
    assert_eq!(
        semantic_attr(&n, "data-anchor-id").as_deref(),
        Some("swatch-1")
    );
    assert!(semantic_attr(&n, "aria-hidden").is_none());
}

#[test]
fn popover_lower_closed_collapses_and_marks_hidden() {
    let n = lower(
        "prism.popover",
        popover_lower,
        serde_json::json!({ "open": false }),
    );
    let UiNode::Container { props, .. } = &n else {
        panic!()
    };
    assert!(matches!(props.width, Sizing::Fixed(0.0)));
    assert_eq!(semantic_attr(&n, "data-open").as_deref(), Some("false"));
    assert_eq!(semantic_attr(&n, "aria-hidden").as_deref(), Some("true"));
}

/// Wave 10.7 — list-picker carries options + selected-id.
#[test]
fn list_picker_lower_emits_options_and_selected_id() {
    let n = lower(
        "prism.list-picker",
        list_picker_lower,
        serde_json::json!({
            "options": [{"id": "a", "label": "A"}, {"id": "b", "label": "B"}],
            "selected-id": "b",
        }),
    );
    assert_eq!(
        semantic_attr(&n, "data-role").as_deref(),
        Some("list-picker")
    );
    assert_eq!(semantic_attr(&n, "data-selected-id").as_deref(), Some("b"));
    assert!(semantic_attr(&n, "data-options")
        .unwrap()
        .contains("\"id\":\"a\""));
}

/// Wave 10.8 — collapsible swallows children when closed.
#[test]
fn collapsible_lower_open_includes_aria_attrs() {
    let n = lower(
        "prism.collapsible",
        collapsible_lower,
        serde_json::json!({
            "open": true, "title": "Details",
        }),
    );
    assert_eq!(semantic_attr(&n, "data-open").as_deref(), Some("true"));
    assert_eq!(semantic_attr(&n, "aria-expanded").as_deref(), Some("true"));
    assert_eq!(semantic_attr(&n, "aria-label").as_deref(), Some("Details"));
}

/// Wave 10.9 — split-handle orientation toggles sizing axes.
#[test]
fn split_handle_lower_vertical_picks_fixed_width_and_grow_height() {
    let n = lower(
        "prism.split-handle",
        split_handle_lower,
        serde_json::json!({
            "orientation": "vertical", "position": 0.6,
        }),
    );
    let UiNode::Container { props, .. } = &n else {
        panic!()
    };
    assert!(matches!(props.width, Sizing::Fixed(6.0)));
    assert!(matches!(props.height, Sizing::Grow));
    assert_eq!(
        semantic_attr(&n, "data-orientation").as_deref(),
        Some("vertical")
    );
    assert_eq!(semantic_attr(&n, "data-position").as_deref(), Some("0.6"));
}

/// Wave 10.12 — select carries options + value + bind.
#[test]
fn select_lower_emits_combobox_data_attrs() {
    let n = lower(
        "prism.select",
        select_lower,
        serde_json::json!({
            "options": [{"id": "x", "label": "X"}],
            "value": "x",
            "bind-value": "form.choice",
        }),
    );
    assert_eq!(semantic_attr(&n, "data-role").as_deref(), Some("select"));
    assert_eq!(semantic_attr(&n, "data-value").as_deref(), Some("x"));
    assert_eq!(
        semantic_attr(&n, "data-bind-value").as_deref(),
        Some("form.choice")
    );
}

/// Wave 10.13 — color-picker swatch carries value + alpha flag.
#[test]
fn color_picker_lower_emits_value_and_alpha() {
    let n = lower(
        "prism.color-picker",
        color_picker_lower,
        serde_json::json!({
            "value": "#ff0080", "alpha": false,
        }),
    );
    assert_eq!(
        semantic_attr(&n, "data-role").as_deref(),
        Some("color-picker")
    );
    assert_eq!(semantic_attr(&n, "data-value").as_deref(), Some("#ff0080"));
    assert_eq!(semantic_attr(&n, "data-alpha").as_deref(), Some("false"));
}

/// Wave 10.14 — file-button has default label + carries accept.
#[test]
fn file_button_lower_emits_default_label_and_accept() {
    let n = lower(
        "prism.file-button",
        file_button_lower,
        serde_json::json!({
            "accept": "image/png", "multiple": true,
        }),
    );
    assert_eq!(
        semantic_attr(&n, "data-role").as_deref(),
        Some("file-button")
    );
    assert_eq!(semantic_attr(&n, "data-label").as_deref(), Some("Browse…"));
    assert_eq!(
        semantic_attr(&n, "data-accept").as_deref(),
        Some("image/png")
    );
    assert_eq!(semantic_attr(&n, "data-multiple").as_deref(), Some("true"));
}

/// Wave 10.15 — resize-edge carries direction + target-id.
#[test]
fn resize_edge_lower_emits_direction_and_target() {
    let n = lower(
        "prism.resize-edge",
        resize_edge_lower,
        serde_json::json!({
            "direction": "br", "target-id": "n42",
        }),
    );
    assert_eq!(
        semantic_attr(&n, "data-role").as_deref(),
        Some("resize-handle")
    );
    assert_eq!(semantic_attr(&n, "data-direction").as_deref(), Some("br"));
    assert_eq!(semantic_attr(&n, "data-target-id").as_deref(), Some("n42"));
}

/// Wave 10.16 — canvas-paint reserves a fixed-size rect carrying
/// the node id so paint callbacks can target it.
#[test]
fn canvas_paint_lower_emits_canvas_id_and_dimensions() {
    let n = lower(
        "prism.canvas-paint",
        canvas_paint_lower,
        serde_json::json!({
            "width": 200, "height": 16,
        }),
    );
    let UiNode::Container { props, .. } = &n else {
        panic!()
    };
    assert!(matches!(props.width, Sizing::Fixed(200.0)));
    assert!(matches!(props.height, Sizing::Fixed(16.0)));
    assert_eq!(
        semantic_attr(&n, "data-canvas-paint-id").as_deref(),
        Some("n1")
    );
    assert_eq!(semantic_attr(&n, "width").as_deref(), Some("200"));
}

/// Wave 10.17 — text-buffer lowers as a multi-line TextInput.
#[test]
fn text_buffer_lower_emits_multiline_text_input() {
    let n = lower(
        "prism.text-buffer",
        text_buffer_lower,
        serde_json::json!({
            "value": "line1\nline2", "bind-value": "doc.body",
        }),
    );
    let UiNode::TextInput {
        value, semantic, ..
    } = &n
    else {
        panic!()
    };
    assert_eq!(value, "line1\nline2");
    let attr = |k: &str| {
        semantic
            .attrs
            .iter()
            .find_map(|(name, v)| (name == k).then(|| v.clone()))
    };
    assert_eq!(attr("data-role").as_deref(), Some("text-buffer"));
    assert_eq!(attr("data-multiline").as_deref(), Some("true"));
    assert_eq!(attr("data-bind-value").as_deref(), Some("doc.body"));
}
