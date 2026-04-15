//! `LanguageRegistry` — the unified language/document registry from
//! ADR-002 §A2.
//!
//! Port of `language/registry/language-registry.ts`. Owns every
//! [`LanguageContribution`] known to the current kernel, indexed by
//! id and by file extension.
//!
//! Resolution order for `resolve({ filename })`:
//!   1. longest compound extension first (`.loom.ink` beats `.ink`)
//!   2. final extension fallback
//!
//! Resolution order for `resolve({ id, filename })`:
//!   1. explicit `id` wins if present
//!   2. otherwise fall through to filename lookup

use indexmap::IndexMap;
use std::collections::HashMap;

use crate::language::document::PrismFile;

use super::language_contribution::LanguageContribution;

/// Options for [`LanguageRegistry::resolve`].
#[derive(Debug, Clone, Default)]
pub struct ResolveOptions<'a> {
    pub id: Option<&'a str>,
    pub filename: Option<&'a str>,
}

impl<'a> ResolveOptions<'a> {
    pub fn by_id(id: &'a str) -> Self {
        Self {
            id: Some(id),
            filename: None,
        }
    }

    pub fn by_filename(filename: &'a str) -> Self {
        Self {
            id: None,
            filename: Some(filename),
        }
    }
}

/// Registry that owns every [`LanguageContribution`] known to the
/// current kernel.
pub struct LanguageRegistry<R = (), E = ()> {
    by_id: IndexMap<String, LanguageContribution<R, E>>,
    by_ext: HashMap<String, String>,
}

impl<R, E> Default for LanguageRegistry<R, E> {
    fn default() -> Self {
        Self {
            by_id: IndexMap::new(),
            by_ext: HashMap::new(),
        }
    }
}

impl<R, E> LanguageRegistry<R, E> {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a contribution. Later registrations replace earlier
    /// ones with the same id; extension mappings for the old record
    /// are cleared before the new ones install.
    pub fn register(&mut self, contribution: LanguageContribution<R, E>) -> &mut Self {
        let id = contribution.id.clone();
        if self.by_id.contains_key(&id) {
            self.clear_exts_for(&id);
        }
        for ext in &contribution.extensions {
            self.by_ext.insert(ext.to_ascii_lowercase(), id.clone());
        }
        self.by_id.insert(id, contribution);
        self
    }

    /// Unregister a contribution by id. Drops all its extension
    /// mappings.
    pub fn unregister(&mut self, id: &str) {
        if self.by_id.shift_remove(id).is_some() {
            self.clear_exts_for(id);
        }
    }

    fn clear_exts_for(&mut self, id: &str) {
        self.by_ext.retain(|_, mapped| mapped != id);
    }

    /// Look up a contribution by its namespaced id.
    pub fn get(&self, id: &str) -> Option<&LanguageContribution<R, E>> {
        self.by_id.get(id)
    }

    /// Look up a contribution by a single extension. Accepts the
    /// extension with or without a leading dot.
    pub fn get_by_extension(&self, ext: &str) -> Option<&LanguageContribution<R, E>> {
        let key = if ext.starts_with('.') {
            ext.to_ascii_lowercase()
        } else {
            format!(".{}", ext.to_ascii_lowercase())
        };
        let id = self.by_ext.get(&key)?;
        self.by_id.get(id)
    }

    /// Resolve a contribution from a file path by trying compound
    /// extensions first (longest match wins).
    pub fn resolve_by_path(&self, file_path: &str) -> Option<&LanguageContribution<R, E>> {
        let lower = file_path.to_ascii_lowercase();
        let parts: Vec<&str> = lower.split('.').collect();
        for i in 1..parts.len() {
            let compound = format!(".{}", parts[i..].join("."));
            if let Some(id) = self.by_ext.get(&compound) {
                return self.by_id.get(id);
            }
        }
        None
    }

    /// Unified resolver — id override first, filename extension
    /// second.
    pub fn resolve(&self, options: ResolveOptions<'_>) -> Option<&LanguageContribution<R, E>> {
        if let Some(id) = options.id {
            return self.get(id);
        }
        if let Some(filename) = options.filename {
            return self.resolve_by_path(filename);
        }
        None
    }

    /// Resolve the contribution that should drive a given
    /// [`PrismFile`]. Honours an explicit `language_id` override and
    /// otherwise falls through to compound-extension resolution on
    /// the file path. The surface the caller should open is stored
    /// on `PrismFile::surface_id` separately and is not consulted
    /// here — that lookup lives on the contribution's surface.
    ///
    /// This is the Phase-4 ADR-002 wiring that lets Studio go from a
    /// `PrismFile` directly to its language contribution without any
    /// intermediate registry.
    pub fn resolve_file(&self, file: &PrismFile) -> Option<&LanguageContribution<R, E>> {
        self.resolve(ResolveOptions {
            id: file.language_id.as_deref(),
            filename: Some(file.path.as_str()),
        })
    }

    /// All registered contributions, in registration order.
    pub fn all(&self) -> impl Iterator<Item = &LanguageContribution<R, E>> {
        self.by_id.values()
    }

    /// All registered contribution ids, in registration order.
    pub fn ids(&self) -> impl Iterator<Item = &str> {
        self.by_id.keys().map(String::as_str)
    }

    /// Total number of registered contributions.
    pub fn len(&self) -> usize {
        self.by_id.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_id.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::language::registry::language_contribution::LanguageSurface;
    use crate::language::registry::surface_types::SurfaceMode;

    fn fixture(id: &str, exts: &[&str]) -> LanguageContribution<(), ()> {
        let surface = LanguageSurface::new(SurfaceMode::Code, vec![SurfaceMode::Code]);
        LanguageContribution::new(id, exts.iter().copied(), id, surface)
    }

    #[test]
    fn register_and_get_by_id() {
        let mut reg = LanguageRegistry::<(), ()>::new();
        reg.register(fixture("prism:markdown", &[".md", ".mdx"]));
        assert!(reg.get("prism:markdown").is_some());
        assert_eq!(reg.len(), 1);
    }

    #[test]
    fn extension_lookup_is_case_insensitive() {
        let mut reg = LanguageRegistry::<(), ()>::new();
        reg.register(fixture("prism:json", &[".json"]));
        assert!(reg.get_by_extension(".JSON").is_some());
        assert!(reg.get_by_extension("json").is_some());
    }

    #[test]
    fn resolve_by_path_prefers_compound_extensions() {
        let mut reg = LanguageRegistry::<(), ()>::new();
        reg.register(fixture("prism:ink", &[".ink"]));
        reg.register(fixture("prism:loom", &[".loom.ink"]));
        let hit = reg.resolve_by_path("pages/story.loom.ink").unwrap();
        assert_eq!(hit.id, "prism:loom");
    }

    #[test]
    fn resolve_by_path_falls_back_to_final_extension() {
        let mut reg = LanguageRegistry::<(), ()>::new();
        reg.register(fixture("prism:ink", &[".ink"]));
        let hit = reg.resolve_by_path("pages/story.ink").unwrap();
        assert_eq!(hit.id, "prism:ink");
    }

    #[test]
    fn resolve_id_wins_over_filename() {
        let mut reg = LanguageRegistry::<(), ()>::new();
        reg.register(fixture("prism:markdown", &[".md"]));
        reg.register(fixture("prism:plaintext", &[".txt"]));
        let hit = reg
            .resolve(ResolveOptions {
                id: Some("prism:plaintext"),
                filename: Some("readme.md"),
            })
            .unwrap();
        assert_eq!(hit.id, "prism:plaintext");
    }

    #[test]
    fn re_register_replaces_old_extensions() {
        let mut reg = LanguageRegistry::<(), ()>::new();
        reg.register(fixture("prism:demo", &[".old"]));
        reg.register(fixture("prism:demo", &[".new"]));
        assert!(reg.get_by_extension(".old").is_none());
        assert!(reg.get_by_extension(".new").is_some());
        assert_eq!(reg.len(), 1);
    }

    #[test]
    fn unregister_drops_extension_mappings() {
        let mut reg = LanguageRegistry::<(), ()>::new();
        reg.register(fixture("prism:gone", &[".gone"]));
        reg.unregister("prism:gone");
        assert!(reg.get("prism:gone").is_none());
        assert!(reg.get_by_extension(".gone").is_none());
        assert!(reg.is_empty());
    }

    // ── PrismFile integration (ADR-002 §A1 / Phase 4) ─────────────
    mod prism_file {
        use super::*;
        use crate::foundation::object_model::GraphObject;
        use crate::foundation::vfs::BinaryRef;
        use crate::language::document::{
            create_binary_file, create_graph_file, create_text_file, BinaryFileParams,
            GraphFileParams, TextFileParams,
        };
        use chrono::Utc;

        fn registry() -> LanguageRegistry<(), ()> {
            let mut reg = LanguageRegistry::new();
            reg.register(fixture("prism:markdown", &[".md"]));
            reg.register(fixture("prism:luau", &[".luau", ".lua"]));
            reg
        }

        #[test]
        fn resolves_text_file_by_path() {
            let reg = registry();
            let file = create_text_file(TextFileParams {
                path: "notes/today.md".into(),
                text: "# hi".into(),
                ..Default::default()
            });
            let hit = reg.resolve_file(&file).expect("resolved");
            assert_eq!(hit.id, "prism:markdown");
        }

        #[test]
        fn resolves_graph_file_by_id_override() {
            let reg = registry();
            let object = GraphObject::new("abc", "task", "Buy milk");
            let file = create_graph_file(GraphFileParams {
                path: "irrelevant.unknown".into(),
                object,
                language_id: Some("prism:luau".into()),
                surface_id: None,
                schema: None,
                metadata: None,
            });
            let hit = reg.resolve_file(&file).expect("resolved");
            assert_eq!(hit.id, "prism:luau");
        }

        #[test]
        fn resolves_binary_file_by_vfs_filename_extension() {
            let reg = registry();
            let binary = BinaryRef {
                hash: "cafef00d".into(),
                filename: "readme.md".into(),
                mime_type: "text/markdown".into(),
                size: 42,
                imported_at: Utc::now(),
            };
            // The `path` field is the surface identity; binary bodies
            // still resolve through it because the VFS filename lives
            // on the BinaryRef.
            let file = create_binary_file(BinaryFileParams {
                path: "assets/readme.md".into(),
                binary,
                language_id: None,
                surface_id: None,
                metadata: None,
            });
            let hit = reg.resolve_file(&file).expect("resolved");
            assert_eq!(hit.id, "prism:markdown");
        }

        #[test]
        fn unknown_file_returns_none() {
            let reg = registry();
            let file = create_text_file(TextFileParams {
                path: "mystery.xyz".into(),
                text: "?".into(),
                ..Default::default()
            });
            assert!(reg.resolve_file(&file).is_none());
        }
    }
}
