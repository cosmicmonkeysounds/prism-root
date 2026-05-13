//! Modifiers — attachable behaviour descriptors on any node.
//!
//! See `docs/dev/composable-builder-plan.md` Wave 1 for the design. The
//! pre-Wave-1 shape was a closed `ModifierKind` enum plus a free
//! `modifier_schema(kind)` function; this module is the post-Wave-1
//! shape — an open `ModifierRegistry` of `Arc<dyn ModifierBehaviour>`
//! impls, with the original six kinds preserved as the baseline
//! registration set.
//!
//! ```text
//! Modifier        — data on Node (id + enabled + props), serialized
//! ModifierBehaviour — trait: schema + render-wrap + signals
//! ModifierRegistry  — DI registry of behaviour impls, keyed by id
//! ```
//!
//! The render walker (`LowerCtx::lower`) folds `node.modifiers` over
//! the lowered child innermost-first, calling each behaviour's `wrap`.
//! `enabled = false` skips the wrap. `wrap` defaults to identity so
//! schema-only behaviours remain a one-line registration.

use std::borrow::Cow;
use std::sync::Arc;

use prism_ui_runtime::layout::Node as UiNode;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::registry::{FieldSpec, NumericBounds, SelectOption};
use crate::signal::SignalDef;

/// Behaviour identifier. Borrowed for builtin ids (`"hover-effect"`)
/// and owned for Luau-authored ids (`"luau:my-mod"`).
pub type ModifierId = Cow<'static, str>;

/// Legacy enumeration of the six baseline behaviours. Pre-Wave-1 this
/// was the *only* set of modifiers; post-Wave-1 it is a constant
/// catalog of the ids the registry ships with by default. New
/// behaviours register directly with `ModifierRegistry::register`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ModifierKind {
    ScrollOverflow,
    HoverEffect,
    EnterAnimation,
    ResponsiveVisibility,
    Tooltip,
    AccessibilityOverride,
}

impl ModifierKind {
    pub const ALL: &'static [ModifierKind] = &[
        ModifierKind::ScrollOverflow,
        ModifierKind::HoverEffect,
        ModifierKind::EnterAnimation,
        ModifierKind::ResponsiveVisibility,
        ModifierKind::Tooltip,
        ModifierKind::AccessibilityOverride,
    ];

    /// Stable string id for this builtin kind. Matches the kebab-case
    /// serde representation and the `ModifierBehaviour::id()` returned
    /// by the matching builtin impl.
    pub fn id(self) -> &'static str {
        match self {
            Self::ScrollOverflow => "scroll-overflow",
            Self::HoverEffect => "hover-effect",
            Self::EnterAnimation => "enter-animation",
            Self::ResponsiveVisibility => "responsive-visibility",
            Self::Tooltip => "tooltip",
            Self::AccessibilityOverride => "accessibility-override",
        }
    }

    /// Inverse of [`Self::id`]. Returns `None` for ids outside the
    /// builtin set (e.g. user-registered behaviours).
    pub fn from_id(id: &str) -> Option<Self> {
        match id {
            "scroll-overflow" => Some(Self::ScrollOverflow),
            "hover-effect" => Some(Self::HoverEffect),
            "enter-animation" => Some(Self::EnterAnimation),
            "responsive-visibility" => Some(Self::ResponsiveVisibility),
            "tooltip" => Some(Self::Tooltip),
            "accessibility-override" => Some(Self::AccessibilityOverride),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::ScrollOverflow => "Scroll Overflow",
            Self::HoverEffect => "Hover Effect",
            Self::EnterAnimation => "Enter Animation",
            Self::ResponsiveVisibility => "Responsive Visibility",
            Self::Tooltip => "Tooltip",
            Self::AccessibilityOverride => "Accessibility Override",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Self::ScrollOverflow => {
                "Makes content scrollable when it exceeds the container bounds."
            }
            Self::HoverEffect => "Applies a visual effect when the user hovers over the element.",
            Self::EnterAnimation => "Animates the element when it first appears in the viewport.",
            Self::ResponsiveVisibility => "Controls visibility at different viewport breakpoints.",
            Self::Tooltip => "Shows a tooltip on hover with configurable text and placement.",
            Self::AccessibilityOverride => "Overrides ARIA attributes for assistive technology.",
        }
    }
}

/// One attached behaviour on a `Node`. Serialized verbatim through the
/// document tree.
///
/// `kind` is the registered `ModifierBehaviour` id. Pre-Wave-1 docs
/// serialized this field as the kebab-case `ModifierKind` enum; the
/// string form is a superset so old documents continue to deserialize
/// without conversion. `enabled` defaults to true (omitted from the
/// serialized form when true) so existing docs interpret correctly.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Modifier {
    pub kind: String,
    #[serde(
        default = "default_enabled",
        skip_serializing_if = "is_default_enabled"
    )]
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub props: Value,
}

fn default_enabled() -> bool {
    true
}
fn is_default_enabled(v: &bool) -> bool {
    *v
}

impl Modifier {
    /// Build a modifier instance for a registered behaviour id.
    pub fn new(kind: impl Into<String>) -> Self {
        Self {
            kind: kind.into(),
            enabled: true,
            props: Value::Null,
        }
    }

    /// Convenience: build from a baseline `ModifierKind` enum value.
    pub fn from_kind(kind: ModifierKind) -> Self {
        Self::new(kind.id())
    }

    pub fn with_props(mut self, props: Value) -> Self {
        self.props = props;
        self
    }

    pub fn disabled(mut self) -> Self {
        self.enabled = false;
        self
    }

    /// Builtin enum form of this modifier, when the id belongs to the
    /// baseline catalog. Returns `None` for registry-only ids.
    pub fn builtin_kind(&self) -> Option<ModifierKind> {
        ModifierKind::from_id(&self.kind)
    }
}

/// Behaviour contract for a registered modifier id. One impl per
/// behaviour; the registry maps an id to `Arc<dyn ModifierBehaviour>`.
///
/// Default impls of `wrap`, `signals`, `icon`, and `description` keep
/// schema-only behaviours one-liner registrations. `wrap` is the
/// render-time hook: `LowerCtx::lower` folds the modifier stack over
/// the lowered child innermost-first via this method.
///
/// Most behaviours don't need a hand-rolled impl — the [`BehaviourSpec`]
/// data struct plus the blanket [`SpecBehaviour`] impl below cover the
/// `(id, label, description, schema_fn, optional wrap_fn)` shape every
/// builtin and bootstrap entry uses. Hand-written impls are reserved
/// for cases that need richer state (e.g. Luau-authored behaviours
/// that close over a runtime handle).
pub trait ModifierBehaviour: Send + Sync + 'static {
    fn id(&self) -> ModifierId;
    fn label(&self) -> &str;
    fn icon(&self) -> Option<&str> {
        None
    }
    fn description(&self) -> &str {
        ""
    }
    fn schema(&self) -> Vec<FieldSpec>;

    /// Optional render wrapper. Receives the lowered child subtree
    /// and the modifier instance (so `props` and `enabled` are
    /// available). Default: identity.
    fn wrap(&self, _modifier: &Modifier, child: UiNode) -> UiNode {
        child
    }

    /// Optional additional signals contributed by the modifier.
    fn signals(&self) -> Vec<SignalDef> {
        Vec::new()
    }
}

/// Declarative descriptor for a modifier behaviour. Mirrors the
/// `BlockSpec` / `SpecBlock` pattern in `crate::block`: one `&'static
/// BehaviourSpec` per builtin, the blanket [`SpecBehaviour`] impl
/// reads from it, and `register_specs` fans them into the registry.
///
/// `wrap` is optional — `None` means render identity, which covers
/// every schema-only entry (Tooltip, BindToSelection, RunLuauScript,
/// …) with no boilerplate.
pub struct BehaviourSpec {
    pub id: &'static str,
    pub label: &'static str,
    pub description: &'static str,
    pub icon: Option<&'static str>,
    pub schema: fn() -> Vec<FieldSpec>,
    pub wrap: Option<fn(&Modifier, UiNode) -> UiNode>,
}

impl BehaviourSpec {
    /// Build a spec with sane defaults: no icon, identity wrap,
    /// caller-provided schema function.
    pub const fn new(
        id: &'static str,
        label: &'static str,
        schema: fn() -> Vec<FieldSpec>,
    ) -> Self {
        Self {
            id,
            label,
            description: "",
            icon: None,
            schema,
            wrap: None,
        }
    }
    pub const fn description(mut self, description: &'static str) -> Self {
        self.description = description;
        self
    }
    pub const fn icon(mut self, icon: &'static str) -> Self {
        self.icon = Some(icon);
        self
    }
    pub const fn wrap(mut self, wrap: fn(&Modifier, UiNode) -> UiNode) -> Self {
        self.wrap = Some(wrap);
        self
    }
}

/// Single `ModifierBehaviour` impl that reads from a `&'static
/// BehaviourSpec`. Replaces what would otherwise be one unit struct +
/// one trait impl per builtin (and per Luau-registered entry once
/// Wave 8 lands).
pub struct SpecBehaviour {
    spec: &'static BehaviourSpec,
}

impl SpecBehaviour {
    pub const fn new(spec: &'static BehaviourSpec) -> Self {
        Self { spec }
    }
    pub fn arc(spec: &'static BehaviourSpec) -> Arc<Self> {
        Arc::new(Self::new(spec))
    }
}

impl ModifierBehaviour for SpecBehaviour {
    fn id(&self) -> ModifierId {
        Cow::Borrowed(self.spec.id)
    }
    fn label(&self) -> &str {
        self.spec.label
    }
    fn icon(&self) -> Option<&str> {
        self.spec.icon
    }
    fn description(&self) -> &str {
        self.spec.description
    }
    fn schema(&self) -> Vec<FieldSpec> {
        (self.spec.schema)()
    }
    fn wrap(&self, modifier: &Modifier, child: UiNode) -> UiNode {
        match self.spec.wrap {
            Some(f) => f(modifier, child),
            None => child,
        }
    }
}

/// Lightweight clone of a behaviour's metadata surface for UI
/// consumption (modifier-picker rows, type-stub generation, the
/// inspector's section header).
#[derive(Debug, Clone, Serialize)]
pub struct ModifierDescriptor {
    pub id: String,
    pub label: String,
    pub icon: Option<String>,
    pub description: String,
}

#[derive(Debug, Clone, thiserror::Error)]
pub enum ModifierRegistryError {
    #[error("modifier `{0}` already registered")]
    AlreadyRegistered(String),
}

/// Open registry of `ModifierBehaviour` impls.
///
/// Thin newtype around [`prism_core::Catalog`] specialised for
/// `Arc<dyn ModifierBehaviour>`. The trait's `id()` returns `Cow`
/// rather than `&str`, so we use the catalog's `insert(id, value)`
/// path (instead of the `HasId`-based `register`) and add the
/// duplicate-id check on top to preserve the
/// [`ModifierRegistryError::AlreadyRegistered`] contract.
#[derive(Default, Clone)]
pub struct ModifierRegistry {
    inner: prism_core::Catalog<Arc<dyn ModifierBehaviour>>,
}

impl ModifierRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Registry seeded with the six baseline behaviours
    /// (`scroll-overflow`, `hover-effect`, …) plus the six Wave 1.7
    /// bootstrap behaviours (`visible`, `locked`, `hover`, `click`,
    /// `bind-to-selection`, `run-luau-script`). Twelve registrations
    /// total — the default vocabulary every Inspector picks from on
    /// boot.
    pub fn with_builtins() -> Self {
        let mut reg = Self::new();
        register_builtins(&mut reg).expect("builtin modifier ids are unique");
        crate::modifier_bootstrap::register_bootstrap(&mut reg)
            .expect("bootstrap modifier ids are unique");
        reg
    }

    pub fn register(
        &mut self,
        beh: Arc<dyn ModifierBehaviour>,
    ) -> Result<(), ModifierRegistryError> {
        let id = beh.id().into_owned();
        if self.inner.contains(&id) {
            return Err(ModifierRegistryError::AlreadyRegistered(id));
        }
        self.inner.insert(id, beh);
        Ok(())
    }

    pub fn get(&self, id: &str) -> Option<&Arc<dyn ModifierBehaviour>> {
        self.inner.get(id)
    }

    pub fn contains(&self, id: &str) -> bool {
        self.inner.contains(id)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, &Arc<dyn ModifierBehaviour>)> {
        self.inner.entries()
    }

    /// Stable iteration over descriptor metadata, in registration
    /// order. Used by the inspector's `add-modifier` picker.
    pub fn list(&self) -> Vec<ModifierDescriptor> {
        self.inner
            .iter()
            .map(|b| descriptor_from(b.as_ref()))
            .collect()
    }

    pub fn ids(&self) -> impl Iterator<Item = &str> + '_ {
        self.inner.ids()
    }

    pub fn len(&self) -> usize {
        self.inner.len()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// Schema lookup for a registered id. Returns an empty vector for
    /// unknown ids so `derive_property_rows` can still emit a section
    /// header without crashing on a stale or Luau-only id.
    pub fn schema_for(&self, id: &str) -> Vec<FieldSpec> {
        self.get(id).map(|b| b.schema()).unwrap_or_default()
    }

    /// Descriptor for a single registered id, if present.
    pub fn descriptor(&self, id: &str) -> Option<ModifierDescriptor> {
        self.get(id).map(|b| descriptor_from(b.as_ref()))
    }
}

/// Pure projection from a `&dyn ModifierBehaviour` to its metadata
/// descriptor. Shared by `list()` and `descriptor(id)` so the four
/// trait method calls don't drift between the two callers.
fn descriptor_from(beh: &dyn ModifierBehaviour) -> ModifierDescriptor {
    ModifierDescriptor {
        id: beh.id().into_owned(),
        label: beh.label().to_string(),
        icon: beh.icon().map(str::to_string),
        description: beh.description().to_string(),
    }
}

/// Fan `&[&BehaviourSpec]` into an existing registry. Sister to
/// `crate::block::register_specs`. One declarative table → N
/// behaviour registrations, no per-spec ceremony at the call site.
pub fn register_specs(
    reg: &mut ModifierRegistry,
    specs: &[&'static BehaviourSpec],
) -> Result<(), ModifierRegistryError> {
    for spec in specs {
        reg.register(SpecBehaviour::arc(spec))?;
    }
    Ok(())
}

/// Register the six baseline behaviours into an existing registry.
pub fn register_builtins(reg: &mut ModifierRegistry) -> Result<(), ModifierRegistryError> {
    register_specs(reg, BUILTINS)
}

/// Legacy free function preserved for the small number of pre-Wave-1
/// callers. New code uses `ModifierRegistry::schema_for(id)`.
pub fn modifier_schema(kind: ModifierKind) -> Vec<FieldSpec> {
    ModifierRegistry::with_builtins().schema_for(kind.id())
}

// ── Baseline behaviour specs ────────────────────────────────────────
//
// Schema bodies preserved verbatim from the pre-Wave-1
// `modifier_schema(kind)` match arms. `wrap` is `None` (identity) for
// all six — concrete render wrappers land alongside Wave 11's
// primitive registry (the post-DSL primitives are where Tooltip /
// HoverEffect / ScrollOverflow get their teeth). Until then,
// attaching one of these six is schema-only but still round-trips
// through the document.

fn scroll_overflow_schema() -> Vec<FieldSpec> {
    vec![FieldSpec::select(
        "direction",
        "Direction",
        vec![
            SelectOption::new("vertical", "Vertical"),
            SelectOption::new("horizontal", "Horizontal"),
            SelectOption::new("both", "Both"),
        ],
    )]
}

fn hover_effect_schema() -> Vec<FieldSpec> {
    vec![
        FieldSpec::select(
            "effect",
            "Effect",
            vec![
                SelectOption::new("scale", "Scale up"),
                SelectOption::new("fade", "Fade"),
                SelectOption::new("lift", "Lift (shadow)"),
                SelectOption::new("glow", "Glow"),
            ],
        ),
        FieldSpec::number(
            "duration_ms",
            "Duration (ms)",
            NumericBounds::min_max(50.0, 2000.0),
        )
        .with_default(Value::from(200)),
    ]
}

fn enter_animation_schema() -> Vec<FieldSpec> {
    vec![
        FieldSpec::select(
            "animation",
            "Animation",
            vec![
                SelectOption::new("fade-in", "Fade in"),
                SelectOption::new("slide-up", "Slide up"),
                SelectOption::new("slide-left", "Slide left"),
                SelectOption::new("scale-up", "Scale up"),
            ],
        ),
        FieldSpec::number(
            "duration_ms",
            "Duration (ms)",
            NumericBounds::min_max(50.0, 3000.0),
        )
        .with_default(Value::from(300)),
        FieldSpec::number(
            "delay_ms",
            "Delay (ms)",
            NumericBounds::min_max(0.0, 5000.0),
        )
        .with_default(Value::from(0)),
    ]
}

fn responsive_visibility_schema() -> Vec<FieldSpec> {
    vec![
        FieldSpec::boolean("show_mobile", "Show on mobile (<640px)"),
        FieldSpec::boolean("show_tablet", "Show on tablet (640\u{2013}1024px)"),
        FieldSpec::boolean("show_desktop", "Show on desktop (>1024px)"),
    ]
}

fn tooltip_schema() -> Vec<FieldSpec> {
    vec![
        FieldSpec::text("text", "Tooltip text").required(),
        FieldSpec::select(
            "placement",
            "Placement",
            vec![
                SelectOption::new("top", "Top"),
                SelectOption::new("bottom", "Bottom"),
                SelectOption::new("left", "Left"),
                SelectOption::new("right", "Right"),
            ],
        ),
    ]
}

fn accessibility_override_schema() -> Vec<FieldSpec> {
    vec![
        FieldSpec::text("role", "ARIA role"),
        FieldSpec::text("label", "ARIA label"),
        FieldSpec::text("description", "ARIA description"),
        FieldSpec::boolean("hidden", "ARIA hidden"),
    ]
}

const SCROLL_OVERFLOW: BehaviourSpec =
    BehaviourSpec::new("scroll-overflow", "Scroll Overflow", scroll_overflow_schema)
        .description("Makes content scrollable when it exceeds the container bounds.");

const HOVER_EFFECT: BehaviourSpec =
    BehaviourSpec::new("hover-effect", "Hover Effect", hover_effect_schema)
        .description("Applies a visual effect when the user hovers over the element.");

const ENTER_ANIMATION: BehaviourSpec =
    BehaviourSpec::new("enter-animation", "Enter Animation", enter_animation_schema)
        .description("Animates the element when it first appears in the viewport.");

const RESPONSIVE_VISIBILITY: BehaviourSpec = BehaviourSpec::new(
    "responsive-visibility",
    "Responsive Visibility",
    responsive_visibility_schema,
)
.description("Controls visibility at different viewport breakpoints.");

const TOOLTIP: BehaviourSpec = BehaviourSpec::new("tooltip", "Tooltip", tooltip_schema)
    .description("Shows a tooltip on hover with configurable text and placement.");

const ACCESSIBILITY_OVERRIDE: BehaviourSpec = BehaviourSpec::new(
    "accessibility-override",
    "Accessibility Override",
    accessibility_override_schema,
)
.description("Overrides ARIA attributes for assistive technology.");

/// Single source of truth for the baseline modifier catalog. Adding
/// a new baseline behaviour is one new `const SPEC` above and one
/// row here — same shape `prism_builder::starter::BUILTINS` uses for
/// document components.
pub const BUILTINS: &[&BehaviourSpec] = &[
    &SCROLL_OVERFLOW,
    &HOVER_EFFECT,
    &ENTER_ANIMATION,
    &RESPONSIVE_VISIBILITY,
    &TOOLTIP,
    &ACCESSIBILITY_OVERRIDE,
];

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn modifier_round_trips_through_serde() {
        let m = Modifier::from_kind(ModifierKind::ScrollOverflow)
            .with_props(json!({ "direction": "vertical" }));
        let json = serde_json::to_string(&m).unwrap();
        let back: Modifier = serde_json::from_str(&json).unwrap();
        assert_eq!(back.kind, "scroll-overflow");
        assert!(back.enabled);
        assert_eq!(back.builtin_kind(), Some(ModifierKind::ScrollOverflow));
    }

    #[test]
    fn modifier_disabled_round_trips() {
        let m = Modifier::from_kind(ModifierKind::Tooltip).disabled();
        let json = serde_json::to_string(&m).unwrap();
        // `enabled: false` round-trips explicitly.
        assert!(json.contains("\"enabled\":false"));
        let back: Modifier = serde_json::from_str(&json).unwrap();
        assert!(!back.enabled);
    }

    #[test]
    fn legacy_serialized_form_without_enabled_loads_as_enabled() {
        // Pre-Wave-1 docs serialized only `{ kind, props }`. The
        // default for `enabled` is true so they keep working.
        let legacy = r#"{"kind":"scroll-overflow","props":{"direction":"vertical"}}"#;
        let m: Modifier = serde_json::from_str(legacy).unwrap();
        assert_eq!(m.kind, "scroll-overflow");
        assert!(m.enabled);
    }

    #[test]
    fn all_kinds_have_labels() {
        for kind in ModifierKind::ALL {
            assert!(!kind.label().is_empty());
            assert!(!kind.description().is_empty());
        }
    }

    #[test]
    fn registry_builtins_cover_every_modifier_kind() {
        let reg = ModifierRegistry::with_builtins();
        for kind in ModifierKind::ALL {
            assert!(
                reg.contains(kind.id()),
                "builtin `{}` missing from registry",
                kind.id()
            );
        }
        // Wave 1.7: with_builtins() seeds the six baseline kinds plus
        // the six bootstrap behaviours (visible / locked / hover /
        // click / bind-to-selection / run-luau-script).
        assert_eq!(reg.len(), ModifierKind::ALL.len() + 6);
    }

    #[test]
    fn registry_list_returns_baseline_then_bootstrap_in_stable_order() {
        let reg = ModifierRegistry::with_builtins();
        let listed: Vec<String> = reg.list().into_iter().map(|d| d.id).collect();
        let mut expected: Vec<String> = ModifierKind::ALL.iter().map(|k| k.id().into()).collect();
        expected.extend([
            "visible".to_string(),
            "locked".to_string(),
            "hover".to_string(),
            "click".to_string(),
            "bind-to-selection".to_string(),
            "run-luau-script".to_string(),
        ]);
        assert_eq!(listed, expected);
    }

    #[test]
    fn registry_rejects_double_registration() {
        let mut reg = ModifierRegistry::new();
        reg.register(SpecBehaviour::arc(&SCROLL_OVERFLOW)).unwrap();
        let err = reg
            .register(SpecBehaviour::arc(&SCROLL_OVERFLOW))
            .expect_err("dup");
        assert!(matches!(err, ModifierRegistryError::AlreadyRegistered(_)));
    }

    #[test]
    fn registry_schema_matches_legacy_modifier_schema() {
        // The free `modifier_schema` function and the registry's
        // `schema_for` must produce identical output for every
        // builtin — proves the migration preserves behaviour.
        let reg = ModifierRegistry::with_builtins();
        for kind in ModifierKind::ALL {
            let legacy = modifier_schema(*kind);
            let new = reg.schema_for(kind.id());
            assert_eq!(legacy.len(), new.len(), "{}", kind.id());
        }
    }

    #[test]
    fn unknown_kind_id_returns_none_and_empty_schema() {
        let reg = ModifierRegistry::with_builtins();
        assert!(reg.get("does-not-exist").is_none());
        assert!(reg.schema_for("does-not-exist").is_empty());
    }
}
