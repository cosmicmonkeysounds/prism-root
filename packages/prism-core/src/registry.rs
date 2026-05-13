//! `Catalog<T>` — a thin, declarative registry the codebase uses
//! wherever it needs "an ordered map of `id → T` with a built-in
//! seeding step + an extension API."
//!
//! Designed to collapse the boilerplate across:
//! - `prism_dock::DockCatalog` (panels)
//! - `prism_shell::components::ShellComponentRegistry` (shell blocks)
//! - `prism_shell::app_registry::ShellAppRegistrar`'s component +
//!   service queues
//!
//! Each of those registries previously hand-rolled the same shape
//! (`IndexMap<String, T>` + `register / get / iter / len /
//! with_builtins`). One source of truth lives here; specialised
//! registries (`ComponentRegistry`, `ServiceRegistry`) that need
//! richer semantics — scopes, factories, command-table side effects
//! — keep their own shapes but can lean on `Catalog<T>` internally.
//!
//! See ADR-010 §"Symmetric to `ComponentRegistry` / `ModifierRegistry`"
//! and `docs/dev/dsl-self-bootstrap.md`.

use std::fmt;

use indexmap::IndexMap;

/// One row in a [`Catalog`]. The id is the lookup key + the
/// equality key for "is this a duplicate?" checks. Implementors
/// typically delegate to a `&'static str` or `String` field.
pub trait HasId {
    fn id(&self) -> &str;
}

/// Ordered map of `id → T`. Insertion order is preserved — that's
/// how built-ins seed first, app extensions land second, and
/// iteration produces a stable order.
#[derive(Clone, Debug)]
pub struct Catalog<T> {
    entries: IndexMap<String, T>,
}

impl<T> Default for Catalog<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> Catalog<T> {
    /// Empty catalog. Most callers use [`Self::with_builtins`] or
    /// [`Self::from_seed`] to start with a populated registry.
    pub fn new() -> Self {
        Self {
            entries: IndexMap::new(),
        }
    }

    /// Build a catalog and let a closure populate it. Mirrors the
    /// `with_builtins() -> Self { let mut c = Self::new();
    /// register_builtins(&mut c); c }` pattern we wrote three times
    /// before this trait existed.
    pub fn from_seed(seed: impl FnOnce(&mut Self)) -> Self {
        let mut c = Self::new();
        seed(&mut c);
        c
    }

    /// Number of currently registered entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the catalog has zero entries.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Reverse lookup from kebab-case id. Returns `None` for unknown
    /// ids.
    pub fn get(&self, id: &str) -> Option<&T> {
        self.entries.get(id)
    }

    /// Iterate every registered entry in insertion order.
    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.entries.values()
    }

    /// Iterate every registered id in insertion order.
    pub fn ids(&self) -> impl Iterator<Item = &str> {
        self.entries.keys().map(|s| s.as_str())
    }

    /// Iterate `(id, entry)` pairs in insertion order. Useful for
    /// callers that need both halves without re-deriving the id.
    pub fn entries(&self) -> impl Iterator<Item = (&str, &T)> {
        self.entries.iter().map(|(k, v)| (k.as_str(), v))
    }

    /// Direct-key registration when `T` doesn't impl [`HasId`]. The
    /// catalog accepts whatever id the caller supplies; re-registering
    /// the same id replaces. This is the path `ServiceRegistry` and
    /// the AppRegistrar queues use because their entries don't
    /// surface a natural `id()` method.
    pub fn insert(&mut self, id: impl Into<String>, value: T) -> &mut Self {
        self.entries.insert(id.into(), value);
        self
    }

    /// Drop the entry at `id`. Returns the prior entry if there was
    /// one. Used by activation passes that drop services / panels an
    /// app didn't keep.
    pub fn remove(&mut self, id: &str) -> Option<T> {
        self.entries.shift_remove(id)
    }

    /// Drain every entry, leaving the catalog empty. Returns the
    /// drained entries in insertion order so callers can hand them
    /// downstream (e.g. registrar queues drain into the live shell
    /// registries at boot).
    pub fn drain(&mut self) -> Vec<T> {
        std::mem::take(&mut self.entries)
            .into_iter()
            .map(|(_, v)| v)
            .collect()
    }
}

impl<T: HasId> Catalog<T> {
    /// `HasId`-friendly registration: the catalog derives the key
    /// from `value.id()`. Re-registering the same id replaces the
    /// prior entry, mirroring how the dock catalog handles app
    /// overrides of built-in panels.
    pub fn register(&mut self, value: T) -> &mut Self {
        let id = value.id().to_string();
        self.entries.insert(id, value);
        self
    }

    /// Whether the catalog already has an entry for this id.
    pub fn contains(&self, id: &str) -> bool {
        self.entries.contains_key(id)
    }
}

impl<T: HasId + fmt::Debug> fmt::Display for Catalog<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_map().entries(self.entries.iter()).finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, PartialEq)]
    struct Spec {
        id: String,
        label: String,
    }

    impl HasId for Spec {
        fn id(&self) -> &str {
            &self.id
        }
    }

    #[test]
    fn empty_catalog_has_no_entries() {
        let c: Catalog<Spec> = Catalog::new();
        assert!(c.is_empty());
        assert_eq!(c.len(), 0);
        assert!(c.get("foo").is_none());
    }

    #[test]
    fn register_inserts_by_self_id() {
        let mut c: Catalog<Spec> = Catalog::new();
        c.register(Spec {
            id: "alpha".into(),
            label: "Alpha".into(),
        });
        c.register(Spec {
            id: "beta".into(),
            label: "Beta".into(),
        });
        assert_eq!(c.len(), 2);
        assert_eq!(c.get("alpha").unwrap().label, "Alpha");
        assert_eq!(c.get("beta").unwrap().label, "Beta");
    }

    #[test]
    fn re_register_same_id_replaces() {
        let mut c: Catalog<Spec> = Catalog::new();
        c.register(Spec {
            id: "shared".into(),
            label: "First".into(),
        });
        c.register(Spec {
            id: "shared".into(),
            label: "Second".into(),
        });
        assert_eq!(c.len(), 1);
        assert_eq!(c.get("shared").unwrap().label, "Second");
    }

    #[test]
    fn from_seed_builds_populated_catalog() {
        let c: Catalog<Spec> = Catalog::from_seed(|c| {
            c.register(Spec {
                id: "a".into(),
                label: "A".into(),
            });
            c.register(Spec {
                id: "b".into(),
                label: "B".into(),
            });
        });
        assert_eq!(c.len(), 2);
    }

    #[test]
    fn iter_preserves_registration_order() {
        let mut c: Catalog<Spec> = Catalog::new();
        for id in ["zebra", "alpha", "monkey"] {
            c.register(Spec {
                id: id.into(),
                label: id.into(),
            });
        }
        let ids: Vec<&str> = c.ids().collect();
        assert_eq!(ids, vec!["zebra", "alpha", "monkey"]);
    }

    #[test]
    fn remove_returns_prior_entry() {
        let mut c: Catalog<Spec> = Catalog::new();
        c.register(Spec {
            id: "x".into(),
            label: "X".into(),
        });
        let popped = c.remove("x").unwrap();
        assert_eq!(popped.label, "X");
        assert!(c.is_empty());
        assert!(c.remove("x").is_none());
    }

    #[test]
    fn drain_empties_catalog_and_returns_entries_in_order() {
        let mut c: Catalog<Spec> = Catalog::new();
        c.register(Spec {
            id: "first".into(),
            label: "1".into(),
        });
        c.register(Spec {
            id: "second".into(),
            label: "2".into(),
        });
        let drained = c.drain();
        assert_eq!(drained.len(), 2);
        assert_eq!(drained[0].id, "first");
        assert_eq!(drained[1].id, "second");
        assert!(c.is_empty());
    }

    #[test]
    fn insert_path_works_without_hasid() {
        // Catalog<T> over a payload that doesn't impl HasId — caller
        // supplies the key explicitly. Same path the AppRegistrar's
        // service queue takes.
        let mut c: Catalog<String> = Catalog::new();
        c.insert("foo", "value-1".into());
        c.insert("bar", "value-2".into());
        assert_eq!(c.get("foo").map(String::as_str), Some("value-1"));
        assert_eq!(c.len(), 2);
    }

    #[test]
    fn contains_reflects_registration() {
        let mut c: Catalog<Spec> = Catalog::new();
        assert!(!c.contains("x"));
        c.register(Spec {
            id: "x".into(),
            label: "X".into(),
        });
        assert!(c.contains("x"));
    }
}
