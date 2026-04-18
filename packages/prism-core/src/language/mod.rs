//! `language` — scanners, parsers, type-checkers for Prism's
//! embedded expression and scripting languages.
//!
//! Port of `packages/prism-core/src/language/*` from the legacy
//! TS tree. Closed on its Phase-2a scope (Slint migration plan §6).
//! Leaf-first order: `syntax` (AST types, scanner, token stream,
//! case utils, LSP-like engine) → `expression` (tokens, parser,
//! evaluator, field resolver) → `registry` (the ADR-002 §A2 unified
//! `LanguageContribution` record) → `document` (ADR-002 §A1
//! `PrismFile`) → `forms` (field / document / form schema + state,
//! wiki links, Prism's in-house markdown dialect) → `markdown` and
//! `luau` contributions → `codegen` (ADR-002 §A3 unified pipeline +
//! symbol DSL + TS/C#/EmmyDoc/GDScript emitters).
//!
//! The `luau::parse` hook is still a stub — the full-moon Rust
//! parser lands in Phase 4. The mlua-backed execution runtime
//! already lives in `prism-daemon::modules::luau_module`, so no
//! parser is needed at runtime.

pub mod codegen;
pub mod document;
pub mod expression;
pub mod forms;
pub mod luau;
pub mod markdown;
pub mod registry;
pub mod syntax;
pub mod visual;
