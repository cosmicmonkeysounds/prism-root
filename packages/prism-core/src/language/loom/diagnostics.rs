//! Stable diagnostic IDs for the Loom parser + validator.
//!
//! Every entry in `docs/dev/loom-grammar.md` §21 lives here as a
//! `const DiagnosticSpec`. Tooling consumes the registry by stable
//! id — never by the human-readable message — so reordering /
//! rewording / adding new entries is safe across versions.
//!
//! ## How this fits in
//!
//! - The **parser** ([`super::parser`]) emits a small subset of these
//!   while building the tree (`doc-header-missing`, `bracket-unbalanced`,
//!   `indent-jump`, `unexpected-child`, `lex-error`, …). It can only
//!   produce diagnostics whose detection is purely syntactic.
//!
//! - The **validator** (future work) consumes the parsed tree plus
//!   the project registry and produces the registry-driven
//!   diagnostics (`unknown-cast`, `divert-target-unknown`,
//!   `stance-cycle`, `goal-no-priority`, …).
//!
//! - The **LSP** ([`super::provider`]) surfaces both layers as one
//!   `Diagnostic[]` stream into the editor.
//!
//! Severity policy mirrors the doc: hard errors block the build;
//! warnings highlight without blocking; info-level entries inform
//! without nagging.

use super::parser::Severity;

/// One row in the §21 catalog.
#[derive(Debug, Clone, Copy)]
pub struct DiagnosticSpec {
    pub id: &'static str,
    pub severity: Severity,
    /// Human-readable summary — used as the default message when no
    /// specific instance text is available. Editor messages override.
    pub summary: &'static str,
}

/// The catalog. Ordered by grammar section / topic for readability.
/// Lookups are linear; the slice is small enough (~70 entries) that
/// a HashMap would be overkill.
pub const CATALOG: &[DiagnosticSpec] = &[
    // ── Lexical / parse-time ────────────────────────────────────────
    spec("lex-error", Severity::Error, "Lexical error"),
    spec("indent-mixed", Severity::Error, "Tab/space mixing within a single indent unit"),
    spec("indent-jump", Severity::Warning, "Indent jumps more than one level"),
    spec("indent-multiple", Severity::Warning, "Indent not a multiple of the established unit"),

    // ── Document structure ──────────────────────────────────────────
    spec("doc-header-missing", Severity::Error, "File contains content but no `#` header"),
    spec("doc-header-id", Severity::Error, "`#` followed by no identifier"),
    spec("doc-type-unknown", Severity::Warning, "Header tag not in registry"),

    // ── Sections ────────────────────────────────────────────────────
    spec("section-duplicate", Severity::Error, "Two sections with the same id in one doc"),
    spec("section-empty", Severity::Info, "Section has no content"),

    // ── Actions / triggers ──────────────────────────────────────────
    spec("unknown-action-kw", Severity::Error, "Action keyword not in registry"),
    spec("unknown-trigger-kw", Severity::Warning, "Trigger type not in registry"),
    spec("unknown-role", Severity::Error, "Reference to an unregistered role"),
    spec("unknown-var", Severity::Error, "`$name` not in operand registry"),
    spec("unknown-ref", Severity::Error, "`@name` not in any registry"),
    spec("divert-target-unknown", Severity::Error, "`->` points at a non-existent section"),
    spec("method-chain-too-long", Severity::Error, "More than one trailing call in a postfix chain"),
    spec("assign-op-mismatch", Severity::Error, "`=` in mutation context or `:=` in binding context"),
    spec("mutation-of-role", Severity::Error, "Cannot mutate a role"),
    spec("bracket-unbalanced", Severity::Error, "Open bracket with no matching close"),
    spec("range-unclosed", Severity::Warning, "Ranged trigger opens but never closes"),
    spec("speaker-no-text", Severity::Warning, "Speaker block with no dialogue lines"),
    spec("stacked-conditions", Severity::Warning, "Multiple conditions back-to-back"),
    spec("orphaned-condition", Severity::Warning, "Condition with no entry below it"),
    spec("pin-action-implicit", Severity::Warning, "NextLink with a condition but no `.skip` / `.block` modifier"),
    spec("backlink-dead", Severity::Warning, "Backlink resolves to no entry"),

    // ── Performance model ───────────────────────────────────────────
    spec("unknown-cast", Severity::Error, "SPEAKER without a declared `cast`"),
    spec("unknown-cue", Severity::Error, "`cue` references an undeclared cue"),
    spec("unknown-location", Severity::Error, "References an undeclared location"),
    spec("unknown-cohort", Severity::Error, "References an undeclared cohort"),
    spec("choice-in-script", Severity::Error, "Choice inside a `:script` or `:film` document"),
    spec("participant-scope-leaked", Severity::Warning, "`as participant` section diverts to show-global without rescoping"),
    spec("broadcast-empty-scope", Severity::Warning, "Broadcast scope resolves to no participants"),
    spec("improv-without-duration", Severity::Info, "`(improv …)` with no `.duration` holds indefinitely"),

    // ── Character archetype ─────────────────────────────────────────
    spec("unknown-slot", Severity::Error, "Slot name not in the slot registry"),
    spec("unknown-hook-pred", Severity::Error, "Hook pattern head not in the registry"),
    spec("unknown-axis", Severity::Error, "Field resolves to no declared axis"),
    spec("unknown-pool", Severity::Error, "Field resolves to no declared pool"),
    spec("unknown-stat", Severity::Error, "Field resolves to no declared stat"),
    spec("unknown-attribute", Severity::Error, "Field resolves to no declared attribute"),
    spec("unknown-goal", Severity::Error, "References an undeclared goal"),
    spec("unknown-generator", Severity::Error, "References an undeclared generator"),
    spec("unknown-scene", Severity::Error, "References an undeclared scene"),
    spec("unknown-tree-node", Severity::Error, "References an undeclared tree node"),
    spec("unknown-compose", Severity::Error, "Invokes an undeclared compose"),
    spec("goal-no-priority", Severity::Error, "Goal body has no `priority` knob"),
    spec("goal-conflicting-knobs", Severity::Warning, "Conflicting goal knob settings"),
    spec("disposition-axis-bounds", Severity::Error, "Init falls outside declared range"),
    spec("disposition-mirror-cycle", Severity::Error, "Disposition mirror cycle"),
    spec("knowledge-bad-type", Severity::Error, "Knowledge field uses a type outside the closed set"),
    spec("knowledge-arithmetic", Severity::Error, "Arithmetic op on a knowledge field"),
    spec("hook-trigger-unmatched", Severity::Warning, "`passes`/`drops below` missing threshold"),

    // ── Stats ───────────────────────────────────────────────────────
    spec("axis-mode-missing", Severity::Error, "AxisDecl has no `mode` knob"),
    spec("axis-curve-required", Severity::Error, "Curve required for the declared axis mode"),
    spec("pool-max-required", Severity::Error, "Pool has no `max` knob"),
    spec("stat-form-ambiguous", Severity::Error, "Stat decl mixes expression and body forms"),
    spec("tree-cycle", Severity::Error, "Tree node `requires` cycle"),
    spec("tree-rank-overflow", Severity::Warning, "Rank exceeds declared max"),
    spec("modify-stack-conflict", Severity::Warning, "Conflicting modify stack policies"),

    // ── Reactivity ──────────────────────────────────────────────────
    spec("generator-yield-outside", Severity::Error, "`yield` outside generator body"),
    spec("wait-outside-coroutine", Severity::Error, "`wait`/`every`/`at` outside coroutine"),
    spec("scene-no-states", Severity::Error, "SceneCoroutineDecl has no SceneState"),
    spec("scene-unreachable-state", Severity::Warning, "SceneState never reached by any divert"),
    spec("scene-return-outside", Severity::Error, "`return` outside scene body"),
    spec("spawn-handle-discarded", Severity::Info, "Spawn handle not bound"),
    spec("cancel-unknown-handle", Severity::Error, "Cancel references unbound resolve"),
    spec("compose-arm-overlap", Severity::Warning, "Two pattern arms match same discriminant"),
    spec("compose-no-default", Severity::Info, "Compose has no `_` arm and non-exhaustive arms"),
    spec("let-rebind", Severity::Error, "`let x = …` declared twice in same scope"),
    spec("list-comprehension-bad-var", Severity::Error, "Comprehension variable shadows outer `let`"),

    // ── Factions ────────────────────────────────────────────────────
    spec("unknown-faction", Severity::Error, "`@F` referenced as faction not in registry"),
    spec("unknown-template", Severity::Error, "References an undeclared `:template` doc"),
    spec("unknown-stance-level", Severity::Error, "Stance level not built-in or registered"),
    spec("faction-no-label", Severity::Error, "Faction has no `.label` property"),
    spec("faction-template-with-members", Severity::Error, "`:template` doc declares MembersBlock"),
    spec("faction-members-empty", Severity::Warning, "Non-template faction has empty MembersBlock"),
    spec("stance-cycle", Severity::Error, "Mirrored stance pairs form a cycle"),
    spec("stance-default-conflict", Severity::Warning, "`default` collides with explicit entry"),
    spec("as-faction-bad-target", Severity::Error, "`as faction` on non-faction resolve"),
    spec("faction-spawn-position", Severity::Error, "`Faction.spawn` outside valid context"),
    spec("faction-event-no-binding", Severity::Warning, "Faction event body never reads `$FACTION`"),
    spec("unknown-namespace-call", Severity::Error, "Unknown method on namespace"),
    spec("faction-dissolve-no-reason", Severity::Info, "Dissolve has no reason"),
    spec("member-match-cyclic", Severity::Error, "Member match references its own faction"),
    spec("faction-membership-cycle", Severity::Error, "Sub-faction graph contains cycle"),
    spec("believed-stance-bad-observer", Severity::Error, "`as_known_by` target doesn't resolve"),
    spec("reveal-stance-uninit", Severity::Error, "Revealing default stance"),
    spec("visibility-bad-target", Severity::Error, "Visibility clause on non-observable target"),
    spec("visibility-level-unknown", Severity::Error, "Visibility level not built-in or registered"),
    spec("actually-double", Severity::Warning, "Nested `actually(actually(…))`"),
    spec("believes-non-observer", Severity::Error, "Believes target isn't a valid observer"),
    spec("parent-redundant", Severity::Info, "`.parent` and member listing both used"),

    // ── Scheduling / improv ─────────────────────────────────────────
    spec("tier-unknown", Severity::Error, "`.tier` not in registry"),
    spec("priority-out-of-range", Severity::Warning, "`.priority` outside [0, 1]"),
    spec("budget-ms-no-tier", Severity::Warning, "`.budget_ms` with no `.tier` anchor"),
    spec("advance-on-quorum-unreachable", Severity::Error, "Quorum count exceeds signal-list length"),
    spec("advance-on-empty", Severity::Error, "`.advance_on` with no signals"),
    spec("settling-window-non-positive", Severity::Warning, "`.settling_window` is 0"),
    spec("spawn-rate-limit-malformed", Severity::Error, "`.spawn_rate_limit` denominator not s/m/h"),
    spec("late-join-template-unknown", Severity::Error, "Late-join template not declared"),
    spec("reveal-membership-uninit", Severity::Error, "Revealing non-existent membership"),
    spec("discovery-hook-bad-arity", Severity::Error, "Discovery hook target doesn't resolve"),
    spec("improv-signal-unknown", Severity::Error, "Advance signal not built-in or registered"),
    spec("stance-as-known-by-symmetric", Severity::Warning, "Asymmetric belief-only update"),

    // ── Generic ─────────────────────────────────────────────────────
    spec("unexpected-child", Severity::Error, "Unexpected child in this position"),
];

const fn spec(id: &'static str, severity: Severity, summary: &'static str) -> DiagnosticSpec {
    DiagnosticSpec { id, severity, summary }
}

/// Look up a diagnostic by its stable id. Returns `None` if the id
/// isn't in the catalog (caller should treat that as a bug).
pub fn lookup(id: &str) -> Option<&'static DiagnosticSpec> {
    CATALOG.iter().find(|s| s.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn catalog_has_unique_ids() {
        let mut seen = HashSet::new();
        for spec in CATALOG {
            assert!(
                seen.insert(spec.id),
                "duplicate diagnostic id `{}`",
                spec.id
            );
        }
    }

    #[test]
    fn lookup_resolves_known_ids() {
        assert!(lookup("doc-header-missing").is_some());
        assert!(lookup("bracket-unbalanced").is_some());
        assert!(lookup("unknown-cast").is_some());
        assert!(lookup("stance-cycle").is_some());
        assert!(lookup("not-a-real-id").is_none());
    }

    #[test]
    fn every_parser_diagnostic_is_in_the_catalog() {
        // Spot-check the ids the parser actively emits today. If you
        // add a new diagnostic to parser.rs, register it here too so
        // this fence stays meaningful.
        for id in [
            "lex-error",
            "doc-header-missing",
            "doc-header-id",
            "doc-type-unknown",
            "bracket-unbalanced",
            "indent-jump",
            "unknown-cast",
            "unknown-cue",
            "unknown-cohort",
            "unknown-var",
            "unknown-ref",
            "unknown-trigger-kw",
            "unknown-action-kw",
            "divert-target-unknown",
            "unexpected-child",
            "assign-op-mismatch",
            "range-unclosed",
            "stat-form-ambiguous",
            "as-faction-bad-target",
            "disposition-mirror-cycle",
            "knowledge-bad-type",
        ] {
            assert!(
                lookup(id).is_some(),
                "parser diagnostic `{id}` missing from CATALOG"
            );
        }
    }
}
