//! Component type registry + property-panel field factories.
//!
//! Two responsibilities sit in this module:
//!
//! 1. [`ComponentRegistry`] — the one-way DI surface. Panels register
//!    their component types at boot, the document tree looks them up by
//!    id at render time. `register` is idempotent-per-id, double-registers
//!    error out.
//! 2. Field factories ([`FieldSpec`], [`FieldKind`], [`SelectOption`],
//!    [`NumericBounds`]) — the typed descriptors a [`Component`] returns
//!    from [`Component::schema`]. Phase 3 brings these over from the old
//!    TS tree so the Studio property panel can render a consistent UI
//!    across every component without the panel having to know what a
//!    given component *does*.
//!
//! Field factories deliberately stay close to the registry: every new
//! block type hits the same set of primitives (string / long-string /
//! number / bool / select / color), so a shared construction surface
//! keeps component authors from hand-rolling schemas.

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;
use thiserror::Error;

use crate::component::{Component, ComponentId};

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
    /// Used by the Studio palette (shows every registered block type)
    /// and by the Slint walker when it pre-declares the global component
    /// dictionary.
    pub fn iter(&self) -> impl Iterator<Item = (&ComponentId, &Arc<dyn Component>)> {
        self.components.iter()
    }
}

// -- Property-panel field factories ----------------------------------

/// Typed field descriptor. A component returns a `Vec<FieldSpec>` from
/// [`Component::schema`]; the Studio property panel walks the list and
/// paints one field per entry. Document nodes store their values under
/// [`FieldSpec::key`] in the node's `props` map.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldSpec {
    /// JSON key this field reads from / writes into `Node::props`.
    pub key: String,
    /// Human-visible label shown in the property panel.
    pub label: String,
    /// Kind of editor the panel should render.
    pub kind: FieldKind,
    /// Default value when the node's props omit this key. Must match
    /// [`FieldKind`] — e.g. `Number`/`Integer` require a JSON number,
    /// `Select` requires one of the option values, etc.
    #[serde(default)]
    pub default: Value,
    /// When `true` the panel blocks save if the field is empty. Cheap
    /// ergonomic hint; validation is still the component's job.
    #[serde(default)]
    pub required: bool,
    /// Optional helper string shown beneath the editor.
    #[serde(default)]
    pub help: Option<String>,
}

impl FieldSpec {
    /// Construct a bare [`FieldSpec`]. Prefer the typed builders
    /// ([`FieldSpec::text`], [`FieldSpec::number`], …) — they preserve
    /// the default/required invariants automatically.
    pub fn new(key: impl Into<String>, label: impl Into<String>, kind: FieldKind) -> Self {
        Self {
            key: key.into(),
            label: label.into(),
            kind,
            default: Value::Null,
            required: false,
            help: None,
        }
    }

    pub fn with_default(mut self, default: Value) -> Self {
        self.default = default;
        self
    }

    pub fn required(mut self) -> Self {
        self.required = true;
        self
    }

    pub fn with_help(mut self, help: impl Into<String>) -> Self {
        self.help = Some(help.into());
        self
    }

    /// Short single-line text editor. Default is an empty string.
    pub fn text(key: impl Into<String>, label: impl Into<String>) -> Self {
        Self::new(key, label, FieldKind::Text).with_default(Value::String(String::new()))
    }

    /// Multi-line text editor. Default is an empty string.
    pub fn textarea(key: impl Into<String>, label: impl Into<String>) -> Self {
        Self::new(key, label, FieldKind::TextArea).with_default(Value::String(String::new()))
    }

    /// Floating-point number with optional bounds. Default is `0.0`.
    pub fn number(key: impl Into<String>, label: impl Into<String>, bounds: NumericBounds) -> Self {
        Self::new(key, label, FieldKind::Number(bounds)).with_default(Value::from(0.0))
    }

    /// Signed integer with optional bounds. Default is `0`.
    pub fn integer(
        key: impl Into<String>,
        label: impl Into<String>,
        bounds: NumericBounds,
    ) -> Self {
        Self::new(key, label, FieldKind::Integer(bounds)).with_default(Value::from(0))
    }

    /// Boolean toggle. Default is `false`.
    pub fn boolean(key: impl Into<String>, label: impl Into<String>) -> Self {
        Self::new(key, label, FieldKind::Boolean).with_default(Value::Bool(false))
    }

    /// Drop-down selector. Default is the first option's value, or
    /// `Value::Null` if the option list is empty.
    pub fn select(
        key: impl Into<String>,
        label: impl Into<String>,
        options: Vec<SelectOption>,
    ) -> Self {
        let default = options
            .first()
            .map(|o| Value::String(o.value.clone()))
            .unwrap_or(Value::Null);
        Self::new(key, label, FieldKind::Select(options)).with_default(default)
    }

    /// Hex color editor (`"#RRGGBB"` or `"#RRGGBBAA"`). Default is
    /// `"#000000"` so the component always has something to paint.
    pub fn color(key: impl Into<String>, label: impl Into<String>) -> Self {
        Self::new(key, label, FieldKind::Color).with_default(Value::String("#000000".into()))
    }
}

/// Discriminator telling the property panel which editor to render.
/// Carries bounds / options inline so the panel doesn't need a second
/// lookup to know how to paint itself.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum FieldKind {
    /// Single-line string.
    Text,
    /// Multi-line string.
    TextArea,
    /// Floating-point number with optional bounds.
    Number(NumericBounds),
    /// Signed integer with optional bounds.
    Integer(NumericBounds),
    /// Boolean.
    Boolean,
    /// Drop-down selector over a fixed option list.
    Select(Vec<SelectOption>),
    /// Hex color editor (`"#RRGGBB"` / `"#RRGGBBAA"`).
    Color,
}

/// Optional min/max bounds for numeric fields. `None` on either end
/// means unbounded.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct NumericBounds {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,
}

impl NumericBounds {
    pub const fn unbounded() -> Self {
        Self {
            min: None,
            max: None,
        }
    }

    pub const fn min_max(min: f64, max: f64) -> Self {
        Self {
            min: Some(min),
            max: Some(max),
        }
    }

    pub const fn min(min: f64) -> Self {
        Self {
            min: Some(min),
            max: None,
        }
    }

    pub const fn max(max: f64) -> Self {
        Self {
            min: None,
            max: Some(max),
        }
    }

    /// Clamp a value to the bounds. Unbounded ends don't clamp.
    pub fn clamp(&self, value: f64) -> f64 {
        let mut v = value;
        if let Some(m) = self.min {
            if v < m {
                v = m;
            }
        }
        if let Some(m) = self.max {
            if v > m {
                v = m;
            }
        }
        v
    }
}

/// One entry in a [`FieldKind::Select`] drop-down.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelectOption {
    pub value: String,
    pub label: String,
}

impl SelectOption {
    pub fn new(value: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            value: value.into(),
            label: label.into(),
        }
    }
}

/// Typed read of a [`FieldSpec`]-shaped slot out of a node's `props`.
/// Components use this to pull out their expected props without
/// juggling `serde_json::Value` match arms inline.
pub struct FieldValue;

impl FieldValue {
    pub fn read_string<'a>(props: &'a Value, spec: &'a FieldSpec) -> &'a str {
        props
            .get(&spec.key)
            .and_then(|v| v.as_str())
            .or_else(|| spec.default.as_str())
            .unwrap_or("")
    }

    pub fn read_number(props: &Value, spec: &FieldSpec) -> f64 {
        let raw = props
            .get(&spec.key)
            .and_then(|v| v.as_f64())
            .or_else(|| spec.default.as_f64())
            .unwrap_or(0.0);
        match &spec.kind {
            FieldKind::Number(b) | FieldKind::Integer(b) => b.clamp(raw),
            _ => raw,
        }
    }

    pub fn read_integer(props: &Value, spec: &FieldSpec) -> i64 {
        let raw = props
            .get(&spec.key)
            .and_then(|v| v.as_i64())
            .or_else(|| spec.default.as_i64())
            .unwrap_or(0);
        match &spec.kind {
            FieldKind::Integer(b) => b.clamp(raw as f64) as i64,
            _ => raw,
        }
    }

    pub fn read_boolean(props: &Value, spec: &FieldSpec) -> bool {
        props
            .get(&spec.key)
            .and_then(|v| v.as_bool())
            .or_else(|| spec.default.as_bool())
            .unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

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
        use crate::component::RenderError;
        use crate::component::{Component, ComponentId, RenderHtmlContext};
        use crate::document::Node;
        use crate::html::Html;

        struct Stub(ComponentId);
        impl Component for Stub {
            fn id(&self) -> &ComponentId {
                &self.0
            }
            fn schema(&self) -> Vec<FieldSpec> {
                vec![]
            }
            fn render_html(
                &self,
                _ctx: &RenderHtmlContext<'_>,
                _props: &Value,
                _children: &[Node],
                _out: &mut Html,
            ) -> Result<(), RenderError> {
                Ok(())
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
        use crate::component::RenderError;
        use crate::component::{Component, ComponentId, RenderHtmlContext};
        use crate::document::Node;
        use crate::html::Html;

        struct Stub;
        impl Component for Stub {
            fn id(&self) -> &ComponentId {
                static ID: std::sync::OnceLock<ComponentId> = std::sync::OnceLock::new();
                ID.get_or_init(|| "dup".to_string())
            }
            fn schema(&self) -> Vec<FieldSpec> {
                vec![]
            }
            fn render_html(
                &self,
                _ctx: &RenderHtmlContext<'_>,
                _props: &Value,
                _children: &[Node],
                _out: &mut Html,
            ) -> Result<(), RenderError> {
                Ok(())
            }
        }

        let mut reg = ComponentRegistry::new();
        reg.register(Arc::new(Stub)).unwrap();
        let err = reg.register(Arc::new(Stub)).unwrap_err();
        assert!(matches!(err, RegistryError::AlreadyRegistered(_)));
    }
}
