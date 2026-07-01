//! Canonical keyword tables — declaration kinds, syntactic-directive
//! names, builtin directive verbs, contract keys, scene-heading
//! prefixes, and the line-marker glyphs the lexer dispatches on.
//!
//! This is the single source of truth for the parser and every external
//! editor surface (TextMate grammar, CodeMirror StreamLanguage).

import {
  declarationKindFromKeyword,
  declarationsWithKind,
  type DeclarationKind,
} from "./ast.ts";

/** `KEYWORD Name [is …]` declaration openers (spec §6, §10, §11). */
export const DECLARATIONS: readonly string[] = [
  "CHARACTER",
  "ROLE",
  "TRAIT",
  "ITEM",
  "LOCATION",
  "FACTION",
  "STATS",
  "TREE",
  "GENERATOR",
  "SCENE",
  "COHORT",
  "PERSON",
  "ROSTER",
  "SPACE",
  "CHANNEL",
];

/** Directive verbs whose shape is part of the grammar (spec §14.2). */
export const SYNTACTIC_DIRECTIVES: readonly string[] = [
  "if",
  "else if",
  "else",
  "match",
  "case",
  "for",
  "each visit",
  "after",
  "otherwise",
  "anchor",
  "let",
];

/** Builtin directive verbs registered by the runtime. */
export const BUILTIN_DIRECTIVES: readonly string[] = [
  "sfx",
  "cue",
  "pause",
  "anchor",
  "fire",
  "set",
  "cast",
  "uncast",
  "recast",
  "promote",
  "demote",
  "load_roster",
];

/** Contract-zone property keys with grammar-level meaning (spec §6, §13). */
export const CONTRACT_KEYS: readonly string[] = [
  "cast",
  "setting",
  "with topic",
  "with",
  "entry",
  "tags",
  "title",
  "author",
  "next",
];

/** Fountain-style scene-heading prefixes (spec §6). */
export const SCENE_HEADING_PREFIXES: readonly string[] = [
  "INT.",
  "EXT.",
  "INT/EXT",
  "INT./EXT.",
  "I/E ",
];

/** Reserved keyword words that appear inside lines (not as openers). */
export const RESERVED_INLINE: readonly string[] = [
  "let",
  "is",
  "with",
  "END",
  "super",
  "none",
  "self",
  "init",
  "method",
];

/** Live-performance vocabulary (spec §13). */
export const LIVE_KEYWORDS: readonly string[] = [
  "participant",
  "Participant",
  "enters",
  "exits",
  "joins",
  "passes",
  "drops",
  "below",
  "cohort",
  "location",
  "but",
  "and",
  "improv",
  "duration",
  "advance",
  "on",
  "quorum",
  "all",
  "any",
  "pedal",
  "speech",
  "gesture",
  "broadcast",
  "enroll",
];

/** Inline keywords introducing structured sub-blocks (spec §10, §11). */
export const SIMULACRA_KEYWORDS: readonly string[] = [
  "trusts",
  "respects",
  "fears",
  "reacts",
  "knows",
  "mirror",
  "of",
  "goal",
  "priority",
  "active when",
  "completes when",
  "fails when",
  "drives",
  "on complete",
  "on fail",
  "generator",
  "tier",
  "spawn",
  "run",
  "wait",
  "yield",
  "loop",
  "return",
  "every",
  "at",
  "when",
];

/** Meridian primitive openers (spec §11). */
export const MERIDIAN_KEYWORDS: readonly string[] = [
  "attribute",
  "axis",
  "pool",
  "stat",
  "node",
  "mode",
  "curve",
  "max",
  "regen",
  "cost",
  "requires",
  "effect",
  "range",
];

/** Stable line-marker glyphs the lexer dispatches on (spec §2, §5). */
export const markers = {
  HEADING: "#",
  KNOT: "==",
  CHOICE_ONCE: "*",
  CHOICE_STICKY: "+",
  DIVERT: "->",
  TUNNEL_RETURN: "<-",
  FENCE: "```",
  DIRECTIVE_OPEN: "<",
  DIRECTIVE_CLOSE: ">",
  INTERPOLATION_OPEN: "{",
  INTERPOLATION_CLOSE: "}",
  SUPPRESSION_OPEN: "[",
  SUPPRESSION_CLOSE: "]",
  PARENTHETICAL_OPEN: "(",
  PARENTHETICAL_CLOSE: ")",
  LINE_COMMENT: "//",
  BLOCK_COMMENT_OPEN: "/*",
  BLOCK_COMMENT_CLOSE: "*/",
} as const;

/** Looks up a declaration keyword. */
export function declarationKind(word: string): DeclarationKind | null {
  return declarationKindFromKeyword(word);
}

export { declarationsWithKind };
