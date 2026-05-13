//! `prss_hash` — fingerprint substrate for `.prss` hot-reload.
//!
//! Mirrors [`crate::template_hash`] for the PRSS stylesheet
//! language. Two hashes per file plus a literal-slot table:
//!
//! * [`prss_structural_hash`] — token *bucket* keys, class names,
//!   `extends` chains, and the *set of property keys* per class /
//!   per state. Doesn't include any property value or any token
//!   value.
//! * [`prss_full_hash`] — same scope plus every literal value.
//! * [`collect_prss_literal_slots`] — every `(class, state?, key,
//!   value)` slot in document order so the hot-reload consumer can
//!   diff just the values that changed.
//!
//! When the structural hash matches but the full hash differs, only
//! literal values changed and the consumer can re-evaluate every
//! reactive context that read a class/token without re-walking the
//! AST.
//!
//! The hasher is the same FNV-1a 64-bit primitive
//! `template_hash` uses, so two builds of the same `.prss` always
//! produce the same fingerprint regardless of host.

use prism_core::language::prss::{StyleSheet, TokenOverrides};

/// FNV-1a 64-bit hash. Same primitive `template_hash` uses; kept
/// inline so the two modules stay independently testable without a
/// shared util layer.
struct FnvHasher(u64);

impl FnvHasher {
    fn new() -> Self {
        Self(0xcbf29ce484222325)
    }

    fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 ^= u64::from(b);
            self.0 = self.0.wrapping_mul(0x100000001b3);
        }
    }

    fn finish(self) -> u64 {
        self.0
    }
}

/// Structural-only fingerprint for a `.prss` file. The hash is
/// invariant over property *values* and token *values* — only the
/// *shape* (class names, extends, key sets, state names) participates.
pub fn prss_structural_hash(sheet: &StyleSheet) -> u64 {
    let mut h = FnvHasher::new();
    hash_tokens(&mut h, &sheet.tokens, /* include_values */ false);
    hash_classes(&mut h, sheet, /* include_values */ false);
    h.finish()
}

/// Full fingerprint — includes every literal value alongside the
/// structural skeleton.
pub fn prss_full_hash(sheet: &StyleSheet) -> u64 {
    let mut h = FnvHasher::new();
    hash_tokens(&mut h, &sheet.tokens, /* include_values */ true);
    hash_classes(&mut h, sheet, /* include_values */ true);
    h.finish()
}

fn hash_tokens(h: &mut FnvHasher, tokens: &TokenOverrides, include_values: bool) {
    // Each bucket is a separate keyspace; tag with a single-byte
    // marker so `colors.foo = "..."` and `spacing.foo = "..."`
    // hash distinctly even if they happen to share a key name.
    hash_token_bucket(h, b"c", &tokens.colors, include_values);
    hash_token_bucket(h, b"s", &tokens.spacing, include_values);
    hash_token_bucket(h, b"r", &tokens.radius, include_values);
    hash_token_bucket(h, b"t", &tokens.typography, include_values);
}

fn hash_token_bucket(
    h: &mut FnvHasher,
    tag: &[u8],
    bucket: &indexmap::IndexMap<String, String>,
    include_values: bool,
) {
    h.write(tag);
    h.write(b"[");
    for (k, v) in bucket {
        h.write(b"k");
        h.write(k.as_bytes());
        if include_values {
            h.write(b"=");
            h.write(v.as_bytes());
        }
    }
    h.write(b"]");
}

fn hash_classes(h: &mut FnvHasher, sheet: &StyleSheet, include_values: bool) {
    h.write(b"C");
    for (name, class) in &sheet.classes {
        h.write(b"n");
        h.write(name.as_bytes());
        if let Some(parent) = &class.extends {
            h.write(b"e");
            h.write(parent.as_bytes());
        }
        h.write(b"p[");
        for (k, v) in &class.properties {
            h.write(b"k");
            h.write(k.as_bytes());
            if include_values {
                h.write(b"=");
                h.write(v.as_bytes());
            }
        }
        h.write(b"]");
        h.write(b"S[");
        for (state, props) in &class.states {
            h.write(b"s");
            h.write(state.as_bytes());
            h.write(b"p[");
            for (k, v) in props {
                h.write(b"k");
                h.write(k.as_bytes());
                if include_values {
                    h.write(b"=");
                    h.write(v.as_bytes());
                }
            }
            h.write(b"]");
        }
        h.write(b"]");
    }
}

/// One literal slot in a `.prss` file. The slot identity is the
/// `(class, state?, key)` triple — same shape as the runtime's
/// `apply_prss_class` consumer, so a literal-only diff can be
/// applied per-slot without re-evaluating the class chain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrssLiteralSlot {
    /// Owner — `Class { name }` for class properties, `Token { bucket }`
    /// for `[tokens.<bucket>]` overrides.
    pub owner: PrssLiteralOwner,
    /// The property key (`background`, `radius`, …) or, for token
    /// slots, the token name (`accent`, `md`, …).
    pub key: String,
    /// The raw literal value.
    pub value: String,
}

/// Where a literal slot lives — a class (with optional state) or a
/// token bucket.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrssLiteralOwner {
    /// Class property — base when `state` is `None`, state override
    /// otherwise.
    Class { name: String, state: Option<String> },
    /// Token override under `[tokens.<bucket>]`.
    Token { bucket: PrssTokenBucket },
}

/// Identifier for the four token buckets a PRSS file may override.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrssTokenBucket {
    Colors,
    Spacing,
    Radius,
    Typography,
}

/// Walk a stylesheet and return every literal slot in a stable
/// document-order traversal. Used by the hot-reload fast path.
pub fn collect_prss_literal_slots(sheet: &StyleSheet) -> Vec<PrssLiteralSlot> {
    let mut out = Vec::new();
    push_token_bucket(&mut out, PrssTokenBucket::Colors, &sheet.tokens.colors);
    push_token_bucket(&mut out, PrssTokenBucket::Spacing, &sheet.tokens.spacing);
    push_token_bucket(&mut out, PrssTokenBucket::Radius, &sheet.tokens.radius);
    push_token_bucket(
        &mut out,
        PrssTokenBucket::Typography,
        &sheet.tokens.typography,
    );
    for (name, class) in &sheet.classes {
        for (k, v) in &class.properties {
            out.push(PrssLiteralSlot {
                owner: PrssLiteralOwner::Class {
                    name: name.clone(),
                    state: None,
                },
                key: k.clone(),
                value: v.clone(),
            });
        }
        for (state, props) in &class.states {
            for (k, v) in props {
                out.push(PrssLiteralSlot {
                    owner: PrssLiteralOwner::Class {
                        name: name.clone(),
                        state: Some(state.clone()),
                    },
                    key: k.clone(),
                    value: v.clone(),
                });
            }
        }
    }
    out
}

fn push_token_bucket(
    out: &mut Vec<PrssLiteralSlot>,
    bucket: PrssTokenBucket,
    map: &indexmap::IndexMap<String, String>,
) {
    for (k, v) in map {
        out.push(PrssLiteralSlot {
            owner: PrssLiteralOwner::Token { bucket },
            key: k.clone(),
            value: v.clone(),
        });
    }
}

/// Bundled fingerprint — both hashes plus the literal-slot table,
/// matching `template_hash::TemplateFingerprint`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrssFingerprint {
    pub structural: u64,
    pub full: u64,
    pub literals: Vec<PrssLiteralSlot>,
}

impl PrssFingerprint {
    pub fn of(sheet: &StyleSheet) -> Self {
        Self {
            structural: prss_structural_hash(sheet),
            full: prss_full_hash(sheet),
            literals: collect_prss_literal_slots(sheet),
        }
    }
}

/// One literal slot changed between two stylesheet versions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrssLiteralPatch {
    pub owner: PrssLiteralOwner,
    pub key: String,
    pub old_value: String,
    pub new_value: String,
}

/// Outcome of comparing two `PrssFingerprint`s.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrssPatchOutcome {
    /// Both hashes match — nothing to patch.
    NoChange,
    /// Same structural hash, different full hash — only literal
    /// values changed. The carried diffs cover every slot whose
    /// value differs; the runtime re-evaluates the dependent
    /// reactive contexts without re-walking the class chain.
    LiteralOnly { diffs: Vec<PrssLiteralPatch> },
    /// Structural hashes differ — class names / extends chains /
    /// property keys changed. The host falls back to a full
    /// stylesheet swap (re-walk every container with a `class="..."`).
    Structural,
}

/// Compare two PRSS fingerprints. Mirrors
/// [`crate::template_hash::compare_fingerprints`] in shape.
pub fn compare_prss_fingerprints(
    prev: &PrssFingerprint,
    next: &PrssFingerprint,
) -> PrssPatchOutcome {
    if prev.full == next.full {
        return PrssPatchOutcome::NoChange;
    }
    if prev.structural != next.structural {
        return PrssPatchOutcome::Structural;
    }
    if prev.literals.len() != next.literals.len() {
        return PrssPatchOutcome::Structural;
    }
    let mut diffs = Vec::new();
    for (a, b) in prev.literals.iter().zip(next.literals.iter()) {
        if a.owner != b.owner || a.key != b.key {
            return PrssPatchOutcome::Structural;
        }
        if a.value != b.value {
            diffs.push(PrssLiteralPatch {
                owner: b.owner.clone(),
                key: b.key.clone(),
                old_value: a.value.clone(),
                new_value: b.value.clone(),
            });
        }
    }
    PrssPatchOutcome::LiteralOnly { diffs }
}

#[cfg(test)]
mod tests {
    use super::*;
    use prism_core::language::prss::parse;

    fn sheet(src: &str) -> StyleSheet {
        let (s, errs) = parse(src);
        assert!(errs.is_empty(), "parse errors: {errs:?}");
        s
    }

    #[test]
    fn empty_sheets_hash_to_same_value() {
        let a = sheet("");
        let b = sheet("");
        assert_eq!(prss_structural_hash(&a), prss_structural_hash(&b));
        assert_eq!(prss_full_hash(&a), prss_full_hash(&b));
    }

    #[test]
    fn changing_a_value_keeps_structural_changes_full() {
        let a = sheet(
            r##"
            [class.btn]
            background = "#0060c0"
        "##,
        );
        let b = sheet(
            r##"
            [class.btn]
            background = "#7c3aed"
        "##,
        );
        assert_eq!(prss_structural_hash(&a), prss_structural_hash(&b));
        assert_ne!(prss_full_hash(&a), prss_full_hash(&b));
    }

    #[test]
    fn adding_a_property_changes_structural() {
        let a = sheet(
            r##"[class.btn]
            background = "#fff"
        "##,
        );
        let b = sheet(
            r##"[class.btn]
            background = "#fff"
            radius = 8
        "##,
        );
        assert_ne!(prss_structural_hash(&a), prss_structural_hash(&b));
    }

    #[test]
    fn adding_a_class_changes_structural() {
        let a = sheet(
            r##"[class.btn]
            background = "#fff"
        "##,
        );
        let b = sheet(
            r##"
            [class.btn]
            background = "#fff"

            [class.icon]
            color = "#000"
        "##,
        );
        assert_ne!(prss_structural_hash(&a), prss_structural_hash(&b));
    }

    #[test]
    fn changing_extends_changes_structural() {
        let a = sheet(
            r##"
            [class.row]
            direction = "row"

            [class.btn]
            background = "#fff"
        "##,
        );
        let b = sheet(
            r##"
            [class.row]
            direction = "row"

            [class.btn]
            extends = "row"
            background = "#fff"
        "##,
        );
        assert_ne!(prss_structural_hash(&a), prss_structural_hash(&b));
    }

    #[test]
    fn changing_a_token_value_keeps_structural() {
        let a = sheet(
            r##"[tokens.colors]
            accent = "#000000"
        "##,
        );
        let b = sheet(
            r##"[tokens.colors]
            accent = "#7c3aed"
        "##,
        );
        assert_eq!(prss_structural_hash(&a), prss_structural_hash(&b));
        assert_ne!(prss_full_hash(&a), prss_full_hash(&b));
    }

    #[test]
    fn adding_a_token_changes_structural() {
        let a = sheet(
            r##"[tokens.colors]
            accent = "#7c3aed"
        "##,
        );
        let b = sheet(
            r##"[tokens.colors]
            accent = "#7c3aed"
            surface = "#ffffff"
        "##,
        );
        assert_ne!(prss_structural_hash(&a), prss_structural_hash(&b));
    }

    #[test]
    fn collect_slots_includes_tokens_and_classes_in_document_order() {
        let s = sheet(
            r##"
            [tokens.colors]
            accent = "#000"

            [class.btn]
            background = "#fff"

            [class.btn.hovered]
            background = "#aaa"
        "##,
        );
        let slots = collect_prss_literal_slots(&s);
        // Token slot first.
        assert_eq!(
            slots[0].owner,
            PrssLiteralOwner::Token {
                bucket: PrssTokenBucket::Colors
            }
        );
        assert_eq!(slots[0].key, "accent");
        assert_eq!(slots[0].value, "#000");
        // Class base + state.
        let class_slots: Vec<_> = slots
            .iter()
            .filter(|s| matches!(&s.owner, PrssLiteralOwner::Class { .. }))
            .collect();
        assert_eq!(class_slots.len(), 2);
        match &class_slots[0].owner {
            PrssLiteralOwner::Class { name, state } => {
                assert_eq!(name, "btn");
                assert_eq!(state, &None);
            }
            _ => unreachable!(),
        }
        match &class_slots[1].owner {
            PrssLiteralOwner::Class { name, state } => {
                assert_eq!(name, "btn");
                assert_eq!(state.as_deref(), Some("hovered"));
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn compare_no_change_for_identical_sheets() {
        let a = PrssFingerprint::of(&sheet(
            r##"[class.btn]
            background = "#fff"
        "##,
        ));
        let b = PrssFingerprint::of(&sheet(
            r##"[class.btn]
            background = "#fff"
        "##,
        ));
        assert_eq!(
            compare_prss_fingerprints(&a, &b),
            PrssPatchOutcome::NoChange
        );
    }

    #[test]
    fn compare_literal_only_for_value_change() {
        let a = PrssFingerprint::of(&sheet(
            r##"[class.btn]
            background = "#fff"
        "##,
        ));
        let b = PrssFingerprint::of(&sheet(
            r##"[class.btn]
            background = "#000"
        "##,
        ));
        match compare_prss_fingerprints(&a, &b) {
            PrssPatchOutcome::LiteralOnly { diffs } => {
                assert_eq!(diffs.len(), 1);
                assert_eq!(diffs[0].key, "background");
                assert_eq!(diffs[0].old_value, "#fff");
                assert_eq!(diffs[0].new_value, "#000");
                match &diffs[0].owner {
                    PrssLiteralOwner::Class { name, state } => {
                        assert_eq!(name, "btn");
                        assert_eq!(state, &None);
                    }
                    _ => panic!("expected Class owner"),
                }
            }
            other => panic!("expected LiteralOnly, got {other:?}"),
        }
    }

    #[test]
    fn compare_structural_when_class_added() {
        let a = PrssFingerprint::of(&sheet(
            r##"[class.btn]
            background = "#fff"
        "##,
        ));
        let b = PrssFingerprint::of(&sheet(
            r##"
            [class.btn]
            background = "#fff"

            [class.icon]
            color = "#000"
        "##,
        ));
        assert_eq!(
            compare_prss_fingerprints(&a, &b),
            PrssPatchOutcome::Structural
        );
    }

    #[test]
    fn compare_state_value_change_is_literal_only() {
        let a = PrssFingerprint::of(&sheet(
            r##"
            [class.btn]
            background = "#fff"

            [class.btn.hovered]
            background = "#aaa"
        "##,
        ));
        let b = PrssFingerprint::of(&sheet(
            r##"
            [class.btn]
            background = "#fff"

            [class.btn.hovered]
            background = "#bbb"
        "##,
        ));
        match compare_prss_fingerprints(&a, &b) {
            PrssPatchOutcome::LiteralOnly { diffs } => {
                assert_eq!(diffs.len(), 1);
                match &diffs[0].owner {
                    PrssLiteralOwner::Class { name, state } => {
                        assert_eq!(name, "btn");
                        assert_eq!(state.as_deref(), Some("hovered"));
                    }
                    _ => panic!("expected Class owner"),
                }
            }
            other => panic!("expected LiteralOnly, got {other:?}"),
        }
    }
}
