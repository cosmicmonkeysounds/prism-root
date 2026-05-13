//! End-to-end integration tests for `docs/dev/dsl-self-bootstrap.md`.
//!
//! Each test writes a self-contained set of `apps/<id>/manifest.toml`
//! files to a temp directory, points `PRISM_APPS_DIR` at it, then
//! exercises the full Shell boot path:
//!
//! - `app_loader::discover` walks the temp tree.
//! - `seed::initial_state_with_apps` hydrates the launchpad.
//! - `ShellAppRegistrar` consumes every `panels.add` row.
//! - `Shell::new` freezes the catalog into `ShellInner::dock_catalog`.
//! - `register_shell_services` + `activate_app_services` filter
//!   `App`-scoped services against `services.{required, optional}`.
//! - `install_components` / `install_services` drain the registrar's
//!   queues into the live shell registries.
//!
//! `PRISM_APPS_DIR` is a process-global env var, so the tests use a
//! shared `Mutex` to run serially against it — concurrent reads of
//! the same env are otherwise racy.

use std::path::PathBuf;
use std::sync::Mutex;

use prism_core::{
    AppRegistrar, ComponentRegistration, NoopAppRegistrar, PanelRegistration, RegistrationError,
    ServiceRegistration,
};
use prism_shell::app_loader;
use prism_shell::app_registry::{
    install_components, install_panels_from_manifests, install_services, LuauComponentBlock,
    LuauScriptedService, ShellAppRegistrar,
};

// `Mutex::new` is const since 1.63 — no `OnceLock` / `Lazy` needed.
static APPS_DIR_LOCK: Mutex<()> = Mutex::new(());

/// Test harness — writes a set of `apps/<id>/manifest.toml` files to
/// a temp dir, sets `PRISM_APPS_DIR` to point at it, runs `f`, then
/// removes the dir and unsets the env var. The harness holds a
/// process-global mutex so concurrent tests don't trip over each
/// other's env mutations.
fn with_apps_dir<F: FnOnce()>(label: &str, manifests: &[(&str, &str)], f: F) {
    // `lock()` returns `Err(PoisonError)` if a prior holder panicked.
    // Recover from poisoning so one panicking test doesn't doom the
    // rest of the suite — we still hold exclusive access to the env.
    let _guard = APPS_DIR_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let root = tempdir(label);
    for (id, body) in manifests {
        let dir = root.join(id);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("manifest.toml"), body).unwrap();
    }
    // SAFETY: we hold `APPS_DIR_LOCK`, so no concurrent reads of the
    // env are happening from this crate. Other crates' tests are in
    // separate processes; `cargo test` isolates target binaries.
    unsafe {
        std::env::set_var("PRISM_APPS_DIR", &root);
    }
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));
    unsafe {
        std::env::remove_var("PRISM_APPS_DIR");
    }
    let _ = std::fs::remove_dir_all(&root);
    if let Err(payload) = result {
        std::panic::resume_unwind(payload);
    }
}

fn tempdir(label: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!(
        "prism-dsl-bootstrap-{}-{}-{}",
        label,
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    p
}

// ── Loop 1 — manifest discovery ───────────────────────────────────

#[test]
fn discovered_apps_drive_launchpad_tiles() {
    let manifests: &[(&str, &str)] = &[
        (
            "alpha",
            r#"id = "alpha"
label = "Alpha"
icon = "icons/alpha.svg"
summary = "First test app."
"#,
        ),
        (
            "beta",
            r#"id = "beta"
label = "Beta"
icon = "icons/beta.svg"
summary = "Second test app."
"#,
        ),
    ];
    with_apps_dir("launchpad", manifests, || {
        let shell = prism_shell::Shell::new().expect("Shell::new");
        let inner = shell.inner.borrow();
        let ids: Vec<&str> = inner
            .state
            .catalog
            .apps
            .iter()
            .map(|a| a.id.as_str())
            .collect();
        assert!(
            ids.contains(&"alpha"),
            "expected `alpha` in launchpad, got {ids:?}"
        );
        assert!(
            ids.contains(&"beta"),
            "expected `beta` in launchpad, got {ids:?}"
        );
        // The four hardcoded fallbacks must NOT appear — once any
        // manifest is discovered the loader takes over completely.
        assert!(
            !ids.contains(&"lattice"),
            "fallback `lattice` should not appear with manifests present"
        );
        assert_eq!(inner.state.catalog.apps.len(), 2);
    });
}

#[test]
fn discovered_apps_drive_launchpad_card_metadata() {
    let manifests: &[(&str, &str)] = &[(
        "lattice",
        r#"id = "lattice"
label = "Lattice (test)"
icon = "icons/grid.svg"
summary = "Test summary."
"#,
    )];
    with_apps_dir("cards", manifests, || {
        let shell = prism_shell::Shell::new().expect("Shell::new");
        let inner = shell.inner.borrow();
        let card = inner.state.catalog.apps.iter().find(|a| a.id == "lattice");
        let card = card.expect("lattice card");
        assert_eq!(card.label, "Lattice (test)");
        assert_eq!(card.icon, "icons/grid.svg");
        assert_eq!(card.summary, "Test summary.");
    });
}

// ── Loop 2 — dock catalog runtime registry ────────────────────────

#[test]
fn manifest_panels_add_surfaces_in_shell_dock_catalog() {
    let manifests: &[(&str, &str)] = &[(
        "lattice",
        r#"id = "lattice"
label = "Lattice"

[[panels.add]]
id = "lattice.peers"
label = "Peers"
icon_hint = "users"
min_width = 240
min_height = 120
tag = "lattice.peers-panel"

[[panels.add]]
id = "lattice.activity"
label = "Activity"
icon_hint = "activity"
min_width = 200
min_height = 100
tag = "lattice.activity-panel"
"#,
    )];
    with_apps_dir("dock_catalog", manifests, || {
        let shell = prism_shell::Shell::new().expect("Shell::new");
        let inner = shell.inner.borrow();
        let catalog = inner.dock_catalog.as_ref();
        // Built-in still present.
        assert!(catalog.get("builder").is_some());
        // Both app-registered panels present.
        let peers = catalog.get("lattice.peers").expect("lattice.peers");
        assert_eq!(peers.label, "Peers");
        assert_eq!(peers.tag, Some("lattice.peers-panel"));
        assert_eq!(peers.min_width, 240.0);
        let activity = catalog.get("lattice.activity").expect("lattice.activity");
        assert_eq!(activity.label, "Activity");
        // Total = 15 built-ins + 2 app panels.
        assert_eq!(catalog.len(), 17);
        // tag_for resolves through the catalog.
        assert_eq!(
            catalog.tag_for("lattice.peers"),
            Some("lattice.peers-panel")
        );
    });
}

// ── Loop 3 — service activation ───────────────────────────────────

#[test]
fn service_filtering_respects_manifest_declarations() {
    let manifests: &[(&str, &str)] = &[(
        "lattice",
        r#"id = "lattice"
label = "Lattice"

[services]
required = ["builder"]
optional = ["luau"]
"#,
    )];
    with_apps_dir("services_filter", manifests, || {
        let shell = prism_shell::Shell::new().expect("Shell::new");
        let inner = shell.inner.borrow();
        // Required + optional kept.
        assert!(inner.services.get("builder").is_some());
        assert!(inner.services.get("luau").is_some());
        // App-scoped services NOT in the manifest are dropped.
        assert!(
            inner.services.get("signals").is_none(),
            "signals should be filtered out"
        );
        assert!(
            inner.services.get("project").is_none(),
            "project should be filtered out"
        );
        // Universal services are always present.
        assert!(inner.services.get("input").is_some());
        assert!(inner.services.get("undo-redo").is_some());
        assert!(inner.services.get("command-palette").is_some());
    });
}

#[test]
fn permissive_default_keeps_all_services_when_no_manifest_declares() {
    let manifests: &[(&str, &str)] = &[(
        "lattice",
        r#"id = "lattice"
label = "Lattice"
"#,
    )];
    with_apps_dir("services_permissive", manifests, || {
        let shell = prism_shell::Shell::new().expect("Shell::new");
        let inner = shell.inner.borrow();
        // No `[services]` block declared → every service stays alive.
        for id in [
            "builder",
            "signals",
            "luau",
            "project",
            "input",
            "undo-redo",
            "command-palette",
        ] {
            assert!(
                inner.services.get(id).is_some(),
                "service `{id}` should survive permissive default"
            );
        }
    });
}

// ── Loop 4 — trait-object registrar + Luau-backed shims ──────────

#[test]
fn registrar_trait_object_routes_panels_to_catalog() {
    // Polymorphic use: code that takes `&dyn AppRegistrar` works
    // identically against `ShellAppRegistrar` and `NoopAppRegistrar`.
    fn push_two<R: AppRegistrar>(reg: &R) -> Result<(), RegistrationError> {
        reg.register_panel(PanelRegistration {
            id: "trait.one".into(),
            label: "One".into(),
            min_width: 100.0,
            min_height: 100.0,
            tag: Some("trait.one-content".into()),
            ..Default::default()
        })?;
        reg.register_panel(PanelRegistration {
            id: "trait.two".into(),
            label: "Two".into(),
            min_width: 100.0,
            min_height: 100.0,
            tag: Some("trait.two-content".into()),
            ..Default::default()
        })?;
        Ok(())
    }

    let shell_reg = ShellAppRegistrar::with_builtin_panels();
    push_two(&shell_reg).expect("shell registrar accepts");
    let snap = shell_reg.snapshot_catalog();
    assert!(snap.get("trait.one").is_some());
    assert!(snap.get("trait.two").is_some());

    // NoopAppRegistrar accepts and silently discards.
    let noop = NoopAppRegistrar;
    push_two(&noop).expect("noop accepts");
}

#[test]
fn manifest_install_panels_round_trips_through_loader_to_registrar() {
    let manifests: &[(&str, &str)] = &[(
        "lattice",
        r#"id = "lattice"
label = "Lattice"

[[panels.add]]
id = "lattice.peers"
label = "Peers"
min_width = 240
min_height = 120
tag = "lattice.peers-panel"
"#,
    )];
    with_apps_dir("install_panels", manifests, || {
        let root = std::env::var("PRISM_APPS_DIR").unwrap();
        let apps = app_loader::discover(&root).expect("discover");
        assert_eq!(apps.len(), 1);

        let reg = ShellAppRegistrar::with_builtin_panels();
        let count = install_panels_from_manifests(&reg, &apps);
        assert_eq!(count, 1);
        let snap = reg.snapshot_catalog();
        assert!(snap.get("lattice.peers").is_some());
    });
}

#[test]
fn register_component_queue_drains_into_shell_component_registry() {
    // Validate the component path independent of Shell::new — exercises
    // the trait, queue, drain, and shim end-to-end.
    let reg = ShellAppRegistrar::with_builtin_panels();
    reg.register_component(ComponentRegistration {
        id: "e2e.card".into(),
        render_key: "e2e.scripts.card.render".into(),
    })
    .unwrap();
    reg.register_component(ComponentRegistration {
        id: "e2e.list".into(),
        render_key: "e2e.scripts.list.render".into(),
    })
    .unwrap();
    let mut registry = prism_shell::components::registry::ShellComponentRegistry::new();
    prism_shell::components::registry::register_full_shell_chrome(&mut registry).unwrap();
    let installed = install_components(&reg, &mut registry);
    assert_eq!(installed, 2);
    assert!(registry.as_component_registry().get("e2e.card").is_some());
    assert!(registry.as_component_registry().get("e2e.list").is_some());
}

#[test]
fn register_service_queue_drains_with_app_scope() {
    let reg = ShellAppRegistrar::with_builtin_panels();
    reg.register_service(ServiceRegistration {
        id: "e2e.metronome".into(),
        on_event_key: "e2e.scripts.metronome.on_event".into(),
    })
    .unwrap();
    let mut services = prism_shell::services::ServiceRegistry::new();
    let installed = install_services(&reg, &mut services);
    assert_eq!(installed, 1);
    assert!(services.get("e2e.metronome").is_some());
    assert_eq!(
        services.scope_of("e2e.metronome"),
        Some(prism_shell::services::ServiceScope::App),
    );
}

#[test]
fn luau_component_block_surfaces_in_render() {
    // Wire the placeholder shim into a registry, then verify its
    // `lower_ui` produces the expected semantic markers — the
    // contract every Luau-defined component renders against until
    // the runtime is wired.
    use prism_builder::block::Block;
    use prism_builder::style::StyleProperties;
    use prism_builder::ui_lower::LowerCtx;
    use prism_ui_runtime::layout::Node as UiNode;

    let block = LuauComponentBlock::new(ComponentRegistration {
        id: "e2e.surface".into(),
        render_key: "e2e.scripts.surface.render".into(),
    });
    let node = prism_builder::Node {
        id: "instance-1".into(),
        component: "e2e.surface".into(),
        ..Default::default()
    };
    let cascade = StyleProperties::default();
    let ctx = LowerCtx::new(None, &cascade);
    let ui = block.lower_ui(&ctx, &node, &cascade);
    let UiNode::Container { id, props, .. } = ui else {
        panic!("expected container, got {ui:?}");
    };
    assert_eq!(id, "instance-1");
    assert!(props
        .semantic
        .attrs
        .iter()
        .any(|(k, v)| k == "data-component" && v == "e2e.surface"));
    assert!(props
        .semantic
        .attrs
        .iter()
        .any(|(k, v)| k == "data-luau-key" && v == "e2e.scripts.surface.render"));
}

#[test]
fn luau_scripted_service_lands_in_service_registry_with_app_scope() {
    use prism_shell::services::{ServiceScope, ShellService};
    let svc = LuauScriptedService::new(ServiceRegistration {
        id: "e2e.peers-sync".into(),
        on_event_key: "e2e.scripts.peers_sync.on_event".into(),
    });
    assert_eq!(svc.id(), "e2e.peers-sync");
    let mut services = prism_shell::services::ServiceRegistry::new();
    services.add_scoped(ServiceScope::App, svc);
    assert!(services.get("e2e.peers-sync").is_some());
    assert_eq!(services.scope_of("e2e.peers-sync"), Some(ServiceScope::App),);
}

// ── Full chain — manifest on disk → live shell ────────────────────

#[test]
fn full_chain_manifest_to_shell_dock_to_render() {
    // The complete e2e contract: write a manifest that adds a panel
    // with a custom content tag, build the Shell, then verify the
    // panel reaches the live dock catalog AND that the binding's
    // sidecar map contains the tag.
    let manifests: &[(&str, &str)] = &[(
        "lattice",
        r#"id = "lattice"
label = "Lattice"

[[panels.add]]
id = "lattice.peers"
label = "Peers"
min_width = 240
min_height = 120
tag = "lattice.peers-panel"
"#,
    )];
    with_apps_dir("full_chain", manifests, || {
        let shell = prism_shell::Shell::new().expect("Shell::new");
        let inner = shell.inner.borrow();
        // Catalog has the panel.
        assert!(
            inner.dock_catalog.get("lattice.peers").is_some(),
            "panel must reach the catalog"
        );
        // The binding produces sidecar maps when the panel is in
        // the current dock tree. The default workflow page is "edit"
        // which doesn't contain `lattice.peers`, so the sidecar map
        // is empty for that id — what we verify is the structure of
        // the emitted JSON (`dock`, `labels`, `tags` keys).
        let ctx = inner.prop_ctx();
        let binding = inner
            .bindings
            .get("shell.dock-workspace")
            .expect("dock-workspace binding registered");
        let emission = binding(&ctx);
        let v = &emission.props;
        assert!(
            v.get("dock").is_some(),
            "expected `dock` key in dock-workspace emission, got {v}"
        );
        assert!(
            v.get("labels").is_some(),
            "expected `labels` sidecar in dock-workspace emission, got {v}"
        );
        assert!(
            v.get("tags").is_some(),
            "expected `tags` sidecar in dock-workspace emission, got {v}"
        );
    });
}

#[test]
fn no_apps_directory_falls_back_to_hardcoded_launchpad() {
    // Sanity check: clear PRISM_APPS_DIR + point at a path that
    // doesn't exist; Shell::new still constructs without panic and
    // the launchpad reflects the hardcoded fallback list.
    let _guard = APPS_DIR_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    unsafe {
        std::env::set_var("PRISM_APPS_DIR", "/tmp/does-not-exist-prism-e2e");
    }
    let shell = prism_shell::Shell::new().expect("Shell::new");
    let inner = shell.inner.borrow();
    let ids: Vec<&str> = inner
        .state
        .catalog
        .apps
        .iter()
        .map(|a| a.id.as_str())
        .collect();
    for required in ["lattice", "musica", "flux", "studio"] {
        assert!(
            ids.contains(&required),
            "fallback should keep `{required}` tile"
        );
    }
    unsafe {
        std::env::remove_var("PRISM_APPS_DIR");
    }
}
