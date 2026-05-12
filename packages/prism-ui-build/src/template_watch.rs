//! `template_watch` — the Phase 10 hot-reload consumer.
//!
//! Sits next to the `template_hash` primitive: a [`FingerprintCache`]
//! keeps the last-seen [`TemplateFingerprint`] of every watched
//! `.prism-ui` file and classifies each on-disk edit into a
//! [`TemplateChange`] (no-op / literal-only / structural / parse
//! error / new file). The CLI's dev loop uses this to decide
//! whether to take the literal-only fast path (patch the running
//! shell's slot table without re-evaluating the AST) or fall back
//! to a full respawn.
//!
//! ## Why a separate module
//!
//! The dev-loop side cares about the *change classification*; the
//! `template_hash` module cares about the *hash math*. Splitting
//! them keeps the consumer testable in isolation and the
//! hashing primitives reusable for other consumers (build-time
//! validation, SSR cache eviction, etc.).
//!
//! ## Lifecycle
//!
//! ```ignore
//! let mut cache = FingerprintCache::new();
//! let change = cache.observe("ui/app.prism-ui");
//! match change {
//!     TemplateChange::FirstSighting { .. } => /* seed; no patch */ ,
//!     TemplateChange::NoChange => /* skip respawn */ ,
//!     TemplateChange::LiteralOnly { patches } => /* fast path */ ,
//!     TemplateChange::Structural => /* full respawn */ ,
//!     TemplateChange::ParseError { message } => /* surface error */ ,
//! }
//! ```

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use prism_core::language::prism_ui::parse;

use crate::template_hash::{compare_fingerprints, LiteralPatch, PatchOutcome, TemplateFingerprint};

/// Classified outcome of observing a `.prism-ui` file on disk.
#[derive(Debug, Clone, PartialEq)]
pub enum TemplateChange {
    /// First time this path has been observed — fingerprint seeded,
    /// no patch to apply. The dev loop typically does the initial
    /// boot render here.
    FirstSighting { fingerprint: TemplateFingerprint },
    /// File on disk matches the last-seen fingerprint exactly.
    /// The dev loop should suppress the respawn for this file.
    NoChange,
    /// Only literal attribute values changed; the cached fast-path
    /// patches can be applied without re-evaluating the AST.
    LiteralOnly { patches: Vec<LiteralPatch> },
    /// Structure changed; the dev loop must fall back to a full
    /// respawn (or the subsecond patcher must drop the cached
    /// `Surface` and re-walk).
    Structural,
    /// The file failed to parse. The dev loop should surface the
    /// error to the user and keep the previous fingerprint cached
    /// (so the next successful save still classifies correctly).
    ParseError { message: String },
    /// The file couldn't be read off disk (deleted, permissions,
    /// in the middle of an editor's atomic-write dance). The dev
    /// loop should retry on the next batch.
    ReadError { message: String },
}

impl TemplateChange {
    /// True when the dev loop's respawn / re-render should fire.
    /// Convenience for "did anything meaningful happen?"
    pub fn needs_attention(&self) -> bool {
        !matches!(self, Self::NoChange)
    }
}

/// Path → last-seen fingerprint. Used by the dev loop to classify
/// each `.prism-ui` change.
#[derive(Default)]
pub struct FingerprintCache {
    seen: HashMap<PathBuf, TemplateFingerprint>,
}

impl FingerprintCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// Observe the current on-disk content of `path` and classify
    /// the change against the cached fingerprint. Updates the
    /// cache as a side effect (so subsequent calls compare against
    /// the latest fingerprint).
    pub fn observe(&mut self, path: impl AsRef<Path>) -> TemplateChange {
        let path = path.as_ref().to_path_buf();
        let source = match std::fs::read_to_string(&path) {
            Ok(s) => s,
            Err(e) => {
                return TemplateChange::ReadError {
                    message: format!("{}: {e}", path.display()),
                }
            }
        };
        self.observe_source(path, &source)
    }

    /// Observe a literal source string rather than reading from
    /// disk. Useful for tests + alternative ingestion (an embedded
    /// editor buffer, an LSP didChange event).
    pub fn observe_source(&mut self, path: PathBuf, source: &str) -> TemplateChange {
        let (document, errors) = parse(source);
        if !errors.is_empty() {
            let message = errors
                .iter()
                .map(|e| format!("{}: {}", e.code, e.message))
                .collect::<Vec<_>>()
                .join("; ");
            return TemplateChange::ParseError { message };
        }
        let fp = TemplateFingerprint::of(&document);
        match self.seen.get(&path) {
            None => {
                self.seen.insert(path, fp.clone());
                TemplateChange::FirstSighting { fingerprint: fp }
            }
            Some(prev) => {
                let outcome = compare_fingerprints(prev, &fp);
                let change = match outcome {
                    PatchOutcome::NoChange => TemplateChange::NoChange,
                    PatchOutcome::LiteralOnly { diffs } => {
                        TemplateChange::LiteralOnly { patches: diffs }
                    }
                    PatchOutcome::Structural => TemplateChange::Structural,
                };
                // Always update the cache, even on NoChange — keeps
                // the fingerprint Vec fresh in case the literals
                // Vec's allocation pattern matters.
                self.seen.insert(path, fp);
                change
            }
        }
    }

    /// True when the cache has a fingerprint recorded for `path`.
    pub fn contains(&self, path: impl AsRef<Path>) -> bool {
        self.seen.contains_key(path.as_ref())
    }

    /// Number of files currently fingerprinted.
    pub fn len(&self) -> usize {
        self.seen.len()
    }

    pub fn is_empty(&self) -> bool {
        self.seen.is_empty()
    }

    /// Drop the fingerprint for `path`. Returns `true` if a
    /// fingerprint was previously recorded.
    pub fn forget(&mut self, path: impl AsRef<Path>) -> bool {
        self.seen.remove(path.as_ref()).is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_sighting_seeds_cache() {
        let mut cache = FingerprintCache::new();
        let change = cache.observe_source("ui/app.prism-ui".into(), r#"<button label="Save"/>"#);
        assert!(matches!(change, TemplateChange::FirstSighting { .. }));
        assert_eq!(cache.len(), 1);
    }

    #[test]
    fn identical_content_is_no_change() {
        let mut cache = FingerprintCache::new();
        let path: PathBuf = "ui/app.prism-ui".into();
        let _ = cache.observe_source(path.clone(), r#"<button label="Save"/>"#);
        let change = cache.observe_source(path, r#"<button label="Save"/>"#);
        assert_eq!(change, TemplateChange::NoChange);
    }

    #[test]
    fn label_edit_classifies_as_literal_only() {
        let mut cache = FingerprintCache::new();
        let path: PathBuf = "ui/app.prism-ui".into();
        let _ = cache.observe_source(path.clone(), r#"<button label="Save"/>"#);
        let change = cache.observe_source(path, r#"<button label="Submit"/>"#);
        match change {
            TemplateChange::LiteralOnly { patches } => {
                assert_eq!(patches.len(), 1);
                assert_eq!(patches[0].attr, "label");
                assert_eq!(patches[0].old_value, "Save");
                assert_eq!(patches[0].new_value, "Submit");
            }
            other => panic!("expected LiteralOnly, got {other:?}"),
        }
    }

    #[test]
    fn tag_change_classifies_as_structural() {
        let mut cache = FingerprintCache::new();
        let path: PathBuf = "ui/app.prism-ui".into();
        let _ = cache.observe_source(path.clone(), r#"<button label="x"/>"#);
        let change = cache.observe_source(path, r#"<heading label="x"/>"#);
        assert_eq!(change, TemplateChange::Structural);
    }

    #[test]
    fn parse_errors_surface_messages() {
        let mut cache = FingerprintCache::new();
        let change =
            cache.observe_source("broken.prism-ui".into(), r#"<button label="unterminated"#);
        match change {
            TemplateChange::ParseError { message } => {
                assert!(!message.is_empty());
            }
            other => panic!("expected ParseError, got {other:?}"),
        }
    }

    #[test]
    fn cache_can_be_cleared_for_a_path() {
        let mut cache = FingerprintCache::new();
        let path: PathBuf = "ui/app.prism-ui".into();
        let _ = cache.observe_source(path.clone(), r#"<x/>"#);
        assert!(cache.contains(&path));
        assert!(cache.forget(&path));
        assert!(!cache.contains(&path));
        // After forget, the next observe is a first sighting again.
        let change = cache.observe_source(path, r#"<x/>"#);
        assert!(matches!(change, TemplateChange::FirstSighting { .. }));
    }

    #[test]
    fn read_error_surfaces_when_file_missing() {
        let mut cache = FingerprintCache::new();
        let change = cache.observe("/this/path/does/not/exist.prism-ui");
        assert!(matches!(change, TemplateChange::ReadError { .. }));
    }

    #[test]
    fn needs_attention_skips_no_change() {
        assert!(!TemplateChange::NoChange.needs_attention());
        assert!(TemplateChange::Structural.needs_attention());
        assert!(TemplateChange::LiteralOnly { patches: vec![] }.needs_attention());
    }
}
