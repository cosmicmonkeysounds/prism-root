//! Smoke tests for `#[derive(PrismField)]` and `#[visual_node]` from
//! `prism-luau-derive`. The macros emit paths into `::prism_core::*`,
//! so these live in `prism-builder` (which depends on prism-core)
//! rather than `prism-core` itself.

use prism_core::language::visual::{DataType, PortDirection, PortKind, ScriptNodeKind};
use prism_core::widget::field::{FieldKind, FieldSpec};
use prism_luau_derive::{visual_node, PrismBlock, PrismField};

#[derive(PrismField)]
#[allow(dead_code)]
struct ExampleProps {
    #[field(label = "Title", default = "Untitled")]
    title: String,
    #[field(label = "Body", multiline)]
    body: String,
    #[field(label = "Count", default = 1, min = 0.0, max = 100.0)]
    count: i64,
    #[field(label = "Visible", default = true)]
    visible: bool,
    #[field(label = "Style", select("primary", "secondary"), default = "primary")]
    style: String,
}

#[test]
fn prism_field_derive_emits_specs_in_order() {
    let specs: Vec<FieldSpec> = ExampleProps::field_specs();
    let keys: Vec<&str> = specs.iter().map(|s| s.key.as_str()).collect();
    assert_eq!(keys, vec!["title", "body", "count", "visible", "style"]);

    assert!(matches!(specs[0].kind, FieldKind::Text));
    assert!(matches!(specs[1].kind, FieldKind::TextArea));
    assert!(matches!(specs[2].kind, FieldKind::Integer(_)));
    assert!(matches!(specs[3].kind, FieldKind::Boolean));
    match &specs[4].kind {
        FieldKind::Select(opts) => {
            assert_eq!(opts.len(), 2);
            assert_eq!(opts[0].value, "primary");
        }
        other => panic!("expected Select, got {other:?}"),
    }

    assert_eq!(
        specs[0].default,
        serde_json::Value::String("Untitled".into())
    );
    assert_eq!(specs[2].default, serde_json::Value::from(1i64));
    assert_eq!(specs[3].default, serde_json::Value::Bool(true));
    assert_eq!(
        specs[4].default,
        serde_json::Value::String("primary".into())
    );
}

#[test]
fn prism_field_explicit_kind_attributes_cover_rich_kinds() {
    #[derive(PrismField)]
    #[allow(dead_code)]
    struct RichProps {
        #[field(kind = "color")]
        accent: String,
        #[field(kind = "date")]
        starts_on: String,
        #[field(kind = "datetime")]
        starts_at: String,
        #[field(kind = "duration")]
        run_for: i64,
        #[field(kind = "file", accept = "image/*")]
        avatar: String,
        #[field(kind = "currency", currency = "USD")]
        price: f64,
        #[field(kind = "calculation", formula = "SUM(items)")]
        total: f64,
    }

    let specs = RichProps::field_specs();
    assert!(matches!(specs[0].kind, FieldKind::Color));
    assert!(matches!(specs[1].kind, FieldKind::Date));
    assert!(matches!(specs[2].kind, FieldKind::DateTime));
    assert!(matches!(specs[3].kind, FieldKind::Duration));
    match &specs[4].kind {
        FieldKind::File(cfg) => assert_eq!(cfg.accept, vec!["image/*".to_string()]),
        other => panic!("expected File, got {other:?}"),
    }
    match &specs[5].kind {
        FieldKind::Currency { currency_code } => assert_eq!(currency_code.as_deref(), Some("USD")),
        other => panic!("expected Currency, got {other:?}"),
    }
    match &specs[6].kind {
        FieldKind::Calculation { formula } => assert_eq!(formula, "SUM(items)"),
        other => panic!("expected Calculation, got {other:?}"),
    }
}

#[test]
fn prism_field_defaults_use_attribute_literals_or_type_default() {
    let p = ExampleProps::defaults();
    assert_eq!(p.title, "Untitled");
    assert_eq!(p.body, "");
    assert_eq!(p.count, 1);
    assert!(p.visible);
    assert_eq!(p.style, "primary");
}

#[test]
fn prism_field_from_value_extracts_typed_fields_with_fallback() {
    use serde_json::json;
    let v = json!({
        "title": "Hello",
        "count": 42,
        "visible": false,
        // body, style omitted — must fall back to defaults
    });
    let p = ExampleProps::from_value(&v);
    assert_eq!(p.title, "Hello");
    assert_eq!(p.body, "");
    assert_eq!(p.count, 42);
    assert!(!p.visible);
    assert_eq!(p.style, "primary");

    // Wrong-typed values fall back to defaults rather than panicking.
    let bad = json!({ "title": 123, "count": "nope", "visible": "yes" });
    let p = ExampleProps::from_value(&bad);
    assert_eq!(p.title, "Untitled");
    assert_eq!(p.count, 1);
    assert!(p.visible);
}

#[test]
fn prism_field_from_value_strips_raw_ident_prefix() {
    #[derive(PrismField)]
    #[allow(dead_code)]
    struct RawIdentProps {
        #[field(default = "submit")]
        r#type: String,
    }
    let p = RawIdentProps::from_value(&serde_json::json!({ "type": "button" }));
    assert_eq!(p.r#type, "button");
    let p = RawIdentProps::defaults();
    assert_eq!(p.r#type, "submit");
}

#[test]
fn prism_field_humanizes_missing_labels() {
    #[derive(PrismField)]
    #[allow(dead_code)]
    struct AutoLabel {
        first_name: String,
        is_active: bool,
    }

    let specs = AutoLabel::field_specs();
    assert_eq!(specs[0].label, "First Name");
    assert_eq!(specs[1].label, "Is Active");
}

#[visual_node(category = "Math", label = "Add")]
#[allow(dead_code)]
fn add(a: f64, b: f64) -> f64 {
    a + b
}

#[test]
fn visual_node_emits_node_def_with_typed_ports() {
    let def = ADD_NODE_DEF();
    assert_eq!(def.label, "Add");
    assert_eq!(def.category, "Math");
    assert!(matches!(def.kind, ScriptNodeKind::Custom(ref s) if s == "add"));

    let inputs: Vec<&str> = def
        .default_ports
        .iter()
        .filter(|p| p.direction == PortDirection::Input)
        .map(|p| p.id.as_str())
        .collect();
    assert_eq!(inputs, vec!["a", "b"]);

    assert!(def
        .default_ports
        .iter()
        .all(|p| matches!(p.kind, PortKind::Data)));

    assert!(def
        .default_ports
        .iter()
        .any(|p| p.id == "result" && matches!(p.data_type, DataType::Number)));

    assert!(def.description.contains("add(a, b)"));
}

#[visual_node(category = "Logic", luau = "not (a)")]
#[allow(dead_code)]
fn not_op(a: bool) -> bool {
    !a
}

#[test]
fn visual_node_uses_explicit_luau_template_when_provided() {
    let def = NOT_OP_NODE_DEF();
    assert_eq!(def.description, "not (a)");
    assert_eq!(def.category, "Logic");
}

#[test]
fn luau_palette_aggregates_derived_node_defs_with_builtins() {
    use prism_core::language::luau::LuauVisualLanguage;
    use prism_core::language::visual::bridge::VisualLanguage;

    let lang = LuauVisualLanguage::new()
        .with_node_def(ADD_NODE_DEF())
        .with_node_def(NOT_OP_NODE_DEF());

    let palette = lang.node_palette();

    // Built-in language-control-flow entries still present.
    assert!(palette.iter().any(|p| p.category == "Control Flow"));
    assert!(palette.iter().any(|p| p.category == "Signals"));

    // Derived entries appended.
    assert!(palette
        .iter()
        .any(|p| p.label == "Add" && p.category == "Math"));
    assert!(palette
        .iter()
        .any(|p| p.label == "NotOp" && p.category == "Logic"));

    // Bare instance still returns built-ins only.
    let bare = LuauVisualLanguage::new().node_palette();
    assert!(!bare.iter().any(|p| p.label == "Add"));
    assert_eq!(palette.len(), bare.len() + 2);
}

// ── PrismBlock derive ───────────────────────────────────────────

#[derive(PrismBlock, Default)]
#[block(id = "demo-card")]
struct DemoCardBlock;

impl DemoCardBlock {
    fn schema() -> Vec<FieldSpec> {
        vec![FieldSpec::text("title", "Title")]
    }

    fn template(
        _props: &serde_json::Value,
        _children: &[prism_builder::Node],
    ) -> prism_core::widget::TemplateNode {
        use prism_core::widget::{LayoutDirection, TemplateNode};
        TemplateNode::Container {
            direction: LayoutDirection::Vertical,
            gap: Some(8),
            padding: Some(12),
            children: vec![
                TemplateNode::DataBinding {
                    field: "title".into(),
                    component_id: "text".into(),
                    prop_key: "body".into(),
                },
                TemplateNode::Children,
            ],
        }
    }
}

#[test]
fn prism_block_derive_emits_block_impl_with_id_and_schema() {
    use prism_builder::Block;
    let block = DemoCardBlock;
    assert_eq!(block.id(), "demo-card");
    let schema = block.schema();
    assert_eq!(schema.len(), 1);
    assert_eq!(schema[0].key, "title");
}

#[test]
fn prism_block_derive_renders_slint_via_template_walker() {
    use prism_builder::{Block, ComponentRegistry, RenderSlintContext, SlintEmitter};
    let mut registry = ComponentRegistry::new();
    prism_builder::starter::register_builtins(&mut registry).unwrap();

    let tokens = prism_core::design_tokens::DesignTokens::default();
    let resources = indexmap::IndexMap::new();
    let prefabs = indexmap::IndexMap::new();
    let facets = indexmap::IndexMap::new();
    let facet_schemas = indexmap::IndexMap::new();
    let ctx = RenderSlintContext::new(
        &tokens,
        &registry,
        &resources,
        &prefabs,
        &facets,
        &facet_schemas,
        false,
    );

    let block = DemoCardBlock;
    let mut out = SlintEmitter::new();
    block
        .render_slint(&ctx, &serde_json::json!({"title": "Hello"}), &[], &mut out)
        .unwrap();
    let source = out.build();
    assert!(source.contains("VerticalLayout") || source.contains("HorizontalLayout"));
    assert!(source.contains("Hello"));
}

// ── PrismBlock with typed `props = "..."` attribute ──────────────────

#[derive(PrismField, Default)]
#[allow(dead_code)]
struct TypedCardProps {
    #[field(label = "Title", default = "Untitled")]
    title: String,
    #[field(label = "Subtitle", default = "")]
    subtitle: String,
}

#[derive(PrismBlock, Default)]
#[block(id = "typed-card", props = "TypedCardProps")]
struct TypedCardBlock;

impl TypedCardBlock {
    // Note: receives `&TypedCardProps`, not `&Value`. The derive
    // extracts the typed struct via `TypedCardProps::from_value(props)`
    // before calling this fn.
    fn template(
        p: &TypedCardProps,
        _children: &[prism_builder::Node],
    ) -> prism_core::widget::TemplateNode {
        use prism_core::widget::{LayoutDirection, TemplateNode};
        // Use the typed struct directly to drive a (trivial) shape
        // decision — proves the typed extraction reached `template()`.
        let mut kids = vec![TemplateNode::DataBinding {
            field: "title".into(),
            component_id: "text".into(),
            prop_key: "body".into(),
        }];
        if !p.subtitle.is_empty() {
            kids.push(TemplateNode::DataBinding {
                field: "subtitle".into(),
                component_id: "text".into(),
                prop_key: "body".into(),
            });
        }
        TemplateNode::Container {
            direction: LayoutDirection::Vertical,
            gap: Some(4),
            padding: Some(8),
            children: kids,
        }
    }
}

#[test]
fn prism_block_typed_props_derives_schema_from_props_struct() {
    use prism_builder::Block;
    let block = TypedCardBlock;
    let schema = block.schema();
    let keys: Vec<&str> = schema.iter().map(|s| s.key.as_str()).collect();
    assert_eq!(keys, vec!["title", "subtitle"]);
}

#[test]
fn prism_block_typed_props_extracts_typed_props_for_template() {
    use prism_builder::{Block, ComponentRegistry, RenderSlintContext, SlintEmitter};
    let mut registry = ComponentRegistry::new();
    prism_builder::starter::register_builtins(&mut registry).unwrap();

    let tokens = prism_core::design_tokens::DesignTokens::default();
    let resources = indexmap::IndexMap::new();
    let prefabs = indexmap::IndexMap::new();
    let facets = indexmap::IndexMap::new();
    let facet_schemas = indexmap::IndexMap::new();
    let ctx = RenderSlintContext::new(
        &tokens,
        &registry,
        &resources,
        &prefabs,
        &facets,
        &facet_schemas,
        false,
    );

    let block = TypedCardBlock;

    // With subtitle set, both bindings render.
    let mut out = SlintEmitter::new();
    block
        .render_slint(
            &ctx,
            &serde_json::json!({"title": "Hi", "subtitle": "Yo"}),
            &[],
            &mut out,
        )
        .unwrap();
    let source = out.build();
    assert!(source.contains("Hi"));
    assert!(source.contains("Yo"));

    // With subtitle missing, only the title binding renders — proves
    // the typed extraction (subtitle defaulted to "") gated the
    // template branch.
    let mut out = SlintEmitter::new();
    block
        .render_slint(&ctx, &serde_json::json!({"title": "Hi"}), &[], &mut out)
        .unwrap();
    let source = out.build();
    assert!(source.contains("Hi"));
    assert!(!source.contains("Yo"));
}
