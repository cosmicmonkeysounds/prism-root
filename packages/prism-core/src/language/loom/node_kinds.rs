//! Loom `SyntaxNode::kind` string constants.
//!
//! Every `SyntaxNode` the parser emits carries a `kind` string. This
//! file is the **closed set** of names that may appear — adding a
//! new node shape means adding a constant here first, so the LSP /
//! semantic-tokens classifier / downstream tooling can do exhaustive
//! lookups.
//!
//! The constants are organised by grammar section (`loom-grammar.md`)
//! so a reader can find the production they care about. The string
//! values use `snake_case` to match the existing `language::luau`
//! convention (`function_call`, `local_assignment`, etc.).

// ─── Root / document structure (§1, §4) ─────────────────────────────

/// `RootNode::kind` is `"root"` by convention — see
/// `language::syntax::ast_types::RootKind`. Documents inside the root
/// carry the kind below.
pub const DOCUMENT: &str = "document";

pub const HEADER: &str = "header";
pub const DOC_TAG: &str = "doc_tag";
pub const PROPERTY: &str = "property";
pub const PROPERTY_VALUE: &str = "property_value";
pub const DOCSTRING: &str = "docstring";

// ─── Performance-model declarations (§5) ────────────────────────────

pub const CAST_DECL: &str = "cast_decl";
pub const CUE_DECL: &str = "cue_decl";
pub const LOCATION_DECL: &str = "location_decl";
pub const COHORT_DECL: &str = "cohort_decl";
pub const PARTICIPANT_LIFECYCLE: &str = "participant_lifecycle";
pub const LOCATION_EVENT: &str = "location_event";
pub const BROADCAST_BLOCK: &str = "broadcast_block";
pub const BROADCAST_SCOPE: &str = "broadcast_scope";
pub const SCOPE_ATOM: &str = "scope_atom";
pub const SLUGLINE_SCENE: &str = "slugline_scene";
pub const SCENE_SLUG: &str = "scene_slug";
pub const PARTICIPANT_SCOPE: &str = "participant_scope";
pub const IMPROV_PARENTHETICAL: &str = "improv_parenthetical";
pub const IMPROV_SPEC: &str = "improv_spec";
pub const ENROLL_ACTION: &str = "enroll_action";

// ─── Sections (§6) ──────────────────────────────────────────────────

pub const SECTION: &str = "section";
pub const BARK_SECTION: &str = "bark_section";
pub const QUEST_STAGE: &str = "quest_stage";
pub const TIMECODE_BLOCK: &str = "timecode_block";
pub const TRACK_COMMAND: &str = "track_command";
pub const MODIFIER: &str = "modifier";
pub const GUARD: &str = "guard";
pub const OBJECTIVE_LINE: &str = "objective_line";
pub const LIFECYCLE_LINE: &str = "lifecycle_line";

// ─── Content (§7) ───────────────────────────────────────────────────

pub const DIALOGUE: &str = "dialogue";
pub const SPEAKER_REF: &str = "speaker_ref";
pub const CHAR_BLOCK: &str = "char_block";
pub const CHAR_ITEM: &str = "char_item";
pub const PARENTHETICAL: &str = "parenthetical";
pub const TEXT_LINE: &str = "text_line";
pub const STAGE_DIRECTION: &str = "stage_direction";
pub const FLAVOR_LINE: &str = "flavor_line";
pub const CHOICE: &str = "choice";
pub const CHOICE_LABEL: &str = "choice_label";
pub const DIVERT: &str = "divert";
pub const RETURN_LINE: &str = "return_line";
pub const TUNNEL_CALL: &str = "tunnel_call";
pub const ACTION_LINE: &str = "action_line";
pub const KEYWORD_ACTION: &str = "keyword_action";
pub const NAMESPACE_CALL: &str = "namespace_call";
pub const MUTATION_EXPR: &str = "mutation_expr";
pub const ANNOTATION: &str = "annotation";

pub const EACH_VISIT_BLOCK: &str = "each_visit_block";
pub const VISIT_BRANCH: &str = "visit_branch";
pub const AFTER_BLOCK: &str = "after_block";
pub const OTHERWISE_BLOCK: &str = "otherwise_block";
pub const WHEN_BLOCK: &str = "when_block";
pub const MATCH_BLOCK: &str = "match_block";
pub const MATCH_ARM: &str = "match_arm";

// ─── Declarations & definitions (§8) ────────────────────────────────

pub const LIST_DECL: &str = "list_decl";
pub const RELATION_DECL: &str = "relation_decl";
pub const ENTITY_DECL: &str = "entity_decl";
pub const CONSTANT_DEF: &str = "constant_def";
pub const FUNCTION_DEF: &str = "function_def";
pub const MACRO_DEF: &str = "macro_def";
pub const LET_BINDING: &str = "let_binding";
pub const DEFINE_BINDING: &str = "define_binding";
pub const IMPORT_DECL: &str = "import_decl";
pub const EXPORT_DECL: &str = "export_decl";
pub const PARAM_LIST: &str = "param_list";

// ─── S-expressions (§9) ─────────────────────────────────────────────

pub const SEXP: &str = "sexp";
pub const SEXP_HEAD: &str = "sexp_head";
pub const SEXP_ATOM: &str = "sexp_atom";

// ─── Expressions (§10) ──────────────────────────────────────────────

pub const BINARY_EXPR: &str = "binary_expr";
pub const UNARY_EXPR: &str = "unary_expr";
pub const POSTFIX_EXPR: &str = "postfix_expr";
pub const CALL_EXPR: &str = "call_expr";
pub const FIELD_ACCESS: &str = "field_access";
pub const SAFE_NAV: &str = "safe_nav";
pub const INDEX_ACCESS: &str = "index_access";
pub const GROUPED_EXPR: &str = "grouped_expr";
pub const NUM_RANGE: &str = "num_range";
pub const LEDGER_PRED: &str = "ledger_pred";
pub const AGGREGATE_CALL: &str = "aggregate_call";
pub const LIST_COMPREHENSION: &str = "list_comprehension";

// ── Expression leaves
pub const NUMBER: &str = "number";
pub const STRING: &str = "string";
pub const IDENT: &str = "ident";
pub const BOOLEAN: &str = "boolean";
pub const NIL: &str = "nil";
pub const SPEAKER: &str = "speaker";

// ─── Sigils & references (§11) ──────────────────────────────────────

pub const RESOLVE_REF: &str = "resolve_ref";
pub const STATIC_REF: &str = "static_ref";
pub const BACKLINK: &str = "backlink";
pub const INLINE_ASSIGN: &str = "inline_assign";
pub const FIELD_CHAIN: &str = "field_chain";
pub const PRESENCE_CHECK: &str = "presence_check";
pub const QUALIFIED_REF: &str = "qualified_ref";

// ─── Inline text (§12) ──────────────────────────────────────────────

pub const TEXT_CONTENT: &str = "text_content";
pub const LITERAL_RUN: &str = "literal_run";
pub const ESCAPED_CHAR: &str = "escaped_char";
pub const TEXT_VARIATION: &str = "text_variation";
pub const VARIATION_MODE: &str = "variation_mode";
pub const VARIATION_VARIANT: &str = "variation_variant";
pub const INLINE_TRIGGER: &str = "inline_trigger";
pub const TRIGGER_ATTR: &str = "trigger_attr";
pub const TRIGGER_RANGE_SPEC: &str = "trigger_range_spec";
pub const CHAIN_TRIGGER: &str = "chain_trigger";
pub const COND_TRIGGER: &str = "cond_trigger";
pub const RANGE_CLOSER: &str = "range_closer";
pub const INLINE_EVAL: &str = "inline_eval";

// ─── Character archetype (§16) ──────────────────────────────────────

pub const KNOWLEDGE_BLOCK: &str = "knowledge_block";
pub const KNOWLEDGE_FIELD: &str = "knowledge_field";
pub const KNOWLEDGE_TYPE: &str = "knowledge_type";
pub const GOAL_DECL: &str = "goal_decl";
pub const GOAL_KNOB: &str = "goal_knob";
pub const ACTION_CHAIN: &str = "action_chain";
pub const DISPOSITION_BLOCK: &str = "disposition_block";
pub const DISPOSITION_AXIS: &str = "disposition_axis";
pub const DISPOSITION_REACT: &str = "disposition_react";
pub const MIRROR_CLAUSE: &str = "mirror_clause";
pub const HOOK_DECL: &str = "hook_decl";
pub const HOOK_PATTERN: &str = "hook_pattern";
pub const SLOT_BLOCK: &str = "slot_block";
pub const SLOT_PROPERTY: &str = "slot_property";
pub const FIELD_DECL: &str = "field_decl";
pub const SLOT_REQUIREMENT: &str = "slot_requirement";
pub const TYPE_EXPR: &str = "type_expr";
pub const DICT_LITERAL: &str = "dict_literal";
pub const DICT_ENTRY: &str = "dict_entry";
pub const LIST_LITERAL: &str = "list_literal";

// ─── Stats + tree (§17) ─────────────────────────────────────────────

pub const ATTRIBUTE_DECL: &str = "attribute_decl";
pub const ATTRIBUTE_MOD: &str = "attribute_mod";
pub const AXIS_DECL: &str = "axis_decl";
pub const AXIS_PROPERTY: &str = "axis_property";
pub const MILESTONE_ENTRY: &str = "milestone_entry";
pub const POOL_DECL: &str = "pool_decl";
pub const POOL_PROPERTY: &str = "pool_property";
pub const STAT_DECL: &str = "stat_decl";
pub const LOOKUP_PROPERTY: &str = "lookup_property";
pub const STAT_DERIVED: &str = "stat_derived";
pub const TREE_NODE_DECL: &str = "tree_node_decl";
pub const TREE_NODE_PROPERTY: &str = "tree_node_property";
pub const TREE_EFFECT: &str = "tree_effect";
pub const MODIFY_ACTION: &str = "modify_action";
pub const MODIFY_CLAUSE: &str = "modify_clause";

// ─── Reactivity (§18) ───────────────────────────────────────────────

pub const GENERATOR_DECL: &str = "generator_decl";
pub const SCENE_DECL: &str = "scene_decl";
pub const SCENE_STATE: &str = "scene_state";
pub const RETURN_STMT: &str = "return_stmt";
pub const SCHEDULER_PROPERTY: &str = "scheduler_property";
pub const LOOP_BLOCK: &str = "loop_block";
pub const TIME_STMT: &str = "time_stmt";
pub const TIME_SPEC: &str = "time_spec";
pub const WAIT_STMT: &str = "wait_stmt";
pub const YIELD_STMT: &str = "yield_stmt";
pub const YIELD_BODY: &str = "yield_body";
pub const COMPOSE_DECL: &str = "compose_decl";
pub const PATTERN_BLOCK: &str = "pattern_block";
pub const PATTERN_ARM: &str = "pattern_arm";
pub const SPAWN_ACTION: &str = "spawn_action";
pub const CANCEL_ACTION: &str = "cancel_action";
pub const AWAIT_EXPR: &str = "await_expr";
pub const RUN_EXPR: &str = "run_expr";

// ─── Faction archetype (§19) ────────────────────────────────────────

pub const INLINE_FACTION_DECL: &str = "inline_faction_decl";
pub const MEMBERS_BLOCK: &str = "members_block";
pub const MEMBER_ENTRY: &str = "member_entry";
pub const VISIBILITY_CLAUSE: &str = "visibility_clause";
pub const VISIBILITY_LEVEL: &str = "visibility_level";
pub const STATE_BLOCK: &str = "state_block";
pub const STATE_AXIS: &str = "state_axis";
pub const STANCE_BLOCK: &str = "stance_block";
pub const STANCE_ENTRY: &str = "stance_entry";
pub const STANCE_LEVEL: &str = "stance_level";
pub const FACTION_REF: &str = "faction_ref";
pub const FACTION_EVENT: &str = "faction_event";
pub const PARTICIPANT_FACTION_LIFECYCLE: &str = "participant_faction_lifecycle";
pub const DISCOVERY_EVENT: &str = "discovery_event";
pub const FACTION_SCOPE: &str = "faction_scope";
pub const STANCE_PRED: &str = "stance_pred";
pub const BELIEVED_STANCE_PRED: &str = "believed_stance_pred";
pub const BELIEVES_PRED: &str = "believes_pred";
pub const OBSERVER: &str = "observer";
pub const STANCE_MUTATION: &str = "stance_mutation";
pub const HIDDEN_FROM_CLAUSE: &str = "hidden_from_clause";
pub const REVEAL_ACTION: &str = "reveal_action";
pub const REVEAL_SUBJECT: &str = "reveal_subject";

// ─── Error & misc ───────────────────────────────────────────────────

/// Catch-all when the parser recovers from an unexpected token. The
/// `value` field holds a human-readable message; children, if any,
/// are the tokens that were skipped to resync.
pub const ERROR: &str = "error";

/// Annotation on a node's `data` map flagging it as containing a
/// parse error. Used by the LSP server when computing semantic
/// tokens — error-tagged nodes get an `invalid` style overlay.
pub const DATA_KEY_HAS_ERROR: &str = "has_error";

/// `SyntaxNode::data` key carrying the diagnostic id when an error
/// node was produced (e.g. `"unknown-action-kw"`). Stable across
/// versions — see `super::diagnostics`.
pub const DATA_KEY_DIAGNOSTIC_ID: &str = "diagnostic_id";

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    /// Every constant in this module must hold a unique string —
    /// duplicates would silently mean "two distinct AST shapes share
    /// a kind name," which is a bug.
    #[test]
    fn no_duplicate_kind_strings() {
        let all = [
            DOCUMENT, HEADER, DOC_TAG, PROPERTY, PROPERTY_VALUE, DOCSTRING,
            CAST_DECL, CUE_DECL, LOCATION_DECL, COHORT_DECL, PARTICIPANT_LIFECYCLE,
            LOCATION_EVENT, BROADCAST_BLOCK, BROADCAST_SCOPE, SCOPE_ATOM,
            SLUGLINE_SCENE, SCENE_SLUG, PARTICIPANT_SCOPE, IMPROV_PARENTHETICAL,
            IMPROV_SPEC, ENROLL_ACTION,
            SECTION, BARK_SECTION, QUEST_STAGE, TIMECODE_BLOCK, TRACK_COMMAND,
            MODIFIER, GUARD, OBJECTIVE_LINE, LIFECYCLE_LINE,
            DIALOGUE, SPEAKER_REF, CHAR_BLOCK, CHAR_ITEM, PARENTHETICAL, TEXT_LINE,
            STAGE_DIRECTION, FLAVOR_LINE, CHOICE, CHOICE_LABEL, DIVERT, RETURN_LINE,
            TUNNEL_CALL, ACTION_LINE, KEYWORD_ACTION, NAMESPACE_CALL, MUTATION_EXPR,
            ANNOTATION,
            EACH_VISIT_BLOCK, VISIT_BRANCH, AFTER_BLOCK, OTHERWISE_BLOCK,
            WHEN_BLOCK, MATCH_BLOCK, MATCH_ARM,
            LIST_DECL, RELATION_DECL, ENTITY_DECL, CONSTANT_DEF, FUNCTION_DEF,
            MACRO_DEF, LET_BINDING, DEFINE_BINDING, IMPORT_DECL, EXPORT_DECL,
            PARAM_LIST,
            SEXP, SEXP_HEAD, SEXP_ATOM,
            BINARY_EXPR, UNARY_EXPR, POSTFIX_EXPR, CALL_EXPR, FIELD_ACCESS,
            SAFE_NAV, INDEX_ACCESS, GROUPED_EXPR, NUM_RANGE, LEDGER_PRED,
            AGGREGATE_CALL, LIST_COMPREHENSION,
            NUMBER, STRING, IDENT, BOOLEAN, NIL, SPEAKER,
            RESOLVE_REF, STATIC_REF, BACKLINK, INLINE_ASSIGN, FIELD_CHAIN,
            PRESENCE_CHECK, QUALIFIED_REF,
            TEXT_CONTENT, LITERAL_RUN, ESCAPED_CHAR, TEXT_VARIATION,
            VARIATION_MODE, VARIATION_VARIANT, INLINE_TRIGGER, TRIGGER_ATTR,
            TRIGGER_RANGE_SPEC, CHAIN_TRIGGER, COND_TRIGGER, RANGE_CLOSER,
            INLINE_EVAL,
            KNOWLEDGE_BLOCK, KNOWLEDGE_FIELD, KNOWLEDGE_TYPE, GOAL_DECL,
            GOAL_KNOB, ACTION_CHAIN, DISPOSITION_BLOCK, DISPOSITION_AXIS,
            DISPOSITION_REACT, MIRROR_CLAUSE, HOOK_DECL, HOOK_PATTERN,
            SLOT_BLOCK, SLOT_PROPERTY, FIELD_DECL, SLOT_REQUIREMENT, TYPE_EXPR,
            DICT_LITERAL, DICT_ENTRY, LIST_LITERAL,
            ATTRIBUTE_DECL, ATTRIBUTE_MOD, AXIS_DECL, AXIS_PROPERTY,
            MILESTONE_ENTRY, POOL_DECL, POOL_PROPERTY, STAT_DECL,
            LOOKUP_PROPERTY, STAT_DERIVED, TREE_NODE_DECL, TREE_NODE_PROPERTY,
            TREE_EFFECT, MODIFY_ACTION, MODIFY_CLAUSE,
            GENERATOR_DECL, SCENE_DECL, SCENE_STATE, RETURN_STMT,
            SCHEDULER_PROPERTY, LOOP_BLOCK, TIME_STMT, TIME_SPEC, WAIT_STMT,
            YIELD_STMT, YIELD_BODY, COMPOSE_DECL, PATTERN_BLOCK, PATTERN_ARM,
            SPAWN_ACTION, CANCEL_ACTION, AWAIT_EXPR, RUN_EXPR,
            INLINE_FACTION_DECL, MEMBERS_BLOCK, MEMBER_ENTRY, VISIBILITY_CLAUSE,
            VISIBILITY_LEVEL, STATE_BLOCK, STATE_AXIS, STANCE_BLOCK,
            STANCE_ENTRY, STANCE_LEVEL, FACTION_REF, FACTION_EVENT,
            PARTICIPANT_FACTION_LIFECYCLE, DISCOVERY_EVENT, FACTION_SCOPE,
            STANCE_PRED, BELIEVED_STANCE_PRED, BELIEVES_PRED, OBSERVER,
            STANCE_MUTATION, HIDDEN_FROM_CLAUSE, REVEAL_ACTION, REVEAL_SUBJECT,
            ERROR,
        ];
        let mut seen = HashSet::with_capacity(all.len());
        for k in all {
            assert!(seen.insert(k), "duplicate node-kind string `{k}`");
        }
    }
}
