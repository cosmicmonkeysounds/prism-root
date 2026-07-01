//! Stable diagnostic ids and severities surfaced by the parser.
//!
//! Each `Code` value IS its stable five-character id (`L` + four
//! digits), mirroring `Code::id()` in the Rust parser. Reserved ranges:
//!
//! * `L1xxx` — lexer / line-classifier
//! * `L2xxx` — parser / structural

import type { Span } from "./source.ts";

export type Severity = "error" | "warning";

/**
 * Stable diagnostic codes. The constant's value is the wire id, so
 * `diag.code === Code.L1110AxisMissingMode` is `diag.code === "L1110"`.
 */
export const Code = {
  /** Indentation used tab characters; v3 is space-only. */
  L1001TabIndent: "L1001",
  /** A `key: value` line was missing the colon. */
  L1002MalformedProperty: "L1002",
  /** A choice line (`*` or `+`) was missing body text. */
  L1003EmptyChoice: "L1003",
  /** A `==` knot marker was missing the knot name. */
  L1004UnnamedKnot: "L1004",
  /** A `< … >` directive opened but never closed on the same line. */
  L1005UnterminatedDirective: "L1005",
  /** A declaration line (`CHARACTER`, …) was missing a name. */
  L1006UnnamedDeclaration: "L1006",
  /** A `/*` block comment opened but never closed before end of file. */
  L1007UnterminatedBlockComment: "L1007",
  /** An `is`-clause wrapped past the opener line (trailing `,` / unbalanced `()`). */
  L1008UnterminatedMixinClause: "L1008",
  /** Dialogue lines appeared without a SPEAKER above them. */
  L2001OrphanedDialogue: "L2001",
  /** `<-` appeared outside any tunnel context. */
  L2002BareTunnelReturn: "L2002",
  /** A disposition line was missing the target character name. */
  L1100MissingDispositionTarget: "L1100",
  /** A disposition line's `N of M` clause was malformed. */
  L1101MalformedDispositionAmount: "L1101",
  /** A `reacts <cond> → <tag>` clause was missing its arrow / tag. */
  L1102MalformedReactClause: "L1102",
  /** A `knows:` field row was missing a type spec. */
  L1103MalformedKnowledgeField: "L1103",
  /** A `goal name` block was missing its name. */
  L1104UnnamedGoal: "L1104",
  /** An `axis name` block did not declare a `mode:`. */
  L1110AxisMissingMode: "L1110",
  /** An `attribute name = N` line was malformed. */
  L1111MalformedAttribute: "L1111",
  /** A `pool name` block was missing `max:`. */
  L1112PoolMissingMax: "L1112",
  /** A `node name` block (in a TREE) was missing its name. */
  L1113UnnamedTreeNode: "L1113",
  /** `(improv …)` parenthetical did not include a `duration:` field. */
  L1140ImprovMissingDuration: "L1140",
  /** `advance on: …` listed an unknown / malformed signal. */
  L1141ImprovBadSignal: "L1141",
  /** `COHORT` body did not declare a `capacity:`. */
  L1142CohortNoCapacity: "L1142",
  /** A SCENE labelled state opener carried no name. */
  L1130SceneStateUnnamed: "L1130",
  /** A GENERATOR declaration carried no body. */
  L1131GeneratorMissingBody: "L1131",
  /** A `wait …` line did not match `wait until <expr>` or `wait <dur>`. */
  L1132WaitExpectsUntilOrDuration: "L1132",
} as const;

export type Code = (typeof Code)[keyof typeof Code];

export interface Diagnostic {
  code: Code;
  severity: Severity;
  message: string;
  span: Span;
}

export function errorDiagnostic(code: Code, span: Span, message: string): Diagnostic {
  return { code, severity: "error", message, span };
}
