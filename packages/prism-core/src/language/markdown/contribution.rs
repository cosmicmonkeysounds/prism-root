//! Markdown [`LanguageContribution`] — the unified registration
//! consumed by `LanguageRegistry::register(create_markdown_contribution())`.
//!
//! Port of `language/markdown/contribution.ts`. Block parsing goes
//! through [`crate::language::forms::markdown::parse_markdown`] so
//! there is exactly one markdown tokenizer in the codebase. Each
//! block becomes a child [`SyntaxNode`] on the emitted [`RootNode`]
//! with `kind` = the block kind and its text in the `value` slot.
//! `hr` blocks have no text, `oli` carries `{ n }` in `data`, `task`
//! carries `{ checked }`, `code` carries `{ lang }` when present —
//! the same projection the TS version used so downstream renderers
//! walk identical tree shapes.
//!
//! The surface exposes `code` and `preview` modes with the built-in
//! wiki-link inline token from the registry. The actual preview
//! renderer is supplied by the Studio shell — the core contribution
//! stays framework-free via the `R` / `E` slot.

use indexmap::IndexMap;
use serde_json::Value as JsonValue;

use crate::language::forms::markdown::{parse_markdown, BlockToken};
use crate::language::registry::{
    wikilink_token, LanguageContribution, LanguageSurface, SurfaceMode,
};
use crate::language::syntax::{RootNode, SyntaxNode};

/// Namespaced contribution id registered with the unified
/// `LanguageRegistry`.
pub const MARKDOWN_ID: &str = "prism:markdown";

/// File extensions the Markdown contribution claims.
pub const MARKDOWN_EXTENSIONS: &[&str] = &[".md", ".mdx", ".markdown"];

const MARKDOWN_MIME_TYPE: &str = "text/markdown";

/// Create the unified Markdown [`LanguageContribution`].
pub fn create_markdown_contribution<R, E>() -> LanguageContribution<R, E> {
    let surface = LanguageSurface::new(
        SurfaceMode::Preview,
        vec![SurfaceMode::Code, SurfaceMode::Preview],
    )
    .with_inline_tokens(vec![wikilink_token()]);

    LanguageContribution::new(
        MARKDOWN_ID,
        MARKDOWN_EXTENSIONS.iter().copied(),
        "Markdown",
        surface,
    )
    .with_mime_type(MARKDOWN_MIME_TYPE)
    .with_parse(markdown_parse)
}

fn markdown_parse(source: &str) -> RootNode {
    let mut root = RootNode::new();
    for block in parse_markdown(source) {
        if let Some(node) = block_to_node(block) {
            root.children.push(node);
        }
    }
    root
}

fn block_to_node(block: BlockToken) -> Option<SyntaxNode> {
    match block {
        BlockToken::Empty => None,
        BlockToken::Hr => Some(SyntaxNode {
            kind: "hr".into(),
            ..SyntaxNode::default()
        }),
        BlockToken::H1 { text } => Some(text_node("h1", text)),
        BlockToken::H2 { text } => Some(text_node("h2", text)),
        BlockToken::H3 { text } => Some(text_node("h3", text)),
        BlockToken::P { text } => Some(text_node("p", text)),
        BlockToken::Blockquote { text } => Some(text_node("blockquote", text)),
        BlockToken::Li { text } => Some(text_node("li", text)),
        BlockToken::Oli { text, n } => {
            let mut data = IndexMap::new();
            data.insert("n".into(), JsonValue::from(n));
            Some(SyntaxNode {
                kind: "oli".into(),
                value: Some(text),
                data,
                ..SyntaxNode::default()
            })
        }
        BlockToken::Task { text, checked } => {
            let mut data = IndexMap::new();
            data.insert("checked".into(), JsonValue::Bool(checked));
            Some(SyntaxNode {
                kind: "task".into(),
                value: Some(text),
                data,
                ..SyntaxNode::default()
            })
        }
        BlockToken::Code { text, lang } => {
            let mut data = IndexMap::new();
            if let Some(lang) = lang {
                data.insert("lang".into(), JsonValue::String(lang));
            }
            Some(SyntaxNode {
                kind: "code".into(),
                value: Some(text),
                data,
                ..SyntaxNode::default()
            })
        }
    }
}

fn text_node(kind: &str, text: String) -> SyntaxNode {
    SyntaxNode {
        kind: kind.into(),
        value: Some(text),
        ..SyntaxNode::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::language::document::{create_text_file, TextFileParams};
    use crate::language::registry::{LanguageRegistry, ResolveOptions};

    #[test]
    fn contribution_has_identity_fields() {
        let c = create_markdown_contribution::<(), ()>();
        assert_eq!(c.id, MARKDOWN_ID);
        assert_eq!(c.extensions, vec![".md", ".mdx", ".markdown"]);
        assert_eq!(c.display_name, "Markdown");
        assert_eq!(c.mime_type.as_deref(), Some(MARKDOWN_MIME_TYPE));
    }

    #[test]
    fn surface_defaults_to_preview_mode() {
        let c = create_markdown_contribution::<(), ()>();
        assert_eq!(c.surface.default_mode, SurfaceMode::Preview);
        assert!(c.surface.available_modes.contains(&SurfaceMode::Code));
        assert!(c.surface.available_modes.contains(&SurfaceMode::Preview));
    }

    #[test]
    fn surface_exposes_wikilink_inline_token() {
        let c = create_markdown_contribution::<(), ()>();
        assert_eq!(c.surface.inline_tokens.len(), 1);
        assert_eq!(c.surface.inline_tokens[0].id, "wikilink");
    }

    #[test]
    fn parse_emits_one_child_per_non_empty_block() {
        let c = create_markdown_contribution::<(), ()>();
        let parse = c.parse.as_ref().expect("parse hook");
        let root = parse("# Hi\n\nsome text\n- a list item");
        let kinds: Vec<_> = root.children.iter().map(|c| c.kind.as_str()).collect();
        assert_eq!(kinds, vec!["h1", "p", "li"]);
    }

    #[test]
    fn parse_projects_task_block_with_checked_data() {
        let c = create_markdown_contribution::<(), ()>();
        let parse = c.parse.as_ref().expect("parse hook");
        let root = parse("- [x] done");
        let task = &root.children[0];
        assert_eq!(task.kind, "task");
        assert_eq!(task.value.as_deref(), Some("done"));
        assert_eq!(task.data.get("checked"), Some(&JsonValue::Bool(true)));
    }

    #[test]
    fn parse_projects_oli_block_with_index_data() {
        let c = create_markdown_contribution::<(), ()>();
        let parse = c.parse.as_ref().expect("parse hook");
        let root = parse("3. third");
        let oli = &root.children[0];
        assert_eq!(oli.kind, "oli");
        assert_eq!(oli.value.as_deref(), Some("third"));
        assert_eq!(oli.data.get("n"), Some(&JsonValue::from(3u32)));
    }

    #[test]
    fn parse_projects_code_block_with_optional_lang_data() {
        let c = create_markdown_contribution::<(), ()>();
        let parse = c.parse.as_ref().expect("parse hook");

        let with_lang = parse("```rust\nfn x() {}\n```");
        let code = &with_lang.children[0];
        assert_eq!(code.kind, "code");
        assert_eq!(code.value.as_deref(), Some("fn x() {}"));
        assert_eq!(
            code.data.get("lang"),
            Some(&JsonValue::String("rust".into()))
        );

        let no_lang = parse("```\nplain\n```");
        let code2 = &no_lang.children[0];
        assert!(code2.data.get("lang").is_none());
    }

    #[test]
    fn parse_drops_empty_and_projects_hr() {
        let c = create_markdown_contribution::<(), ()>();
        let parse = c.parse.as_ref().expect("parse hook");
        let root = parse("a\n\n---\nb");
        let kinds: Vec<_> = root.children.iter().map(|c| c.kind.as_str()).collect();
        assert_eq!(kinds, vec!["p", "hr", "p"]);
    }

    #[test]
    fn registry_resolves_every_extension() {
        let mut registry = LanguageRegistry::<(), ()>::new();
        registry.register(create_markdown_contribution());

        for ext in MARKDOWN_EXTENSIONS {
            let filename = format!("notes/a{ext}");
            let hit = registry
                .resolve(ResolveOptions::by_filename(&filename))
                .unwrap_or_else(|| panic!("resolve {ext}"));
            assert_eq!(hit.id, MARKDOWN_ID);
        }
    }

    #[test]
    fn registry_resolves_markdown_prism_file() {
        let mut registry = LanguageRegistry::<(), ()>::new();
        registry.register(create_markdown_contribution());

        let file = create_text_file(TextFileParams {
            path: "notes/today.md".into(),
            text: "# hi".into(),
            ..Default::default()
        });
        let hit = registry.resolve_file(&file).expect("markdown resolves");
        assert_eq!(hit.id, MARKDOWN_ID);
    }
}
