//! `language` — scanners, parsers, type-checkers for Prism's
//! embedded expression and scripting languages.
//!
//! Port of `packages/prism-core/src/language/*` from the legacy
//! TS tree. Ordered leaf-first: `syntax` (AST types, scanner,
//! token stream, case utils) lands first, then `expression`
//! (tokens, parser, evaluator, field resolver), then the
//! expression-aware pieces of `syntax` (engine, providers).

pub mod expression;
pub mod syntax;
