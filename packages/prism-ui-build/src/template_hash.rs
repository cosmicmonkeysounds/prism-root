//! `template_hash` — Phase 10 of the Dioxus-inspired reactive
//! overhaul (`docs/dev/dioxus-inspiration.md`).
//!
//! Dioxus's `rsx!` macro stamps every template with a stable hash
//! and a list of dynamic-slot indices. At hot-reload time the
//! runtime compares the new template's structural hash with the
//! cached one: if structural matches but the full hash differs,
//! only literal values changed and the patch can fast-path —
//! no AST re-evaluation, no diff walk.
//!
//! This module computes the same two fingerprints for a Prism
//! `.prui` template:
//!
//! * [`structural_hash`] — ignores literal attribute *values*,
//!   keeps tag names + attribute *names* + nesting shape. Same
//!   structure → same hash.
//! * [`full_hash`] — hashes everything, including literal values.
//!
//! And one extraction helper:
//!
//! * [`collect_literal_slots`] — walks the document and returns
//!   every literal attribute value's `(node_path, attr_name,
//!   value)` so the patch consumer can update slots in place
//!   without re-parsing.
//!
//! All three are deterministic for the same input across machines
//! and Rust versions — the hasher is FNV-1a 64-bit, chosen for its
//! stability and lack of randomized seeds.

use prism_core::language::prism_ui::ast::TemplatePart;
use prism_core::language::prism_ui::{AttributeValue, Document, Node};

/// FNV-1a 64-bit hash. Stable across machines + Rust versions.
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

/// Structural-only fingerprint. Same tree shape → same hash, even
/// if literal attribute values differ. Used at hot-reload time to
/// decide whether the patch is a fast-path literal-only edit.
pub fn structural_hash(document: &Document) -> u64 {
    let mut h = FnvHasher::new();
    for node in &document.nodes {
        hash_node(&mut h, node, /* include_literals */ false);
    }
    h.finish()
}

/// Full fingerprint — every tag, attribute name, *and* literal
/// attribute value is hashed.
pub fn full_hash(document: &Document) -> u64 {
    let mut h = FnvHasher::new();
    for node in &document.nodes {
        hash_node(&mut h, node, /* include_literals */ true);
    }
    h.finish()
}

fn hash_node(h: &mut FnvHasher, node: &Node, include_literals: bool) {
    match node {
        Node::Element(el) => {
            h.write(b"E");
            h.write(el.tag.as_bytes());
            // Attributes hashed in document order so reorderings
            // count as structural changes (which they are —
            // priority can matter in lower).
            for attr in &el.attributes {
                h.write(b"a");
                h.write(attr.name.raw.as_bytes());
                if include_literals {
                    h.write(b"=");
                    hash_attr_value(h, &attr.value);
                } else {
                    // Structural pass: hash only the *kind* of
                    // value so adding/removing expressions still
                    // counts as structural. Equivalent string
                    // values keep the same structural hash.
                    h.write(b"=");
                    h.write(attr_value_kind(&attr.value).as_bytes());
                }
            }
            // Children walked in document order; the trailing `]`
            // marker prevents `<a><b/></a>` and `<a/><b/>` from
            // colliding (otherwise both would hash as `EaEb`).
            h.write(b"[");
            for child in &el.children {
                hash_node(h, child, include_literals);
            }
            h.write(b"]");
        }
        Node::Text { value, .. } => {
            h.write(b"T");
            if include_literals {
                h.write(value.as_bytes());
            }
        }
        Node::Interpolation(expr) => {
            h.write(b"I");
            // Interpolations always count as structural (they're
            // not literals); body text rides only on the full hash.
            if include_literals {
                h.write(expr.body.as_bytes());
            }
        }
        Node::Comment { .. } => {
            // Comments never affect structure or behaviour; skip
            // entirely so trivial doc edits don't invalidate either
            // hash.
        }
    }
}

fn attr_value_kind(value: &AttributeValue) -> &'static str {
    match value {
        AttributeValue::Empty => "empty",
        AttributeValue::String { .. } => "string",
        AttributeValue::Expression(_) => "expression",
        AttributeValue::Template { .. } => "template",
    }
}

fn hash_attr_value(h: &mut FnvHasher, value: &AttributeValue) {
    match value {
        AttributeValue::Empty => {
            h.write(b"e");
        }
        AttributeValue::String { value, .. } => {
            h.write(b"s");
            h.write(value.as_bytes());
        }
        AttributeValue::Expression(expr) => {
            // Expression bodies need re-evaluation — but their
            // *text* is still part of the full hash so swapping
            // `{a}` for `{b}` invalidates correctly.
            h.write(b"x");
            h.write(expr.body.as_bytes());
        }
        AttributeValue::Template { parts, .. } => {
            h.write(b"t");
            for part in parts {
                match part {
                    TemplatePart::Literal { value, .. } => {
                        h.write(b"l");
                        h.write(value.as_bytes());
                    }
                    TemplatePart::Expression(expr) => {
                        h.write(b"x");
                        h.write(expr.body.as_bytes());
                    }
                }
            }
        }
    }
}

// ───── Literal slot extraction ───────────────────────────────────

/// One literal-attribute slot in document order. `path` is a
/// dotted-zero-indexed walk through the children of each parent
/// element (e.g. `0.2.1` = root's first child's third child's
/// second child); `attr` is the attribute name.
#[derive(Debug, Clone, PartialEq)]
pub struct LiteralSlot {
    /// Dotted child-index path from the document root to the
    /// owning element.
    pub path: String,
    /// The attribute name (e.g. `label`, `on:click`).
    pub attr: String,
    /// The literal value at the slot, rendered as a string for
    /// quick comparison + patch payload.
    pub value: String,
}

/// Walk the document and return every literal attribute value in
/// document order. Used by the hot-reload fast path: when two
/// versions of a template have the same [`structural_hash`] but
/// different [`full_hash`]es, the diff is exactly the literal
/// slots that disagree.
pub fn collect_literal_slots(document: &Document) -> Vec<LiteralSlot> {
    let mut out = Vec::new();
    for (idx, node) in document.nodes.iter().enumerate() {
        walk_for_slots(node, &idx.to_string(), &mut out);
    }
    out
}

fn walk_for_slots(node: &Node, path: &str, out: &mut Vec<LiteralSlot>) {
    if let Node::Element(el) = node {
        for attr in &el.attributes {
            if let Some(value) = render_literal(&attr.value) {
                out.push(LiteralSlot {
                    path: path.to_string(),
                    attr: attr.name.raw.clone(),
                    value,
                });
            }
        }
        for (idx, child) in el.children.iter().enumerate() {
            let child_path = format!("{path}.{idx}");
            walk_for_slots(child, &child_path, out);
        }
    }
}

fn render_literal(value: &AttributeValue) -> Option<String> {
    match value {
        // Pure string literals are the canonical literal slot.
        AttributeValue::String { value, .. } => Some(value.clone()),
        // Empty (boolean) attribute — the slot is "present", no
        // value to patch.
        AttributeValue::Empty => Some(String::new()),
        // Expressions and mixed templates aren't literal-only —
        // they require AST re-evaluation. Skipping them is what
        // makes the "fast-path literal-only patch" sound.
        AttributeValue::Expression(_) | AttributeValue::Template { .. } => None,
    }
}

/// Compile-time bundle the build-script emits next to `SOURCE`.
/// Holds both hashes plus the literal-slot table; consumers compare
/// `structural` to a runtime-reparsed template's structural hash
/// to gate the fast path.
#[derive(Debug, Clone, PartialEq)]
pub struct TemplateFingerprint {
    pub structural: u64,
    pub full: u64,
    pub literals: Vec<LiteralSlot>,
}

impl TemplateFingerprint {
    pub fn of(document: &Document) -> Self {
        Self {
            structural: structural_hash(document),
            full: full_hash(document),
            literals: collect_literal_slots(document),
        }
    }
}

/// One literal slot changed between two template versions.
#[derive(Debug, Clone, PartialEq)]
pub struct LiteralPatch {
    pub path: String,
    pub attr: String,
    pub old_value: String,
    pub new_value: String,
}

/// Outcome of comparing two `TemplateFingerprint`s. Drives the
/// hot-reload fast path: literal-only edits skip AST re-evaluation;
/// structural changes fall back to full re-render.
#[derive(Debug, Clone, PartialEq)]
pub enum PatchOutcome {
    /// Both hashes match — no change. Nothing to patch.
    NoChange,
    /// Same structural hash, different full hash. The patch is
    /// exactly the carrying [`LiteralPatch`] entries — slots whose
    /// literal values differ between the two versions. No structural
    /// change; the runtime can patch in place without re-evaluating
    /// the AST.
    LiteralOnly { diffs: Vec<LiteralPatch> },
    /// Structural hashes differ. The patch can't fast-path —
    /// callers must re-evaluate the AST and (typically) drop the
    /// `Surface` tree.
    Structural,
}

/// Compare two template fingerprints and return the appropriate
/// patch shape for hot-reload. This is the **Phase 10** consumer
/// the hashes were built to enable: the build script emits a
/// `TemplateFingerprint` at compile time, the dev loop computes one
/// from the on-disk source after each edit, and a literal-only
/// outcome lets the runtime patch in place via the slot table
/// without parsing the full AST.
pub fn compare_fingerprints(
    prev: &TemplateFingerprint,
    next: &TemplateFingerprint,
) -> PatchOutcome {
    if prev.full == next.full {
        return PatchOutcome::NoChange;
    }
    if prev.structural != next.structural {
        return PatchOutcome::Structural;
    }
    // Same structure, different full hash → diff the literal slots.
    // The slot lists are produced by the same `collect_literal_slots`
    // walk on each side, so identical paths + attrs line up
    // positionally. (A path or attr mismatch here means the slot
    // walk diverged, which by definition is a structural change —
    // shouldn't reach this arm; fall back to Structural defensively.)
    let mut diffs = Vec::new();
    if prev.literals.len() != next.literals.len() {
        return PatchOutcome::Structural;
    }
    for (a, b) in prev.literals.iter().zip(next.literals.iter()) {
        if a.path != b.path || a.attr != b.attr {
            return PatchOutcome::Structural;
        }
        if a.value != b.value {
            diffs.push(LiteralPatch {
                path: b.path.clone(),
                attr: b.attr.clone(),
                old_value: a.value.clone(),
                new_value: b.value.clone(),
            });
        }
    }
    PatchOutcome::LiteralOnly { diffs }
}

#[cfg(test)]
mod tests {
    use super::*;
    use prism_core::language::prism_ui::parse;

    fn doc(source: &str) -> Document {
        let (d, errors) = parse(source);
        assert!(errors.is_empty(), "parse errors: {errors:?}");
        d
    }

    #[test]
    fn empty_documents_hash_to_same_value() {
        let a = doc("");
        let b = doc("");
        assert_eq!(structural_hash(&a), structural_hash(&b));
        assert_eq!(full_hash(&a), full_hash(&b));
    }

    #[test]
    fn structural_hash_ignores_literal_attribute_values() {
        // Same shape, different label text → structural matches,
        // full differs.
        let a = doc(r#"<button label="Save"/>"#);
        let b = doc(r#"<button label="Submit"/>"#);
        assert_eq!(
            structural_hash(&a),
            structural_hash(&b),
            "same shape → same structural hash"
        );
        assert_ne!(
            full_hash(&a),
            full_hash(&b),
            "different literal → different full hash"
        );
    }

    #[test]
    fn structural_hash_changes_when_tag_changes() {
        let a = doc(r#"<button label="x"/>"#);
        let b = doc(r#"<heading label="x"/>"#);
        assert_ne!(structural_hash(&a), structural_hash(&b));
    }

    #[test]
    fn structural_hash_changes_when_attribute_added() {
        let a = doc(r#"<button label="x"/>"#);
        let b = doc(r#"<button label="x" disabled="true"/>"#);
        assert_ne!(structural_hash(&a), structural_hash(&b));
    }

    #[test]
    fn structural_hash_changes_when_nesting_changes() {
        let a = doc("<a><b/></a>");
        let b = doc("<a/><b/>");
        assert_ne!(structural_hash(&a), structural_hash(&b));
    }

    #[test]
    fn full_hash_is_stable_for_identical_inputs() {
        let a = doc(r#"<button label="Save" on:click="emit save"/>"#);
        let b = doc(r#"<button label="Save" on:click="emit save"/>"#);
        assert_eq!(full_hash(&a), full_hash(&b));
    }

    #[test]
    fn collect_literal_slots_returns_attribute_values_in_document_order() {
        let d = doc(r#"<container><button label="A"/><button label="B"/></container>"#);
        let slots = collect_literal_slots(&d);
        let by_attr: Vec<(&str, &str)> = slots
            .iter()
            .filter(|s| s.attr == "label")
            .map(|s| (s.path.as_str(), s.value.as_str()))
            .collect();
        // Children of the root container at index 0; first child
        // index = 0, second = 1.
        assert!(by_attr.contains(&("0.0", "A")));
        assert!(by_attr.contains(&("0.1", "B")));
    }

    #[test]
    fn collect_literal_slots_skips_expression_attributes() {
        // Expressions aren't literals — they need AST re-eval, so
        // a literal-only fast path must not include them.
        let d = doc(r#"<button label={state.title}/>"#);
        let slots = collect_literal_slots(&d);
        // The `label` attribute is an expression, so no slot for it.
        assert!(slots.iter().all(|s| s.attr != "label"));
    }

    #[test]
    fn template_fingerprint_bundles_all_three() {
        let d = doc(r#"<button label="Save"/>"#);
        let fp = TemplateFingerprint::of(&d);
        assert_eq!(fp.structural, structural_hash(&d));
        assert_eq!(fp.full, full_hash(&d));
        assert_eq!(fp.literals, collect_literal_slots(&d));
    }

    #[test]
    fn fast_path_check_only_literals_changed() {
        // The Phase 10 contract: if structural matches but full
        // differs, only literals changed. Verify with two templates
        // that satisfy that exactly.
        let a = doc(r#"<container><button label="A"/><button label="B"/></container>"#);
        let b = doc(r#"<container><button label="X"/><button label="Y"/></container>"#);
        assert_eq!(structural_hash(&a), structural_hash(&b));
        assert_ne!(full_hash(&a), full_hash(&b));
        let slots_a = collect_literal_slots(&a);
        let slots_b = collect_literal_slots(&b);
        assert_eq!(slots_a.len(), slots_b.len());
        // Same paths + attrs, different values → exactly the slots
        // the hot-reload patch needs to update.
        for (sa, sb) in slots_a.iter().zip(slots_b.iter()) {
            assert_eq!(sa.path, sb.path);
            assert_eq!(sa.attr, sb.attr);
            assert_ne!(sa.value, sb.value);
        }
    }

    // ── compare_fingerprints (Phase 10 hot-reload consumer) ──

    #[test]
    fn compare_fingerprints_no_change_when_identical() {
        let a = TemplateFingerprint::of(&doc(r#"<button label="Save"/>"#));
        let b = TemplateFingerprint::of(&doc(r#"<button label="Save"/>"#));
        assert_eq!(compare_fingerprints(&a, &b), PatchOutcome::NoChange);
    }

    #[test]
    fn compare_fingerprints_literal_only_when_only_values_differ() {
        let a = TemplateFingerprint::of(&doc(r#"<button label="Save"/>"#));
        let b = TemplateFingerprint::of(&doc(r#"<button label="Submit"/>"#));
        match compare_fingerprints(&a, &b) {
            PatchOutcome::LiteralOnly { diffs } => {
                assert_eq!(diffs.len(), 1);
                assert_eq!(diffs[0].path, "0");
                assert_eq!(diffs[0].attr, "label");
                assert_eq!(diffs[0].old_value, "Save");
                assert_eq!(diffs[0].new_value, "Submit");
            }
            other => panic!("expected LiteralOnly, got {other:?}"),
        }
    }

    #[test]
    fn compare_fingerprints_structural_when_tag_changes() {
        let a = TemplateFingerprint::of(&doc(r#"<button label="x"/>"#));
        let b = TemplateFingerprint::of(&doc(r#"<heading label="x"/>"#));
        assert_eq!(compare_fingerprints(&a, &b), PatchOutcome::Structural);
    }

    #[test]
    fn compare_fingerprints_structural_when_nesting_changes() {
        let a = TemplateFingerprint::of(&doc("<a><b/></a>"));
        let b = TemplateFingerprint::of(&doc("<a/><b/>"));
        assert_eq!(compare_fingerprints(&a, &b), PatchOutcome::Structural);
    }

    #[test]
    fn compare_fingerprints_literal_only_with_multiple_diffs() {
        let a = TemplateFingerprint::of(&doc(
            r#"<container><button label="A"/><text body="B"/></container>"#,
        ));
        let b = TemplateFingerprint::of(&doc(
            r#"<container><button label="X"/><text body="Y"/></container>"#,
        ));
        match compare_fingerprints(&a, &b) {
            PatchOutcome::LiteralOnly { diffs } => {
                assert_eq!(diffs.len(), 2);
                assert_eq!(diffs[0].new_value, "X");
                assert_eq!(diffs[1].new_value, "Y");
            }
            other => panic!("expected LiteralOnly, got {other:?}"),
        }
    }
}
