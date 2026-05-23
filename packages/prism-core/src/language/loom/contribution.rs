//! Loom [`LanguageContribution`] — the unified registration used by
//! `LanguageRegistry::register(create_loom_contribution())`.
//!
//! Mirrors the shape of [`super::super::luau::contribution`]: the
//! parser slot is wired to [`super::parser::parse`] and produces a
//! [`RootNode`] adapted from the typed Loom AST; the syntax provider
//! is the [`super::provider::LoomSyntaxProvider`].

use crate::language::registry::{LanguageContribution, LanguageSurface, SurfaceMode};
use crate::language::syntax::{RootNode, SyntaxProvider};

use super::parser::parse as parse_loom_source;
use super::provider::LoomSyntaxProvider;
use super::{LOOM_EXTENSIONS, LOOM_ID, LOOM_MIME_TYPE};

/// Create the unified [`LanguageContribution`] for Loom.
///
/// The surface defaults to `code` (the standard editing experience
/// for a textual format); the future visual / timeline editors that
/// `loom-design.md` describes will register additional modes through
/// the same record once they land.
pub fn create_loom_contribution<R, E>() -> LanguageContribution<R, E> {
    let surface = LanguageSurface::new(
        SurfaceMode::Code,
        vec![SurfaceMode::Code, SurfaceMode::Preview],
    );

    LanguageContribution::new(LOOM_ID, LOOM_EXTENSIONS.iter().copied(), "Loom", surface)
        .with_mime_type(LOOM_MIME_TYPE)
        .with_parse(loom_parse)
        .with_syntax_provider(loom_syntax_provider_factory)
}

fn loom_parse(source: &str) -> RootNode {
    parse_loom_source(source).root
}

fn loom_syntax_provider_factory() -> Box<dyn SyntaxProvider> {
    Box::new(LoomSyntaxProvider::new())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::language::document::{create_text_file, TextFileParams};
    use crate::language::registry::{LanguageRegistry, ResolveOptions};

    #[test]
    fn contribution_has_identity_fields() {
        let c = create_loom_contribution::<(), ()>();
        assert_eq!(c.id, LOOM_ID);
        assert_eq!(c.extensions, vec![".loom"]);
        assert_eq!(c.display_name, "Loom");
        assert_eq!(c.mime_type.as_deref(), Some(LOOM_MIME_TYPE));
    }

    #[test]
    fn contribution_wires_parse_and_syntax_provider() {
        let c = create_loom_contribution::<(), ()>();
        assert!(c.parse.is_some(), "parse hook should be wired");
        assert!(
            c.syntax_provider.is_some(),
            "syntax provider factory should be wired"
        );
    }

    #[test]
    fn parse_returns_ast_nodes() {
        let c = create_loom_contribution::<(), ()>();
        let parse = c.parse.as_ref().expect("parse hook");
        let root = parse("# tiny \"Hello\"\n");
        assert!(!root.children.is_empty(), "parser should produce AST nodes");
        // First child should be the document.
        assert!(root.children[0].kind == "document");
    }

    #[test]
    fn syntax_provider_name_matches_contribution_id() {
        let c = create_loom_contribution::<(), ()>();
        let factory = c.syntax_provider.as_ref().expect("syntax provider");
        let provider = factory();
        assert_eq!(provider.name(), LOOM_ID);
    }

    #[test]
    fn surface_defaults_to_code_mode() {
        let c = create_loom_contribution::<(), ()>();
        assert_eq!(c.surface.default_mode, SurfaceMode::Code);
        assert!(c.surface.available_modes.contains(&SurfaceMode::Code));
        assert!(c.surface.available_modes.contains(&SurfaceMode::Preview));
    }

    #[test]
    fn registry_resolves_loom_extension() {
        let mut registry = LanguageRegistry::<(), ()>::new();
        registry.register(create_loom_contribution());

        let loom = registry
            .resolve(ResolveOptions::by_filename("stories/intro.loom"))
            .expect("resolve .loom");
        assert_eq!(loom.id, LOOM_ID);
    }

    #[test]
    fn registry_resolves_prism_file_by_id_override() {
        let mut registry = LanguageRegistry::<(), ()>::new();
        registry.register(create_loom_contribution());

        let file = create_text_file(TextFileParams {
            path: "readme.md".into(),
            text: "ignored".into(),
            language_id: Some(LOOM_ID.into()),
            ..Default::default()
        });

        let hit = registry
            .resolve_file(&file)
            .expect("id override should win over filename");
        assert_eq!(hit.id, LOOM_ID);
    }
}
