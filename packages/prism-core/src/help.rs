//! Context-sensitive help registry.
//!
//! Pure Rust data layer — no UI. Any crate in the workspace can register
//! `HelpEntry` objects into a `HelpRegistry`; the shell crate looks them
//! up at hover time to populate tooltips.
//!
//! Port of the React `HelpRegistry` from
//! `packages/prism-core/src/bindings/react-shell/help/help-registry.ts`
//! per ADR-005.

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

/// Trait for types that contribute help entries to the registry.
///
/// Implement this on any struct that owns help-relevant metadata
/// (components, panels, registries) so it can register its own
/// entries without a central manifest.
pub trait HelpProvider {
    fn help_entries(&self) -> Vec<HelpEntry>;
}

/// Convenience impl: a bare `HelpEntry` is its own single-entry provider.
impl HelpProvider for HelpEntry {
    fn help_entries(&self) -> Vec<HelpEntry> {
        vec![self.clone()]
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HelpEntry {
    pub id: String,
    pub title: String,
    pub summary: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub doc_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub doc_anchor: Option<String>,
}

impl HelpEntry {
    pub fn new(
        id: impl Into<String>,
        title: impl Into<String>,
        summary: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            summary: summary.into(),
            body: None,
            doc_path: None,
            doc_anchor: None,
        }
    }

    pub fn with_body(mut self, body: impl Into<String>) -> Self {
        self.body = Some(body.into());
        self
    }

    pub fn with_doc(mut self, path: impl Into<String>) -> Self {
        self.doc_path = Some(path.into());
        self
    }

    pub fn with_anchor(mut self, anchor: impl Into<String>) -> Self {
        self.doc_anchor = Some(anchor.into());
        self
    }
}

#[derive(Debug, Default)]
pub struct HelpRegistry {
    entries: IndexMap<String, HelpEntry>,
}

impl HelpRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, entry: HelpEntry) {
        self.entries.insert(entry.id.clone(), entry);
    }

    pub fn register_many(&mut self, entries: impl IntoIterator<Item = HelpEntry>) {
        for entry in entries {
            self.entries.insert(entry.id.clone(), entry);
        }
    }

    pub fn register_provider(&mut self, provider: &dyn HelpProvider) {
        self.register_many(provider.help_entries());
    }

    pub fn get(&self, id: &str) -> Option<&HelpEntry> {
        self.entries.get(id)
    }

    pub fn get_all(&self) -> Vec<&HelpEntry> {
        self.entries.values().collect()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// Case-insensitive AND search: every whitespace-separated word in
    /// `query` must appear somewhere in `title + " " + summary`.
    pub fn search(&self, query: &str) -> Vec<&HelpEntry> {
        let q = query.to_lowercase();
        let q = q.trim();
        if q.is_empty() {
            return Vec::new();
        }
        let words: Vec<&str> = q.split_whitespace().collect();
        self.entries
            .values()
            .filter(|entry| {
                let haystack = format!("{} {}", entry.title, entry.summary).to_lowercase();
                words.iter().all(|w| haystack.contains(w))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry_a() -> HelpEntry {
        HelpEntry::new(
            "builder.components.heading",
            "Heading",
            "Renders an h1-h6 with inline editing, alignment, and weight overrides.",
        )
        .with_doc("heading")
    }

    fn entry_b() -> HelpEntry {
        HelpEntry::new(
            "builder.components.text",
            "Text",
            "Paragraph or long-form text block.",
        )
    }

    fn entry_c() -> HelpEntry {
        HelpEntry::new(
            "builder.fields.spacing",
            "Spacing",
            "Margin and padding for this block.",
        )
    }

    #[test]
    fn register_and_get() {
        let mut reg = HelpRegistry::new();
        reg.register(entry_a());
        assert_eq!(
            reg.get("builder.components.heading").unwrap().title,
            "Heading"
        );
    }

    #[test]
    fn register_overwrites() {
        let mut reg = HelpRegistry::new();
        reg.register(entry_a());
        let mut updated = entry_a();
        updated.title = "Heading v2".into();
        reg.register(updated);
        assert_eq!(
            reg.get("builder.components.heading").unwrap().title,
            "Heading v2"
        );
    }

    #[test]
    fn register_many() {
        let mut reg = HelpRegistry::new();
        reg.register_many([entry_a(), entry_b(), entry_c()]);
        assert_eq!(reg.len(), 3);
        assert!(reg.get("builder.components.heading").is_some());
        assert!(reg.get("builder.components.text").is_some());
        assert!(reg.get("builder.fields.spacing").is_some());
    }

    #[test]
    fn get_returns_none_for_missing() {
        let reg = HelpRegistry::new();
        assert!(reg.get("nope").is_none());
    }

    #[test]
    fn get_all_returns_every_entry() {
        let mut reg = HelpRegistry::new();
        reg.register_many([entry_a(), entry_b()]);
        let all = reg.get_all();
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn search_by_title() {
        let mut reg = HelpRegistry::new();
        reg.register_many([entry_a(), entry_b(), entry_c()]);
        let results = reg.search("heading");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "builder.components.heading");
    }

    #[test]
    fn search_by_summary() {
        let mut reg = HelpRegistry::new();
        reg.register_many([entry_a(), entry_b(), entry_c()]);
        let results = reg.search("paragraph");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "builder.components.text");
    }

    #[test]
    fn search_case_insensitive() {
        let mut reg = HelpRegistry::new();
        reg.register_many([entry_a(), entry_b()]);
        assert_eq!(reg.search("HEADING").len(), 1);
        assert_eq!(reg.search("Heading").len(), 1);
        assert_eq!(reg.search("heading").len(), 1);
    }

    #[test]
    fn search_and_logic() {
        let mut reg = HelpRegistry::new();
        reg.register_many([entry_a(), entry_b(), entry_c()]);
        assert_eq!(reg.search("heading inline").len(), 1);
        assert_eq!(reg.search("heading paragraph").len(), 0);
    }

    #[test]
    fn search_empty_query_returns_empty() {
        let mut reg = HelpRegistry::new();
        reg.register(entry_a());
        assert!(reg.search("").is_empty());
        assert!(reg.search("   ").is_empty());
    }

    #[test]
    fn search_no_match_returns_empty() {
        let mut reg = HelpRegistry::new();
        reg.register(entry_a());
        assert!(reg.search("zzz-nope").is_empty());
    }

    #[test]
    fn clear_removes_all() {
        let mut reg = HelpRegistry::new();
        reg.register_many([entry_a(), entry_b()]);
        reg.clear();
        assert!(reg.is_empty());
        assert!(reg.get("builder.components.heading").is_none());
    }

    #[test]
    fn entry_builder_with_doc_and_anchor() {
        let e = HelpEntry::new("x", "X", "summary")
            .with_doc("docs/x")
            .with_anchor("section-1");
        assert_eq!(e.doc_path.as_deref(), Some("docs/x"));
        assert_eq!(e.doc_anchor.as_deref(), Some("section-1"));
    }

    #[test]
    fn entry_serializes() {
        let e = entry_a();
        let json = serde_json::to_string(&e).unwrap();
        let restored: HelpEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.title, "Heading");
        assert_eq!(restored.doc_path.as_deref(), Some("heading"));
    }
}
