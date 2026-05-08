//! Builder panel — the Studio page builder surface.
//!
//! Shows the generated `.slint` DSL that `prism_builder` emits from
//! the active `ComponentRegistry` + `BuilderDocument`. The Slint side
//! paints the source into a monospaced `Text` for now; once the
//! interpreter path is wired into a `ComponentContainer` slot the
//! same data feed materialises a live preview.

use prism_builder::{render_document_slint_source, BuilderDocument, ComponentRegistry};
use prism_core::design_tokens::DesignTokens;
use prism_core::help::HelpEntry;

use super::Panel;

pub struct BuilderPanel;

impl BuilderPanel {
    pub const ID: i32 = 1;
    pub fn new() -> Self {
        Self
    }

    /// Emit the `.slint` DSL for `doc` against `registry`. Errors
    /// come from unknown component ids or panicking render impls;
    /// the shell surfaces them as a red banner string rather than
    /// panicking.
    pub fn source(
        doc: &BuilderDocument,
        registry: &ComponentRegistry,
        tokens: &DesignTokens,
    ) -> String {
        match render_document_slint_source(doc, registry, tokens) {
            Ok(s) => s,
            Err(e) => format!("// render error: {e}"),
        }
    }

    /// Count every node reachable from `doc.root`. Used by the panel
    /// header so the user sees at a glance how big the walked tree
    /// is — the raw DSL source is otherwise opaque.
    pub fn node_count(doc: &BuilderDocument) -> usize {
        fn walk(node: &prism_builder::Node) -> usize {
            1 + node.children.iter().map(walk).sum::<usize>()
        }
        doc.root.as_ref().map(walk).unwrap_or(0)
    }
}

impl Default for BuilderPanel {
    fn default() -> Self {
        Self::new()
    }
}

impl Panel for BuilderPanel {
    fn id(&self) -> i32 {
        Self::ID
    }
    fn label(&self) -> &'static str {
        "Builder"
    }
    fn title(&self) -> &'static str {
        "Builder"
    }
    fn hint(&self) -> &'static str {
        "Live preview of the generated .slint DSL for the active document."
    }
    fn help_entry(&self) -> Option<HelpEntry> {
        Some(HelpEntry::new(
            "shell.panels.builder",
            "Builder",
            "Visual page builder with WYSIWYG editing. Click components to select, then edit inline or in the properties panel.",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use prism_builder::{starter::register_builtins, BuilderDocument, ComponentRegistry, Node};
    use prism_core::design_tokens::DEFAULT_TOKENS;
    use serde_json::json;

    fn sample_doc() -> BuilderDocument {
        BuilderDocument {
            root: Some(Node {
                id: "root".into(),
                component: "container".into(),
                props: json!({ "spacing": 12 }),
                children: vec![Node {
                    id: "h".into(),
                    component: "text".into(),
                    props: json!({ "body": "Hello", "level": "h1" }),
                    children: vec![],
                    ..Default::default()
                }],
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    #[test]
    fn builder_source_contains_text_body() {
        let mut reg = ComponentRegistry::new();
        register_builtins(&mut reg).unwrap();
        let doc = sample_doc();
        let src = BuilderPanel::source(&doc, &reg, &DEFAULT_TOKENS);
        assert!(src.contains(r#"text: "Hello";"#));
        assert!(src.contains("VerticalLayout {"));
    }

    #[test]
    fn node_count_walks_children() {
        let doc = sample_doc();
        assert_eq!(BuilderPanel::node_count(&doc), 2);
    }

    #[test]
    fn empty_document_has_zero_nodes() {
        assert_eq!(BuilderPanel::node_count(&BuilderDocument::default()), 0);
    }
}
