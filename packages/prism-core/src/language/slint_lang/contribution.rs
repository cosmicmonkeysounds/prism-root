//! Slint [`LanguageContribution`] — the unified registration used by
//! `LanguageRegistry::register(create_slint_contribution())`.

use crate::language::registry::{LanguageContribution, LanguageSurface, SurfaceMode};
use crate::language::syntax::{RootNode, SyntaxProvider};

use super::provider::SlintSyntaxProvider;

pub const SLINT_ID: &str = "prism:slint";
pub const SLINT_EXTENSIONS: &[&str] = &[".slint"];
const SLINT_MIME_TYPE: &str = "text/x-slint";

pub fn create_slint_contribution<R, E>() -> LanguageContribution<R, E> {
    let surface = LanguageSurface::new(
        SurfaceMode::Code,
        vec![SurfaceMode::Code, SurfaceMode::Preview],
    );

    LanguageContribution::new(SLINT_ID, SLINT_EXTENSIONS.iter().copied(), "Slint", surface)
        .with_mime_type(SLINT_MIME_TYPE)
        .with_parse(slint_parse)
        .with_syntax_provider(slint_syntax_provider_factory)
}

fn slint_parse(_source: &str) -> RootNode {
    RootNode {
        kind: crate::language::syntax::RootKind::Root,
        position: None,
        children: Vec::new(),
    }
}

fn slint_syntax_provider_factory() -> Box<dyn SyntaxProvider> {
    Box::new(SlintSyntaxProvider::new())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::language::document::{create_text_file, TextFileParams};
    use crate::language::registry::{LanguageRegistry, ResolveOptions};

    #[test]
    fn contribution_has_identity_fields() {
        let c = create_slint_contribution::<(), ()>();
        assert_eq!(c.id, SLINT_ID);
        assert_eq!(c.extensions, vec![".slint"]);
        assert_eq!(c.display_name, "Slint");
        assert_eq!(c.mime_type.as_deref(), Some(SLINT_MIME_TYPE));
    }

    #[test]
    fn contribution_wires_parse_and_syntax_provider() {
        let c = create_slint_contribution::<(), ()>();
        assert!(c.parse.is_some());
        assert!(c.syntax_provider.is_some());
    }

    #[test]
    fn syntax_provider_name_matches_contribution_id() {
        let c = create_slint_contribution::<(), ()>();
        let factory = c.syntax_provider.as_ref().unwrap();
        let provider = factory();
        assert_eq!(provider.name(), SLINT_ID);
    }

    #[test]
    fn surface_defaults_to_code_with_preview() {
        let c = create_slint_contribution::<(), ()>();
        assert_eq!(c.surface.default_mode, SurfaceMode::Code);
        assert!(c.surface.available_modes.contains(&SurfaceMode::Preview));
    }

    #[test]
    fn registry_resolves_slint_extension() {
        let mut registry = LanguageRegistry::<(), ()>::new();
        registry.register(create_slint_contribution());

        let hit = registry
            .resolve(ResolveOptions::by_filename("ui/app.slint"))
            .expect("resolve .slint");
        assert_eq!(hit.id, SLINT_ID);
    }

    #[test]
    fn registry_resolves_prism_file_by_id() {
        let mut registry = LanguageRegistry::<(), ()>::new();
        registry.register(create_slint_contribution());

        let file = create_text_file(TextFileParams {
            path: "readme.md".into(),
            text: "ignored".into(),
            language_id: Some(SLINT_ID.into()),
            ..Default::default()
        });

        let hit = registry.resolve_file(&file).expect("id override");
        assert_eq!(hit.id, SLINT_ID);
    }

    #[test]
    fn registry_resolves_prism_file_by_path() {
        let mut registry = LanguageRegistry::<(), ()>::new();
        registry.register(create_slint_contribution());

        let file = create_text_file(TextFileParams {
            path: "ui/panel.slint".into(),
            text: "export component Panel { }".into(),
            ..Default::default()
        });

        let hit = registry.resolve_file(&file).expect("extension match");
        assert_eq!(hit.id, SLINT_ID);
    }
}
