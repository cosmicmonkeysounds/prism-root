//! `DockCatalog` — runtime panel registry.
//!
//! Replaces the const `PanelKind::ALL` table with a registration-based
//! catalog that built-ins seed via [`register_builtins`] and apps
//! extend with their own [`PanelKind`] entries.
//!
//! Mirrors the DI shape `prism_builder::ComponentRegistry` already
//! uses: `Registry::new()` + `register(entry)` + `register_builtins(&mut reg)`.
//! See `docs/dev/dsl-self-bootstrap.md` Loop 2.
//!
//! The catalog stores `PanelKind` values (Copy + 'static) directly —
//! the built-in `pub const PanelKind`s in [`panel`](crate::panel) stay
//! the canonical declarations. Apps that need to push panels with
//! owned strings build their own `PanelKind` from a manifest entry at
//! load time.

use indexmap::IndexMap;

use crate::panel::PanelKind;

/// Runtime registry of dockable panels. One row per panel id; lookups
/// are O(log n) over an `IndexMap` so iteration order is registration
/// order (built-ins first, app extensions after).
#[derive(Clone, Debug, Default)]
pub struct DockCatalog {
    entries: IndexMap<String, PanelKind>,
}

impl DockCatalog {
    /// Empty catalog. Use [`Self::with_builtins`] for the seeded form.
    pub fn new() -> Self {
        Self {
            entries: IndexMap::new(),
        }
    }

    /// Seeded with the built-in panel set via [`register_builtins`].
    pub fn with_builtins() -> Self {
        let mut cat = Self::new();
        register_builtins(&mut cat);
        cat
    }

    /// Push a panel into the catalog. Re-registering the same id
    /// replaces the prior entry — apps can override a built-in by
    /// re-registering with the same id.
    pub fn register(&mut self, kind: PanelKind) -> &mut Self {
        self.entries.insert(kind.id.to_string(), kind);
        self
    }

    /// Reverse lookup from kebab-case id.
    pub fn get(&self, id: &str) -> Option<&PanelKind> {
        self.entries.get(id)
    }

    /// Shell content tag for a panel id (the `tag` field on
    /// [`PanelKind`]). Replaces the static `PanelKind::tag_for`.
    pub fn tag_for(&self, id: &str) -> Option<&'static str> {
        self.get(id).and_then(|p| p.tag)
    }

    /// Iterate over every registered panel in insertion order.
    pub fn panels(&self) -> impl Iterator<Item = &PanelKind> {
        self.entries.values()
    }

    /// Iterate over every registered id in insertion order.
    pub fn ids(&self) -> impl Iterator<Item = &str> {
        self.entries.keys().map(|s| s.as_str())
    }

    /// Number of registered panels.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the catalog has zero entries.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Push every built-in panel into the catalog. Single source of truth
/// for the 15 built-in panel kinds — adding a new one is one
/// `pub const PanelKind` in [`panel`](crate::panel) plus one row here.
pub fn register_builtins(catalog: &mut DockCatalog) {
    catalog.register(PanelKind::BUILDER);
    catalog.register(PanelKind::INSPECTOR);
    catalog.register(PanelKind::PROPERTIES);
    catalog.register(PanelKind::EXPLORER);
    catalog.register(PanelKind::CODE_EDITOR);
    catalog.register(PanelKind::IDENTITY);
    catalog.register(PanelKind::TIMELINE);
    catalog.register(PanelKind::NODE_GRAPH);
    catalog.register(PanelKind::ASSET_BROWSER);
    catalog.register(PanelKind::COMPONENT_PALETTE);
    catalog.register(PanelKind::CONSOLE);
    catalog.register(PanelKind::SIGNALS);
    catalog.register(PanelKind::NAVIGATION);
    catalog.register(PanelKind::SCHEMA_DESIGNER);
    catalog.register(PanelKind::DOCS);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_catalog_has_no_entries() {
        let cat = DockCatalog::new();
        assert_eq!(cat.len(), 0);
        assert!(cat.is_empty());
        assert!(cat.get("builder").is_none());
        assert!(cat.tag_for("builder").is_none());
    }

    #[test]
    fn builtins_seed_every_panel() {
        let cat = DockCatalog::with_builtins();
        assert_eq!(cat.len(), 15);
        for id in [
            "builder",
            "inspector",
            "properties",
            "explorer",
            "code-editor",
            "identity",
            "timeline",
            "node-graph",
            "asset-browser",
            "component-palette",
            "console",
            "signals",
            "navigation",
            "schema-designer",
            "docs",
        ] {
            assert!(cat.get(id).is_some(), "builtin `{id}` missing");
        }
    }

    #[test]
    fn tag_for_resolves_through_catalog() {
        let cat = DockCatalog::with_builtins();
        assert_eq!(cat.tag_for("builder"), Some("shell.builder-canvas"));
        assert_eq!(cat.tag_for("inspector"), Some("shell.inspector-tree"));
        // Panels without a `tag` resolve to None (Console is the canonical example).
        assert_eq!(cat.tag_for("console"), None);
        assert_eq!(cat.tag_for("not-a-panel"), None);
    }

    #[test]
    fn registering_same_id_replaces_entry() {
        // Apps overriding a built-in: register a `builder` panel with
        // a different label, last-writer-wins.
        let mut cat = DockCatalog::with_builtins();
        cat.register(PanelKind {
            id: "builder",
            label: "Builder (custom)",
            icon_hint: "builder",
            min_width: 200.0,
            min_height: 100.0,
            allow_multiple: false,
            tag: Some("my.builder-canvas"),
        });
        let p = cat.get("builder").unwrap();
        assert_eq!(p.label, "Builder (custom)");
        assert_eq!(p.tag, Some("my.builder-canvas"));
        assert_eq!(cat.len(), 15, "override should not grow the catalog");
    }

    #[test]
    fn ids_preserve_registration_order() {
        let cat = DockCatalog::with_builtins();
        let ids: Vec<&str> = cat.ids().collect();
        assert_eq!(ids.first(), Some(&"builder"));
        assert_eq!(ids.last(), Some(&"docs"));
    }
}
