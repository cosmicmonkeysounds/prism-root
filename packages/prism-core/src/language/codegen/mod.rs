//! `codegen` — the ADR-002 §A3 unified emitter pipeline.
//!
//! Port of `language/codegen/*`. Replaces the two parallel emitter
//! hierarchies the TS tree used to carry (`syntax/codegen/` for
//! symbols and `facet/emitters.ts` for schemas) with one pipeline
//! that dispatches heterogeneous emitters by an `input_kind`
//! discriminator. See [`pipeline::CodegenPipeline`] and
//! [`types::CodegenInputs`] for the dispatch contract.
//!
//! Modules:
//!
//! - [`types`] — `EmittedFile`, `CodegenMeta`, `CodegenResult`,
//!   `Emitter` trait, `CodegenInputs` bundle, well-known kind
//!   constants.
//! - [`source_builder`] — line-oriented buffer with an indent stack
//!   (shared by every emitter).
//! - [`pipeline`] — `CodegenPipeline` itself.
//! - [`symbol_def`] — the declarative DSL (`SymbolDef`, `SymbolKind`,
//!   `SymbolParam`, `EnumValue`) plus the `constant_namespace` /
//!   `fn_symbol` convenience builders.
//! - [`symbol_emitter`] — four concrete emitters targeting TS, C#,
//!   Luau type stubs, and GDScript.
//! - [`text_emitter`] — `TextEmitter` trait for AST → source
//!   round-tripping.

pub mod pipeline;
pub mod source_builder;
pub mod symbol_def;
pub mod symbol_emitter;
pub mod text_emitter;
pub mod types;

pub use pipeline::CodegenPipeline;
pub use source_builder::SourceBuilder;
pub use symbol_def::{
    constant_namespace, fn_symbol, EnumValue, SymbolDef, SymbolKind, SymbolParam,
};
pub use symbol_emitter::{
    cs_name_transform, default_gdscript_name_transform, ts_name_transform, NameTransform,
    SymbolCSharpEmitter, SymbolEmmyDocEmitter, SymbolGDScriptEmitter, SymbolTypeScriptEmitter,
};
pub use text_emitter::TextEmitter;
pub use types::{
    CodegenInputs, CodegenMeta, CodegenResult, EmittedFile, Emitter, EMITTER_KIND_AST,
    EMITTER_KIND_DATA, EMITTER_KIND_FACET, EMITTER_KIND_SCHEMA, EMITTER_KIND_SYMBOLS,
};
