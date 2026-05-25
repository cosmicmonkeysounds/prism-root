//! Stable diagnostic ids and severities surfaced by the parser. The
//! LSP republishes these unchanged so editor squiggles round-trip
//! across versions.

use serde::{Deserialize, Serialize};

use crate::source::Span;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum Severity {
    Error,
    Warning,
}

/// Stable codes are five characters: `L` + four digits. Reserved
/// ranges:
///
/// * `L1xxx` — lexer / line-classifier
/// * `L2xxx` — parser / structural
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum Code {
    /// Indentation used tab characters; v3 is space-only.
    L1001TabIndent,
    /// A `key: value` line was missing the colon.
    L1002MalformedProperty,
    /// A choice line (`*` or `+`) was missing body text.
    L1003EmptyChoice,
    /// A `==` knot marker was missing the knot name.
    L1004UnnamedKnot,
    /// A `< … >` directive opened but never closed on the same line.
    L1005UnterminatedDirective,
    /// A declaration line (`CHARACTER`, `TRAIT`, …) was missing a name.
    L1006UnnamedDeclaration,
    /// A `/*` block comment opened but never closed before end of file.
    L1007UnterminatedBlockComment,
    /// Dialogue lines appeared without a SPEAKER above them.
    L2001OrphanedDialogue,
    /// `<-` appeared outside any tunnel context. Parser-level: we
    /// only check shape; semantic tunnel-depth tracking is the
    /// runtime's job.
    L2002BareTunnelReturn,
    /// A `trusts X: …` / `respects X: …` / `fears X: …` disposition
    /// line was missing the target character name.
    L1100MissingDispositionTarget,
    /// A disposition line's `N of M` clause was malformed.
    L1101MalformedDispositionAmount,
    /// A `reacts <cond> → <tag>` clause was missing its arrow / tag.
    L1102MalformedReactClause,
    /// A `knows:` field row was missing a type spec.
    L1103MalformedKnowledgeField,
    /// A `goal name` block was missing its name.
    L1104UnnamedGoal,
    /// An `axis name` block did not declare a `mode:`.
    L1110AxisMissingMode,
    /// An `attribute name = N` line was malformed.
    L1111MalformedAttribute,
    /// A `pool name` block was missing `max:`.
    L1112PoolMissingMax,
    /// A `node name` block (in a TREE) was missing its name.
    L1113UnnamedTreeNode,
    /// `(improv …)` parenthetical did not include a `duration:` field.
    L1140ImprovMissingDuration,
    /// `advance on: …` listed an unknown / malformed signal.
    L1141ImprovBadSignal,
    /// `COHORT` body did not declare a `capacity:`.
    L1142CohortNoCapacity,
    /// A SCENE labelled state opener carried no name.
    L1130SceneStateUnnamed,
    /// A GENERATOR declaration carried no body.
    L1131GeneratorMissingBody,
    /// A `wait …` line did not match `wait until <expr>` or
    /// `wait <duration>`.
    L1132WaitExpectsUntilOrDuration,
}

impl Code {
    pub fn id(self) -> &'static str {
        match self {
            Self::L1001TabIndent => "L1001",
            Self::L1002MalformedProperty => "L1002",
            Self::L1003EmptyChoice => "L1003",
            Self::L1004UnnamedKnot => "L1004",
            Self::L1005UnterminatedDirective => "L1005",
            Self::L1006UnnamedDeclaration => "L1006",
            Self::L1007UnterminatedBlockComment => "L1007",
            Self::L2001OrphanedDialogue => "L2001",
            Self::L2002BareTunnelReturn => "L2002",
            Self::L1100MissingDispositionTarget => "L1100",
            Self::L1101MalformedDispositionAmount => "L1101",
            Self::L1102MalformedReactClause => "L1102",
            Self::L1103MalformedKnowledgeField => "L1103",
            Self::L1104UnnamedGoal => "L1104",
            Self::L1110AxisMissingMode => "L1110",
            Self::L1111MalformedAttribute => "L1111",
            Self::L1112PoolMissingMax => "L1112",
            Self::L1113UnnamedTreeNode => "L1113",
            Self::L1140ImprovMissingDuration => "L1140",
            Self::L1141ImprovBadSignal => "L1141",
            Self::L1142CohortNoCapacity => "L1142",
            Self::L1130SceneStateUnnamed => "L1130",
            Self::L1131GeneratorMissingBody => "L1131",
            Self::L1132WaitExpectsUntilOrDuration => "L1132",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Diagnostic {
    pub code: Code,
    pub severity: Severity,
    pub message: String,
    pub span: Span,
}

impl Diagnostic {
    pub fn error(code: Code, span: Span, message: impl Into<String>) -> Self {
        Self {
            code,
            severity: Severity::Error,
            message: message.into(),
            span,
        }
    }
}
