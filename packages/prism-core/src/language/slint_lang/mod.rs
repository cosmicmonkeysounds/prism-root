//! `language/slint_lang` — the Slint `.slint` `LanguageContribution`.
//!
//! Provides keyword/element/property completions, hover info, and
//! basic brace-balance diagnostics for `.slint` files.  Full compiler
//! diagnostics require `slint-interpreter` which lives in
//! `prism-builder` behind the `interpreter` feature — this module
//! stays lightweight so prism-core's dep graph remains Slint-free.

pub mod contribution;
pub mod provider;

pub use contribution::{create_slint_contribution, SLINT_EXTENSIONS, SLINT_ID};
pub use provider::SlintSyntaxProvider;
