//! Luau [`LanguageContribution`] — the unified registration used by
//! `LanguageRegistry::register(create_luau_contribution())`.
//!
//! Port of `language/luau/contribution.ts` from the pre-Rust
//! reference commit. Replaces the legacy
//! `createLuauLanguageDefinition()` + ad-hoc
//! `DocumentSurfaceRegistry` entry pair that ADR-002 Phase 4
//! retired.
//!
//! The pre-Rust version parsed via a full-moon WASM module and had
//! an async `initLuauSyntax()` gate. The Rust port:
//!
//! - keeps `parse` synchronous (the registry requires it to be so),
//! - returns an empty [`RootNode`] until a full-moon Rust parser is
//!   wired through the contribution, and
//! - wires the same `code` + `preview` surface modes the TS
//!   contribution used. `code` is the default so CodeMirror-style
//!   editing is the out-of-the-box experience; `preview` is the
//!   mode the Luau debugger panel opens when stepping through a
//!   trace.

use crate::language::registry::{LanguageContribution, LanguageSurface, SurfaceMode};
use crate::language::syntax::{RootNode, SyntaxProvider};

use super::provider::LuauSyntaxProvider;

/// Namespaced contribution id registered with the unified
/// `LanguageRegistry`.
pub const LUAU_ID: &str = "prism:luau";

/// File extensions the Luau contribution claims. The `.lua` alias is
/// kept so existing `.lua` files resolve through the same surface.
pub const LUAU_EXTENSIONS: &[&str] = &[".luau", ".lua"];

const LUAU_MIME_TYPE: &str = "text/x-luau";

/// Create the unified [`LanguageContribution`] for Luau.
///
/// Registers both the parser slot and the surface on a single
/// record. The surface defaults to `code` mode and additionally
/// exposes `preview` so a Luau debugger panel can open the same
/// buffer in a trace-aware view.
///
/// The `R` / `E` type parameters let host crates specialise the
/// contribution — Studio will bind `R` to its Clay renderer handle,
/// while tests and headless callers leave them as `()`.
pub fn create_luau_contribution<R, E>() -> LanguageContribution<R, E> {
    let surface = LanguageSurface::new(
        SurfaceMode::Code,
        vec![SurfaceMode::Code, SurfaceMode::Preview],
    );

    LanguageContribution::new(LUAU_ID, LUAU_EXTENSIONS.iter().copied(), "Luau", surface)
        .with_mime_type(LUAU_MIME_TYPE)
        .with_parse(luau_parse_stub)
        .with_syntax_provider(luau_syntax_provider_factory)
}

/// Placeholder `parse` hook. Returns an empty root node while the
/// full-moon Rust parser is being ported — matches the TS
/// contribution's behaviour when `isLuauParserReady()` was false.
fn luau_parse_stub(_source: &str) -> RootNode {
    RootNode::default()
}

/// Factory wired into `LanguageContribution::syntax_provider`.
fn luau_syntax_provider_factory() -> Box<dyn SyntaxProvider> {
    Box::new(LuauSyntaxProvider::new())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::language::document::{create_text_file, TextFileParams};
    use crate::language::registry::{LanguageRegistry, ResolveOptions};

    #[test]
    fn contribution_has_identity_fields() {
        let c = create_luau_contribution::<(), ()>();
        assert_eq!(c.id, LUAU_ID);
        assert_eq!(c.extensions, vec![".luau", ".lua"]);
        assert_eq!(c.display_name, "Luau");
        assert_eq!(c.mime_type.as_deref(), Some(LUAU_MIME_TYPE));
    }

    #[test]
    fn contribution_wires_parse_and_syntax_provider() {
        let c = create_luau_contribution::<(), ()>();
        assert!(c.parse.is_some(), "parse hook should be wired");
        assert!(
            c.syntax_provider.is_some(),
            "syntax provider factory should be wired"
        );
        assert!(
            c.serialize.is_none(),
            "serialize round-trips through full-moon; not wired yet"
        );
    }

    #[test]
    fn parse_stub_returns_empty_root() {
        let c = create_luau_contribution::<(), ()>();
        let parse = c.parse.as_ref().expect("parse hook");
        let root = parse("return 1 + 2");
        assert!(root.children.is_empty());
    }

    #[test]
    fn syntax_provider_name_matches_contribution_id() {
        let c = create_luau_contribution::<(), ()>();
        let factory = c.syntax_provider.as_ref().expect("syntax provider");
        let provider = factory();
        assert_eq!(provider.name(), LUAU_ID);
    }

    #[test]
    fn surface_defaults_to_code_mode_with_preview_available() {
        let c = create_luau_contribution::<(), ()>();
        assert_eq!(c.surface.default_mode, SurfaceMode::Code);
        assert!(c.surface.available_modes.contains(&SurfaceMode::Code));
        assert!(c.surface.available_modes.contains(&SurfaceMode::Preview));
    }

    #[test]
    fn registry_resolves_both_extensions() {
        let mut registry = LanguageRegistry::<(), ()>::new();
        registry.register(create_luau_contribution());

        let luau = registry
            .resolve(ResolveOptions::by_filename("scripts/action.luau"))
            .expect("resolve .luau");
        assert_eq!(luau.id, LUAU_ID);

        let lua = registry
            .resolve(ResolveOptions::by_filename("scripts/action.lua"))
            .expect("resolve .lua");
        assert_eq!(lua.id, LUAU_ID);
    }

    #[test]
    fn registry_resolves_prism_file_by_id_override() {
        let mut registry = LanguageRegistry::<(), ()>::new();
        registry.register(create_luau_contribution());

        let file = create_text_file(TextFileParams {
            path: "readme.md".into(),
            text: "ignored".into(),
            language_id: Some(LUAU_ID.into()),
            ..Default::default()
        });

        let hit = registry
            .resolve_file(&file)
            .expect("id override should win over filename");
        assert_eq!(hit.id, LUAU_ID);
    }

    #[test]
    fn registry_resolves_prism_file_by_path() {
        let mut registry = LanguageRegistry::<(), ()>::new();
        registry.register(create_luau_contribution());

        let file = create_text_file(TextFileParams {
            path: "scripts/action.luau".into(),
            text: "return 1".into(),
            ..Default::default()
        });

        let hit = registry
            .resolve_file(&file)
            .expect("filename should fall through to extension resolver");
        assert_eq!(hit.id, LUAU_ID);
    }
}
