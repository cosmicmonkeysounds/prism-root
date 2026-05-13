//! `ShellAppRegistrar` — concrete `prism_core::AppRegistrar` impl that
//! routes registrations into the shell's live dock catalog (and, in
//! follow-up work, the builder's `ComponentRegistry` + the shell's
//! `ServiceRegistry`).
//!
//! Loop 4 of `docs/dev/dsl-self-bootstrap.md`. The trait surface lives
//! in `prism_core::app_registry` because `prism-core` is the leaf
//! crate Luau bindings call into. This module supplies the production
//! glue — `Arc<Mutex<DockCatalog>>` is the runtime-mutable shared
//! handle the shell hands to each Luau-script-loading step.

use std::sync::{Arc, Mutex};

use prism_core::{
    AppRegistrar, ComponentRegistration, PanelRegistration, RegistrationError, ServiceRegistration,
};
use prism_dock::{DockCatalog, PanelKind};

/// Production `AppRegistrar` impl. Wraps a shared `DockCatalog` so
/// registrations made during app load are visible to every subsequent
/// dock lookup.
///
/// The catalog is wrapped in `Arc<Mutex<...>>` because:
/// 1. Multiple apps may register concurrently in the future.
/// 2. The shell consumes the catalog from many sites (dock-panel
///    lower, dock-workspace lower, tab-bar lower); cloning the `Arc`
///    is cheap.
/// 3. Built-ins seed the catalog at construction; per-app additions
///    layer on without losing them.
#[derive(Clone)]
pub struct ShellAppRegistrar {
    panels: Arc<Mutex<DockCatalog>>,
}

impl ShellAppRegistrar {
    /// Build a registrar over a catalog pre-seeded with built-in
    /// panels (`prism_dock::register_builtins`). Apps register on top
    /// of those built-ins.
    pub fn with_builtin_panels() -> Self {
        Self {
            panels: Arc::new(Mutex::new(DockCatalog::with_builtins())),
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
        _component: ComponentRegistration,
    ) -> Result<(), RegistrationError> {
        // Wiring `register_component` requires:
        // - a `Luau-backed `Block` impl that calls the script's
        //   render function during `lower_ui`;
        // - mutable shared access to `prism_builder::ComponentRegistry`
        //   (today the registry is owned by `ShellInner` and not
        //   wrapped in a `Mutex`).
        // Both are tracked as follow-up — see
        // `docs/dev/dsl-self-bootstrap.md` Loop 4 "Remaining work".
        Err(RegistrationError::Unsupported("register_component"))
    }

    fn register_service(&self, _service: ServiceRegistration) -> Result<(), RegistrationError> {
        // Wiring `register_service` requires a Luau-backed
        // `ShellService` shim that routes `on_event` calls through
        // the script's handler. Same follow-up as `register_component`.
        Err(RegistrationError::Unsupported("register_service"))
    }
}

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
    fn register_component_and_service_are_explicit_unsupported_today() {
        let reg = ShellAppRegistrar::with_builtin_panels();
        assert!(matches!(
            reg.register_component(ComponentRegistration::default()),
            Err(RegistrationError::Unsupported("register_component"))
        ));
        assert!(matches!(
            reg.register_service(ServiceRegistration::default()),
            Err(RegistrationError::Unsupported("register_service"))
        ));
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
            },
            LoadedApp {
                manifest: AppManifest {
                    id: "musica".into(),
                    label: "Musica".into(),
                    ..Default::default()
                },
                base_dir: std::path::PathBuf::from("/tmp/musica"),
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
        }];
        let reg = ShellAppRegistrar::with_builtin_panels();
        let count = install_panels_from_manifests(&reg, &apps);
        assert_eq!(count, 1, "only the valid row should land");
        assert!(reg.snapshot_catalog().get("broken.good").is_some());
    }
}
