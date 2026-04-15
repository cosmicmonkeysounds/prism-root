//! `language` — scanners, parsers, type-checkers for Prism's
//! embedded expression and scripting languages.
//!
//! Port of `packages/prism-core/src/language/*` from the legacy
//! TS tree. Ordered leaf-first: `syntax` (AST types, scanner,
//! token stream, case utils) lands first, then `expression`
//! (tokens, parser, evaluator, field resolver), then `registry`
//! (the ADR-002 §A2 unified `LanguageContribution` record), then
//! the expression-aware pieces of `syntax` (engine, providers).
//!
//! Later phases layer on the unified file abstraction (`document`,
//! ADR-002 §A1) and per-language contributions (`luau`, …) that
//! plug into the registry.

pub mod document;
pub mod expression;
pub mod luau;
pub mod registry;
pub mod syntax;
