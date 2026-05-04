//! `prism-ui` [`LanguageContribution`] — registered via
//! `LanguageRegistry::register(create_prism_ui_contribution())`.

use crate::language::registry::{LanguageContribution, LanguageSurface, SurfaceMode};
use crate::language::syntax::{RootNode, SyntaxProvider};

use super::grammar::parse_to_root;
use super::provider::PrismUiSyntaxProvider;

pub const PRISM_UI_ID: &str = "prism:prism-ui";
pub const PRISM_UI_EXTENSIONS: &[&str] = &[".prism-ui"];
const PRISM_UI_MIME_TYPE: &str = "text/x-prism-ui";

pub fn create_prism_ui_contribution<R, E>() -> LanguageContribution<R, E> {
    let surface = LanguageSurface::new(
        SurfaceMode::Code,
        vec![SurfaceMode::Code, SurfaceMode::Preview],
    );

    LanguageContribution::new(
        PRISM_UI_ID,
        PRISM_UI_EXTENSIONS.iter().copied(),
        "Prism UI",
        surface,
    )
    .with_mime_type(PRISM_UI_MIME_TYPE)
    .with_parse(prism_ui_parse)
    .with_syntax_provider(prism_ui_syntax_provider_factory)
}

fn prism_ui_parse(source: &str) -> RootNode {
    parse_to_root(source)
}

fn prism_ui_syntax_provider_factory() -> Box<dyn SyntaxProvider> {
    Box::new(PrismUiSyntaxProvider::new())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::language::document::{create_text_file, TextFileParams};
    use crate::language::registry::{LanguageRegistry, ResolveOptions};

    #[test]
    fn contribution_has_identity_fields() {
        let c = create_prism_ui_contribution::<(), ()>();
        assert_eq!(c.id, PRISM_UI_ID);
        assert_eq!(c.extensions, vec![".prism-ui"]);
        assert_eq!(c.display_name, "Prism UI");
        assert_eq!(c.mime_type.as_deref(), Some(PRISM_UI_MIME_TYPE));
    }

    #[test]
    fn contribution_wires_parse_and_syntax_provider() {
        let c = create_prism_ui_contribution::<(), ()>();
        assert!(c.parse.is_some());
        assert!(c.syntax_provider.is_some());
    }

    #[test]
    fn syntax_provider_name_matches_contribution_id() {
        let c = create_prism_ui_contribution::<(), ()>();
        let factory = c.syntax_provider.as_ref().unwrap();
        let provider = factory();
        assert_eq!(provider.name(), PRISM_UI_ID);
    }

    #[test]
    fn surface_defaults_to_code_with_preview() {
        let c = create_prism_ui_contribution::<(), ()>();
        assert_eq!(c.surface.default_mode, SurfaceMode::Code);
        assert!(c.surface.available_modes.contains(&SurfaceMode::Preview));
    }

    #[test]
    fn registry_resolves_prism_ui_extension() {
        let mut registry = LanguageRegistry::<(), ()>::new();
        registry.register(create_prism_ui_contribution());

        let hit = registry
            .resolve(ResolveOptions::by_filename("ui/app.prism-ui"))
            .expect("resolve .prism-ui");
        assert_eq!(hit.id, PRISM_UI_ID);
    }

    #[test]
    fn registry_resolves_prism_file_by_id() {
        let mut registry = LanguageRegistry::<(), ()>::new();
        registry.register(create_prism_ui_contribution());

        let file = create_text_file(TextFileParams {
            path: "readme.md".into(),
            text: "ignored".into(),
            language_id: Some(PRISM_UI_ID.into()),
            ..Default::default()
        });

        let hit = registry.resolve_file(&file).expect("id override");
        assert_eq!(hit.id, PRISM_UI_ID);
    }

    #[test]
    fn parse_round_trips_through_contribution() {
        let c = create_prism_ui_contribution::<(), ()>();
        let parse = c.parse.expect("parse fn registered");
        let root = parse(r#"<button label="Save"/>"#);
        assert_eq!(root.children.len(), 1);
        assert_eq!(root.children[0].kind, "element");
    }
}
