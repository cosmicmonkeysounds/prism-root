//! Component type registry + property-panel field factories.
//!
//! Two responsibilities sit in this module:
//!
//! 1. [`ComponentRegistry`] — the one-way DI surface. Panels register
//!    their component types at boot, the document tree looks them up by
//!    id at render time. `register` is idempotent-per-id, double-registers
//!    error out.
//! 2. Field factories ([`FieldSpec`], [`FieldKind`], [`SelectOption`],
//!    [`NumericBounds`]) — re-exported from `prism_core::widget::field`
//!    so the API surface is unchanged for existing consumers.

use indexmap::IndexMap;
use prism_core::help::{HelpEntry, HelpProvider};
use std::sync::Arc;
use thiserror::Error;

use crate::component::{Component, ComponentId};

// Re-export field types from prism-core so existing `use crate::registry::*`
// imports throughout prism-builder continue to work unchanged.
pub use prism_core::widget::field::{
    prop_bool, prop_f64, prop_str, prop_u64, FieldKind, FieldSpec, FieldValue, FileFieldConfig,
    NumericBounds, SelectOption,
};

#[derive(Debug, Error)]
pub enum RegistryError {
    #[error("component already registered: {0}")]
    AlreadyRegistered(ComponentId),
    #[error("component not found: {0}")]
    NotFound(ComponentId),
}

/// Registry of component types keyed by stable [`ComponentId`]. Single
/// DI surface — no side registries, no hand-rolled `Node` factories.
#[derive(Default)]
pub struct ComponentRegistry {
    components: IndexMap<ComponentId, Arc<dyn Component>>,
}

impl ComponentRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, component: Arc<dyn Component>) -> Result<(), RegistryError> {
        let id = component.id().clone();
        if self.components.contains_key(&id) {
            return Err(RegistryError::AlreadyRegistered(id));
        }
        self.components.insert(id, component);
        Ok(())
    }

    pub fn get(&self, id: &str) -> Option<Arc<dyn Component>> {
        self.components.get(id).cloned()
    }

    pub fn len(&self) -> usize {
        self.components.len()
    }

    pub fn is_empty(&self) -> bool {
        self.components.is_empty()
    }

    pub fn ids(&self) -> impl Iterator<Item = &ComponentId> {
        self.components.keys()
    }

    /// Iterate the full `(id, component)` pairs in registration order.
    pub fn iter(&self) -> impl Iterator<Item = (&ComponentId, &Arc<dyn Component>)> {
        self.components.iter()
    }
}

impl HelpProvider for ComponentRegistry {
    fn help_entries(&self) -> Vec<HelpEntry> {
        self.components
            .values()
            .filter_map(|c| c.help_entry())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    #[test]
    fn text_builder_defaults_to_empty_string() {
        let spec = FieldSpec::text("title", "Title");
        assert_eq!(spec.default, Value::String(String::new()));
        assert!(matches!(spec.kind, FieldKind::Text));
        assert!(!spec.required);
    }

    #[test]
    fn number_bounds_clamp_on_read() {
        let spec = FieldSpec::number("volume", "Volume", NumericBounds::min_max(0.0, 1.0));
        let props = json!({ "volume": 2.5 });
        assert!((FieldValue::read_number(&props, &spec) - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn integer_clamps_to_bounds() {
        let spec = FieldSpec::integer("level", "Level", NumericBounds::min_max(1.0, 6.0));
        let props = json!({ "level": 99 });
        assert_eq!(FieldValue::read_integer(&props, &spec), 6);
    }

    #[test]
    fn select_default_is_first_option_value() {
        let spec = FieldSpec::select(
            "style",
            "Style",
            vec![
                SelectOption::new("solid", "Solid"),
                SelectOption::new("dashed", "Dashed"),
            ],
        );
        assert_eq!(spec.default, Value::String("solid".into()));
    }

    #[test]
    fn required_flag_is_chainable() {
        let spec = FieldSpec::text("title", "Title").required();
        assert!(spec.required);
    }

    #[test]
    fn read_string_falls_back_to_default() {
        let spec = FieldSpec::text("text", "Text").with_default(Value::String("hi".into()));
        let props = json!({});
        assert_eq!(FieldValue::read_string(&props, &spec), "hi");
    }

    #[test]
    fn read_boolean_reads_actual_value() {
        let spec = FieldSpec::boolean("enabled", "Enabled");
        let props = json!({ "enabled": true });
        assert!(FieldValue::read_boolean(&props, &spec));
    }

    #[test]
    fn registry_iter_preserves_insertion_order() {
        use crate::component::{Component, ComponentId};

        struct Stub(ComponentId);
        impl Component for Stub {
            fn id(&self) -> &ComponentId {
                &self.0
            }
            fn schema(&self) -> Vec<FieldSpec> {
                vec![]
            }
        }

        let mut reg = ComponentRegistry::new();
        reg.register(Arc::new(Stub("a".into()))).unwrap();
        reg.register(Arc::new(Stub("b".into()))).unwrap();
        reg.register(Arc::new(Stub("c".into()))).unwrap();
        let ids: Vec<_> = reg.iter().map(|(id, _)| id.clone()).collect();
        assert_eq!(ids, vec!["a".to_string(), "b".into(), "c".into()]);
    }

    #[test]
    fn double_register_errors() {
        use crate::component::{Component, ComponentId};

        struct Stub;
        impl Component for Stub {
            fn id(&self) -> &ComponentId {
                static ID: std::sync::OnceLock<ComponentId> = std::sync::OnceLock::new();
                ID.get_or_init(|| "dup".to_string())
            }
            fn schema(&self) -> Vec<FieldSpec> {
                vec![]
            }
        }

        let mut reg = ComponentRegistry::new();
        reg.register(Arc::new(Stub)).unwrap();
        let err = reg.register(Arc::new(Stub)).unwrap_err();
        assert!(matches!(err, RegistryError::AlreadyRegistered(_)));
    }
}
