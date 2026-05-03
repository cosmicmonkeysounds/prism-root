//! Smoke tests for `#[derive(PrismField)]` and `#[visual_node]` from
//! `prism-luau-derive`. The macros emit paths into `::prism_core::*`,
//! so these live in `prism-builder` (which depends on prism-core)
//! rather than `prism-core` itself.

use prism_core::language::visual::{DataType, PortDirection, PortKind, ScriptNodeKind};
use prism_core::widget::field::{FieldKind, FieldSpec};
use prism_luau_derive::{visual_node, PrismField};

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

    assert_eq!(specs[0].default, serde_json::Value::String("Untitled".into()));
    assert_eq!(specs[2].default, serde_json::Value::from(1i64));
    assert_eq!(specs[3].default, serde_json::Value::Bool(true));
    assert_eq!(specs[4].default, serde_json::Value::String("primary".into()));
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
