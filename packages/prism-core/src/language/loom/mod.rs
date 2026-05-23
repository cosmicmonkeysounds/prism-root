//! `language::loom` — the Loom storytelling language contribution.
//!
//! Loom is the `.loom` source format defined by
//! `docs/dev/loom-design.md` and `docs/dev/loom-grammar.md`. This
//! module is the **canonical home** for the language inside Prism:
//! every keyword, sigil, doc-tag, and stance level lives in
//! [`keywords`] as a `const` slice, and every consumer that needs
//! that vocabulary — the future parser, the LSP semantic-tokens
//! provider, the editor extension grammars — reads from there.
//!
//! ## Status
//!
//! Phase 0: keyword/sigil registry + TextMate emitter.
//!
//! The full Loom parser (a `Scanner`-based recursive descent, mirroring
//! `language::luau::parser`) and the matching `LoomSyntaxProvider` are
//! deferred — when they land, the [`keywords`] table becomes the
//! reserved-word check for the lexer with zero drift between editor
//! highlights and parse errors. See `docs/dev/loom-grammar.md` §3.1.
//!
//! ## Module map
//!
//! - [`keywords`] — every Loom reservation as a categorised `const`
//!   slice. **Source of truth** for the language's vocabulary.
//! - [`tmgrammar`] — emits a TextMate JSON grammar
//!   (`loom.tmLanguage.json`) from the [`keywords`] tables. Consumed by
//!   `prism codegen loom-tmgrammar` and shipped in the VSCode / GitHub
//!   Linguist editor extensions.

pub mod contribution;
pub mod diagnostics;
pub mod keywords;
pub mod lexer;
pub mod node_kinds;
pub mod parser;
pub mod provider;
pub mod tmgrammar;

pub use contribution::create_loom_contribution;

/// Namespaced contribution id reserved for the future
/// `LanguageContribution` registration. Used today only by the
/// TextMate emitter to stamp the grammar's metadata; wired up to
/// `LanguageRegistry::register` once the parser lands.
pub const LOOM_ID: &str = "prism:loom";

/// File extensions the Loom contribution claims.
pub const LOOM_EXTENSIONS: &[&str] = &[".loom"];

/// IANA-style mime type for `.loom` source.
pub const LOOM_MIME_TYPE: &str = "text/x-loom";
