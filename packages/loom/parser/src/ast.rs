//! Loom v3 AST (spec §6).
//!
//! A `.loom` file lowers to a [`LoomFile`] carrying three zones:
//!
//! * `header`       — `# Title` + `key: value` properties
//! * `declarations` — `CHARACTER`, `TRAIT`, `ITEM`, `LOCATION`,
//!   `FACTION`, `STATS`, `TREE`, `GENERATOR`, `SCENE`, `COHORT`
//! * `beats`        — `== knot_name` with a contract + woven body
//!
//! Plus top-level `let` bindings, which can appear anywhere in the
//! declaration zone (spec §12.1).
//!
//! Phase-2 invariant: declaration bodies and `<…>` directives are
//! captured *opaquely* (raw text + span). Later phases lower them
//! into structured sub-ASTs without changing the surrounding shape.

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use crate::source::Span;

/// One parsed `.loom` file. Names are flat across the project; the
/// runtime's resolver indexes every `LoomFile` and answers
/// cross-file lookups.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct LoomFile {
    pub header: Header,
    pub items: Vec<Item>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Header {
    /// The `# Title` line, if present.
    pub title: Option<String>,
    /// `key: value` properties in source order (`entry`, `tags`, …).
    pub properties: IndexMap<String, PropertyValue>,
    pub span: Span,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PropertyValue {
    pub value: String,
    pub span: Span,
}

/// Top-level item in a file. Declarations and beats can interleave
/// freely (spec §3: "any declaration may live in any file").
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Item {
    Declaration(Declaration),
    LetBinding(LetBinding),
    Beat(Beat),
}

/// A declaration block — `CHARACTER`, `TRAIT`, `ITEM`, `LOCATION`,
/// `FACTION`, `STATS`, `TREE`, `GENERATOR`, `SCENE`, `COHORT`.
///
/// Phase-2: the body is captured as raw indented text. The §9 mixin
/// resolver, §10 Simulacra body, §11 Meridian primitives, §12.3
/// Scene state machine, and §12.4 Generator coroutines are all
/// produced from this raw body in subsequent phases.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Declaration {
    pub kind: DeclarationKind,
    pub name: String,
    /// `is X, Y, Z` mixin clause on the opener line. Empty if absent.
    pub mixin: Vec<String>,
    /// Raw indented body lines, with their leading whitespace
    /// preserved relative to the opener's column.
    pub body: Vec<RawLine>,
    pub span: Span,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum DeclarationKind {
    Character,
    Trait,
    Item,
    Location,
    Faction,
    Stats,
    Tree,
    Generator,
    Scene,
    Cohort,
}

impl DeclarationKind {
    /// The screaming-snake spelling that appears in source.
    pub fn keyword(self) -> &'static str {
        match self {
            Self::Character => "CHARACTER",
            Self::Trait => "TRAIT",
            Self::Item => "ITEM",
            Self::Location => "LOCATION",
            Self::Faction => "FACTION",
            Self::Stats => "STATS",
            Self::Tree => "TREE",
            Self::Generator => "GENERATOR",
            Self::Scene => "SCENE",
            Self::Cohort => "COHORT",
        }
    }

    pub fn from_keyword(word: &str) -> Option<Self> {
        Some(match word {
            "CHARACTER" => Self::Character,
            "TRAIT" => Self::Trait,
            "ITEM" => Self::Item,
            "LOCATION" => Self::Location,
            "FACTION" => Self::Faction,
            "STATS" => Self::Stats,
            "TREE" => Self::Tree,
            "GENERATOR" => Self::Generator,
            "SCENE" => Self::Scene,
            "COHORT" => Self::Cohort,
            _ => return None,
        })
    }
}

/// Top-level reactive `let` binding (spec §12.1).
///
/// Phase-2: the expression is stored as raw text. The expression
/// parser + reactive graph wiring lands with the runtime crate.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LetBinding {
    pub name: String,
    pub expression: String,
    pub span: Span,
}

/// A beat — `== knot_name` plus contract + body (spec §6).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Beat {
    pub name: String,
    /// `cast:`, `setting:`, `with topic:`, … contract properties.
    pub contract: IndexMap<String, PropertyValue>,
    pub body: Vec<BodyItem>,
    pub span: Span,
}

/// One unit inside a beat body. Mixed freely; ordering matters
/// because the playhead reads top-to-bottom (spec §4).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum BodyItem {
    /// Fountain-style scene heading — `INT. LIGHTHOUSE - DAWN`.
    /// Director / reader-facing; not structural (spec §6).
    SceneHeading(Located<String>),
    /// Flush-left prose paragraph.
    Action(Located<String>),
    /// A speaker block: SPEAKER line + optional `(parenthetical)` +
    /// indented dialogue lines.
    Dialogue(DialogueBlock),
    /// `* …` (once-only) or `+ …` (sticky) choice.
    Choice(Choice),
    /// `-> target`, `<-`, or `-> END`.
    Divert(Divert),
    /// Raw `<kind: args>` directive — interpreted by the runtime.
    Directive(Directive),
    /// Triple-backtick production-metadata fence (spec §15).
    Metadata(Located<String>),
    /// `<if: cond> … <else if: cond> … <else> …` syntactic form (spec §14.2).
    Conditional(Conditional),
    /// Any other block-opening `<kind: args>` directive that carries
    /// an indented body (e.g. `<broadcast: …>`). The body runs after
    /// the directive's side effects.
    DirectiveBlock(DirectiveBlock),
}

/// One `<if:>` chain — first true arm wins (spec §14.2).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Conditional {
    pub arms: Vec<ConditionalArm>,
    pub span: Span,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ConditionalArm {
    /// `None` for the trailing `<else>` arm.
    pub condition: Option<String>,
    pub body: Vec<BodyItem>,
    pub span: Span,
}

/// A directive that opens an indented body — `<broadcast: …>`,
/// `<for: …>`, … (spec §14). The body lowers after the directive's
/// dispatch returns.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DirectiveBlock {
    pub directive: Directive,
    pub body: Vec<BodyItem>,
    pub span: Span,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DialogueBlock {
    pub speaker: String,
    /// Inline `(parenthetical)` on the line directly under the
    /// speaker. Additional inline parens inside the dialogue body
    /// appear in `lines` as `DialogueLine::Parenthetical`.
    pub parenthetical: Option<String>,
    pub lines: Vec<DialogueLine>,
    pub span: Span,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum DialogueLine {
    Text(Located<String>),
    Parenthetical(Located<String>),
    Directive(Directive),
    Divert(Divert),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Choice {
    pub sticky: bool,
    /// Visible choice text — everything before the optional
    /// `[...]` suppression.
    pub text: String,
    /// Ink-style `[hidden]` tail. Played as narration after the
    /// choice is taken; not shown in the choice prompt (spec §5).
    pub suppressed: Option<String>,
    /// Indented continuation — usually a single `Divert`.
    pub body: Vec<BodyItem>,
    pub span: Span,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Divert {
    /// `-> name` or `-> Lighthouse/ringing` or `-> name with k: v, …`.
    To {
        target: DivertTarget,
        params: IndexMap<String, String>,
        span: Span,
    },
    /// `-> (name) ->` tunnel call (spec §7).
    Tunnel { target: DivertTarget, span: Span },
    /// `<-` tunnel return.
    Return { span: Span },
    /// `-> END` — terminate the playhead.
    End { span: Span },
}

/// A divert reference. The file qualifier supports both
/// `folder/beat` (folder hint, spec §7) and `cast/Wren#knot` (file
/// + explicit knot, spec §7 "#-form").
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DivertTarget {
    /// Optional path qualifier — the slash-separated prefix before
    /// the final segment, or the file part before `#`.
    pub qualifier: Option<String>,
    /// The bare target name — beat name, file name, or knot name.
    pub name: String,
    /// Set when the source used the `#knot` form.
    pub knot: Option<String>,
}

/// Raw `<kind: args>` directive. Phase-2 keeps the body opaque; the
/// runtime's Luau bridge tokenises it on dispatch.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Directive {
    pub raw: String,
    pub span: Span,
}

/// Generic span-carrying wrapper.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Located<T> {
    pub value: T,
    pub span: Span,
}

/// A raw indented line, used inside declaration bodies until the
/// next-phase sub-grammars consume them.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RawLine {
    pub indent: u32,
    pub text: String,
    pub span: Span,
}
