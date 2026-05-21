use super::*;
use crate::registry::FieldKind;

fn setup() -> ComponentRegistry {
    let mut reg = ComponentRegistry::new();
    register_builtins(&mut reg).expect("register builtins");
    reg
}

#[test]
fn text_schema_has_body_level_href() {
    let reg = setup();
    let comp = reg.get("text").expect("text registered");
    let schema = comp.schema();
    assert_eq!(schema.len(), 3);
    assert_eq!(schema[0].key, "body");
    assert_eq!(schema[1].key, "level");
    assert_eq!(schema[2].key, "href");
}

#[test]
fn register_builtins_seeds_sixteen_components() {
    let reg = setup();
    for id in [
        "text",
        "image",
        "container",
        "form",
        "input",
        "button",
        "card",
        "code",
        "divider",
        "spacer",
        "columns",
        "list",
        "table",
        "tabs",
        "accordion",
        "graph-view",
    ] {
        assert!(reg.get(id).is_some(), "missing component: {id}");
    }
}

#[test]
fn divider_schema_is_empty() {
    let reg = setup();
    let comp = reg.get("divider").expect("divider registered");
    assert!(comp.schema().is_empty());
}

#[test]
fn graph_view_id() {
    let reg = setup();
    let comp = reg.get("graph-view").expect("graph-view registered");
    assert_eq!(comp.id().as_str(), "graph-view");
}

#[test]
fn graph_view_schema_has_layout_options() {
    let reg = setup();
    let comp = reg.get("graph-view").expect("graph-view registered");
    let schema = comp.schema();
    let layout_field = schema
        .iter()
        .find(|f| f.key == "layout")
        .expect("missing layout field");
    match &layout_field.kind {
        FieldKind::Select(options) => {
            assert_eq!(options.len(), 4);
            let values: Vec<&str> = options.iter().map(|o| o.value.as_str()).collect();
            assert!(values.contains(&"force"));
            assert!(values.contains(&"tree"));
            assert!(values.contains(&"radial"));
            assert!(values.contains(&"grid"));
        }
        other => panic!("expected Select, got {:?}", other),
    }
}

#[test]
fn graph_view_signals_include_node_and_edge() {
    let reg = setup();
    let comp = reg.get("graph-view").expect("graph-view registered");
    let signals = comp.signals();
    let names: Vec<&str> = signals.iter().map(|s| s.name.as_str()).collect();
    assert!(names.contains(&"node-clicked"), "missing node-clicked");
    assert!(names.contains(&"edge-clicked"), "missing edge-clicked");
    assert!(
        names.contains(&"node-double-clicked"),
        "missing node-double-clicked"
    );
    assert!(names.contains(&"clicked"));
    assert!(names.contains(&"hovered"));
}

#[test]
fn builtin_block_factory_returns_known_ids() {
    assert!(builtin_block("text").is_some());
    assert!(builtin_block("graph-view").is_some());
    assert!(builtin_block("nope").is_none());
}
