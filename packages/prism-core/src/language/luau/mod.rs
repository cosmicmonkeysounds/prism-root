//! `language/luau` — the Luau `LanguageContribution`.
//!
//! Port of `packages/prism-core/src/language/luau/*.ts` from the
//! pre-Rust reference commit (8426588, ADR-002 §A4 / Phase 4).
//! Collapses the old `createLuauLanguageDefinition()` +
//! `DocumentSurfaceRegistry` pair into a single `LanguageContribution`
//! record that the unified registry consumes.
//!
//! The TS tree shipped a full-moon-backed WASM parser and a
//! `wasm-loader` that had to be awaited before `parse` could run.
//! The Rust port has two execution targets:
//!
//! - **Daemon (native)** — `prism-daemon::modules::luau_module` owns
//!   the mlua-backed runtime and exposes `luau.exec` over the command
//!   registry. No parser is needed at runtime; mlua compiles and runs
//!   source directly.
//! - **Studio / web** — a future full-moon Rust parser will feed
//!   [`RootNode`] trees through the `parse` hook. Until that lands,
//!   `parse` returns an empty root, matching the TS behaviour when
//!   `isLuauParserReady()` was false.
//!
//! Everything below is framework-free: the contribution returned by
//! [`create_luau_contribution`] uses the default `R = ()` and `E =
//! ()` slots on [`LanguageContribution`] so host crates (Studio,
//! tests) can specialise.

pub mod contribution;
pub mod parser;
pub mod provider;
pub mod visual;

pub use contribution::{create_luau_contribution, LUAU_EXTENSIONS, LUAU_ID};
pub use parser::{parse_errors, parse_luau};
pub use provider::LuauSyntaxProvider;
pub use visual::LuauVisualLanguage;
