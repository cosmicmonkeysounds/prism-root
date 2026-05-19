//! `prui_doc` — **Wave H.4** of `docs/dev/prui-luau-fusion.md` §5.7:
//! virtual-file fingerprinting for a `.prui` document's *inline*
//! `<script>` / `<style>` blocks.
//!
//! The plain [`crate::template_watch::FingerprintCache`] keys a whole
//! `.prui` file by path and classifies it as literal-only or
//! structural. That is too coarse once a single-file widget carries
//! colocated Luau and PRSS: editing a `<script>` body should reload
//! *only* that script's Lua frame, and editing a `<style>` body
//! should route through the PRSS literal-only fast path — neither
//! should structurally respawn the markup.
//!
//! This module decomposes a document into three independently-keyed
//! resources, exactly the §5.7 contract:
//!
//! * the **template** — the PRUI tree with every raw-text block body
//!   normalized away, so a block-body edit never perturbs the
//!   template fingerprint;
//! * each inline `<script>` block — virtual key `{path}#script:{i}`,
//!   change-detected by a stable body hash (Luau is opaque — any
//!   change is a script reload);
//! * each inline `<style>` block — virtual key `{path}#style:{i}`,
//!   classified through the existing [`crate::prss_hash`] machinery
//!   so a value-only PRSS edit stays literal-only.
//!
//! [`PruiDocCache::observe_source`] returns the *narrowest* set of
//! patches that covers the edit: a structural template change
//! dominates (full respawn); otherwise the literal template slots,
//! the changed script keys, and the changed style keys are reported
//! independently so the dev loop applies the smallest reload.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use prism_core::language::prism_ui::{parse, Document, Node};
use prism_core::language::prss::parse as parse_prss;

use crate::prss_hash::{compare_prss_fingerprints, PrssFingerprint, PrssPatchOutcome};
use crate::template_hash::{compare_fingerprints, LiteralPatch, PatchOutcome, TemplateFingerprint};

/// Stable 64-bit FNV-1a — matches `template_hash`'s hasher choice so
/// script-body keying is deterministic across machines + runs.
fn fnv1a(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in bytes {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

/// One changed `<style>` block: its virtual key plus the PRSS
/// classifier's verdict (so a value-only edit stays literal-only).
#[derive(Debug, Clone, PartialEq)]
pub struct StyleDelta {
    pub key: String,
    pub outcome: PrssPatchOutcome,
}

/// Classified outcome of observing a `.prui` document with inline
/// blocks. Mirrors [`crate::template_watch::TemplateChange`] but
/// splits the inline-block deltas out of the template verdict.
#[derive(Debug, Clone, PartialEq)]
pub enum PruiDocChange {
    /// First time this path was observed — fingerprint seeded.
    FirstSighting,
    /// Nothing changed.
    NoChange,
    /// The markup tree shape changed — a full respawn is required
    /// (this subsumes any block edits in the same save).
    Structural,
    /// Narrowest independent patches. Any of the three may be
    /// non-empty; an empty `template` with non-empty `scripts` is
    /// the "only a `<script>` body changed" fast path.
    Patches {
        /// Literal-only template attribute slots.
        template: Vec<LiteralPatch>,
        /// Virtual keys (`{path}#script:{i}`) whose Luau body changed.
        scripts: Vec<String>,
        /// Per-`<style>`-block PRSS deltas.
        styles: Vec<StyleDelta>,
    },
    /// The `.prui` failed to parse.
    ParseError { message: String },
    /// The file couldn't be read off disk.
    ReadError { message: String },
}

impl PruiDocChange {
    /// True when the dev loop should act (anything but `NoChange`).
    pub fn needs_attention(&self) -> bool {
        !matches!(self, Self::NoChange)
    }
}

/// Per-document fingerprint: the body-normalized template plus one
/// entry per inline block, positionally keyed (block `i` is the
/// i-th `<script>` / `<style>` in document order).
#[derive(Debug, Clone)]
struct DocFingerprint {
    template: TemplateFingerprint,
    scripts: Vec<u64>,
    styles: Vec<PrssFingerprint>,
}

/// Collect the concatenated body of every top-level `<script>` /
/// `<style>` element, in document order — mirrors the runtime's
/// `collect_script_bodies` / `collect_inline_stylesheets`.
fn collect_blocks(document: &Document) -> (Vec<String>, Vec<String>) {
    let mut scripts = Vec::new();
    let mut styles = Vec::new();
    for node in &document.nodes {
        let Node::Element(el) = node else { continue };
        let bucket = match el.tag.as_str() {
            "script" => &mut scripts,
            "style" => &mut styles,
            _ => continue,
        };
        let mut body = String::new();
        for child in &el.children {
            if let Node::Text { value, .. } = child {
                body.push_str(value);
            }
        }
        bucket.push(body);
    }
    (scripts, styles)
}

/// Clone `document` with every `<script>` / `<style>` raw-text body
/// blanked, so the template fingerprint is invariant to block-body
/// edits (those are tracked as their own virtual files).
fn normalized_template(document: &Document) -> Document {
    fn scrub(node: &mut Node) {
        if let Node::Element(el) = node {
            if matches!(el.tag.as_str(), "script" | "style") {
                for child in &mut el.children {
                    if let Node::Text { value, .. } = child {
                        value.clear();
                    }
                }
            }
            for child in &mut el.children {
                scrub(child);
            }
        }
    }
    let mut doc = document.clone();
    for node in &mut doc.nodes {
        scrub(node);
    }
    doc
}

fn fingerprint(document: &Document) -> DocFingerprint {
    let (scripts, styles) = collect_blocks(document);
    let template = TemplateFingerprint::of(&normalized_template(document));
    let script_hashes = scripts.iter().map(|s| fnv1a(s.as_bytes())).collect();
    let style_fps = styles
        .iter()
        .map(|s| {
            let (sheet, _errs) = parse_prss(s);
            PrssFingerprint::of(&sheet)
        })
        .collect();
    DocFingerprint {
        template,
        scripts: script_hashes,
        styles: style_fps,
    }
}

/// Path → last-seen [`DocFingerprint`]. The Wave-H.4 sibling of
/// [`crate::template_watch::FingerprintCache`] for single-file
/// widgets that colocate `<script>` / `<style>`.
#[derive(Default)]
pub struct PruiDocCache {
    seen: HashMap<PathBuf, DocFingerprint>,
}

impl PruiDocCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// Read `path` off disk and classify against the cache.
    pub fn observe(&mut self, path: impl AsRef<Path>) -> PruiDocChange {
        let path = path.as_ref().to_path_buf();
        match std::fs::read_to_string(&path) {
            Ok(s) => self.observe_source(path, &s),
            Err(e) => PruiDocChange::ReadError {
                message: format!("{}: {e}", path.display()),
            },
        }
    }

    /// Classify `source` for `path` against the cached fingerprint,
    /// updating the cache. Virtual keys are `{path}#script:{i}` /
    /// `{path}#style:{i}`.
    pub fn observe_source(&mut self, path: PathBuf, source: &str) -> PruiDocChange {
        let (document, errors) = parse(source);
        if !errors.is_empty() {
            return PruiDocChange::ParseError {
                message: errors
                    .iter()
                    .map(|e| format!("{}: {}", e.code, e.message))
                    .collect::<Vec<_>>()
                    .join("; "),
            };
        }
        let fp = fingerprint(&document);
        let Some(prev) = self.seen.get(&path) else {
            self.seen.insert(path, fp);
            return PruiDocChange::FirstSighting;
        };

        // Template verdict first — a structural markup change
        // subsumes every block edit in the same save.
        let template_outcome = compare_fingerprints(&prev.template, &fp.template);
        if matches!(template_outcome, PatchOutcome::Structural) {
            self.seen.insert(path, fp);
            return PruiDocChange::Structural;
        }
        // Block-count changes are structural too — a new/removed
        // `<script>`/`<style>` reshapes the document.
        if prev.scripts.len() != fp.scripts.len() || prev.styles.len() != fp.styles.len() {
            self.seen.insert(path, fp);
            return PruiDocChange::Structural;
        }

        let template = match &template_outcome {
            PatchOutcome::LiteralOnly { diffs } => diffs.clone(),
            _ => Vec::new(),
        };
        let scripts: Vec<String> = fp
            .scripts
            .iter()
            .enumerate()
            .filter(|(i, h)| prev.scripts[*i] != **h)
            .map(|(i, _)| format!("{}#script:{i}", path.display()))
            .collect();
        let styles: Vec<StyleDelta> = fp
            .styles
            .iter()
            .enumerate()
            .filter_map(
                |(i, cur)| match compare_prss_fingerprints(&prev.styles[i], cur) {
                    PrssPatchOutcome::NoChange => None,
                    outcome => Some(StyleDelta {
                        key: format!("{}#style:{i}", path.display()),
                        outcome,
                    }),
                },
            )
            .collect();

        self.seen.insert(path, fp);
        if template.is_empty() && scripts.is_empty() && styles.is_empty() {
            PruiDocChange::NoChange
        } else {
            PruiDocChange::Patches {
                template,
                scripts,
                styles,
            }
        }
    }

    pub fn contains(&self, path: impl AsRef<Path>) -> bool {
        self.seen.contains_key(path.as_ref())
    }

    pub fn forget(&mut self, path: impl AsRef<Path>) -> bool {
        self.seen.remove(path.as_ref()).is_some()
    }

    pub fn len(&self) -> usize {
        self.seen.len()
    }

    pub fn is_empty(&self) -> bool {
        self.seen.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOC: &str = r##"<style>
[class.card]
background = "#fff"
</style>
<script>
local n = 1
</script>
<container class=card, gap=8>
  <text>{n}</text>
</container>"##;

    fn seed() -> (PruiDocCache, PathBuf) {
        let mut cache = PruiDocCache::new();
        let path: PathBuf = "w.prui".into();
        assert_eq!(
            cache.observe_source(path.clone(), DOC),
            PruiDocChange::FirstSighting
        );
        (cache, path)
    }

    #[test]
    fn identical_is_no_change() {
        let (mut cache, path) = seed();
        assert_eq!(cache.observe_source(path, DOC), PruiDocChange::NoChange);
    }

    #[test]
    fn script_body_edit_is_script_only_patch() {
        let (mut cache, path) = seed();
        let edited = DOC.replace("local n = 1", "local n = 42");
        match cache.observe_source(path, &edited) {
            PruiDocChange::Patches {
                template,
                scripts,
                styles,
            } => {
                assert!(template.is_empty(), "markup untouched");
                assert!(styles.is_empty(), "style untouched");
                assert_eq!(scripts, vec!["w.prui#script:0".to_string()]);
            }
            other => panic!("expected script-only Patches, got {other:?}"),
        }
    }

    #[test]
    fn style_value_edit_is_literal_only_style_patch() {
        let (mut cache, path) = seed();
        let edited = DOC.replace("#fff", "#eee");
        match cache.observe_source(path, &edited) {
            PruiDocChange::Patches {
                template,
                scripts,
                styles,
            } => {
                assert!(template.is_empty());
                assert!(scripts.is_empty());
                assert_eq!(styles.len(), 1);
                assert_eq!(styles[0].key, "w.prui#style:0");
                assert!(matches!(
                    styles[0].outcome,
                    PrssPatchOutcome::LiteralOnly { .. }
                ));
            }
            other => panic!("expected style-only Patches, got {other:?}"),
        }
    }

    #[test]
    fn markup_literal_edit_is_template_only_patch() {
        let (mut cache, path) = seed();
        let edited = DOC.replace("gap=8", "gap=12");
        match cache.observe_source(path, &edited) {
            PruiDocChange::Patches {
                template,
                scripts,
                styles,
            } => {
                assert_eq!(template.len(), 1);
                assert_eq!(template[0].attr, "gap");
                assert!(scripts.is_empty() && styles.is_empty());
            }
            other => panic!("expected template-only Patches, got {other:?}"),
        }
    }

    #[test]
    fn tag_change_is_structural() {
        let (mut cache, path) = seed();
        let edited = DOC.replace("<text>{n}</text>", "<image src=x/>");
        assert_eq!(
            cache.observe_source(path, &edited),
            PruiDocChange::Structural
        );
    }

    #[test]
    fn adding_a_script_block_is_structural() {
        let (mut cache, path) = seed();
        let edited = format!("{DOC}\n<script>local m = 2</script>");
        assert_eq!(
            cache.observe_source(path, &edited),
            PruiDocChange::Structural
        );
    }

    #[test]
    fn simultaneous_script_and_style_edits_report_both_keys() {
        let (mut cache, path) = seed();
        let edited = DOC
            .replace("local n = 1", "local n = 9")
            .replace("#fff", "#000");
        match cache.observe_source(path, &edited) {
            PruiDocChange::Patches {
                template,
                scripts,
                styles,
            } => {
                assert!(template.is_empty());
                assert_eq!(scripts, vec!["w.prui#script:0".to_string()]);
                assert_eq!(styles.len(), 1);
                assert_eq!(styles[0].key, "w.prui#style:0");
            }
            other => panic!("expected combined Patches, got {other:?}"),
        }
    }

    #[test]
    fn parse_error_surfaces() {
        let mut cache = PruiDocCache::new();
        match cache.observe_source("b.prui".into(), "<container>") {
            PruiDocChange::ParseError { message } => assert!(!message.is_empty()),
            other => panic!("expected ParseError, got {other:?}"),
        }
    }

    #[test]
    fn forget_resets_to_first_sighting() {
        let (mut cache, path) = seed();
        assert!(cache.forget(&path));
        assert_eq!(
            cache.observe_source(path, DOC),
            PruiDocChange::FirstSighting
        );
    }
}
