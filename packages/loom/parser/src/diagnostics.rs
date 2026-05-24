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
