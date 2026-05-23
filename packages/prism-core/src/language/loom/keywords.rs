//! Canonical keyword / sigil / token registry for the Loom language.
//!
//! Every reservation in [`docs/dev/loom-grammar.md` §3.1] lives here as a
//! categorised `const` slice. The categorisation matters: the TextMate
//! grammar emitter ([`super::tmgrammar`]) maps each category to a distinct
//! scope name (`keyword.control.flow.loom`, `keyword.story.loom`, etc.) so
//! editor themes can colour them independently.
//!
//! When the Loom parser lands, this table also becomes the lexer's
//! reserved-word check — the same const slice that drives editor
//! highlights is the one that drives parse errors. No drift, no second
//! source of truth.
//!
//! ## Adding a keyword
//!
//! 1. Find the right category below (or add a new one — see
//!    `KEYWORD_CATEGORIES` at the bottom).
//! 2. Append it. The order within a category is informational only;
//!    duplicates inside the table are caught by `tests::no_overlap`.
//! 3. Re-run `prism codegen loom-tmgrammar` to refresh the editor
//!    grammar.

// ─── Control / expression ────────────────────────────────────────────────
//
// `if`, boolean / logical operators, comparison / membership words. These
// appear inside `Guard`s and expressions (grammar §10) and are the most
// "code-like" of Loom's keywords — themes typically render them in the
// `keyword.control` family.

pub const KW_CONTROL: &[&str] = &["if", "and", "or", "not", "is", "has", "in", "of"];

// ─── Bindings & definitions ──────────────────────────────────────────────
//
// Words that introduce a binding form. Mostly used in s-expression
// position (`(define ...)`, `(defn ...)`).

pub const KW_BINDING: &[&str] = &["var", "let", "define", "defn", "defmacro"];

// ─── Action keywords (registry-driven) ───────────────────────────────────
//
// Used on `ActionLine` (grammar §7.5). Studio-registered actions extend
// this set at runtime; what's listed here is the built-in floor.

pub const KW_ACTION: &[&str] = &["fire", "advance", "trigger", "modify"];

// ─── Story / block keywords ──────────────────────────────────────────────
//
// Each / visit / after / otherwise / when / match — the structural blocks
// inside a section (grammar §7.7), plus the import / export module forms.

pub const KW_STORY_BLOCKS: &[&str] = &[
    "each",
    "visit",
    "first",
    "then",
    "finally",
    "after",
    "otherwise",
    "when",
    "match",
    "import",
    "export",
];

// ─── Literal constants ───────────────────────────────────────────────────

pub const KW_LITERAL_CONSTANTS: &[&str] = &["true", "false", "nil"];

// ─── Live-performance vocabulary (grammar §5) ────────────────────────────

pub const KW_PERFORMANCE: &[&str] = &[
    "cast",
    "cue",
    "cohort",
    "location",
    "broadcast",
    "improv",
    "enroll",
    "as",
    "joins",
    "leaves",
    "enters",
    "exits",
    "participant",
];

// ─── Character archetype vocabulary (grammar §16) ────────────────────────

pub const KW_CHARACTER: &[&str] = &[
    "type",
    "extends",
    "knowledge",
    "goal",
    "disposition",
    "mirror",
    "on",
    "meeting",
    "passes",
    "drops",
    "below",
    "reacts",
    "init",
    "range",
];

// ─── Stats / progression vocabulary (grammar §17) ────────────────────────

pub const KW_STATS: &[&str] = &[
    "attribute",
    "axis",
    "pool",
    "stat",
    "node",
    "ability",
    "rank",
    "mode",
    "curve",
    "milestones",
    "table",
    "interpolate",
    "lookup",
];

// ─── Axis modes (grammar §17.2) ──────────────────────────────────────────

pub const KW_AXIS_MODES: &[&str] = &[
    "xp_curve",
    "use_tracking",
    "point_buy",
    "milestone",
    "narrative_trigger",
    "sdk_controlled",
];

// ─── Reactivity & dynamics (grammar §18) ─────────────────────────────────

pub const KW_REACTIVITY: &[&str] = &[
    "generator",
    "scene",
    "loop",
    "wait",
    "until",
    "at",
    "every",
    "yield",
    "spawn",
    "cancel",
    "await",
    "return",
    "with_chance",
    "random",
    "compose",
    "pattern",
    "bark",
    "from",
    "run",
];

// ─── Goal knobs (grammar §16.3) ──────────────────────────────────────────

pub const KW_GOAL: &[&str] = &[
    "priority",
    "active_when",
    "completes_when",
    "fails_when",
    "drives",
    "on_complete",
    "on_fail",
];

// ─── Faction vocabulary (grammar §19) ────────────────────────────────────

pub const KW_FACTION: &[&str] = &[
    "faction",
    "template",
    "members",
    "state",
    "stance",
    "emerges",
    "dissolves",
    "grows",
    "shrinks",
    "changes",
    "proposes",
    "founds",
    "asymmetric",
    "parent",
    "default",
    "size",
    "age",
    "has_stance",
    "is_at_least",
    "is_at_most",
    "discovers_member",
    "membership",
];

// ─── Visibility / belief vocabulary (grammar §19.3) ──────────────────────

pub const KW_VISIBILITY: &[&str] = &[
    "visibility",
    "public",
    "private",
    "actually",
    "as_known_by",
    "believes",
    "reveal",
    "revealed",
    "hidden_from",
];

// ─── Stance levels (grammar §19.3) ───────────────────────────────────────
//
// Built-in stance values. The list is open — studios register
// additional levels via `LoomRegistry`.

pub const KW_STANCE_LEVELS: &[&str] = &["hostile", "wary", "neutral", "friendly", "allied"];

// ─── Scheduling tiers (grammar §9.8 / §18) ───────────────────────────────

pub const KW_SCHEDULING: &[&str] = &["focal", "active", "ambient", "budget_ms", "tier"];

// ─── Improv advancement signals (grammar §5.1 / §18) ─────────────────────

pub const KW_IMPROV: &[&str] = &[
    "advance_on",
    "pedal",
    "tap",
    "speech",
    "gesture",
    "quorum",
    "stage_manager",
    "performer",
];

// ─── Template knobs (grammar §19.9) ──────────────────────────────────────

pub const KW_TEMPLATE: &[&str] = &[
    "settling_window",
    "spawn_rate_limit",
    "late_join_observer_template",
];

// ─── S-expression operator keywords (grammar §9) ─────────────────────────
//
// Words that lead s-expressions when used as the head of a `(op args ...)`
// form: declaration keywords plus the cardinality words for relations.

pub const KW_SEXP_DECL: &[&str] = &[
    "list",
    "relation",
    "entity",
    "one-to-one",
    "one-to-many",
    "many-to-many",
];

// ─── Document type tags (grammar §4.1) ───────────────────────────────────
//
// Header tags. The leading `:` is grammar; the words below are what come
// after it. Stored without the `:` so consumers can decide whether to
// match `:conversation` (text source) or `conversation` (AST node).

pub const DOC_TAGS: &[&str] = &[
    "conversation",
    "barks",
    "quest",
    "cutscene",
    "script",
    "film",
    "immersive",
    "character",
    "type",
    "stats",
    "tree",
    "faction",
    "template",
    "typewriter",
    "module",
];

// ─── Built-in inline trigger types (grammar §12.2) ───────────────────────
//
// Trigger types are open — the registry decides which ones are valid in
// a given project — but these are the ones the docs use as canonical
// examples and are safe to colour as built-ins.

pub const BUILTIN_TRIGGER_TYPES: &[&str] = &[
    "pause",  // <pause:300>
    "sfx",    // <sfx:bell>
    "vo",     // <vo:line>
    "speed",  // <speed:0.7> ... </>
    "camera", // <camera:shake>
    "cue",    // <cue:lights_warm>
    "color",  // <color:red> ... </>
    "shake",  // <shake:strong> ... </>
];

// ─── Built-in annotations (grammar §7.6) ─────────────────────────────────

pub const BUILTIN_ANNOTATIONS: &[&str] = &["vo", "director", "status", "note", "hint", "loc"];

// ─── Built-in section / divert modifiers (grammar §6.1 / §7.3 / §7.4) ───

pub const BUILTIN_MODIFIERS: &[&str] = &[
    "once",
    "hub",
    "return", // section
    "sticky",
    "fallback",
    "interrupt",
    "show",     // choice
    "dispatch", // divert
    "critical",
    "high",
    "normal",
    "low", // bark priority
    "skip",
    "block", // next link
];

// ─── Built-in ledger predicates (grammar §11.6) ──────────────────────────

pub const BUILTIN_LEDGER_PREDS: &[&str] = &["played", "visits", "chose", "last", "since", "count"];

// ─── Aggregate built-ins (grammar §18.4) ─────────────────────────────────

pub const BUILTIN_AGGREGATES: &[&str] = &[
    "any", "all", "count", "min", "max", "closest", "first", "last",
];

// ─── Knowledge type names (grammar §16.2) ────────────────────────────────

pub const KNOWLEDGE_TYPES: &[&str] = &["bool", "int", "float", "string", "list"];

// ─── Categories — drives TextMate scope generation ───────────────────────
//
// Each entry maps a const slice to a TextMate scope-name suffix. The
// emitter joins it with `loom` to build the full scope name (e.g.
// `keyword.control.flow` → `keyword.control.flow.loom`).
//
// The order matters only for theme overrides: more specific categories
// shadow more general ones if a word happens to appear in both. We
// detect that via `tests::no_overlap`.

pub struct KeywordCategory {
    pub name: &'static str,
    /// TextMate scope suffix (no trailing `.loom`).
    pub scope: &'static str,
    pub words: &'static [&'static str],
}

pub const KEYWORD_CATEGORIES: &[KeywordCategory] = &[
    KeywordCategory {
        name: "control",
        scope: "keyword.control.flow",
        words: KW_CONTROL,
    },
    KeywordCategory {
        name: "binding",
        scope: "keyword.declaration",
        words: KW_BINDING,
    },
    KeywordCategory {
        name: "action",
        scope: "keyword.other.action",
        words: KW_ACTION,
    },
    KeywordCategory {
        name: "story_blocks",
        scope: "keyword.control.block",
        words: KW_STORY_BLOCKS,
    },
    KeywordCategory {
        name: "literals",
        scope: "constant.language",
        words: KW_LITERAL_CONSTANTS,
    },
    KeywordCategory {
        name: "performance",
        scope: "keyword.other.performance",
        words: KW_PERFORMANCE,
    },
    KeywordCategory {
        name: "character",
        scope: "keyword.other.character",
        words: KW_CHARACTER,
    },
    KeywordCategory {
        name: "stats",
        scope: "keyword.other.stats",
        words: KW_STATS,
    },
    KeywordCategory {
        name: "axis_modes",
        scope: "constant.language.axis-mode",
        words: KW_AXIS_MODES,
    },
    KeywordCategory {
        name: "reactivity",
        scope: "keyword.other.reactivity",
        words: KW_REACTIVITY,
    },
    KeywordCategory {
        name: "goal",
        scope: "keyword.other.goal",
        words: KW_GOAL,
    },
    KeywordCategory {
        name: "faction",
        scope: "keyword.other.faction",
        words: KW_FACTION,
    },
    KeywordCategory {
        name: "visibility",
        scope: "keyword.other.visibility",
        words: KW_VISIBILITY,
    },
    KeywordCategory {
        name: "stance_levels",
        scope: "constant.language.stance",
        words: KW_STANCE_LEVELS,
    },
    KeywordCategory {
        name: "scheduling",
        scope: "keyword.other.scheduling",
        words: KW_SCHEDULING,
    },
    KeywordCategory {
        name: "improv",
        scope: "keyword.other.improv",
        words: KW_IMPROV,
    },
    KeywordCategory {
        name: "template",
        scope: "keyword.other.template",
        words: KW_TEMPLATE,
    },
    KeywordCategory {
        name: "sexp_decl",
        scope: "keyword.declaration.sexp",
        words: KW_SEXP_DECL,
    },
    KeywordCategory {
        name: "doc_tags",
        scope: "entity.name.tag",
        words: DOC_TAGS,
    },
    KeywordCategory {
        name: "builtin_triggers",
        scope: "support.function.trigger",
        words: BUILTIN_TRIGGER_TYPES,
    },
    KeywordCategory {
        name: "builtin_annotations",
        scope: "support.function.annotation",
        words: BUILTIN_ANNOTATIONS,
    },
    KeywordCategory {
        name: "builtin_modifiers",
        scope: "support.type.modifier",
        words: BUILTIN_MODIFIERS,
    },
    KeywordCategory {
        name: "builtin_ledger",
        scope: "support.function.ledger",
        words: BUILTIN_LEDGER_PREDS,
    },
    KeywordCategory {
        name: "builtin_aggregates",
        scope: "support.function.aggregate",
        words: BUILTIN_AGGREGATES,
    },
    KeywordCategory {
        name: "knowledge_types",
        scope: "storage.type",
        words: KNOWLEDGE_TYPES,
    },
];

// ─── Sigils (single-token sentinels) ─────────────────────────────────────

pub struct Sigil {
    pub name: &'static str,
    pub literal: &'static str,
    /// TextMate scope suffix (no trailing `.loom`).
    pub scope: &'static str,
}

/// Sigils that are matched on their own (not part of a larger pattern in
/// `tmgrammar`). Multi-character sigils like `->`, `<-`, `[[`, `]]`,
/// `${`, `$(`, `==`, `!=`, `:=`, `+=`, `-=`, `++`, `?.`, `?=`, `!?=`,
/// `</>`, `</%` are encoded directly in the grammar's patterns and not
/// listed here (their context matters).
pub const ATOMIC_SIGILS: &[Sigil] = &[
    Sigil {
        name: "section",
        literal: "--",
        scope: "punctuation.section.section",
    },
    Sigil {
        name: "slugline",
        literal: "##",
        scope: "punctuation.section.slugline",
    },
    Sigil {
        name: "header",
        literal: "#",
        scope: "punctuation.section.header",
    },
    Sigil {
        name: "choice_once",
        literal: "*",
        scope: "punctuation.section.choice.once",
    },
    Sigil {
        name: "choice_sticky",
        literal: "+",
        scope: "punctuation.section.choice.sticky",
    },
    Sigil {
        name: "flavor",
        literal: ">",
        scope: "punctuation.section.flavor",
    },
    Sigil {
        name: "action",
        literal: "~",
        scope: "punctuation.section.action",
    },
    Sigil {
        name: "anchor",
        literal: "%",
        scope: "punctuation.section.anchor",
    },
];

// ─── Multi-character operators ───────────────────────────────────────────
//
// Used by the TextMate emitter to colour assignments / comparisons /
// arrows separately from arithmetic. Order matters: longest-first so the
// emitter's regex prefers `:=` over `=`, `<=` over `<`, etc.

pub struct Operator {
    pub literal: &'static str,
    /// TextMate scope suffix (no trailing `.loom`).
    pub scope: &'static str,
}

pub const OPERATORS: &[Operator] = &[
    // Arrows / control
    Operator {
        literal: "->",
        scope: "keyword.control.divert",
    },
    Operator {
        literal: "<-",
        scope: "keyword.control.return",
    },
    // Assignment
    Operator {
        literal: ":=",
        scope: "keyword.operator.assignment.walrus",
    },
    Operator {
        literal: "+=",
        scope: "keyword.operator.assignment.augmented",
    },
    Operator {
        literal: "-=",
        scope: "keyword.operator.assignment.augmented",
    },
    Operator {
        literal: "++",
        scope: "keyword.operator.assignment.increment",
    },
    // Comparison
    Operator {
        literal: "==",
        scope: "keyword.operator.comparison",
    },
    Operator {
        literal: "!=",
        scope: "keyword.operator.comparison",
    },
    Operator {
        literal: ">=",
        scope: "keyword.operator.comparison",
    },
    Operator {
        literal: "<=",
        scope: "keyword.operator.comparison",
    },
    // Safe nav / membership
    Operator {
        literal: "?.",
        scope: "keyword.operator.navigation.safe",
    },
    Operator {
        literal: "?=",
        scope: "keyword.operator.membership",
    },
    Operator {
        literal: "!?=",
        scope: "keyword.operator.membership.negated",
    },
    // Plain assignment / arithmetic — listed last so longer ops win.
    Operator {
        literal: "=",
        scope: "keyword.operator.assignment",
    },
    Operator {
        literal: "+",
        scope: "keyword.operator.arithmetic",
    },
    Operator {
        literal: "-",
        scope: "keyword.operator.arithmetic",
    },
    Operator {
        literal: "*",
        scope: "keyword.operator.arithmetic",
    },
    Operator {
        literal: "/",
        scope: "keyword.operator.arithmetic",
    },
    Operator {
        literal: "%",
        scope: "keyword.operator.arithmetic",
    },
    Operator {
        literal: "<",
        scope: "keyword.operator.comparison",
    },
    Operator {
        literal: ">",
        scope: "keyword.operator.comparison",
    },
];

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{HashMap, HashSet};

    #[test]
    fn no_duplicate_keywords_within_a_category() {
        for cat in KEYWORD_CATEGORIES {
            let mut seen = HashSet::new();
            for w in cat.words {
                assert!(
                    seen.insert(*w),
                    "category `{}` lists `{}` more than once",
                    cat.name,
                    w
                );
            }
        }
    }

    #[test]
    fn keyword_overlap_is_documented() {
        // Track which categories claim each word. Overlap is allowed —
        // some words (e.g. `at`, `last`, `first`, `count`, `default`)
        // legitimately serve double duty — but the emitter needs to know
        // which one wins. We assert the known set so a new accidental
        // overlap surfaces in CI.
        let mut owners: HashMap<&str, Vec<&str>> = HashMap::new();
        for cat in KEYWORD_CATEGORIES {
            for w in cat.words {
                owners.entry(*w).or_default().push(cat.name);
            }
        }
        let actual: HashSet<&&str> = owners
            .iter()
            .filter(|(_, v)| v.len() > 1)
            .map(|(k, _)| k)
            .collect();
        let expected_overlap: HashSet<&&str> = [
            // Time / generator words that also serve as aggregate / event names.
            &"at",
            &"last",
            &"first",
            &"count",
            // Doc-tag words that also live in their archetype's vocabulary.
            // `:faction`, `:template`, `:type` appear in DOC_TAGS *and* in
            // their keyword categories.
            &"faction",
            &"template",
            &"type",
            // Built-in trigger types that also have a non-trigger reservation.
            // `cue` is both a CueDecl keyword and an inline trigger built-in.
            // `vo` is both an annotation and a trigger built-in.
            &"cue",
            &"vo",
            // `list` is an s-expr decl (`(list ...)`) *and* a Knowledge field
            // type (`list<int>`).
            &"list",
            // Resolver / control words that double as identifiers in
            // different positions.
            &"default",
            &"return",
            &"range",
            &"on",
            &"node",
            &"size",
            &"age",
        ]
        .iter()
        .copied()
        .collect();
        let unexpected: Vec<_> = actual.difference(&expected_overlap).collect();
        assert!(
            unexpected.is_empty(),
            "new keyword overlap detected: {:?}\nadd them to expected_overlap or rename to disambiguate",
            unexpected
        );
    }

    #[test]
    fn every_grammar_keyword_is_listed_somewhere() {
        // Spot-check a handful of keywords from §3.1 that should always
        // be present. A full check requires parsing the doc, which we
        // don't do — this is just a smoke test.
        let mut all = HashSet::new();
        for cat in KEYWORD_CATEGORIES {
            for w in cat.words {
                all.insert(*w);
            }
        }
        for kw in [
            "if",
            "and",
            "or",
            "not",
            "var",
            "let",
            "define",
            "defn",
            "cast",
            "cue",
            "cohort",
            "location",
            "broadcast",
            "knowledge",
            "goal",
            "disposition",
            "attribute",
            "axis",
            "pool",
            "stat",
            "generator",
            "scene",
            "loop",
            "wait",
            "until",
            "every",
            "faction",
            "members",
            "state",
            "stance",
            "hostile",
            "neutral",
            "allied",
            "true",
            "false",
            "nil",
        ] {
            assert!(
                all.contains(kw),
                "keyword `{}` missing from KEYWORD_CATEGORIES",
                kw
            );
        }
    }
}
