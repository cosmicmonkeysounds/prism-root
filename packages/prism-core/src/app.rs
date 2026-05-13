//! `AppManifest` — declarative IR every Prism app declares itself
//! through. See `docs/dev/dsl-self-bootstrap.md`.
//!
//! Pure data + TOML parsing; filesystem discovery lives in
//! `prism_shell::app_loader`. Keeping this leaf-only means the
//! relay, codegen, and headless test paths can hydrate manifests
//! without touching disk.

use serde::{Deserialize, Serialize};

/// The full app declaration. Hydrated from `apps/<id>/manifest.toml`.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct AppManifest {
    /// Stable kebab-case identifier (e.g. `"lattice"`).
    #[serde(default)]
    pub id: String,
    /// Human-facing label shown on the launchpad tile + window chrome.
    #[serde(default)]
    pub label: String,
    /// Icon path relative to the app directory.
    #[serde(default)]
    pub icon: String,
    /// One-line summary used in the launchpad tile.
    #[serde(default)]
    pub summary: String,
    /// Optional DSL entry points (skeleton / styles / script).
    #[serde(default)]
    pub entry: AppEntry,
    /// Service activation spec.
    #[serde(default)]
    pub services: AppServicesSpec,
    /// Panel inclusion + extension spec.
    #[serde(default)]
    pub panels: AppPanelsSpec,
}

/// Optional DSL entry points. Each path is interpreted relative to
/// the app's manifest file.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct AppEntry {
    #[serde(default)]
    pub skeleton: Option<String>,
    #[serde(default)]
    pub styles: Option<String>,
    #[serde(default)]
    pub script: Option<String>,
}

/// Subset of optional services the app activates. Universal services
/// (input, undo/redo, command-palette, field-focus) are always on
/// regardless of what an app declares here.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct AppServicesSpec {
    #[serde(default)]
    pub required: Vec<String>,
    #[serde(default)]
    pub optional: Vec<String>,
}

/// Subset of built-in panels the app exposes plus any additional
/// panel kinds it registers itself.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct AppPanelsSpec {
    /// Whitelist of built-in panel ids. Empty = include every built-in.
    #[serde(default)]
    pub include: Vec<String>,
    /// Custom panel definitions to push into the dock catalog.
    #[serde(default)]
    pub add: Vec<AppPanelDef>,
}

/// A panel kind contributed by an app. Maps 1:1 onto
/// `prism_dock::PanelKind` but uses owned `String`s because manifest
/// data is loaded at runtime.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct AppPanelDef {
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub icon_hint: String,
    #[serde(default = "default_min_size")]
    pub min_width: f32,
    #[serde(default = "default_min_size")]
    pub min_height: f32,
    #[serde(default)]
    pub allow_multiple: bool,
    #[serde(default)]
    pub tag: Option<String>,
}

fn default_min_size() -> f32 {
    100.0
}

impl Eq for AppPanelDef {}

#[derive(Debug, thiserror::Error)]
pub enum AppManifestError {
    #[error("toml parse: {0}")]
    Toml(#[from] toml::de::Error),
    #[error("missing required field: {0}")]
    MissingField(&'static str),
}

impl AppManifest {
    /// Parse a manifest from a TOML source string.
    pub fn parse(source: &str) -> Result<Self, AppManifestError> {
        let m: AppManifest = toml::from_str(source)?;
        if m.id.trim().is_empty() {
            return Err(AppManifestError::MissingField("id"));
        }
        if m.label.trim().is_empty() {
            return Err(AppManifestError::MissingField("label"));
        }
        Ok(m)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_minimal_manifest() {
        let src = r#"
            id = "lattice"
            label = "Lattice"
        "#;
        let m = AppManifest::parse(src).unwrap();
        assert_eq!(m.id, "lattice");
        assert_eq!(m.label, "Lattice");
        assert!(m.icon.is_empty());
        assert!(m.summary.is_empty());
        assert!(m.entry.skeleton.is_none());
        assert!(m.services.required.is_empty());
        assert!(m.panels.include.is_empty());
    }

    #[test]
    fn parses_full_manifest() {
        let src = r#"
            id = "lattice"
            label = "Lattice"
            icon = "icons/grid.svg"
            summary = "Collaborative workspace."

            [entry]
            skeleton = "shell.prism-ui"
            styles = "app.prss"
            script = "main.luau"

            [services]
            required = ["selection", "builder"]
            optional = ["luau"]

            [panels]
            include = ["builder", "inspector"]

            [[panels.add]]
            id = "lattice.peers"
            label = "Peers"
            tag = "lattice.peers-panel"
            min_width = 240
        "#;
        let m = AppManifest::parse(src).unwrap();
        assert_eq!(m.entry.skeleton.as_deref(), Some("shell.prism-ui"));
        assert_eq!(m.services.required, vec!["selection", "builder"]);
        assert_eq!(m.panels.include, vec!["builder", "inspector"]);
        assert_eq!(m.panels.add.len(), 1);
        assert_eq!(m.panels.add[0].id, "lattice.peers");
        assert_eq!(m.panels.add[0].min_width, 240.0);
        assert_eq!(m.panels.add[0].min_height, 100.0);
    }

    #[test]
    fn rejects_missing_id() {
        let src = r#"label = "Lattice""#;
        let err = AppManifest::parse(src).unwrap_err();
        assert!(matches!(err, AppManifestError::MissingField("id")));
    }

    #[test]
    fn rejects_missing_label() {
        let src = r#"id = "lattice""#;
        let err = AppManifest::parse(src).unwrap_err();
        assert!(matches!(err, AppManifestError::MissingField("label")));
    }

    #[test]
    fn rejects_malformed_toml() {
        let err = AppManifest::parse("id = \"unterminated").unwrap_err();
        assert!(matches!(err, AppManifestError::Toml(_)));
    }
}
