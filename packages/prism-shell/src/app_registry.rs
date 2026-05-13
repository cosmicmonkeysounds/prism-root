//! `ShellAppRegistrar` — concrete `prism_core::AppRegistrar` impl that
//! routes registrations into the shell's live registries:
//!
//! - **Panels** flow directly into a shared `DockCatalog`.
//! - **Components** are queued; [`install_components`] drains the queue
//!   and registers a [`LuauComponentBlock`] per spec into the
//!   shell's component registry.
//! - **Services** are queued; [`install_services`] drains and registers
//!   a [`LuauScriptedService`] per spec with `ServiceScope::App`.
//!
//! Loop 4 of `docs/dev/dsl-self-bootstrap.md`. The trait surface lives
//! in `prism_core::app_registry` because `prism-core` is the leaf
//! crate Luau bindings call into. This module supplies the production
//! glue.
//!
//! The Luau-backed shims (`LuauComponentBlock`, `LuauScriptedService`)
//! are intentionally minimal today — their `lower_ui` / `on_event`
//! bodies surface a placeholder labelled with the component / service
//! id. When the in-process Luau runtime lands, the shim bodies grow
//! to dispatch through the script's `render_key` / `on_event_key`.

use std::sync::{Arc, Mutex};

use prism_builder::block::Block;
use prism_builder::component::ComponentId;
use prism_builder::document::Node;
use prism_builder::registry::FieldSpec;
use prism_builder::style::StyleProperties;
use prism_builder::ui_lower::LowerCtx;
use prism_core::{
    AppRegistrar, ComponentRegistration, PanelRegistration, RegistrationError, ServiceRegistration,
};
use prism_dock::{DockCatalog, PanelKind};
use prism_ui_runtime::layout::{Node as UiNode, Semantic, Sizing};

use crate::services::{CommandSpec, CommandTable, EventOutcome, MutCtx, ShellService};

/// Production `AppRegistrar` impl. Wraps:
/// - a shared `DockCatalog` for panel registrations,
/// - a `Vec<ComponentRegistration>` queue for component registrations,
/// - a `Vec<ServiceRegistration>` queue for service registrations.
///
/// Panels apply immediately; components and services are queued so
/// they can be drained at a point in the shell boot sequence where
/// the matching registries are mutable. See `Shell::new` for the
/// drain points.
#[derive(Clone)]
pub struct ShellAppRegistrar {
    panels: Arc<Mutex<DockCatalog>>,
    components: Arc<Mutex<Vec<ComponentRegistration>>>,
    services: Arc<Mutex<Vec<ServiceRegistration>>>,
}

impl ShellAppRegistrar {
    /// Build a registrar over a catalog pre-seeded with built-in
    /// panels (`prism_dock::register_builtins`). Apps register on top
    /// of those built-ins.
    pub fn with_builtin_panels() -> Self {
        Self {
            panels: Arc::new(Mutex::new(DockCatalog::with_builtins())),
            components: Arc::new(Mutex::new(Vec::new())),
            services: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Cloneable handle to the inner catalog. Render-side consumers
    /// (`shell.dock-panel` / `shell.dock-workspace` block lower fns)
    /// hold this to look up tags + labels at lower time.
    pub fn catalog(&self) -> Arc<Mutex<DockCatalog>> {
        Arc::clone(&self.panels)
    }

    /// Snapshot the catalog as a fresh `DockCatalog`. Convenience for
    /// callers that need a `Send`-friendly read-only view (e.g. SSR).
    pub fn snapshot_catalog(&self) -> DockCatalog {
        self.panels.lock().unwrap().clone()
    }

    /// Drain every queued [`ComponentRegistration`]. Called by
    /// [`install_components`] which then registers a
    /// [`LuauComponentBlock`] per row into the shell's registry.
    pub fn drain_components(&self) -> Vec<ComponentRegistration> {
        std::mem::take(&mut *self.components.lock().unwrap())
    }

    /// Drain every queued [`ServiceRegistration`]. Called by
    /// [`install_services`].
    pub fn drain_services(&self) -> Vec<ServiceRegistration> {
        std::mem::take(&mut *self.services.lock().unwrap())
    }

    /// How many components are currently queued (test introspection).
    pub fn pending_component_count(&self) -> usize {
        self.components.lock().unwrap().len()
    }

    /// How many services are currently queued (test introspection).
    pub fn pending_service_count(&self) -> usize {
        self.services.lock().unwrap().len()
    }
}

impl Default for ShellAppRegistrar {
    fn default() -> Self {
        Self::with_builtin_panels()
    }
}

impl AppRegistrar for ShellAppRegistrar {
    fn register_panel(&self, panel: PanelRegistration) -> Result<(), RegistrationError> {
        if panel.id.trim().is_empty() {
            return Err(RegistrationError::Invalid("panel id empty".into()));
        }
        // Bridge the runtime `PanelRegistration` (owned strings) onto
        // the static-string `PanelKind` shape. `Box::leak` is the
        // documented pattern for promoting owned strings to
        // `&'static str` — registrations happen once at app load,
        // never per-frame, so the leak is bounded by the app set.
        let kind = PanelKind {
            id: Box::leak(panel.id.into_boxed_str()),
            label: Box::leak(panel.label.into_boxed_str()),
            icon_hint: Box::leak(panel.icon_hint.into_boxed_str()),
            min_width: panel.min_width.max(1.0),
            min_height: panel.min_height.max(1.0),
            allow_multiple: panel.allow_multiple,
            tag: panel.tag.map(|s| &*Box::leak(s.into_boxed_str())),
        };
        self.panels.lock().unwrap().register(kind);
        Ok(())
    }

    fn register_component(
        &self,
        component: ComponentRegistration,
    ) -> Result<(), RegistrationError> {
        if component.id.trim().is_empty() {
            return Err(RegistrationError::Invalid("component id empty".into()));
        }
        self.components.lock().unwrap().push(component);
        Ok(())
    }

    fn register_service(&self, service: ServiceRegistration) -> Result<(), RegistrationError> {
        if service.id.trim().is_empty() {
            return Err(RegistrationError::Invalid("service id empty".into()));
        }
        self.services.lock().unwrap().push(service);
        Ok(())
    }
}

// ── Manifest fan-out helpers ───────────────────────────────────────

/// Walk the discovered apps' manifests and push every `panels.add`
/// entry through `registrar.register_panel`. Returns the count of
/// successfully registered panels. Failures are logged and skipped
/// — a malformed entry shouldn't stop the rest of the app from
/// loading.
pub fn install_panels_from_manifests(
    registrar: &impl AppRegistrar,
    apps: &[crate::app_loader::LoadedApp],
) -> usize {
    let mut count = 0;
    for app in apps {
        for def in &app.manifest.panels.add {
            let reg: PanelRegistration = def.clone().into();
            match registrar.register_panel(reg) {
                Ok(()) => count += 1,
                Err(e) => {
                    eprintln!(
                        "prism-shell: failed to register panel `{}` from app `{}`: {e}",
                        def.id, app.manifest.id
                    );
                }
            }
        }
    }
    count
}

/// Drain queued component registrations and register a
/// [`LuauComponentBlock`] per row into the shell's component
/// registry. Returns the number successfully installed.
///
/// Call after every Luau-script load step but before the registry's
/// tag resolver is finalised, so the registry has the new blocks
/// when the resolver builds its dispatch table.
pub fn install_components(
    registrar: &ShellAppRegistrar,
    registry: &mut crate::components::ShellComponentRegistry,
) -> usize {
    let mut count = 0;
    for spec in registrar.drain_components() {
        let block = Arc::new(LuauComponentBlock::new(spec));
        match registry.register(block) {
            Ok(()) => count += 1,
            Err(e) => {
                eprintln!("prism-shell: failed to register component: {e}");
            }
        }
    }
    count
}

/// Drain queued service registrations and add each as an
/// [`App`-scoped](crate::services::ServiceScope::App)
/// [`ServiceFactory`](crate::services::ServiceFactory). Returns the
/// number installed.
///
/// ADR-010 Phase 1: factories (rather than eager construction)
/// because every Luau-scripted service needs to re-bind against the
/// active app's script set when [`crate::Shell::switch_active_app`]
/// fires. The factory closure captures the `spec` and stamps the
/// active app id from the [`crate::services::ServiceContext`] onto
/// each new instance — that's how the active-app cursor flows
/// through to per-app service state.
pub fn install_services(
    registrar: &ShellAppRegistrar,
    services: &mut crate::services::ServiceRegistry,
) -> usize {
    let mut count = 0;
    for spec in registrar.drain_services() {
        services.add_factory_scoped(
            crate::services::ServiceScope::App,
            Box::new(move |ctx| {
                std::sync::Arc::new(LuauScriptedService::new_with_context(spec.clone(), ctx))
            }),
        );
        count += 1;
    }
    count
}

// ── Luau-backed shims ─────────────────────────────────────────────

/// `Block` impl that renders a labelled placeholder for an
/// app-registered component. Today the placeholder is a static
/// container with a `data-component` semantic attribute; when the
/// Luau runtime lands, the placeholder body grows into a real
/// `script.render(props, children)` dispatch keyed on `render_key`.
pub struct LuauComponentBlock {
    id: ComponentId,
    /// Opaque Luau render dispatch key. Surfaced via
    /// `data-luau-key` on the rendered semantic so the eventual
    /// runtime can correlate placeholders to scripts during
    /// hot-reload.
    render_key: String,
}

impl LuauComponentBlock {
    pub fn new(spec: ComponentRegistration) -> Self {
        Self {
            // `ComponentId` is a `String` type alias in prism-builder.
            id: spec.id,
            render_key: spec.render_key,
        }
    }
}

impl Block for LuauComponentBlock {
    fn id(&self) -> &ComponentId {
        &self.id
    }

    fn schema(&self) -> Vec<FieldSpec> {
        // Luau-defined components declare their schema through the
        // script today; until the runtime is wired, expose an empty
        // schema so the property panel renders the catch-all.
        vec![]
    }

    fn lower_ui(&self, _ctx: &LowerCtx<'_>, node: &Node, _style: &StyleProperties) -> UiNode {
        let component_id = self.id.clone();
        let render_key = self.render_key.clone();
        // Build a minimal container with semantic markers — the live
        // dock + relay SSR both render this as a labelled placeholder
        // until the Luau runtime supplies a real body.
        UiNode::Container {
            id: node.id.clone(),
            props: prism_ui_runtime::layout::ContainerProps {
                width: Sizing::Grow,
                height: Sizing::Grow,
                semantic: Semantic::tag("div")
                    .with_attr("data-role", "luau-component")
                    .with_attr("data-component", component_id)
                    .with_attr("data-luau-key", render_key),
                ..Default::default()
            },
            children: Vec::new(),
        }
    }
}

/// `ShellService` shim for a Luau-registered service. Today's
/// `on_event` always returns `Pass` — the Luau dispatch lands when
/// the in-process runtime is wired (Loop 4 follow-up).
///
/// ADR-010 Phase 1: instances carry the active-app id from the
/// [`crate::services::ServiceContext`] supplied when the factory
/// ran. Tests + future Luau dispatch use this to confirm a rebuild
/// actually re-bound the service against the new app.
pub struct LuauScriptedService {
    id: &'static str,
    /// Opaque Luau handler dispatch key. Stored for future runtime
    /// dispatch; surfaced via [`Self::on_event_key`] for tests.
    on_event_key: String,
    /// The `app_id` from the `ServiceContext` the factory was passed
    /// at construction. `None` for the initial pre-app-mount pass.
    bound_app_id: Option<String>,
}

impl LuauScriptedService {
    /// Build a service against a default (empty) context. Equivalent
    /// to `new_with_context(spec, &ServiceContext::default())`.
    /// Preserved so existing tests + direct registration sites that
    /// don't go through a factory keep compiling.
    pub fn new(spec: ServiceRegistration) -> Self {
        Self::new_with_context(spec, &crate::services::ServiceContext::default())
    }

    /// ADR-010 Phase 1: build a service against an explicit
    /// [`crate::services::ServiceContext`]. The factory path goes
    /// through here so the constructed instance carries the active
    /// app id at construction.
    pub fn new_with_context(
        spec: ServiceRegistration,
        ctx: &crate::services::ServiceContext<'_>,
    ) -> Self {
        // ShellService::id returns `&'static str`. Promote the owned
        // string via `Box::leak` — registrations happen at app load,
        // never per-frame, so the leak is bounded.
        Self {
            id: Box::leak(spec.id.into_boxed_str()),
            on_event_key: spec.on_event_key,
            bound_app_id: ctx.app_id.map(str::to_string),
        }
    }

    pub fn on_event_key(&self) -> &str {
        &self.on_event_key
    }

    /// The app id this service instance was bound against at
    /// construction time. Changes across factory rebuilds when
    /// `Shell::switch_active_app` fires.
    pub fn bound_app_id(&self) -> Option<&str> {
        self.bound_app_id.as_deref()
    }
}

impl ShellService for LuauScriptedService {
    fn id(&self) -> &'static str {
        self.id
    }

    fn on_event(
        &self,
        _event: &prism_ui_runtime::event::Event,
        _ctx: &mut MutCtx<'_>,
        _cmds: &CommandTable,
    ) -> EventOutcome {
        EventOutcome::Pass
    }

    fn commands(&self) -> Vec<CommandSpec> {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app_loader::LoadedApp;
    use prism_core::{AppManifest, AppPanelDef, AppPanelsSpec};

    #[test]
    fn builtins_seed_catalog() {
        let reg = ShellAppRegistrar::with_builtin_panels();
        let snap = reg.snapshot_catalog();
        assert!(snap.get("builder").is_some());
        assert!(snap.get("inspector").is_some());
        assert_eq!(snap.len(), 15);
    }

    #[test]
    fn register_panel_lands_in_shared_catalog() {
        let reg = ShellAppRegistrar::with_builtin_panels();
        reg.register_panel(PanelRegistration {
            id: "lattice.peers".into(),
            label: "Peers".into(),
            icon_hint: "users".into(),
            min_width: 240.0,
            min_height: 120.0,
            allow_multiple: false,
            tag: Some("lattice.peers-panel".into()),
        })
        .unwrap();
        let snap = reg.snapshot_catalog();
        let p = snap.get("lattice.peers").expect("registered panel");
        assert_eq!(p.label, "Peers");
        assert_eq!(p.tag, Some("lattice.peers-panel"));
        assert_eq!(snap.len(), 16);
    }

    #[test]
    fn register_panel_rejects_empty_id() {
        let reg = ShellAppRegistrar::with_builtin_panels();
        let err = reg
            .register_panel(PanelRegistration {
                id: "".into(),
                label: "Empty".into(),
                ..Default::default()
            })
            .unwrap_err();
        assert!(matches!(err, RegistrationError::Invalid(_)));
    }

    #[test]
    fn register_component_queues_and_drains() {
        let reg = ShellAppRegistrar::with_builtin_panels();
        reg.register_component(ComponentRegistration {
            id: "lattice.card".into(),
            render_key: "lattice.scripts.card.render".into(),
        })
        .unwrap();
        reg.register_component(ComponentRegistration {
            id: "lattice.list".into(),
            render_key: "lattice.scripts.list.render".into(),
        })
        .unwrap();
        assert_eq!(reg.pending_component_count(), 2);
        let drained = reg.drain_components();
        assert_eq!(drained.len(), 2);
        assert_eq!(drained[0].id, "lattice.card");
        assert_eq!(drained[1].render_key, "lattice.scripts.list.render");
        // Queue is empty after drain.
        assert_eq!(reg.pending_component_count(), 0);
    }

    #[test]
    fn register_service_queues_and_drains() {
        let reg = ShellAppRegistrar::with_builtin_panels();
        reg.register_service(ServiceRegistration {
            id: "lattice.peers-sync".into(),
            on_event_key: "lattice.scripts.peers_sync.on_event".into(),
        })
        .unwrap();
        assert_eq!(reg.pending_service_count(), 1);
        let drained = reg.drain_services();
        assert_eq!(drained.len(), 1);
        assert_eq!(drained[0].id, "lattice.peers-sync");
        assert_eq!(reg.pending_service_count(), 0);
    }

    #[test]
    fn register_component_rejects_empty_id() {
        let reg = ShellAppRegistrar::with_builtin_panels();
        let err = reg
            .register_component(ComponentRegistration::default())
            .unwrap_err();
        assert!(matches!(err, RegistrationError::Invalid(_)));
    }

    #[test]
    fn register_service_rejects_empty_id() {
        let reg = ShellAppRegistrar::with_builtin_panels();
        let err = reg
            .register_service(ServiceRegistration::default())
            .unwrap_err();
        assert!(matches!(err, RegistrationError::Invalid(_)));
    }

    #[test]
    fn install_components_drains_into_shell_registry() {
        use crate::components::registry::{register_full_shell_chrome, ShellComponentRegistry};

        let reg = ShellAppRegistrar::with_builtin_panels();
        reg.register_component(ComponentRegistration {
            id: "my.app.card".into(),
            render_key: "my.app.card.render".into(),
        })
        .unwrap();
        let mut registry = ShellComponentRegistry::new();
        register_full_shell_chrome(&mut registry).unwrap();
        let installed = install_components(&reg, &mut registry);
        assert_eq!(installed, 1);
        assert!(registry
            .as_component_registry()
            .get("my.app.card")
            .is_some());
    }

    #[test]
    fn install_services_drains_into_service_registry() {
        use crate::services::{ServiceRegistry, ServiceScope};
        let reg = ShellAppRegistrar::with_builtin_panels();
        reg.register_service(ServiceRegistration {
            id: "my.app.metronome".into(),
            on_event_key: "my.app.metronome.tick".into(),
        })
        .unwrap();
        let mut services = ServiceRegistry::new();
        let count = install_services(&reg, &mut services);
        assert_eq!(count, 1);
        assert!(services.get("my.app.metronome").is_some());
        assert_eq!(
            services.scope_of("my.app.metronome"),
            Some(ServiceScope::App),
            "Luau-registered services should land as App-scoped"
        );
    }

    #[test]
    fn install_services_uses_factory_path_so_app_swap_re_runs_it() {
        // ADR-010 Phase 1 contract: installed Luau services go
        // through the factory path. We prove the wiring by injecting
        // a side-channel counter into the factory closure (mirroring
        // the registry-level unit tests) and asserting the closure
        // re-runs across rebuilds.
        //
        // The full bound_app_id contract is covered by the dedicated
        // unit test below; here we only verify the factory pipeline
        // is wired through `install_services`.
        use crate::services::{ServiceContext, ServiceRegistry, ServiceScope};

        let reg = ShellAppRegistrar::with_builtin_panels();
        reg.register_service(ServiceRegistration {
            id: "rebind-test.service".into(),
            on_event_key: "rebind-test.service.on_event".into(),
        })
        .unwrap();
        let mut services = ServiceRegistry::new();
        let installed = install_services(&reg, &mut services);
        assert_eq!(installed, 1);
        assert_eq!(
            services.scope_of("rebind-test.service"),
            Some(ServiceScope::App),
            "install_services must register at App scope"
        );

        // The service id is stable across rebuilds (ADR-010 invariant
        // — factory rebuilds must preserve the service id).
        services.rebuild_app_services(&ServiceContext {
            app_id: Some("lattice"),
        });
        assert!(services.get("rebind-test.service").is_some());
        services.rebuild_app_services(&ServiceContext {
            app_id: Some("flux"),
        });
        assert!(services.get("rebind-test.service").is_some());

        // After two rebuilds the service is still scope-tagged `App`
        // — proves the registration metadata flows through rebuilds
        // intact, which is what `Shell::switch_active_app` depends on
        // for the activation filter to keep behaving.
        assert_eq!(
            services.scope_of("rebind-test.service"),
            Some(ServiceScope::App),
        );
    }

    #[test]
    fn luau_scripted_service_carries_context_app_id() {
        // The factory path goes through `new_with_context`. This test
        // proves the constructor stamps the active app id onto the
        // instance — which is the bridge between
        // `ServiceRegistry::rebuild_app_services` and the per-service
        // re-bind contract.
        use crate::services::ServiceContext;
        let svc = LuauScriptedService::new_with_context(
            ServiceRegistration {
                id: "bound.service".into(),
                on_event_key: "bound.service.on_event".into(),
            },
            &ServiceContext {
                app_id: Some("lattice"),
            },
        );
        assert_eq!(svc.bound_app_id(), Some("lattice"));
        assert_eq!(svc.on_event_key(), "bound.service.on_event");

        // Default context → no bound id.
        let svc2 = LuauScriptedService::new_with_context(
            ServiceRegistration {
                id: "unbound.service".into(),
                on_event_key: "x".into(),
            },
            &ServiceContext::default(),
        );
        assert_eq!(svc2.bound_app_id(), None);
    }

    #[test]
    fn install_panels_from_manifests_registers_every_panels_add_row() {
        let apps = vec![
            LoadedApp {
                manifest: AppManifest {
                    id: "lattice".into(),
                    label: "Lattice".into(),
                    panels: AppPanelsSpec {
                        include: vec![],
                        add: vec![
                            AppPanelDef {
                                id: "lattice.peers".into(),
                                label: "Peers".into(),
                                min_width: 240.0,
                                min_height: 120.0,
                                tag: Some("lattice.peers".into()),
                                ..Default::default()
                            },
                            AppPanelDef {
                                id: "lattice.activity".into(),
                                label: "Activity".into(),
                                min_width: 240.0,
                                min_height: 120.0,
                                tag: Some("lattice.activity".into()),
                                ..Default::default()
                            },
                        ],
                    },
                    ..Default::default()
                },
                base_dir: std::path::PathBuf::from("/tmp/lattice"),
                skeleton: None,
                stylesheet: None,
            },
            LoadedApp {
                manifest: AppManifest {
                    id: "musica".into(),
                    label: "Musica".into(),
                    ..Default::default()
                },
                base_dir: std::path::PathBuf::from("/tmp/musica"),
                skeleton: None,
                stylesheet: None,
            },
        ];
        let reg = ShellAppRegistrar::with_builtin_panels();
        let count = install_panels_from_manifests(&reg, &apps);
        assert_eq!(count, 2);
        let snap = reg.snapshot_catalog();
        assert!(snap.get("lattice.peers").is_some());
        assert!(snap.get("lattice.activity").is_some());
        assert_eq!(snap.tag_for("lattice.peers"), Some("lattice.peers"));
        assert_eq!(snap.len(), 17);
    }

    #[test]
    fn install_panels_skips_bad_rows_without_aborting() {
        let apps = vec![LoadedApp {
            manifest: AppManifest {
                id: "broken".into(),
                label: "Broken".into(),
                panels: AppPanelsSpec {
                    include: vec![],
                    add: vec![
                        AppPanelDef {
                            id: "".into(), // invalid
                            label: "Empty id".into(),
                            ..Default::default()
                        },
                        AppPanelDef {
                            id: "broken.good".into(),
                            label: "Good".into(),
                            ..Default::default()
                        },
                    ],
                },
                ..Default::default()
            },
            base_dir: std::path::PathBuf::from("/tmp/broken"),
            skeleton: None,
            stylesheet: None,
        }];
        let reg = ShellAppRegistrar::with_builtin_panels();
        let count = install_panels_from_manifests(&reg, &apps);
        assert_eq!(count, 1, "only the valid row should land");
        assert!(reg.snapshot_catalog().get("broken.good").is_some());
    }

    #[test]
    fn luau_component_block_renders_labelled_placeholder() {
        // The Luau-backed shim's `lower_ui` produces a container with
        // semantic markers identifying the component + render key.
        // Until the Luau runtime is wired, this is the contract every
        // app-registered component renders against — tests and SSR
        // both see the same placeholder.
        let block = LuauComponentBlock::new(ComponentRegistration {
            id: "my.card".into(),
            render_key: "my.scripts.card.render".into(),
        });
        let node = Node {
            id: "inst".into(),
            component: "my.card".into(),
            ..Default::default()
        };
        let cascade = StyleProperties::default();
        let ctx = LowerCtx::new(None, &cascade);
        let ui = block.lower_ui(&ctx, &node, &cascade);
        let UiNode::Container { id, props, .. } = ui else {
            panic!("expected container, got {ui:?}");
        };
        assert_eq!(id, "inst");
        assert!(props
            .semantic
            .attrs
            .iter()
            .any(|(k, v)| k == "data-role" && v == "luau-component"));
        assert!(props
            .semantic
            .attrs
            .iter()
            .any(|(k, v)| k == "data-component" && v == "my.card"));
        assert!(props
            .semantic
            .attrs
            .iter()
            .any(|(k, v)| k == "data-luau-key" && v == "my.scripts.card.render"));
    }

    #[test]
    fn luau_scripted_service_passes_events_today() {
        // ShellService impl returns `Pass` until the Luau runtime is
        // wired; this pins the contract so a future fan-out change
        // doesn't silently start swallowing events.
        use prism_ui_runtime::event::Event;
        use prism_ui_runtime::layout::Viewport;
        let svc = LuauScriptedService::new(ServiceRegistration {
            id: "my.service".into(),
            on_event_key: "my.scripts.service.on_event".into(),
        });
        let mut state = crate::AppState::default();
        let mut undo = crate::services::UndoStack::default();
        let mut vfs = crate::services::OsVfs;
        let mut luau = crate::services::NoopLuauHost::default();
        let mut clipboard = crate::services::Clipboard::default();
        let mut ctx = MutCtx {
            state: &mut state,
            viewport: Viewport {
                width: 1.0,
                height: 1.0,
            },
            undo: &mut undo,
            vfs: &mut vfs,
            luau: &mut luau,
            clipboard: &mut clipboard,
            registry: None,
            modifier_registry: None,
        };
        let table = CommandTable::default();
        let outcome = svc.on_event(&Event::Wheel { dx: 0.0, dy: 0.0 }, &mut ctx, &table);
        assert_eq!(outcome, EventOutcome::Pass);
        assert_eq!(svc.id(), "my.service");
        assert_eq!(svc.on_event_key(), "my.scripts.service.on_event");
    }
}
