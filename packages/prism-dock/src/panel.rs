//! Declarative panel catalog — every dockable panel is one row in
//! [`PanelKind::ALL`].
//!
//! `PanelKind` is a *data* struct (not an enum) holding everything the
//! dock or its consumers need to know about a panel: its id, its
//! human-facing label, its icon hint, its minimum size, whether
//! multiple instances are allowed, and (optionally) the `shell.*`
//! content tag the renderer should embed inside the dock leaf.
//!
//! Adding a new panel = one `pub const FOO: PanelKind = PanelKind { ... }`
//! plus one `&Self::FOO` row in [`PanelKind::ALL`]. No enum match
//! arms, no parallel routing table, no serde dance. The shell-side
//! content-tag mapping (formerly `prism_shell::components::panel_routing`)
//! is now folded into the `tag` field on each spec.
//!
//! See `docs/dev/clay-migration-plan.md` §32–§34 for the broader
//! "one row per registered thing" pattern this collapse follows.

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

    /// Single source of truth for the dockable panel catalog. Adding
    /// a new panel = one `pub const` above + one `&Self::FOO` row here.
    pub const ALL: &'static [&'static PanelKind] = &[
        &Self::BUILDER,
        &Self::INSPECTOR,
        &Self::PROPERTIES,
        &Self::EXPLORER,
        &Self::CODE_EDITOR,
        &Self::IDENTITY,
        &Self::TIMELINE,
        &Self::NODE_GRAPH,
        &Self::ASSET_BROWSER,
        &Self::COMPONENT_PALETTE,
        &Self::CONSOLE,
        &Self::SIGNALS,
        &Self::NAVIGATION,
        &Self::SCHEMA_DESIGNER,
        &Self::DOCS,
    ];

    /// Reverse lookup from kebab-case id. Linear scan is fine — the
    /// catalog is small and lookups are cold-path (skeleton resolve,
    /// not per-frame).
    pub fn from_id(id: &str) -> Option<&'static PanelKind> {
        Self::ALL.iter().copied().find(|p| p.id == id)
    }

    /// Convenience: shell content tag (the `tag` field). Subsumes the
    /// former `prism_shell::components::panel_routing::tag_for_panel`.
    pub fn tag_for(panel_id: &str) -> Option<&'static str> {
        Self::from_id(panel_id).and_then(|p| p.tag)
    }

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

    #[test]
    fn ids_are_kebab() {
        assert_eq!(PanelKind::CODE_EDITOR.id, "code-editor");
        assert_eq!(PanelKind::NODE_GRAPH.id, "node-graph");
        assert_eq!(PanelKind::BUILDER.id, "builder");
    }

    #[test]
    fn meta_fields_populated() {
        assert_eq!(PanelKind::TIMELINE.label, "Timeline");
        // The only multi-instance panel today is `CodeEditor` — every
        // other built-in is single-instance.
        let multi: Vec<&str> = PanelKind::ALL
            .iter()
            .filter(|p| p.allow_multiple)
            .map(|p| p.id)
            .collect();
        assert_eq!(multi, ["code-editor"]);
    }

    #[test]
    fn all_kinds_have_label_and_min_size() {
        for k in PanelKind::ALL {
            assert!(!k.label.is_empty());
            assert!(k.min_width > 0.0);
            assert!(k.min_height > 0.0);
        }
    }

    #[test]
    fn from_id_roundtrip() {
        for k in PanelKind::ALL {
            let recovered = PanelKind::from_id(k.id).unwrap();
            assert_eq!(recovered.id, k.id);
        }
    }

    #[test]
    fn from_id_unknown() {
        assert!(PanelKind::from_id("nonexistent").is_none());
        assert!(PanelKind::from_id("").is_none());
    }

    #[test]
    fn tag_for_known_panel() {
        assert_eq!(PanelKind::tag_for("builder"), Some("shell.builder-canvas"));
        assert_eq!(
            PanelKind::tag_for("inspector"),
            Some("shell.inspector-tree"),
        );
    }

    #[test]
    fn tag_for_unmapped_panel() {
        assert_eq!(PanelKind::tag_for("console"), None);
        assert_eq!(PanelKind::tag_for("not-a-panel"), None);
    }

    #[test]
    fn panel_id_is_owned_string() {
        let id: PanelId = PanelKind::BUILDER.panel_id();
        assert_eq!(id, "builder");
    }
}
