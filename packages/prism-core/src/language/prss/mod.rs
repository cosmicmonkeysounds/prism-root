//! `language/prss` — the PRSS stylesheet language.
//!
//! PRSS is the "CSS" of the Prism stack — TOML-shaped files
//! carrying theme tokens and named classes that any `.prui`
//! element opts into via `class="…"`. See `docs/dev/prss-reference.md`
//! for the full spec. The runtime integration lives in
//! `prism-ui-runtime::interpret::LowerScope::with_stylesheet`.
//!
//! Module layout:
//!
//! - [`stylesheet`] — the [`StyleSheet`] IR + [`parse`] entry point.
//!
//! The IR is intentionally narrow: a token-override table plus an
//! `IndexMap<String, ClassDef>`. Class property values are stored
//! as raw strings (not pre-parsed colors / numbers) so the runtime's
//! existing `apply_style_override` consumes them through the same
//! parser inline `style:` attributes use. One vocabulary, one source
//! of truth.

pub mod stylesheet;

pub use stylesheet::{
    is_descendant_selector, parse, selector_segments, ClassDef, ParseError, ResolvedClass,
    StyleSheet, TokenOverrides, PRSS_EXTENSIONS,
};
