//! Declarative panel data. Built-ins are `pub const PanelKind` rows
//! below; the runtime registry that holds them is the
//! [`DockCatalog`](crate::DockCatalog), seeded by
//! [`register_builtins`](crate::register_builtins).
//!
//! `PanelKind` is a *data* struct (not an enum) holding everything the
//! dock or its consumers need to know about a panel: its id, its
//! human-facing label, its icon hint, its minimum size, whether
//! multiple instances are allowed, and (optionally) the `shell.*`
//! content tag the renderer should embed inside the dock leaf.
//!
//! Adding a new built-in panel = one `pub const FOO: PanelKind` row
//! below + one `catalog.register(PanelKind::FOO)` line in
//! [`catalog::register_builtins`](crate::catalog::register_builtins).
//! Apps push their own [`PanelKind`] entries through the same
//! `catalog.register` method — see `docs/dev/dsl-self-bootstrap.md`
//! Loop 2.

use serde::{Deserialize, Serialize};

pub type PanelId = String;

/// One dockable panel — id, presentation, sizing, and (optionally)
/// the shell content tag to embed at render time.
///
/// Treated as a `pub const` everywhere; cheap to copy because every
/// field is `&'static str` / `f32` / `bool` / `Option<&'static str>`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PanelKind {
    /// Stable kebab-case identifier (e.g. `"builder"`, `"code-editor"`).
    pub id: &'static str,
    /// Human-facing label used in tab bars + workflow page menus.
    pub label: &'static str,
    /// Icon-set hint consumed by the chrome render pass.
    pub icon_hint: &'static str,
    pub min_width: f32,
    pub min_height: f32,
    pub allow_multiple: bool,
    /// Shell content tag this panel routes to (e.g.
    /// `"shell.builder-canvas"`). `None` means the panel chrome renders
    /// empty — used for in-flight panels whose content block hasn't
    /// landed yet (e.g. `Console`).
    pub tag: Option<&'static str>,
}

impl PanelKind {
    pub const BUILDER: PanelKind = PanelKind {
        id: "builder",
        label: "Builder",
        icon_hint: "builder",
        min_width: 200.0,
        min_height: 100.0,
        allow_multiple: false,
        tag: Some("shell.builder-canvas"),
    };
    pub const INSPECTOR: PanelKind = PanelKind {
        id: "inspector",
        label: "Inspector",
        icon_hint: "inspector",
        min_width: 180.0,
        min_height: 100.0,
        allow_multiple: false,
        tag: Some("shell.inspector-tree"),
    };
    pub const PROPERTIES: PanelKind = PanelKind {
        id: "properties",
        label: "Properties",
        icon_hint: "properties",
        min_width: 180.0,
        min_height: 100.0,
        allow_multiple: false,
        tag: Some("shell.properties-panel"),
    };
    pub const EXPLORER: PanelKind = PanelKind {
        id: "explorer",
        label: "Explorer",
        icon_hint: "explorer",
        min_width: 160.0,
        min_height: 100.0,
        allow_multiple: false,
        tag: Some("shell.explorer"),
    };
    pub const CODE_EDITOR: PanelKind = PanelKind {
        id: "code-editor",
        label: "Code Editor",
        icon_hint: "code",
        min_width: 200.0,
        min_height: 100.0,
        allow_multiple: true,
        tag: Some("shell.code-editor"),
    };
    pub const IDENTITY: PanelKind = PanelKind {
        id: "identity",
        label: "Identity",
        icon_hint: "identity",
        min_width: 160.0,
        min_height: 100.0,
        allow_multiple: false,
        tag: None,
    };
    pub const TIMELINE: PanelKind = PanelKind {
        id: "timeline",
        label: "Timeline",
        icon_hint: "timeline",
        min_width: 300.0,
        min_height: 80.0,
        allow_multiple: false,
        tag: None,
    };
    pub const NODE_GRAPH: PanelKind = PanelKind {
        id: "node-graph",
        label: "Node Graph",
        icon_hint: "node-graph",
        min_width: 200.0,
        min_height: 200.0,
        allow_multiple: false,
        tag: None,
    };
    pub const ASSET_BROWSER: PanelKind = PanelKind {
        id: "asset-browser",
        label: "Asset Browser",
        icon_hint: "assets",
        min_width: 200.0,
        min_height: 100.0,
        allow_multiple: false,
        tag: None,
    };
    pub const COMPONENT_PALETTE: PanelKind = PanelKind {
        id: "component-palette",
        label: "Components",
        icon_hint: "palette",
        min_width: 160.0,
        min_height: 100.0,
        allow_multiple: false,
        tag: Some("shell.component-palette"),
    };
    pub const CONSOLE: PanelKind = PanelKind {
        id: "console",
        label: "Console",
        icon_hint: "console",
        min_width: 200.0,
        min_height: 60.0,
        allow_multiple: false,
        tag: None,
    };
    pub const SIGNALS: PanelKind = PanelKind {
        id: "signals",
        label: "Signals",
        icon_hint: "signals",
        min_width: 180.0,
        min_height: 100.0,
        allow_multiple: false,
        tag: Some("shell.signals-panel"),
    };
    pub const NAVIGATION: PanelKind = PanelKind {
        id: "navigation",
        label: "Navigation",
        icon_hint: "navigation",
        min_width: 200.0,
        min_height: 200.0,
        allow_multiple: false,
        tag: Some("shell.nav-graph"),
    };
    pub const SCHEMA_DESIGNER: PanelKind = PanelKind {
        id: "schema-designer",
        label: "Schema Designer",
        icon_hint: "schema",
        min_width: 300.0,
        min_height: 200.0,
        allow_multiple: false,
        tag: Some("shell.schema-designer"),
    };
    pub const DOCS: PanelKind = PanelKind {
        id: "docs",
        label: "Docs",
        icon_hint: "docs",
        min_width: 200.0,
        min_height: 100.0,
        allow_multiple: false,
        tag: Some("shell.docs-view"),
    };
    /// **IDE-mode Phase 4 / cross-cutting §4.3** — runtime
    /// inspector + DevTools surface. Four lenses (Document /
    /// Presence / Probes / Bindings) packed into one tabbed panel.
    /// Sister to `INSPECTOR` (which is the canvas-node tree) — this
    /// one is the runtime-state lens for live debugging.
    pub const DEVTOOLS: PanelKind = PanelKind {
        id: "devtools",
        label: "Inspector",
        icon_hint: "inspector",
        min_width: 240.0,
        min_height: 160.0,
        allow_multiple: false,
        tag: Some("shell.devtools"),
    };

    /// IDE-mode Phase 3 — the Luau diagnostics / problems panel.
    pub const DIAGNOSTICS: PanelKind = PanelKind {
        id: "diagnostics",
        label: "Problems",
        icon_hint: "diagnostics",
        min_width: 240.0,
        min_height: 140.0,
        allow_multiple: false,
        tag: Some("shell.diagnostics-panel"),
    };

    /// Owned `PanelId` (kebab-case `String`). Equivalent to `id.to_string()`
    /// — kept as a method so the `PanelKind::BUILDER.panel_id()` call
    /// sites read consistently with the `WorkflowPage` / `DockNode`
    /// APIs that take an owned `PanelId`.
    pub fn panel_id(&self) -> PanelId {
        self.id.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::DockCatalog;

    #[test]
    fn ids_are_kebab() {
        assert_eq!(PanelKind::CODE_EDITOR.id, "code-editor");
        assert_eq!(PanelKind::NODE_GRAPH.id, "node-graph");
        assert_eq!(PanelKind::BUILDER.id, "builder");
    }

    #[test]
    fn meta_fields_populated() {
        assert_eq!(PanelKind::TIMELINE.label, "Timeline");
        // The only multi-instance built-in is `CodeEditor` — every
        // other built-in is single-instance.
        let cat = DockCatalog::with_builtins();
        let multi: Vec<&str> = cat
            .panels()
            .filter(|p| p.allow_multiple)
            .map(|p| p.id)
            .collect();
        assert_eq!(multi, ["code-editor"]);
    }

    #[test]
    fn all_kinds_have_label_and_min_size() {
        let cat = DockCatalog::with_builtins();
        for k in cat.panels() {
            assert!(!k.label.is_empty());
            assert!(k.min_width > 0.0);
            assert!(k.min_height > 0.0);
        }
    }

    #[test]
    fn panel_id_is_owned_string() {
        let id: PanelId = PanelKind::BUILDER.panel_id();
        assert_eq!(id, "builder");
    }
}
