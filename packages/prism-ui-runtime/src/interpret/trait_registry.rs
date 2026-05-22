//! Phase 9 — the open trait registry for the §7.4 attribute system.
//!
//! Today's attribute classifier (`prism_core::language::prism_ui::ast::
//! AttributeNamespace::classify`) treats `foo:bar` colon-separated
//! prefixes as a closed enum: `style`, `on`, `bind`, `aria`, `data`,
//! and a handful of others. §7.4's design collapses that to the
//! `trait.method=value` shape (`layout.gap=12`, `style.background=
//! accent`) and **opens the registry** — new traits can be added
//! without grammar edits.
//!
//! Phase 9 ships with four built-in traits that mirror the namespaces
//! authors reach for today:
//!
//! | Trait     | Methods (sampling)                | Lowered as           |
//! |-----------|-----------------------------------|----------------------|
//! | `layout`  | `direction`, `gap`, `padding`, …  | bare prop on container |
//! | `style`   | `background`, `color`, `radius`, …| `style:<method>`     |
//! | `pointer` | `on-click`, `on-pointer-down`, …  | `on:<method>` (drops the `on-` prefix) |
//! | `a11y`    | `label`, `role`, `hidden`, …      | `aria:<method>`      |
//!
//! Resolution is a one-shot lookup: `trait.method` → `TraitTarget`,
//! which carries a routing hint downstream attribute classifiers can
//! consume. The §7.4 plan is for the parser-side classifier to grow
//! a single `split_at_first_dot` branch that consults this registry;
//! Phase 9 lands the registry + the four built-ins, and downstream
//! parser work in Phase 17 can fold the legacy colon-prefix forms
//! into the new dotted shape uniformly.
//!
//! The registry is `Arc`-cheap to clone (one Arc-bound `HashMap`),
//! and is propagated through every `LowerScope` fork the same way
//! every other immutable scope-context field is. Default is the four
//! built-ins; user code can install an extended registry via
//! [`LowerScope::with_trait_registry`](super::LowerScope::with_trait_registry).

use std::collections::HashMap;
use std::sync::Arc;

use prism_core::language::prism_ui::AttributeNamespace;

/// How a `trait.method=` attribute lowers downstream. The variants
/// cover the four built-in traits today; opening the registry means
/// a user-provided trait can pick any of these targets (or the
/// generic `Data` pass-through) without grammar edits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TraitTarget {
    /// Routes to a bare property — same as today's `gap=12`,
    /// `direction=column`. `layout.*` lowers this way.
    Bare,
    /// Routes to the `style:` namespace (existing token-resolved
    /// style attribute). `style.background=accent` ≡
    /// `style:background=accent`.
    Style,
    /// Routes to the `on:` namespace (event handler). `pointer.on-click=
    /// $cb` ≡ `on:click=$cb`. The local part drops the `on-` prefix
    /// — `pointer.click=` is also accepted as a synonym for
    /// authoring ergonomics.
    On,
    /// Routes to the `aria:` namespace (a11y pass-through).
    /// `a11y.label="Submit"` ≡ `aria:label="Submit"`.
    Aria,
    /// Routes to the `data:` namespace — open vocabulary, identical
    /// to today's `data:<key>` pass-through. The default lowering for
    /// a registered trait whose target wasn't otherwise specified.
    Data,
}

impl TraitTarget {
    /// The matching closed-enum [`AttributeNamespace`] — kept around
    /// for downstream lowering code that still keys off the existing
    /// namespace machinery. New attribute kinds (Phase 13+) can
    /// short-circuit through the registry directly without needing
    /// a backing enum variant.
    pub fn namespace(self) -> AttributeNamespace {
        match self {
            TraitTarget::Bare => AttributeNamespace::Bare,
            TraitTarget::Style => AttributeNamespace::Style,
            TraitTarget::On => AttributeNamespace::On,
            TraitTarget::Aria => AttributeNamespace::Aria,
            TraitTarget::Data => AttributeNamespace::Data,
        }
    }
}

/// Open trait registry — name → routing target. Cloning is cheap
/// (`Arc<HashMap<…>>`); the inner map is built once at construction
/// and replaced wholesale through [`Self::with_trait`].
#[derive(Debug, Clone)]
pub struct TraitRegistry {
    traits: Arc<HashMap<String, TraitTarget>>,
}

impl Default for TraitRegistry {
    fn default() -> Self {
        Self::builtin()
    }
}

impl TraitRegistry {
    /// An empty registry — useful for tests that want to assert the
    /// closed set behaves identically without any trait dispatch.
    pub fn empty() -> Self {
        Self {
            traits: Arc::new(HashMap::new()),
        }
    }

    /// The four built-in traits per §7.4 "the 17 namespaces — fate"
    /// table. Authors get the dotted form (`style.background=…`) for
    /// free without registering anything; user-defined traits layer
    /// on top via [`Self::with_trait`].
    pub fn builtin() -> Self {
        let mut traits = HashMap::with_capacity(4);
        traits.insert("layout".to_string(), TraitTarget::Bare);
        traits.insert("style".to_string(), TraitTarget::Style);
        traits.insert("pointer".to_string(), TraitTarget::On);
        traits.insert("a11y".to_string(), TraitTarget::Aria);
        Self {
            traits: Arc::new(traits),
        }
    }

    /// Register `name → target`, returning a fresh registry. The
    /// existing entries are preserved; a colliding `name` is replaced
    /// (last-write-wins — user traits may shadow the built-ins).
    pub fn with_trait(mut self, name: impl Into<String>, target: TraitTarget) -> Self {
        let mut map = (*self.traits).clone();
        map.insert(name.into(), target);
        self.traits = Arc::new(map);
        self
    }

    /// Look up `trait_name` and return its routing target. Returns
    /// `None` for an unknown trait — the caller should fall back to
    /// the closed-enum classifier (the parser's existing
    /// `AttributeNamespace::classify`).
    pub fn lookup(&self, trait_name: &str) -> Option<TraitTarget> {
        self.traits.get(trait_name).copied()
    }

    /// Classify a raw attribute name (`trait.method` shape). Returns
    /// `Some((target, local))` when the prefix before the first `.`
    /// is a registered trait, and `local` is everything after the
    /// dot — with the leading `on-` stripped when the target is
    /// [`TraitTarget::On`] (`pointer.on-click` → local `click`).
    /// Returns `None` for bare attribute names, colon-namespaced
    /// names, and unknown trait prefixes.
    pub fn classify(&self, raw: &str) -> Option<(TraitTarget, String)> {
        let (prefix, rest) = raw.split_once('.')?;
        let target = self.lookup(prefix)?;
        let local = match target {
            TraitTarget::On => rest.strip_prefix("on-").unwrap_or(rest).to_string(),
            _ => rest.to_string(),
        };
        if local.is_empty() {
            return None;
        }
        Some((target, local))
    }

    /// Number of registered traits — handy for asserting the
    /// built-in seed in tests.
    pub fn len(&self) -> usize {
        self.traits.len()
    }

    pub fn is_empty(&self) -> bool {
        self.traits.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_seeds_four_traits() {
        let r = TraitRegistry::builtin();
        assert_eq!(r.len(), 4);
        assert_eq!(r.lookup("layout"), Some(TraitTarget::Bare));
        assert_eq!(r.lookup("style"), Some(TraitTarget::Style));
        assert_eq!(r.lookup("pointer"), Some(TraitTarget::On));
        assert_eq!(r.lookup("a11y"), Some(TraitTarget::Aria));
    }

    #[test]
    fn classify_style_dot_background() {
        let r = TraitRegistry::builtin();
        let (target, local) = r.classify("style.background").unwrap();
        assert_eq!(target, TraitTarget::Style);
        assert_eq!(local, "background");
    }

    #[test]
    fn classify_layout_dot_gap_routes_to_bare() {
        let r = TraitRegistry::builtin();
        let (target, local) = r.classify("layout.gap").unwrap();
        assert_eq!(target, TraitTarget::Bare);
        assert_eq!(local, "gap");
    }

    #[test]
    fn classify_pointer_on_click_strips_prefix() {
        let r = TraitRegistry::builtin();
        let (target, local) = r.classify("pointer.on-click").unwrap();
        assert_eq!(target, TraitTarget::On);
        assert_eq!(local, "click");
    }

    #[test]
    fn classify_pointer_bare_click_is_accepted() {
        // Authoring ergonomics: `pointer.click` is the natural form
        // and should reach the same target as `pointer.on-click`.
        let r = TraitRegistry::builtin();
        let (target, local) = r.classify("pointer.click").unwrap();
        assert_eq!(target, TraitTarget::On);
        assert_eq!(local, "click");
    }

    #[test]
    fn classify_a11y_label_routes_to_aria() {
        let r = TraitRegistry::builtin();
        let (target, local) = r.classify("a11y.label").unwrap();
        assert_eq!(target, TraitTarget::Aria);
        assert_eq!(local, "label");
    }

    #[test]
    fn classify_unknown_trait_returns_none() {
        let r = TraitRegistry::builtin();
        assert!(r.classify("widget.foo").is_none());
    }

    #[test]
    fn classify_bare_name_returns_none() {
        let r = TraitRegistry::builtin();
        assert!(r.classify("bare-prop").is_none());
        assert!(r.classify("on:click").is_none());
    }

    #[test]
    fn empty_registry_classifies_nothing() {
        let r = TraitRegistry::empty();
        assert!(r.is_empty());
        assert!(r.classify("style.background").is_none());
    }

    #[test]
    fn with_trait_extends_registry() {
        let r = TraitRegistry::builtin().with_trait("drag", TraitTarget::Data);
        assert_eq!(r.lookup("drag"), Some(TraitTarget::Data));
        // Built-ins survive.
        assert_eq!(r.lookup("style"), Some(TraitTarget::Style));
    }

    #[test]
    fn with_trait_shadows_builtin() {
        let r = TraitRegistry::builtin().with_trait("style", TraitTarget::Data);
        assert_eq!(r.lookup("style"), Some(TraitTarget::Data));
    }
}
