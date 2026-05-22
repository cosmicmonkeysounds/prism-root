//! This crate provides the [Loom] grammar for the [tree-sitter] parsing library.
//!
//! Typically, you will use the [LANGUAGE] constant to add this grammar to a
//! tree-sitter [Parser], and then use the parser to parse some code:
//!
//! ```ignore
//! let code = r#"
//! # harbor_greeting "The Harbor"
//!   .actors wren, hale
//!
//! cast WREN
//!   .label "Wren the Fisher"
//!
//! -- start
//!
//! WREN { worried }
//!   The bell went silent three days ago.
//! "#;
//! let mut parser = tree_sitter::Parser::new();
//! let language = tree_sitter_loom::LANGUAGE;
//! parser.set_language(&language.into()).expect("error loading Loom parser");
//! let tree = parser.parse(code, None).unwrap();
//! ```
//!
//! [Loom]: https://github.com/prism-framework/prism/tree/main/docs/dev/loom-design.md
//! [Parser]: https://docs.rs/tree-sitter/*/tree_sitter/struct.Parser.html
//! [tree-sitter]: https://tree-sitter.github.io/

use tree_sitter_language::LanguageFn;

extern "C" {
    fn tree_sitter_loom() -> *const ();
}

/// The tree-sitter [`LanguageFn`] for this grammar.
pub const LANGUAGE: LanguageFn = unsafe { LanguageFn::from_raw(tree_sitter_loom) };

/// The content of the [`node-types.json`] file for this grammar.
pub const NODE_TYPES: &str = include_str!("../../src/node-types.json");

/// The syntax highlighting queries for this grammar.
pub const HIGHLIGHTS_QUERY: &str = include_str!("../../queries/highlights.scm");

/// The injections queries for this grammar.
pub const INJECTIONS_QUERY: &str = include_str!("../../queries/injections.scm");

/// The local-variable queries for this grammar.
pub const LOCALS_QUERY: &str = include_str!("../../queries/locals.scm");
