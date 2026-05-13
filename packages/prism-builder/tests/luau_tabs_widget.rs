//! Phase 7 canary: the Luau-authored `tabs-luau` widget compiles,
//! registers, and lowers through the same `LowerCtx` the Rust
//! built-ins use.

#![cfg(feature = "luau")]

use std::path::PathBuf;

use prism_builder::{
    load_widgets, register_block, starter::register_builtins, ComponentRegistry, LuauRenderRegistry,
};
use serde_json::json;

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

#[test]
fn tabs_luau_compiles_and_registers() {
    let mut luau = LuauRenderRegistry::new();
    let mut components = ComponentRegistry::new();

    let report = load_widgets(
        &fixtures_dir(),
        "widgets/*.luau",
        &mut luau,
        &mut components,
    )
    .expect("load");

    assert!(
        report.failures.is_empty(),
        "compile failures: {:?}",
        report.failures
    );
    assert!(report.loaded.contains(&"tabs-luau".to_string()));
    assert!(components.get("tabs-luau").is_some());
}

#[test]
fn tabs_luau_renders_a_pill_per_label() {
    let mut luau = LuauRenderRegistry::new();
    let mut components = ComponentRegistry::new();
    load_widgets(
        &fixtures_dir(),
        "widgets/*.luau",
        &mut luau,
        &mut components,
    )
    .expect("load");

    // Invoke the render fn directly — the virtual tree is what the
    // walker would consume on a real lower pass.
    let virt = luau
        .invoke(
            "tabs-luau",
            &json!({ "labels": "Overview, Activity, Settings" }),
            &serde_json::Value::Null,
        )
        .expect("invoke");

    // Top-level: container with a strip + panel pair.
    assert_eq!(virt.component, "container");
    assert_eq!(virt.children.len(), 2, "strip + panel");

    let strip = &virt.children[0];
    assert_eq!(strip.component, "container");
    assert_eq!(strip.children.len(), 3, "one pill per label");
    for (i, pill) in strip.children.iter().enumerate() {
        assert_eq!(pill.component, "container");
        // First pill gets the highlight background, the rest sit muted.
        let bg = pill
            .props
            .get("background")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        if i == 0 {
            assert_eq!(bg, "#2e3440");
        } else {
            assert_eq!(bg, "#1a1e28");
        }
        assert_eq!(pill.children.len(), 1);
        assert_eq!(pill.children[0].component, "text");
    }

    // Panel exists with no children — placeholder for the child
    // fan-out follow-up.
    let panel = &virt.children[1];
    assert_eq!(panel.component, "container");
    assert!(panel.children.is_empty());
}

#[test]
fn tabs_luau_coexists_with_rust_tabs_in_one_registry() {
    // Both should live in the same ComponentRegistry — the Luau
    // widget uses a distinct `tabs-luau` id so it doesn't collide
    // with the Rust `tabs` builtin. Adding both at once proves the
    // namespace stays clean ahead of any future rename.
    let mut luau = LuauRenderRegistry::new();
    let mut components = ComponentRegistry::new();
    register_builtins(&mut components).expect("rust builtins");
    let report = load_widgets(
        &fixtures_dir(),
        "widgets/*.luau",
        &mut luau,
        &mut components,
    )
    .expect("load");
    assert!(report.failures.is_empty());
    assert!(components.get("tabs").is_some(), "rust tabs lives");
    assert!(components.get("tabs-luau").is_some(), "luau tabs lives");
}

// Workaround so unused-import warnings don't fire when the fixture
// only exercises the public surface above.
#[allow(dead_code)]
fn _force_link() {
    let _: fn(_, _) -> _ = register_block::<prism_builder::LuauComponent>;
}
