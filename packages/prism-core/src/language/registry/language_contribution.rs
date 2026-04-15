//! `LanguageContribution` — unified per-format registration.
//!
//! Port of `language/registry/language-contribution.ts` (ADR-002 §A2).
//! Collapses the legacy `LanguageRegistry` (parsing) and
//! `DocumentSurfaceRegistry` (rendering) into one record that owns
//! *everything* about a language: parse/serialize, syntax provider,
//! editor surface, and optional codegen emitters.
//!
//! Two generic slots keep `prism-core` framework-free:
//!
//! - `R` — the surface renderer type (a Slint-component handle in the
//!   Studio shell, `()` in headless tests).
//! - `E` — the editor extension type (CodeMirror `Extension` on the
//!   web hybrid, `()` elsewhere).
//!
//! Optional hooks (`parse`, `serialize`, `syntax_provider`,
//! `editor_extensions`, `codegen`) are stored as `Option<Arc<...>>` so
//! the registry stays clonable and hot-reload-friendly.

use std::collections::HashMap;
use std::sync::Arc;

use crate::language::syntax::{RootNode, SyntaxProvider};

use super::surface_types::{InlineTokenDef, SurfaceMode};

// ── Surface ────────────────────────────────────────────────────────

/// The editor surface contributed by a language.
///
/// `renderers` is a partial map — not every language supports every
/// mode (markdown has no spreadsheet, CSV has no preview). The
/// renderer type is opaque at the core level; Studio specialises it
/// to a Slint-component handle, a headless test can use `()`.
#[derive(Clone)]
pub struct LanguageSurface<R = ()> {
    /// Default editing mode when opening a file of this language.
    pub default_mode: SurfaceMode,
    /// All modes the user can switch between. Must include
    /// `default_mode`.
    pub available_modes: Vec<SurfaceMode>,
    /// Inline tokens rendered identically across surface modes.
    pub inline_tokens: Vec<InlineTokenDef>,
    /// Optional renderers keyed by mode.
    pub renderers: HashMap<SurfaceMode, R>,
}

impl<R> LanguageSurface<R> {
    pub fn new(default_mode: SurfaceMode, available_modes: Vec<SurfaceMode>) -> Self {
        Self {
            default_mode,
            available_modes,
            inline_tokens: Vec::new(),
            renderers: HashMap::new(),
        }
    }

    pub fn with_inline_tokens(mut self, tokens: Vec<InlineTokenDef>) -> Self {
        self.inline_tokens = tokens;
        self
    }

    pub fn with_renderer(mut self, mode: SurfaceMode, renderer: R) -> Self {
        self.renderers.insert(mode, renderer);
        self
    }
}

impl<R> std::fmt::Debug for LanguageSurface<R> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LanguageSurface")
            .field("default_mode", &self.default_mode)
            .field("available_modes", &self.available_modes)
            .field("inline_tokens", &self.inline_tokens)
            .field("renderer_modes", &self.renderers.keys().collect::<Vec<_>>())
            .finish()
    }
}

// ── Codegen ────────────────────────────────────────────────────────

/// Optional codegen slot. Phase 4 wires this into `CodegenPipeline` so
/// `LanguageDefinition::serialize` stops being a dead hook.
///
/// Intentionally an opaque handle until `language/codegen` ports —
/// the emitter pipeline lives there, and the registry only needs to
/// stash and return it.
#[derive(Clone, Default)]
pub struct LanguageCodegen {
    /// Placeholder for the ported `Emitter` trait list. For now the
    /// registry round-trips this slot unchanged; phase-4 replaces the
    /// unit field with a real `Vec<Arc<dyn Emitter>>`.
    pub _todo: (),
}

// ── Hook signatures ────────────────────────────────────────────────

pub type ParseFn = Arc<dyn Fn(&str) -> RootNode + Send + Sync>;
pub type SerializeFn = Arc<dyn Fn(&RootNode) -> String + Send + Sync>;
pub type SyntaxProviderFn = Arc<dyn Fn() -> Box<dyn SyntaxProvider> + Send + Sync>;
pub type EditorExtensionsFn<E> = Arc<dyn Fn() -> Vec<E> + Send + Sync>;

// ── LanguageContribution ───────────────────────────────────────────

/// The unified record a language plugin registers with the core.
///
/// Required fields: `id`, `extensions`, `display_name`, `surface`.
/// Everything else is optional so binary formats (images, CAD files)
/// can participate by contributing only a surface, and pure-parse
/// languages (headless CLI use) can contribute without a surface.
#[derive(Clone)]
pub struct LanguageContribution<R = (), E = ()> {
    /// Namespaced contribution id: `"prism:luau"`, `"prism:markdown"`.
    pub id: String,
    /// File extensions this contribution handles: `[".md", ".mdx"]`.
    pub extensions: Vec<String>,
    /// Human-readable format name shown in the toolbar.
    pub display_name: String,
    /// MIME type for clipboard / drag-and-drop interop.
    pub mime_type: Option<String>,

    // ── Syntax (optional — binary formats may omit) ───────────────
    /// Parse source text into an AST.
    pub parse: Option<ParseFn>,
    /// Round-trip an AST back into source text.
    pub serialize: Option<SerializeFn>,
    /// LSP-like provider for diagnostics, completion, hover.
    pub syntax_provider: Option<SyntaxProviderFn>,
    /// Lazy editor extensions (CodeMirror `Extension` on the web
    /// hybrid; anything opaque elsewhere).
    pub editor_extensions: Option<EditorExtensionsFn<E>>,

    // ── Surface (editor UI) ───────────────────────────────────────
    /// The editor surface this language exposes.
    pub surface: LanguageSurface<R>,

    // ── Codegen (optional) ────────────────────────────────────────
    /// Optional codegen pipeline wiring.
    pub codegen: Option<LanguageCodegen>,
}

impl<R, E> LanguageContribution<R, E> {
    /// Minimal constructor — you still have to fill in `surface`
    /// because it's required by the contract.
    pub fn new(
        id: impl Into<String>,
        extensions: impl IntoIterator<Item = impl Into<String>>,
        display_name: impl Into<String>,
        surface: LanguageSurface<R>,
    ) -> Self {
        Self {
            id: id.into(),
            extensions: extensions.into_iter().map(Into::into).collect(),
            display_name: display_name.into(),
            mime_type: None,
            parse: None,
            serialize: None,
            syntax_provider: None,
            editor_extensions: None,
            surface,
            codegen: None,
        }
    }

    pub fn with_mime_type(mut self, mime: impl Into<String>) -> Self {
        self.mime_type = Some(mime.into());
        self
    }

    pub fn with_parse<F>(mut self, f: F) -> Self
    where
        F: Fn(&str) -> RootNode + Send + Sync + 'static,
    {
        self.parse = Some(Arc::new(f));
        self
    }

    pub fn with_serialize<F>(mut self, f: F) -> Self
    where
        F: Fn(&RootNode) -> String + Send + Sync + 'static,
    {
        self.serialize = Some(Arc::new(f));
        self
    }

    pub fn with_syntax_provider<F>(mut self, f: F) -> Self
    where
        F: Fn() -> Box<dyn SyntaxProvider> + Send + Sync + 'static,
    {
        self.syntax_provider = Some(Arc::new(f));
        self
    }

    pub fn with_editor_extensions<F>(mut self, f: F) -> Self
    where
        F: Fn() -> Vec<E> + Send + Sync + 'static,
    {
        self.editor_extensions = Some(Arc::new(f));
        self
    }

    pub fn with_codegen(mut self, codegen: LanguageCodegen) -> Self {
        self.codegen = Some(codegen);
        self
    }
}

impl<R, E> std::fmt::Debug for LanguageContribution<R, E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LanguageContribution")
            .field("id", &self.id)
            .field("extensions", &self.extensions)
            .field("display_name", &self.display_name)
            .field("mime_type", &self.mime_type)
            .field("has_parse", &self.parse.is_some())
            .field("has_serialize", &self.serialize.is_some())
            .field("has_syntax_provider", &self.syntax_provider.is_some())
            .field("has_editor_extensions", &self.editor_extensions.is_some())
            .field("surface", &self.surface)
            .field("has_codegen", &self.codegen.is_some())
            .finish()
    }
}
