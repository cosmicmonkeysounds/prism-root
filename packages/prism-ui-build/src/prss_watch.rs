//! `prss_watch` — `.prss` hot-reload consumer.
//!
//! Sister to [`crate::template_watch`] for the PRSS stylesheet
//! language. Holds the last-seen [`PrssFingerprint`] of every
//! watched `.prss` file and classifies each on-disk edit into a
//! [`PrssChange`].
//!
//! ## Lifecycle
//!
//! ```ignore
//! let mut cache = PrssFingerprintCache::new();
//! let change = cache.observe("ui/theme.prss");
//! match change {
//!     PrssChange::FirstSighting { .. } => /* seed; load stylesheet */ ,
//!     PrssChange::NoChange => /* nothing to do */ ,
//!     PrssChange::LiteralOnly { patches } => /* fast-path token/value swap */ ,
//!     PrssChange::Structural => /* re-walk every container with class="..." */ ,
//!     PrssChange::ParseError { message } => /* surface error */ ,
//!     PrssChange::ReadError { message } => /* retry next batch */ ,
//! }
//! ```

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use prism_core::language::prss::parse;

use crate::prss_hash::{
    compare_prss_fingerprints, PrssFingerprint, PrssLiteralPatch, PrssPatchOutcome,
};

/// Classified outcome of observing a `.prss` file on disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrssChange {
    /// First time this path has been seen — fingerprint seeded, no
    /// patch to apply. Caller does the initial stylesheet install
    /// here.
    FirstSighting { fingerprint: PrssFingerprint },
    /// File on disk matches the last-seen fingerprint exactly.
    NoChange,
    /// Only literal property / token values changed — the host can
    /// fast-path the patch (re-evaluate dependent reactive contexts
    /// without re-walking class chains).
    LiteralOnly { patches: Vec<PrssLiteralPatch> },
    /// Structural change (class added/removed, key set changed,
    /// `extends` chain changed). Host must re-walk every container
    /// that consumes a `class="..."` to pick up the new shape.
    Structural,
    /// The file failed to parse. Host surfaces the error to the
    /// editor and keeps the previous fingerprint so the next
    /// successful save still classifies correctly.
    ParseError { message: String },
    /// The file couldn't be read (deleted, permission denied,
    /// editor's atomic-write dance mid-flight). Caller retries on
    /// the next batch.
    ReadError { message: String },
}

impl PrssChange {
    /// True when the dev loop's reload pipeline should fire.
    pub fn needs_attention(&self) -> bool {
        !matches!(self, Self::NoChange)
    }
}

/// Path → last-seen fingerprint. Mirrors
/// [`crate::template_watch::FingerprintCache`] in shape and
/// lifecycle.
#[derive(Default)]
pub struct PrssFingerprintCache {
    seen: HashMap<PathBuf, PrssFingerprint>,
}

impl PrssFingerprintCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// Observe the current on-disk content of `path` and classify
    /// the change against the cached fingerprint. Updates the cache
    /// as a side effect.
    pub fn observe(&mut self, path: impl AsRef<Path>) -> PrssChange {
        let path = path.as_ref().to_path_buf();
        let source = match std::fs::read_to_string(&path) {
            Ok(s) => s,
            Err(e) => {
                return PrssChange::ReadError {
                    message: format!("{}: {e}", path.display()),
                }
            }
        };
        self.observe_source(path, &source)
    }

    /// Observe a literal source string rather than reading from
    /// disk. Useful for tests + alternative ingestion (LSP didChange,
    /// embedded editor buffer).
    pub fn observe_source(&mut self, path: PathBuf, source: &str) -> PrssChange {
        let (sheet, errors) = parse(source);
        // PRSS uses recovery-friendly diagnostics — partial sheets
        // still parse and might be load-worthy. Treat a non-empty
        // error list as a parse error only when no class / token
        // landed at all (a hard syntax error). Anything else is a
        // best-effort fingerprint with diagnostics surfaced to the
        // host through the parser's separate channel.
        if !errors.is_empty()
            && sheet.classes.is_empty()
            && sheet.tokens.colors.is_empty()
            && sheet.tokens.spacing.is_empty()
            && sheet.tokens.radius.is_empty()
            && sheet.tokens.typography.is_empty()
        {
            let message = errors
                .iter()
                .map(|e| format!("{}: {}", e.code, e.message))
                .collect::<Vec<_>>()
                .join("; ");
            return PrssChange::ParseError { message };
        }
        let fp = PrssFingerprint::of(&sheet);
        match self.seen.get(&path) {
            None => {
                self.seen.insert(path, fp.clone());
                PrssChange::FirstSighting { fingerprint: fp }
            }
            Some(prev) => {
                let outcome = compare_prss_fingerprints(prev, &fp);
                let change = match outcome {
                    PrssPatchOutcome::NoChange => PrssChange::NoChange,
                    PrssPatchOutcome::LiteralOnly { diffs } => {
                        PrssChange::LiteralOnly { patches: diffs }
                    }
                    PrssPatchOutcome::Structural => PrssChange::Structural,
                };
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
    use crate::prss_hash::{PrssLiteralOwner, PrssTokenBucket};

    #[test]
    fn first_sighting_seeds_cache() {
        let mut cache = PrssFingerprintCache::new();
        let change = cache.observe_source(
            "ui/theme.prss".into(),
            r##"[class.btn]
            background = "#fff"
            "##,
        );
        assert!(matches!(change, PrssChange::FirstSighting { .. }));
        assert_eq!(cache.len(), 1);
    }

    #[test]
    fn identical_content_is_no_change() {
        let mut cache = PrssFingerprintCache::new();
        let path: PathBuf = "ui/theme.prss".into();
        let _ = cache.observe_source(
            path.clone(),
            r##"[class.btn]
            background = "#fff"
            "##,
        );
        let change = cache.observe_source(
            path,
            r##"[class.btn]
            background = "#fff"
            "##,
        );
        assert_eq!(change, PrssChange::NoChange);
    }

    #[test]
    fn value_edit_classifies_as_literal_only() {
        let mut cache = PrssFingerprintCache::new();
        let path: PathBuf = "ui/theme.prss".into();
        let _ = cache.observe_source(
            path.clone(),
            r##"[class.btn]
            background = "#fff"
            "##,
        );
        let change = cache.observe_source(
            path,
            r##"[class.btn]
            background = "#000"
            "##,
        );
        match change {
            PrssChange::LiteralOnly { patches } => {
                assert_eq!(patches.len(), 1);
                assert_eq!(patches[0].key, "background");
                assert_eq!(patches[0].old_value, "#fff");
                assert_eq!(patches[0].new_value, "#000");
            }
            other => panic!("expected LiteralOnly, got {other:?}"),
        }
    }

    #[test]
    fn token_edit_classifies_as_literal_only() {
        let mut cache = PrssFingerprintCache::new();
        let path: PathBuf = "ui/theme.prss".into();
        let _ = cache.observe_source(
            path.clone(),
            r##"[tokens.colors]
            accent = "#000000"
            "##,
        );
        let change = cache.observe_source(
            path,
            r##"[tokens.colors]
            accent = "#7c3aed"
            "##,
        );
        match change {
            PrssChange::LiteralOnly { patches } => {
                assert_eq!(patches.len(), 1);
                assert_eq!(
                    patches[0].owner,
                    PrssLiteralOwner::Token {
                        bucket: PrssTokenBucket::Colors
                    }
                );
                assert_eq!(patches[0].key, "accent");
                assert_eq!(patches[0].new_value, "#7c3aed");
            }
            other => panic!("expected LiteralOnly, got {other:?}"),
        }
    }

    #[test]
    fn class_added_classifies_as_structural() {
        let mut cache = PrssFingerprintCache::new();
        let path: PathBuf = "ui/theme.prss".into();
        let _ = cache.observe_source(
            path.clone(),
            r##"[class.btn]
            background = "#fff"
            "##,
        );
        let change = cache.observe_source(
            path,
            r##"
            [class.btn]
            background = "#fff"

            [class.icon]
            color = "#000"
            "##,
        );
        assert_eq!(change, PrssChange::Structural);
    }

    #[test]
    fn parse_error_surfaces_messages_for_hard_syntax_break() {
        let mut cache = PrssFingerprintCache::new();
        let change = cache.observe_source(
            "broken.prss".into(),
            r##"[class.btn
            background = "
            "##,
        );
        match change {
            PrssChange::ParseError { message } => assert!(!message.is_empty()),
            other => panic!("expected ParseError, got {other:?}"),
        }
    }

    #[test]
    fn cache_can_be_cleared_for_a_path() {
        let mut cache = PrssFingerprintCache::new();
        let path: PathBuf = "ui/theme.prss".into();
        let _ = cache.observe_source(
            path.clone(),
            r##"[class.btn]
            background = "#fff"
            "##,
        );
        assert!(cache.contains(&path));
        assert!(cache.forget(&path));
        assert!(!cache.contains(&path));
        let change = cache.observe_source(
            path,
            r##"[class.btn]
            background = "#fff"
            "##,
        );
        assert!(matches!(change, PrssChange::FirstSighting { .. }));
    }

    #[test]
    fn read_error_surfaces_when_file_missing() {
        let mut cache = PrssFingerprintCache::new();
        let change = cache.observe("/this/path/does/not/exist.prss");
        assert!(matches!(change, PrssChange::ReadError { .. }));
    }

    #[test]
    fn needs_attention_skips_no_change() {
        assert!(!PrssChange::NoChange.needs_attention());
        assert!(PrssChange::Structural.needs_attention());
        assert!(PrssChange::LiteralOnly { patches: vec![] }.needs_attention());
    }
}
