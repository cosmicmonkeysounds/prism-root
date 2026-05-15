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
        ..Default::default()
    })
    .unwrap();
    reg.register_component(ComponentRegistration {
        id: "e2e.list".into(),
        render_key: "e2e.scripts.list.render".into(),
        ..Default::default()
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
        ..Default::default()
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

// ── ADR-009 — per-app skeletons ───────────────────────────────────

#[test]
fn manifest_declared_skeleton_loads_and_caches_in_shell() {
    // The manifest points at `shell.prism-ui` in the same dir. The
    // loader reads + parses it; Shell::new caches it in
    // `ShellInner.app_skeletons`.
    let manifests: &[(&str, &str)] = &[(
        "lattice",
        r#"id = "lattice"
label = "Lattice"

[entry]
skeleton = "shell.prism-ui"
"#,
    )];
    with_apps_dir("skeleton_loads", manifests, || {
        // Also write the skeleton file alongside the manifest.
        let root = std::env::var("PRISM_APPS_DIR").unwrap();
        let skel_path = std::path::Path::new(&root)
            .join("lattice")
            .join("shell.prism-ui");
        std::fs::write(&skel_path, r#"<shell.dock-workspace id="custom-dock"/>"#).unwrap();

        let shell = prism_shell::Shell::new().expect("Shell::new");
        let inner = shell.inner.borrow();
        assert!(
            inner.app_skeletons.contains_key("lattice"),
            "expected `lattice` skeleton to be cached, got keys={:?}",
            inner.app_skeletons.keys().collect::<Vec<_>>()
        );
    });
}

#[test]
fn missing_skeleton_file_falls_back_silently_without_aborting_boot() {
    // The manifest declares a skeleton that doesn't exist on disk.
    // Shell::new must still succeed; that app simply gets no entry
    // in `app_skeletons` and renders against the default body.
    let manifests: &[(&str, &str)] = &[(
        "broken",
        r#"id = "broken"
label = "Broken"

[entry]
skeleton = "does-not-exist.prism-ui"
"#,
    )];
    with_apps_dir("skeleton_missing", manifests, || {
        let shell =
            prism_shell::Shell::new().expect("Shell::new should succeed despite missing skel");
        let inner = shell.inner.borrow();
        assert!(
            !inner.app_skeletons.contains_key("broken"),
            "broken app should have no cached skeleton"
        );
        // Launchpad tile still surfaces.
        assert!(inner.state.catalog.apps.iter().any(|a| a.id == "broken"));
    });
}

#[test]
fn active_app_skeleton_selects_per_app_or_default() {
    // ADR-009 contract: `inner.active_app_skeleton()` picks the
    // active app's skeleton if cached, else the default.
    let manifests: &[(&str, &str)] = &[(
        "lattice",
        r#"id = "lattice"
label = "Lattice"

[entry]
skeleton = "shell.prism-ui"
"#,
    )];
    with_apps_dir("active_app_skel", manifests, || {
        let root = std::env::var("PRISM_APPS_DIR").unwrap();
        let skel_path = std::path::Path::new(&root)
            .join("lattice")
            .join("shell.prism-ui");
        std::fs::write(&skel_path, r#"<shell.dock-workspace id="lattice-dock"/>"#).unwrap();

        let shell = prism_shell::Shell::new().expect("Shell::new");
        let mut inner = shell.inner.borrow_mut();

        // No active app → default skeleton applies.
        inner.state.workspace.active_app = None;
        let default_dock_id = first_element_attr(inner.active_app_skeleton(), "id");
        assert_eq!(default_dock_id.as_deref(), Some("dock"));

        // Switch to lattice → cached app skeleton applies.
        inner.state.workspace.active_app = Some("lattice".to_string());
        let active_dock_id = first_element_attr(inner.active_app_skeleton(), "id");
        assert_eq!(active_dock_id.as_deref(), Some("lattice-dock"));

        // Switch to an unknown app id → falls back to default.
        inner.state.workspace.active_app = Some("nonexistent".to_string());
        let fallback_dock_id = first_element_attr(inner.active_app_skeleton(), "id");
        assert_eq!(fallback_dock_id.as_deref(), Some("dock"));
    });
}

#[test]
fn shell_render_composes_host_with_active_app_body() {
    // The render path grafts the active app's skeleton into the host
    // skeleton's `<shell.app-window>` body. The composed AST drives
    // the actual lower pass, so the rendered output reflects the
    // app's skeleton.
    let manifests: &[(&str, &str)] = &[(
        "lattice",
        r#"id = "lattice"
label = "Lattice"

[entry]
skeleton = "shell.prism-ui"
"#,
    )];
    with_apps_dir("shell_render_compose", manifests, || {
        let root = std::env::var("PRISM_APPS_DIR").unwrap();
        let skel_path = std::path::Path::new(&root)
            .join("lattice")
            .join("shell.prism-ui");
        std::fs::write(&skel_path, r#"<shell.dock-workspace id="lattice-dock"/>"#).unwrap();

        let shell = prism_shell::Shell::new().expect("Shell::new");
        {
            let mut inner = shell.inner.borrow_mut();
            inner.state.workspace.active_app = Some("lattice".to_string());
        }
        let tree = shell.render();
        // Walk the rendered tree to find any node with id starting
        // with `lattice-dock` — the dock workspace id from the
        // lattice skeleton flows through to the rendered output.
        let mut found = false;
        walk_ids(&tree, "lattice-dock", &mut found);
        assert!(
            found,
            "expected rendered tree to contain a node with id starting with `lattice-dock`"
        );
    });
}

/// Helper — first element node in a skeleton, return its named attr value.
fn first_element_attr(skel: &prism_shell::Skeleton, attr: &str) -> Option<String> {
    use prism_core::language::prism_ui::{AttributeValue, Node};
    for n in &skel.doc.nodes {
        if let Node::Element(el) = n {
            for a in &el.attributes {
                if a.name.raw == attr {
                    if let AttributeValue::String { value, .. } = &a.value {
                        return Some(value.clone());
                    }
                }
            }
        }
    }
    None
}

/// Walk the rendered `UiNode` tree looking for any container whose
/// id starts with `needle`. Sets `found` to true on the first hit.
fn walk_ids(nodes: &[prism_ui_runtime::layout::Node], needle: &str, found: &mut bool) {
    use prism_ui_runtime::layout::Node as UiNode;
    if *found {
        return;
    }
    for n in nodes {
        if *found {
            return;
        }
        if let UiNode::Container { id, children, .. } = n {
            if id.starts_with(needle) {
                *found = true;
                return;
            }
            walk_ids(children, needle, found);
        }
    }
}

// ── ADR-009 follow-on — per-app stylesheets ──────────────────────

#[test]
fn manifest_declared_stylesheet_loads_and_caches_in_shell() {
    let manifests: &[(&str, &str)] = &[(
        "lattice",
        r#"id = "lattice"
label = "Lattice"

[entry]
styles = "app.prss"
"#,
    )];
    with_apps_dir("stylesheet_loads", manifests, || {
        let root = std::env::var("PRISM_APPS_DIR").unwrap();
        let styles_path = std::path::Path::new(&root).join("lattice").join("app.prss");
        std::fs::write(
            &styles_path,
            "[tokens.colors]\n\
             accent = \"#ff0000\"\n\
             \n\
             [class.lattice-card]\n\
             background = \"{tokens.colors.accent}\"\n",
        )
        .unwrap();
        let shell = prism_shell::Shell::new().expect("Shell::new");
        let inner = shell.inner.borrow();
        let app_sheet = inner.app_stylesheets.get("lattice").expect("cached");
        // The sheet's class table has the lattice-card class.
        assert!(
            app_sheet.sheet().classes.contains_key("lattice-card"),
            "expected `lattice-card` class in cached app stylesheet"
        );
    });
}

#[test]
fn active_app_stylesheet_selects_per_app_or_none() {
    let manifests: &[(&str, &str)] = &[(
        "lattice",
        r#"id = "lattice"
label = "Lattice"

[entry]
styles = "app.prss"
"#,
    )];
    with_apps_dir("active_app_sheet", manifests, || {
        let root = std::env::var("PRISM_APPS_DIR").unwrap();
        let styles_path = std::path::Path::new(&root).join("lattice").join("app.prss");
        std::fs::write(
            &styles_path,
            "[class.lattice-only]\nbackground = \"#deadbe\"\n",
        )
        .unwrap();
        let shell = prism_shell::Shell::new().expect("Shell::new");
        // No active app: returns None.
        assert!(shell.inner.borrow().active_app_stylesheet().is_none());

        // Switch to lattice: stylesheet surfaces.
        shell.switch_active_app(Some("lattice"));
        let inner = shell.inner.borrow();
        let sheet = inner.active_app_stylesheet().expect("active stylesheet");
        assert!(sheet.sheet().classes.contains_key("lattice-only"));

        // Switch to an unknown app: falls back to None.
        drop(inner);
        shell.switch_active_app(Some("nonexistent"));
        assert!(shell.inner.borrow().active_app_stylesheet().is_none());
    });
}

#[test]
fn stylesheet_merge_layers_overlay_over_host() {
    use prism_shell::render::Stylesheet;
    let host = Stylesheet::from_source(
        "[tokens.colors]\n\
         accent = \"#000000\"\n\
         text = \"#111111\"\n\
         \n\
         [class.btn]\n\
         background = \"#000000\"\n",
    );
    let overlay = Stylesheet::from_source(
        "[tokens.colors]\n\
         accent = \"#ff0000\"\n\
         \n\
         [class.btn]\n\
         background = \"#ff0000\"\n\
         \n\
         [class.lattice-card]\n\
         background = \"#dddddd\"\n",
    );
    let merged = host.merge_with(&overlay);
    let sheet = merged.sheet();

    // Overlay wins on conflict (accent).
    assert_eq!(
        sheet.tokens.colors.get("accent").map(String::as_str),
        Some("#ff0000")
    );
    // Host-only tokens preserved (text).
    assert_eq!(
        sheet.tokens.colors.get("text").map(String::as_str),
        Some("#111111")
    );
    // Overlay class replaces base class (atomic).
    let btn = sheet.classes.get("btn").expect("btn class");
    assert_eq!(
        btn.properties.get("background").map(String::as_str),
        Some("#ff0000")
    );
    // Overlay-only class added.
    assert!(sheet.classes.contains_key("lattice-card"));
}

// ── ADR-010 Phase 1 + Shell::switch_active_app ───────────────────

#[test]
fn switch_active_app_triggers_render_and_service_rebuild() {
    let manifests: &[(&str, &str)] = &[
        (
            "alpha",
            r#"id = "alpha"
label = "Alpha"
"#,
        ),
        (
            "beta",
            r#"id = "beta"
label = "Beta"
"#,
        ),
    ];
    with_apps_dir("switch_chain", manifests, || {
        let shell = prism_shell::Shell::new().expect("Shell::new");
        // Boot: no active app.
        assert!(shell.active_app().is_none());
        let _ = shell.render();

        // Switch to alpha → state updates, render scope dirty, render
        // produces a new tree.
        let moved = shell.switch_active_app(Some("alpha"));
        assert!(moved);
        assert_eq!(shell.active_app().as_deref(), Some("alpha"));
        let tree_a = shell.render();
        assert!(!tree_a.is_empty());

        // Switch to beta → state updates again.
        shell.switch_active_app(Some("beta"));
        assert_eq!(shell.active_app().as_deref(), Some("beta"));

        // Idempotent: same target returns false, no dirty bump.
        let _ = shell.render();
        let moved_again = shell.switch_active_app(Some("beta"));
        assert!(!moved_again);
    });
}

#[test]
fn full_swap_chain_skeleton_stylesheet_services_all_track() {
    // The headline integration: a single switch_active_app call
    // drives skeleton swap + service activation + stylesheet swap
    // + render dirty all together.
    let manifests: &[(&str, &str)] = &[(
        "lattice",
        r#"id = "lattice"
label = "Lattice"

[entry]
skeleton = "shell.prism-ui"
styles = "app.prss"

[services]
required = ["builder"]
"#,
    )];
    with_apps_dir("full_swap_chain", manifests, || {
        let root = std::env::var("PRISM_APPS_DIR").unwrap();
        let dir = std::path::Path::new(&root).join("lattice");
        std::fs::write(
            dir.join("shell.prism-ui"),
            r#"<shell.dock-workspace id="lattice-dock"/>"#,
        )
        .unwrap();
        std::fs::write(
            dir.join("app.prss"),
            "[class.lattice-class]\nbackground = \"#abcdef\"\n",
        )
        .unwrap();

        let shell = prism_shell::Shell::new().expect("Shell::new");

        // Boot: service filter applied (builder kept, signals dropped).
        {
            let inner = shell.inner.borrow();
            assert!(inner.services.get("builder").is_some());
            assert!(
                inner.services.get("signals").is_none(),
                "signals should be filtered out by manifest's services.required"
            );
            // No app active yet — default skeleton, no app stylesheet.
            assert!(inner.active_app_stylesheet().is_none());
        }

        // Activate lattice — all three pieces should flip.
        shell.switch_active_app(Some("lattice"));
        {
            let inner = shell.inner.borrow();
            // Skeleton: app's lattice-dock id surfaces.
            let skel = inner.active_app_skeleton();
            let mut found_id = None;
            for n in &skel.doc.nodes {
                if let prism_core::language::prism_ui::Node::Element(el) = n {
                    for a in &el.attributes {
                        if a.name.raw == "id" {
                            if let prism_core::language::prism_ui::AttributeValue::String {
                                value,
                                ..
                            } = &a.value
                            {
                                found_id = Some(value.clone());
                            }
                        }
                    }
                }
            }
            assert_eq!(found_id.as_deref(), Some("lattice-dock"));
            // Stylesheet: the lattice-class lands.
            let sheet = inner.active_app_stylesheet().expect("active sheet");
            assert!(sheet.sheet().classes.contains_key("lattice-class"));
        }
        // Render produces a coherent tree.
        let tree = shell.render();
        assert!(!tree.is_empty());

        // Deactivate — falls back to defaults.
        shell.switch_active_app(None);
        {
            let inner = shell.inner.borrow();
            assert!(inner.active_app_stylesheet().is_none());
        }
    });
}

#[test]
fn per_app_stylesheet_hot_reload_round_trips_through_install_method() {
    // End-to-end PRSS hot-reload chain: load an app with a styles
    // declaration, activate it, then push a fresh stylesheet via
    // `install_app_stylesheet` (the API the dev-loop watcher will
    // call when `apps/<id>/app.prss` changes). The render must
    // surface the new class set on the next frame.
    use prism_shell::render::Stylesheet;
    let manifests: &[(&str, &str)] = &[(
        "lattice",
        r#"id = "lattice"
label = "Lattice"

[entry]
styles = "app.prss"
"#,
    )];
    with_apps_dir("stylesheet_hot_reload", manifests, || {
        let root = std::env::var("PRISM_APPS_DIR").unwrap();
        let styles_path = std::path::Path::new(&root).join("lattice").join("app.prss");
        std::fs::write(&styles_path, "[class.lattice-v1]\nbackground = \"#000\"\n").unwrap();
        let shell = prism_shell::Shell::new().expect("Shell::new");
        shell.switch_active_app(Some("lattice"));
        let _ = shell.render();

        // Initial sheet has v1 only.
        {
            let inner = shell.inner.borrow();
            let sheet = inner.active_app_stylesheet().expect("active");
            assert!(sheet.sheet().classes.contains_key("lattice-v1"));
            assert!(!sheet.sheet().classes.contains_key("lattice-v2"));
        }

        // Hot-reload: parse a new sheet and install for `lattice`.
        let v2 = Stylesheet::from_source("[class.lattice-v2]\nbackground = \"#fff\"\n");
        let dirty = shell.install_app_stylesheet("lattice", v2);
        assert!(dirty, "active-app hot-reload must mark dirty");

        // Render picks up the new sheet.
        let _ = shell.render();
        let inner = shell.inner.borrow();
        let sheet = inner.active_app_stylesheet().expect("active");
        assert!(
            sheet.sheet().classes.contains_key("lattice-v2"),
            "hot-reloaded class should be present after install"
        );
        assert!(
            !sheet.sheet().classes.contains_key("lattice-v1"),
            "old class should be gone after install replaces"
        );
    });
}

#[test]
fn musica_and_flux_each_swap_in_a_distinct_app_skeleton() {
    // ADR-009 Phase 1: Musica + Flux author distinct skeletons under
    // `apps/<id>/shell.prism-ui`. The active-app cursor swap selects
    // the matching skeleton; each app's rendered tree contains a
    // root container with the app-specific dock id.
    let manifests: &[(&str, &str)] = &[
        (
            "lattice",
            r#"id = "lattice"
label = "Lattice"

[entry]
skeleton = "shell.prism-ui"
"#,
        ),
        (
            "musica",
            r#"id = "musica"
label = "Musica"

[entry]
skeleton = "shell.prism-ui"
"#,
        ),
        (
            "flux",
            r#"id = "flux"
label = "Flux"

[entry]
skeleton = "shell.prism-ui"
"#,
        ),
    ];
    with_apps_dir("musica_flux_skeletons", manifests, || {
        let root = std::env::var("PRISM_APPS_DIR").unwrap();
        // Author each app's skeleton inline so the test is fully
        // self-contained — it doesn't depend on the on-disk
        // `apps/<id>/shell.prism-ui` fixtures.
        std::fs::write(
            std::path::Path::new(&root)
                .join("lattice")
                .join("shell.prism-ui"),
            r#"<shell.dock-workspace id="lattice-dock"/>"#,
        )
        .unwrap();
        std::fs::write(
            std::path::Path::new(&root)
                .join("musica")
                .join("shell.prism-ui"),
            r#"<shell.dock-workspace id="musica-stage"/>"#,
        )
        .unwrap();
        std::fs::write(
            std::path::Path::new(&root)
                .join("flux")
                .join("shell.prism-ui"),
            r#"<shell.dock-workspace id="flux-canvas"/>"#,
        )
        .unwrap();

        let shell = prism_shell::Shell::new().expect("Shell::new");

        // Each app id selects its own dock root id end-to-end.
        for (app_id, root_id) in [
            ("lattice", "lattice-dock"),
            ("musica", "musica-stage"),
            ("flux", "flux-canvas"),
        ] {
            // Use the public swap entry point so service rebuild + dirty
            // mark fire alongside the skeleton swap — ADR-009 + ADR-010
            // composed end-to-end.
            let moved = shell.inner.borrow_mut().switch_active_app(Some(app_id));
            assert!(
                moved || shell.inner.borrow().state.workspace.active_app.as_deref() == Some(app_id),
                "switch_active_app should move (or be a no-op if already active) for `{app_id}`"
            );
            let tree = shell.render();
            let mut found = false;
            walk_ids(&tree, root_id, &mut found);
            assert!(
                found,
                "expected rendered tree for `{app_id}` to carry id `{root_id}`"
            );
        }
    });
}

// ── Persistent-Luau end-to-end ──────────────────────────────────────

#[test]
fn app_main_luau_registers_component_whose_render_dispatches_through_runtime() {
    // The full persistent-Luau chain: an app's `[entry] script`
    // points at a `main.luau` body that calls
    // `prism.app:register_component({render = ...})`. At boot,
    // Shell::new builds a LuauRuntime, runs the script against it,
    // drains the registrar's component queue into the live
    // ShellComponentRegistry with the runtime attached, and the
    // resulting `LuauComponentBlock::lower_ui` dispatches through
    // the retained closure.
    let manifests: &[(&str, &str)] = &[(
        "lattice",
        r#"id = "lattice"
label = "Lattice"

[entry]
skeleton = "shell.prism-ui"
script = "main.luau"
"#,
    )];
    with_apps_dir("persistent_luau_render", manifests, || {
        let root = std::env::var("PRISM_APPS_DIR").unwrap();
        let dir = std::path::Path::new(&root).join("lattice");
        // The skeleton hosts a custom `<my.card name="Hi"/>` tag —
        // the Luau-registered component the script provides.
        std::fs::write(
            dir.join("shell.prism-ui"),
            r#"<my.card id="card-1" name="Hi"/>"#,
        )
        .unwrap();
        // The boot script registers `my.card` with a render body
        // that returns a `<section data-role="card">` containing
        // the `name` prop. Verifies the props flow from skeleton →
        // node.props → JSON → Lua table → script return →
        // VirtualNode → UiNode end-to-end.
        std::fs::write(
            dir.join("main.luau"),
            r#"
                prism.app:register_component({
                    id = "my.card",
                    render = function(props, _children)
                        return prism.element("section",
                            { ["data-name"] = props.name },
                            { props.name })
                    end,
                })
            "#,
        )
        .unwrap();

        let shell = prism_shell::Shell::new().expect("Shell::new");
        {
            let mut inner = shell.inner.borrow_mut();
            inner.state.workspace.active_app = Some("lattice".to_string());
        }
        let tree = shell.render();

        // Walk the rendered tree looking for a container whose
        // semantic carries `data-name="Hi"` (the script wrote that
        // attr from props.name). Its presence proves the chain
        // executed end-to-end — without the runtime the placeholder
        // body would carry `data-role="luau-component"` but no
        // `data-name`.
        fn walk_for_attr(
            nodes: &[prism_ui_runtime::layout::Node],
            attr_k: &str,
            attr_v: &str,
            found: &mut bool,
        ) {
            use prism_ui_runtime::layout::Node as UiNode;
            if *found {
                return;
            }
            for n in nodes {
                if *found {
                    return;
                }
                if let UiNode::Container {
                    props, children, ..
                } = n
                {
                    for (k, v) in &props.semantic.attrs {
                        if k == attr_k && v == attr_v {
                            *found = true;
                            return;
                        }
                    }
                    walk_for_attr(children, attr_k, attr_v, found);
                }
            }
        }
        let mut found = false;
        walk_for_attr(&tree, "data-name", "Hi", &mut found);
        assert!(
            found,
            "expected rendered tree to carry script-emitted data-name=\"Hi\" \
             — Luau dispatch did not flow through end-to-end"
        );
    });
}

#[test]
fn app_main_luau_with_no_render_fn_falls_back_to_placeholder() {
    // Persistent-Luau degrades gracefully: a script that registers a
    // component *without* an inline `render` fn (i.e. only the spec
    // table) gets the labelled placeholder, same as a NoopAppRegistrar
    // would. Proves the fallback path is reachable.
    let manifests: &[(&str, &str)] = &[(
        "lattice",
        r#"id = "lattice"
label = "Lattice"

[entry]
skeleton = "shell.prism-ui"
script = "main.luau"
"#,
    )];
    with_apps_dir("persistent_luau_placeholder", manifests, || {
        let root = std::env::var("PRISM_APPS_DIR").unwrap();
        let dir = std::path::Path::new(&root).join("lattice");
        std::fs::write(dir.join("shell.prism-ui"), r#"<my.card id="card-1"/>"#).unwrap();
        std::fs::write(
            dir.join("main.luau"),
            r#"prism.app:register_component({ id = "my.card" })"#,
        )
        .unwrap();

        let shell = prism_shell::Shell::new().expect("Shell::new");
        {
            let mut inner = shell.inner.borrow_mut();
            inner.state.workspace.active_app = Some("lattice".to_string());
        }
        let tree = shell.render();
        fn walk_for_attr(
            nodes: &[prism_ui_runtime::layout::Node],
            attr_k: &str,
            attr_v: &str,
            found: &mut bool,
        ) {
            use prism_ui_runtime::layout::Node as UiNode;
            if *found {
                return;
            }
            for n in nodes {
                if *found {
                    return;
                }
                if let UiNode::Container {
                    props, children, ..
                } = n
                {
                    for (k, v) in &props.semantic.attrs {
                        if k == attr_k && v == attr_v {
                            *found = true;
                            return;
                        }
                    }
                    walk_for_attr(children, attr_k, attr_v, found);
                }
            }
        }
        let mut found_placeholder = false;
        walk_for_attr(&tree, "data-role", "luau-component", &mut found_placeholder);
        assert!(
            found_placeholder,
            "expected placeholder body for component without render fn"
        );
    });
}

#[test]
fn app_main_luau_registers_service_dispatching_on_event() {
    // Persistent-Luau service path: the script registers a service
    // with an inline `on_event` body. When the shell's event router
    // calls `on_event`, the retained closure runs and its return
    // value decodes into the matching EventOutcome. Drives the
    // service directly through the registry since the event router
    // isn't trivial to invoke from a test harness.
    use prism_ui_runtime::event::{Event, Modifiers};
    use prism_ui_runtime::layout::Viewport;

    let manifests: &[(&str, &str)] = &[(
        "lattice",
        r#"id = "lattice"
label = "Lattice"

[entry]
script = "main.luau"
"#,
    )];
    with_apps_dir("persistent_luau_on_event", manifests, || {
        let root = std::env::var("PRISM_APPS_DIR").unwrap();
        let dir = std::path::Path::new(&root).join("lattice");
        std::fs::write(
            dir.join("main.luau"),
            r#"
                prism.app:register_service({
                    id = "lattice.click-catcher",
                    on_event = function(event)
                        if event.kind == "PointerDown" then
                            return "Handled"
                        end
                        return "Pass"
                    end,
                })
            "#,
        )
        .unwrap();

        let shell = prism_shell::Shell::new().expect("Shell::new");
        let inner = shell.inner.borrow();
        let svc = inner
            .services
            .get("lattice.click-catcher")
            .expect("service registered");
        // Drive `on_event` directly with a synthetic event. The
        // service's return shapes the EventOutcome the router would
        // see if dispatched live.
        drop(inner);
        let mut bind = shell.inner.borrow_mut();
        let mut state = std::mem::take(&mut bind.state);
        let mut undo = std::mem::take(&mut bind.undo);
        let mut vfs: Box<dyn prism_shell::services::Vfs> =
            std::mem::replace(&mut bind.vfs, Box::new(prism_shell::services::OsVfs));
        let mut luau: Box<dyn prism_shell::services::LuauHost> = std::mem::replace(
            &mut bind.luau,
            Box::new(prism_shell::services::NoopLuauHost::default()),
        );
        let mut clipboard = std::mem::take(&mut bind.clipboard);
        let cmds = bind.services.commands();
        let mut ctx = prism_shell::services::MutCtx {
            state: &mut state,
            viewport: Viewport {
                width: 1.0,
                height: 1.0,
            },
            undo: &mut undo,
            vfs: vfs.as_mut(),
            luau: luau.as_mut(),
            clipboard: &mut clipboard,
            registry: None,
            modifier_registry: None,
        };
        // The script's `on_event` returns "Handled" only for
        // `kind == "PointerDown"` — the projection in
        // `event_to_json` carries that name on PointerDown events.
        let handled = svc.on_event(
            &Event::PointerDown {
                x: 0.0,
                y: 0.0,
                button: prism_ui_runtime::event::PointerButton::Primary,
                modifiers: Modifiers::default(),
            },
            &mut ctx,
            cmds,
        );
        assert_eq!(handled, prism_shell::services::EventOutcome::Handled);
        // Wheel events fall through to "Pass" — the script's
        // explicit else branch.
        let passed = svc.on_event(&Event::Wheel { dx: 0.0, dy: 0.0 }, &mut ctx, cmds);
        assert_eq!(passed, prism_shell::services::EventOutcome::Pass);
    });
}

#[test]
fn musica_main_luau_drives_full_render_chain() {
    // End-to-end against the on-disk Musica artefacts. The script
    // registers three `musica.*` components; the skeleton references
    // each one with authored props. Shell::new boots the runtime,
    // loads the script, and the render walk dispatches every
    // component through the script's `render(props, _)` body. The
    // resulting tree carries script-emitted attrs like
    // `data-app="musica"` / `data-bpm="120"`, so the test asserts
    // the chain ran by walking for those markers.
    let manifests: &[(&str, &str)] = &[(
        "musica",
        r#"id = "musica"
label = "Musica"

[entry]
skeleton = "shell.prism-ui"
script = "main.luau"
"#,
    )];
    let workspace_root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("workspace root")
        .to_path_buf();
    let real_skeleton =
        std::fs::read_to_string(workspace_root.join("apps/musica/shell.prism-ui")).unwrap();
    let real_script =
        std::fs::read_to_string(workspace_root.join("apps/musica/main.luau")).unwrap();

    with_apps_dir("musica_full_chain", manifests, || {
        let root = std::env::var("PRISM_APPS_DIR").unwrap();
        let dir = std::path::Path::new(&root).join("musica");
        std::fs::write(dir.join("shell.prism-ui"), &real_skeleton).unwrap();
        std::fs::write(dir.join("main.luau"), &real_script).unwrap();

        let shell = prism_shell::Shell::new().expect("Shell::new");
        {
            let mut inner = shell.inner.borrow_mut();
            inner.state.workspace.active_app = Some("musica".to_string());
        }
        let tree = shell.render();

        fn walk_count_attr(
            nodes: &[prism_ui_runtime::layout::Node],
            attr_k: &str,
            attr_v: &str,
            count: &mut usize,
        ) {
            use prism_ui_runtime::layout::Node as UiNode;
            for n in nodes {
                if let UiNode::Container {
                    props, children, ..
                } = n
                {
                    for (k, v) in &props.semantic.attrs {
                        if k == attr_k && v == attr_v {
                            *count += 1;
                            break;
                        }
                    }
                    walk_count_attr(children, attr_k, attr_v, count);
                }
            }
        }
        let mut musica_hits = 0;
        walk_count_attr(&tree, "data-app", "musica", &mut musica_hits);
        assert!(
            musica_hits >= 3,
            "expected >=3 musica components in render tree, got {musica_hits}"
        );

        fn walk_seek(
            nodes: &[prism_ui_runtime::layout::Node],
            attr: &str,
            value_prefix: &str,
            seen: &mut bool,
        ) {
            use prism_ui_runtime::layout::Node as UiNode;
            for n in nodes {
                if *seen {
                    return;
                }
                if let UiNode::Container {
                    props, children, ..
                } = n
                {
                    for (k, v) in &props.semantic.attrs {
                        if k == attr && v.starts_with(value_prefix) {
                            *seen = true;
                            return;
                        }
                    }
                    walk_seek(children, attr, value_prefix, seen);
                }
            }
        }
        let mut bpm_seen = false;
        walk_seek(&tree, "data-bpm", "120", &mut bpm_seen);
        assert!(
            bpm_seen,
            "expected data-bpm=120 from musica.transport render"
        );
    });
}

#[test]
fn flux_main_luau_renders_nested_canvas_with_nodes() {
    let manifests: &[(&str, &str)] = &[(
        "flux",
        r#"id = "flux"
label = "Flux"

[entry]
skeleton = "shell.prism-ui"
script = "main.luau"
"#,
    )];
    let workspace_root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("workspace root")
        .to_path_buf();
    let real_skeleton =
        std::fs::read_to_string(workspace_root.join("apps/flux/shell.prism-ui")).unwrap();
    let real_script = std::fs::read_to_string(workspace_root.join("apps/flux/main.luau")).unwrap();

    with_apps_dir("flux_full_chain", manifests, || {
        let root = std::env::var("PRISM_APPS_DIR").unwrap();
        let dir = std::path::Path::new(&root).join("flux");
        std::fs::write(dir.join("shell.prism-ui"), &real_skeleton).unwrap();
        std::fs::write(dir.join("main.luau"), &real_script).unwrap();

        let shell = prism_shell::Shell::new().expect("Shell::new");
        {
            let mut inner = shell.inner.borrow_mut();
            inner.state.workspace.active_app = Some("flux".to_string());
        }
        let tree = shell.render();

        fn walk_for_attr_value(
            nodes: &[prism_ui_runtime::layout::Node],
            attr: &str,
            value: &str,
            seen: &mut bool,
        ) {
            use prism_ui_runtime::layout::Node as UiNode;
            for n in nodes {
                if *seen {
                    return;
                }
                if let UiNode::Container {
                    props, children, ..
                } = n
                {
                    for (k, v) in &props.semantic.attrs {
                        if k == attr && v == value {
                            *seen = true;
                            return;
                        }
                    }
                    walk_for_attr_value(children, attr, value, seen);
                }
            }
        }
        let mut canvas_seen = false;
        walk_for_attr_value(&tree, "data-block", "canvas", &mut canvas_seen);
        assert!(canvas_seen, "expected flux.canvas to render");
        let mut source_seen = false;
        walk_for_attr_value(&tree, "data-kind", "source", &mut source_seen);
        assert!(source_seen, "expected flux.node[kind=source] to render");
        let mut sink_seen = false;
        walk_for_attr_value(&tree, "data-kind", "sink", &mut sink_seen);
        assert!(sink_seen, "expected flux.node[kind=sink] to render");
    });
}

#[test]
fn install_app_script_hot_swaps_render_body() {
    // Persistent-Luau hot-reload: after Shell::new boots with one
    // render body, a second `install_app_script` call against the
    // same id swaps the retained closure. The next render walks
    // the new body — proving the dev-loop file-watcher contract
    // works end-to-end (re-run script → re-render with the swap).
    let manifests: &[(&str, &str)] = &[(
        "lattice",
        r#"id = "lattice"
label = "Lattice"

[entry]
skeleton = "shell.prism-ui"
script = "main.luau"
"#,
    )];
    with_apps_dir("hot_reload_swap", manifests, || {
        let root = std::env::var("PRISM_APPS_DIR").unwrap();
        let dir = std::path::Path::new(&root).join("lattice");
        std::fs::write(dir.join("shell.prism-ui"), r#"<my.greet id="card-1"/>"#).unwrap();
        std::fs::write(
            dir.join("main.luau"),
            r#"
                prism.app:register_component({
                    id = "my.greet",
                    render = function(_p, _c)
                        return prism.element("section",
                            { ["data-v"] = "v1" },
                            { "hello v1" })
                    end,
                })
            "#,
        )
        .unwrap();

        let shell = prism_shell::Shell::new().expect("Shell::new");
        {
            let mut inner = shell.inner.borrow_mut();
            inner.state.workspace.active_app = Some("lattice".to_string());
        }

        // Initial render — v1 body.
        let tree_v1 = shell.render();
        fn walk_seek(
            nodes: &[prism_ui_runtime::layout::Node],
            attr: &str,
            value: &str,
            seen: &mut bool,
        ) {
            use prism_ui_runtime::layout::Node as UiNode;
            for n in nodes {
                if *seen {
                    return;
                }
                if let UiNode::Container {
                    props, children, ..
                } = n
                {
                    for (k, v) in &props.semantic.attrs {
                        if k == attr && v == value {
                            *seen = true;
                            return;
                        }
                    }
                    walk_seek(children, attr, value, seen);
                }
            }
        }
        let mut v1_seen = false;
        walk_seek(&tree_v1, "data-v", "v1", &mut v1_seen);
        assert!(v1_seen, "expected initial v1 render");

        // Hot-swap: install a new script body for the same id.
        let v2_script = r#"
            prism.app:register_component({
                id = "my.greet",
                render = function(_p, _c)
                    return prism.element("section",
                        { ["data-v"] = "v2" },
                        { "hello v2" })
                end,
            })
        "#;
        shell
            .install_app_script("lattice", v2_script)
            .expect("install_app_script");

        // Re-render — v2 body now in effect.
        let tree_v2 = shell.render();
        let mut v2_seen = false;
        walk_seek(&tree_v2, "data-v", "v2", &mut v2_seen);
        assert!(v2_seen, "expected v2 render after install_app_script");
        // And v1 should be gone — the new closure replaced the old.
        let mut v1_after = false;
        walk_seek(&tree_v2, "data-v", "v1", &mut v1_after);
        assert!(!v1_after, "expected v1 to be replaced, not coexist");
    });
}

#[test]
fn install_app_script_errors_surface_without_corrupting_state() {
    // Hot-reload failure path: a script body with a syntax / runtime
    // error must surface as Err and leave the prior registration
    // intact. The next render still emits the v1 body.
    let manifests: &[(&str, &str)] = &[(
        "lattice",
        r#"id = "lattice"
label = "Lattice"

[entry]
skeleton = "shell.prism-ui"
script = "main.luau"
"#,
    )];
    with_apps_dir("hot_reload_err", manifests, || {
        let root = std::env::var("PRISM_APPS_DIR").unwrap();
        let dir = std::path::Path::new(&root).join("lattice");
        std::fs::write(dir.join("shell.prism-ui"), r#"<my.greet id="card-1"/>"#).unwrap();
        std::fs::write(
            dir.join("main.luau"),
            r#"
                prism.app:register_component({
                    id = "my.greet",
                    render = function(_p, _c)
                        return prism.element("section",
                            { ["data-v"] = "v1" }, { "v1" })
                    end,
                })
            "#,
        )
        .unwrap();
        let shell = prism_shell::Shell::new().expect("Shell::new");
        {
            let mut inner = shell.inner.borrow_mut();
            inner.state.workspace.active_app = Some("lattice".to_string());
        }
        // Broken script: missing `end`, ill-formed function.
        let bad = r#"
            prism.app:register_component({
                id = "my.greet",
                render = function(_p _c) return "broken" end,
            })
        "#;
        let err = shell.install_app_script("lattice", bad).unwrap_err();
        assert!(
            !err.is_empty(),
            "expected error message from failed install"
        );
        // Render still surfaces the v1 body.
        let tree = shell.render();
        fn walk_seek(
            nodes: &[prism_ui_runtime::layout::Node],
            attr: &str,
            value: &str,
            seen: &mut bool,
        ) {
            use prism_ui_runtime::layout::Node as UiNode;
            for n in nodes {
                if *seen {
                    return;
                }
                if let UiNode::Container {
                    props, children, ..
                } = n
                {
                    for (k, v) in &props.semantic.attrs {
                        if k == attr && v == value {
                            *seen = true;
                            return;
                        }
                    }
                    walk_seek(children, attr, value, seen);
                }
            }
        }
        let mut v1_seen = false;
        walk_seek(&tree, "data-v", "v1", &mut v1_seen);
        assert!(v1_seen, "v1 must survive a failed install");
    });
}

#[test]
fn luau_component_children_render_through_prism_slot() {
    // The persistent-Luau children-projection chain:
    //   1. The skeleton authors `<my.box><my.inner ... /></my.box>`
    //   2. `my.box`'s render reads `children` (descriptors) + emits
    //      `prism.slot(0)` for each child it wants laid out.
    //   3. The shell pre-lowers each skeleton-authored child and
    //      substitutes them for the slot markers during VirtualNode →
    //      UiNode translation.
    // The rendered tree therefore carries *both* `my.box`'s own attrs
    // and the inner component's `data-luau-key` — proving the
    // pre-lowered child flowed through.
    let manifests: &[(&str, &str)] = &[(
        "lattice",
        r#"id = "lattice"
label = "Lattice"

[entry]
skeleton = "shell.prism-ui"
script = "main.luau"
"#,
    )];
    with_apps_dir("children_projection", manifests, || {
        let root = std::env::var("PRISM_APPS_DIR").unwrap();
        let dir = std::path::Path::new(&root).join("lattice");
        std::fs::write(
            dir.join("shell.prism-ui"),
            r#"<my.box id="b1">
                <my.inner id="i1" label="alpha"/>
                <my.inner id="i2" label="beta"/>
            </my.box>"#,
        )
        .unwrap();
        std::fs::write(
            dir.join("main.luau"),
            r#"
                prism.app:register_component({
                    id = "my.box",
                    render = function(_props, children)
                        -- Emit one slot per declared child.
                        local slots = {}
                        for i = 1, #children do
                            table.insert(slots, prism.slot(i - 1))
                        end
                        return prism.element("section",
                            { ["data-role-extra"] = "box" },
                            slots)
                    end,
                })
                prism.app:register_component({
                    id = "my.inner",
                    render = function(props, _)
                        return prism.element("article",
                            { ["data-inner-label"] = props.label or "" },
                            { props.label or "" })
                    end,
                })
            "#,
        )
        .unwrap();

        let shell = prism_shell::Shell::new().expect("Shell::new");
        {
            let mut inner = shell.inner.borrow_mut();
            inner.state.workspace.active_app = Some("lattice".to_string());
        }
        let tree = shell.render();

        // The box's data-role-extra and both inner labels survive
        // through the slot substitution.
        fn walk_for_attr_value(
            nodes: &[prism_ui_runtime::layout::Node],
            attr: &str,
            value: &str,
            seen: &mut bool,
        ) {
            use prism_ui_runtime::layout::Node as UiNode;
            for n in nodes {
                if *seen {
                    return;
                }
                if let UiNode::Container {
                    props, children, ..
                } = n
                {
                    for (k, v) in &props.semantic.attrs {
                        if k == attr && v == value {
                            *seen = true;
                            return;
                        }
                    }
                    walk_for_attr_value(children, attr, value, seen);
                }
            }
        }
        let mut box_seen = false;
        walk_for_attr_value(&tree, "data-role-extra", "box", &mut box_seen);
        assert!(box_seen, "expected my.box render to surface");
        let mut alpha_seen = false;
        walk_for_attr_value(&tree, "data-inner-label", "alpha", &mut alpha_seen);
        assert!(
            alpha_seen,
            "expected first child slot to render with label=alpha"
        );
        let mut beta_seen = false;
        walk_for_attr_value(&tree, "data-inner-label", "beta", &mut beta_seen);
        assert!(
            beta_seen,
            "expected second child slot to render with label=beta"
        );
    });
}

#[test]
fn luau_slot_out_of_range_renders_oob_placeholder() {
    // Defensive contract: a script asking for `prism.slot(99)` when
    // only one child was authored shouldn't panic — the renderer
    // emits a labelled `data-role="luau-slot-oob"` empty container
    // so authors can spot + fix the bad index.
    let manifests: &[(&str, &str)] = &[(
        "lattice",
        r#"id = "lattice"
label = "Lattice"

[entry]
skeleton = "shell.prism-ui"
script = "main.luau"
"#,
    )];
    with_apps_dir("slot_oob", manifests, || {
        let root = std::env::var("PRISM_APPS_DIR").unwrap();
        let dir = std::path::Path::new(&root).join("lattice");
        std::fs::write(dir.join("shell.prism-ui"), r#"<my.box id="b1"/>"#).unwrap();
        std::fs::write(
            dir.join("main.luau"),
            r#"
                prism.app:register_component({
                    id = "my.box",
                    render = function(_p, _c)
                        return prism.element("section", nil, { prism.slot(99) })
                    end,
                })
            "#,
        )
        .unwrap();
        let shell = prism_shell::Shell::new().expect("Shell::new");
        {
            let mut inner = shell.inner.borrow_mut();
            inner.state.workspace.active_app = Some("lattice".to_string());
        }
        let tree = shell.render();
        fn walk_for_attr_value(
            nodes: &[prism_ui_runtime::layout::Node],
            attr: &str,
            value: &str,
            seen: &mut bool,
        ) {
            use prism_ui_runtime::layout::Node as UiNode;
            for n in nodes {
                if *seen {
                    return;
                }
                if let UiNode::Container {
                    props, children, ..
                } = n
                {
                    for (k, v) in &props.semantic.attrs {
                        if k == attr && v == value {
                            *seen = true;
                            return;
                        }
                    }
                    walk_for_attr_value(children, attr, value, seen);
                }
            }
        }
        let mut oob_seen = false;
        walk_for_attr_value(&tree, "data-role", "luau-slot-oob", &mut oob_seen);
        assert!(
            oob_seen,
            "out-of-range slot should produce a labelled empty"
        );
    });
}

#[test]
fn luau_script_watcher_drives_install_app_script_end_to_end() {
    // The dev-loop hot-reload contract: a file watcher (real impl
    // ships in `app_loader::LuauScriptWatcher`) sees `main.luau`
    // change on disk and hands the new source to
    // `Shell::install_app_script`. The next render reflects the
    // re-run script's body. Drives both the watcher and the shell
    // through one tick to prove the contract end-to-end.
    use prism_shell::app_loader::{LuauScriptChange, LuauScriptWatcher};

    let manifests: &[(&str, &str)] = &[(
        "lattice",
        r#"id = "lattice"
label = "Lattice"

[entry]
skeleton = "shell.prism-ui"
script = "main.luau"
"#,
    )];
    with_apps_dir("watcher_e2e", manifests, || {
        let root = std::env::var("PRISM_APPS_DIR").unwrap();
        let dir = std::path::Path::new(&root).join("lattice");
        let script_path = dir.join("main.luau");
        std::fs::write(dir.join("shell.prism-ui"), r#"<my.greet id="g-1"/>"#).unwrap();
        std::fs::write(
            &script_path,
            r#"
                prism.app:register_component({
                    id = "my.greet",
                    render = function(_p, _c)
                        return prism.element("section",
                            { ["data-version"] = "boot" },
                            { "boot" })
                    end,
                })
            "#,
        )
        .unwrap();

        let shell = prism_shell::Shell::new().expect("Shell::new");
        {
            let mut inner = shell.inner.borrow_mut();
            inner.state.workspace.active_app = Some("lattice".to_string());
        }
        let boot = shell.render();
        fn walk(
            nodes: &[prism_ui_runtime::layout::Node],
            attr: &str,
            value: &str,
            seen: &mut bool,
        ) {
            use prism_ui_runtime::layout::Node as UiNode;
            for n in nodes {
                if *seen {
                    return;
                }
                if let UiNode::Container {
                    props, children, ..
                } = n
                {
                    for (k, v) in &props.semantic.attrs {
                        if k == attr && v == value {
                            *seen = true;
                            return;
                        }
                    }
                    walk(children, attr, value, seen);
                }
            }
        }
        let mut boot_seen = false;
        walk(&boot, "data-version", "boot", &mut boot_seen);
        assert!(boot_seen, "boot version should render");

        // Prime the watcher (matches the dev-loop boot seam: it
        // catches up to the on-disk state once before entering its
        // poll loop, so the first poll is a FirstSighting).
        let mut watcher = LuauScriptWatcher::new();
        match watcher.observe("lattice", &script_path) {
            LuauScriptChange::FirstSighting { .. } => {}
            other => panic!("expected FirstSighting, got {other:?}"),
        }

        // Edit the file as a developer would.
        std::fs::write(
            &script_path,
            r#"
                prism.app:register_component({
                    id = "my.greet",
                    render = function(_p, _c)
                        return prism.element("section",
                            { ["data-version"] = "hotreload" },
                            { "hotreload" })
                    end,
                })
            "#,
        )
        .unwrap();

        // Dev-loop tick: watcher detects change; host hands the new
        // source to the shell.
        let new_source = match watcher.observe("lattice", &script_path) {
            LuauScriptChange::Changed { source } => source,
            other => panic!("expected Changed, got {other:?}"),
        };
        shell
            .install_app_script("lattice", &new_source)
            .expect("install_app_script");
        let hot = shell.render();
        let mut hot_seen = false;
        walk(&hot, "data-version", "hotreload", &mut hot_seen);
        assert!(hot_seen, "hot-reloaded version should render");
        // Steady-state poll: no further change.
        assert!(matches!(
            watcher.observe("lattice", &script_path),
            LuauScriptChange::NoChange
        ));
    });
}

#[test]
fn install_app_script_hot_swaps_service_without_duplicate_id_panic() {
    // The persistent-Luau hot-reload chain has to survive re-installs
    // of the same service id. ServiceRegistry's standard install
    // asserts no duplicates — `install_services_replace` drops the
    // prior entry before re-installing. This test runs the same
    // script twice and reads the post-install service to confirm
    // it's the new instance.
    let manifests: &[(&str, &str)] = &[(
        "lattice",
        r#"id = "lattice"
label = "Lattice"

[entry]
script = "main.luau"
"#,
    )];
    with_apps_dir("svc_hot_reload", manifests, || {
        let root = std::env::var("PRISM_APPS_DIR").unwrap();
        let dir = std::path::Path::new(&root).join("lattice");
        std::fs::write(
            dir.join("main.luau"),
            r#"
                prism.app:register_service({
                    id = "lattice.svc",
                    on_event = function(event)
                        if event.kind == "Wheel" then return "Handled" end
                        return "Pass"
                    end,
                })
            "#,
        )
        .unwrap();
        let shell = prism_shell::Shell::new().expect("Shell::new");
        // Boot installed the v1 body: Wheel → Handled.
        {
            let inner = shell.inner.borrow();
            assert!(
                inner.services.get("lattice.svc").is_some(),
                "v1 service should be installed at boot"
            );
        }
        // Re-install with the inverse policy: Wheel → Pass, PointerDown → Handled.
        let v2 = r#"
            prism.app:register_service({
                id = "lattice.svc",
                on_event = function(event)
                    if event.kind == "PointerDown" then return "Handled" end
                    return "Pass"
                end,
            })
        "#;
        shell.install_app_script("lattice", v2).expect("hot-reload");
        // Service still present (with same id) and dispatches v2 body.
        let inner = shell.inner.borrow();
        let svc = inner.services.get("lattice.svc").expect("v2 service");
        drop(inner);
        let mut bind = shell.inner.borrow_mut();
        let mut state = std::mem::take(&mut bind.state);
        let mut undo = std::mem::take(&mut bind.undo);
        let mut vfs: Box<dyn prism_shell::services::Vfs> =
            std::mem::replace(&mut bind.vfs, Box::new(prism_shell::services::OsVfs));
        let mut luau: Box<dyn prism_shell::services::LuauHost> = std::mem::replace(
            &mut bind.luau,
            Box::new(prism_shell::services::NoopLuauHost::default()),
        );
        let mut clipboard = std::mem::take(&mut bind.clipboard);
        let cmds = bind.services.commands();
        let mut ctx = prism_shell::services::MutCtx {
            state: &mut state,
            viewport: prism_ui_runtime::layout::Viewport {
                width: 1.0,
                height: 1.0,
            },
            undo: &mut undo,
            vfs: vfs.as_mut(),
            luau: luau.as_mut(),
            clipboard: &mut clipboard,
            registry: None,
            modifier_registry: None,
        };
        // v2 contract: PointerDown→Handled, Wheel→Pass (inverse of v1).
        let p_outcome = svc.on_event(
            &prism_ui_runtime::event::Event::PointerDown {
                x: 0.0,
                y: 0.0,
                button: prism_ui_runtime::event::PointerButton::Primary,
                modifiers: prism_ui_runtime::event::Modifiers::default(),
            },
            &mut ctx,
            cmds,
        );
        assert_eq!(p_outcome, prism_shell::services::EventOutcome::Handled);
        let w_outcome = svc.on_event(
            &prism_ui_runtime::event::Event::Wheel { dx: 0.0, dy: 0.0 },
            &mut ctx,
            cmds,
        );
        assert_eq!(w_outcome, prism_shell::services::EventOutcome::Pass);
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
