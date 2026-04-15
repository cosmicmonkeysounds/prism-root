//! `language/registry` — unified `LanguageContribution` registry.
//!
//! Port of `language/registry/*.ts` (ADR-002 §A2). Collapses the
//! legacy split between the parser registry and the document surface
//! registry into a single record keyed by namespaced contribution id
//! and indexed by file extension.

pub mod language_contribution;
pub mod language_registry;
pub mod surface_types;

pub use language_contribution::{
    EditorExtensionsFn, LanguageCodegen, LanguageContribution, LanguageSurface, ParseFn,
    SerializeFn, SyntaxProviderFn,
};
pub use language_registry::{LanguageRegistry, ResolveOptions};
pub use surface_types::{
    inline_token, wikilink_token, InlineTokenBuildError, InlineTokenBuilder, InlineTokenDef,
    InlineTokenExtract, InlineTokenExtractFn, SurfaceMode,
};
