//! `AppRegistrar` — the abstract registration seam apps push into.
//!
//! Loop 4 of `docs/dev/dsl-self-bootstrap.md`. Apps (whether authored
//! in Rust, Luau, or via a hot-reloaded manifest) declare their UI
//! surface through three verbs: register a component, register a
//! panel, register a service. The trait defines the verbs; the
//! concrete impl lives in `prism_shell::app_registry` and threads
//! through to the matching downstream registries
//! ([`prism_builder::ComponentRegistry`],
//! [`prism_dock::DockCatalog`], `prism_shell::ServiceRegistry`).
//!
//! `prism-core` is the leaf — it can't depend on the downstream
//! registries. Defining the trait here lets the Luau bindings (also
//! in `prism-core`) call a `dyn AppRegistrar` handle installed by the
//! shell at script-load time without a dependency cycle.

use serde::{Deserialize, Serialize};

use crate::app::AppPanelDef;

/// One panel an app contributes to the dock catalog. Same shape as
/// [`AppPanelDef`] in the manifest IR — converted directly at the
/// registration site.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct PanelRegistration {
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub icon_hint: String,
    pub min_width: f32,
    pub min_height: f32,
    #[serde(default)]
    pub allow_multiple: bool,
    #[serde(default)]
    pub tag: Option<String>,
}

impl From<AppPanelDef> for PanelRegistration {
    fn from(d: AppPanelDef) -> Self {
        Self {
            id: d.id,
            label: d.label,
            icon_hint: d.icon_hint,
            min_width: d.min_width,
            min_height: d.min_height,
            allow_multiple: d.allow_multiple,
            tag: d.tag,
        }
    }
}

/// One component an app contributes to the block registry. The
/// `render_callback` opaque handle is whatever the host runtime
/// uses to dispatch — for Luau, it's the registry key under which
/// the script's render function is stored; for Rust, it's a
/// `Box<dyn Block>` shimmed through the registrar impl.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ComponentRegistration {
    pub id: String,
    /// Opaque key by which the host resolves the component's
    /// `lower_ui` body. Interpretation is up to the registrar
    /// impl — typically a Luau-script-registered render-fn id.
    #[serde(default)]
    pub render_key: String,
    /// Declared schema. The property panel walks these to paint a
    /// field editor per entry. Empty by default — scripts opt in by
    /// passing a `schema = {...}` table to `register_component`. The
    /// table grammar mirrors [`crate::widget::field::FieldSpec`]
    /// exactly (key/label/kind/default/required/help/group).
    #[serde(default)]
    pub schema: Vec<crate::widget::field::FieldSpec>,
}

/// One service an app contributes to the shell's service registry.
/// Like [`ComponentRegistration`], the `on_event_key` is an opaque
/// dispatch handle the host knows how to interpret.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ServiceRegistration {
    pub id: String,
    /// Opaque key by which the host resolves the service's
    /// `on_event` body. Interpretation is up to the registrar
    /// impl — typically a Luau-script-registered handler id.
    #[serde(default)]
    pub on_event_key: String,
}

/// What can go wrong during registration.
#[derive(Debug, thiserror::Error)]
pub enum RegistrationError {
    #[error("registration `{0}` not supported by this registrar")]
    Unsupported(&'static str),
    #[error("duplicate registration id: {0}")]
    Duplicate(String),
    #[error("invalid registration: {0}")]
    Invalid(String),
}

/// The verbs an app calls to surface its UI through the framework.
///
/// Implementations: `NoopAppRegistrar` (this module) for tests and
/// the offline luau-codegen path; `prism_shell::app_registry::ShellAppRegistrar`
/// for production.
pub trait AppRegistrar: Send + Sync {
    fn register_panel(&self, panel: PanelRegistration) -> Result<(), RegistrationError>;
    fn register_component(
        &self,
        component: ComponentRegistration,
    ) -> Result<(), RegistrationError> {
        let _ = component;
        Err(RegistrationError::Unsupported("register_component"))
    }
    fn register_service(&self, service: ServiceRegistration) -> Result<(), RegistrationError> {
        let _ = service;
        Err(RegistrationError::Unsupported("register_service"))
    }
}

/// Drop-everything registrar. Useful for offline codegen + tests that
/// don't care whether registrations actually land.
#[derive(Default)]
pub struct NoopAppRegistrar;

impl AppRegistrar for NoopAppRegistrar {
    fn register_panel(&self, _panel: PanelRegistration) -> Result<(), RegistrationError> {
        Ok(())
    }
    fn register_component(
        &self,
        _component: ComponentRegistration,
    ) -> Result<(), RegistrationError> {
        Ok(())
    }
    fn register_service(&self, _service: ServiceRegistration) -> Result<(), RegistrationError> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    struct CountingRegistrar {
        panels: Mutex<Vec<String>>,
    }

    impl AppRegistrar for CountingRegistrar {
        fn register_panel(&self, panel: PanelRegistration) -> Result<(), RegistrationError> {
            self.panels.lock().unwrap().push(panel.id);
            Ok(())
        }
    }

    #[test]
    fn noop_registrar_accepts_every_verb() {
        let reg = NoopAppRegistrar;
        reg.register_panel(PanelRegistration {
            id: "noop.panel".into(),
            label: "Noop".into(),
            ..Default::default()
        })
        .unwrap();
        reg.register_component(ComponentRegistration {
            id: "noop.component".into(),
            render_key: "noop.key".into(),
            ..Default::default()
        })
        .unwrap();
        reg.register_service(ServiceRegistration {
            id: "noop.service".into(),
            on_event_key: "noop.handler".into(),
        })
        .unwrap();
    }

    #[test]
    fn default_impl_rejects_component_and_service() {
        struct PanelOnly;
        impl AppRegistrar for PanelOnly {
            fn register_panel(&self, _panel: PanelRegistration) -> Result<(), RegistrationError> {
                Ok(())
            }
        }
        let reg = PanelOnly;
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
    fn registrar_records_registered_panels() {
        let reg = CountingRegistrar {
            panels: Mutex::new(Vec::new()),
        };
        reg.register_panel(PanelRegistration {
            id: "lattice.peers".into(),
            label: "Peers".into(),
            ..Default::default()
        })
        .unwrap();
        reg.register_panel(PanelRegistration {
            id: "lattice.activity".into(),
            label: "Activity".into(),
            ..Default::default()
        })
        .unwrap();
        let panels = reg.panels.lock().unwrap();
        assert_eq!(panels.as_slice(), &["lattice.peers", "lattice.activity"]);
    }

    #[test]
    fn panel_def_converts_into_registration() {
        let def = AppPanelDef {
            id: "x".into(),
            label: "X".into(),
            icon_hint: "i".into(),
            min_width: 100.0,
            min_height: 50.0,
            allow_multiple: true,
            tag: Some("x.canvas".into()),
        };
        let r: PanelRegistration = def.into();
        assert_eq!(r.id, "x");
        assert_eq!(r.tag.as_deref(), Some("x.canvas"));
        assert!(r.allow_multiple);
    }
}
