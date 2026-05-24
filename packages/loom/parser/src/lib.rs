//! Loom v3 parser — lexer, AST, and diagnostics.
//!
//! Loom is a non-linear narrative scripting language whose surface
//! looks like a Hollywood screenplay (ALL CAPS speaker cues, indented
//! dialogue, parenthetical directions). Underneath, the same file is a
//! program: reactive `let`, character state machines (Simulacra), stat
//! systems (Meridian), live-theatre participants, improv coroutines,
//! and a tiered scheduler. The full design lives in
//! [`docs/dev/loom-v3.html`](../../../../docs/dev/loom-v3.html).
//!
//! ## Module roadmap
//!
//! | Module          | Spec § | Status                                  |
//! |-----------------|--------|-----------------------------------------|
//! | [`source`]      | —      | positions + spans                       |
//! | [`lexer`]       | §2, §5 | line classifier (Phase 2)               |
//! | [`ast`]         | §6     | full AST shape (declaration bodies raw) |
//! | [`parser`]      | §6, §7 | scanned-lines → `LoomFile` (Phase 2)    |
//! | [`brackets`]    | §5     | scope reference (no behaviour yet)      |
//! | [`directives`]  | §14    | placeholder — sub-grammar in Phase 3    |
//! | [`diagnostics`] | §17    | stable codes + severities               |
//! | [`keywords`]    | §5–§14 | shared with `loom-syntax`               |
//!
//! ## Public entry point
//!
//! [`parse`] consumes a single `.loom` source string and returns the
//! file's AST plus a diagnostic stream. Multi-file project loading
//! is the runtime's job (see `loom-runtime::project`).

pub mod ast;
pub mod brackets;
pub mod comments;
pub mod diagnostics;
pub mod directives;
pub mod keywords;
pub mod lexer;
pub mod parser;
pub mod source;

pub use ast::LoomFile;
pub use diagnostics::{Code, Diagnostic, Severity};
pub use parser::parse;
pub use source::{Position, Span};
