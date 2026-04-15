//! Document-level render entry points.
//!
//! Walks a [`BuilderDocument`] against a [`ComponentRegistry`] and
//! produces a rendered output for a given target backend. Today that
//! means HTML for the Sovereign Portal SSR path; Clay's walker lands
//! here in Phase 3 alongside the real `ClayLayoutScope` integration.

use prism_core::design_tokens::DesignTokens;

use crate::component::{RenderError, RenderHtmlContext};
use crate::document::BuilderDocument;
use crate::html::Html;
use crate::registry::ComponentRegistry;

/// Render a document to an HTML fragment. Emits the root node's
/// markup and every descendant in order. Zones are ignored for now
/// — the portal layer wraps the returned fragment in its own
/// chrome (doctype, `<head>`, OpenGraph meta, etc.).
///
/// Returns a bare fragment string on success. The caller is free to
/// embed it in a full document or stream it through additional
/// transforms.
pub fn render_document_html(
    doc: &BuilderDocument,
    registry: &ComponentRegistry,
    tokens: &DesignTokens,
) -> Result<String, RenderError> {
    let ctx = RenderHtmlContext { tokens, registry };
    let mut out = Html::with_capacity(512);
    if let Some(root) = &doc.root {
        ctx.render_child(root, &mut out)?;
    }
    Ok(out.into_string())
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use serde_json::{json, Value};

    use super::*;
    use crate::component::{Component, ComponentId, RenderHtmlContext};
    use crate::document::Node;

    /// Tiny `h1`-rendering component used by the walker tests. Reads
    /// its label from `props.text`.
    struct Heading {
        id: ComponentId,
    }

    impl Component for Heading {
        fn id(&self) -> &ComponentId {
            &self.id
        }

        fn schema(&self) -> Value {
            json!({ "text": "string" })
        }

        fn render_html(
            &self,
            _ctx: &RenderHtmlContext<'_>,
            props: &Value,
            _children: &[Node],
            out: &mut Html,
        ) -> Result<(), RenderError> {
            let text = props.get("text").and_then(|v| v.as_str()).unwrap_or("");
            out.open("h1");
            out.text(text);
            out.close("h1");
            Ok(())
        }
    }

    /// Container component used to verify child recursion.
    struct Section {
        id: ComponentId,
    }

    impl Component for Section {
        fn id(&self) -> &ComponentId {
            &self.id
        }

        fn schema(&self) -> Value {
            json!({})
        }

        fn render_html(
            &self,
            ctx: &RenderHtmlContext<'_>,
            _props: &Value,
            children: &[Node],
            out: &mut Html,
        ) -> Result<(), RenderError> {
            out.open("section");
            ctx.render_children(children, out)?;
            out.close("section");
            Ok(())
        }
    }

    fn registry_with_samples() -> ComponentRegistry {
        let mut reg = ComponentRegistry::new();
        reg.register(Arc::new(Heading {
            id: "heading".into(),
        }))
        .unwrap();
        reg.register(Arc::new(Section {
            id: "section".into(),
        }))
        .unwrap();
        reg
    }

    #[test]
    fn renders_empty_document_to_empty_string() {
        let doc = BuilderDocument::default();
        let registry = ComponentRegistry::new();
        let tokens = DesignTokens::default();
        let html = render_document_html(&doc, &registry, &tokens).unwrap();
        assert_eq!(html, "");
    }

    #[test]
    fn renders_single_heading() {
        let doc = BuilderDocument {
            root: Some(Node {
                id: "n1".into(),
                component: "heading".into(),
                props: json!({ "text": "Hello Prism" }),
                children: vec![],
            }),
            zones: Default::default(),
        };
        let registry = registry_with_samples();
        let tokens = DesignTokens::default();
        let html = render_document_html(&doc, &registry, &tokens).unwrap();
        assert_eq!(html, "<h1>Hello Prism</h1>");
    }

    #[test]
    fn escapes_user_supplied_text() {
        let doc = BuilderDocument {
            root: Some(Node {
                id: "n1".into(),
                component: "heading".into(),
                props: json!({ "text": "<script>alert('xss')</script>" }),
                children: vec![],
            }),
            zones: Default::default(),
        };
        let registry = registry_with_samples();
        let tokens = DesignTokens::default();
        let html = render_document_html(&doc, &registry, &tokens).unwrap();
        assert_eq!(
            html,
            "<h1>&lt;script&gt;alert(&#39;xss&#39;)&lt;/script&gt;</h1>"
        );
    }

    #[test]
    fn recursive_children_walk() {
        let doc = BuilderDocument {
            root: Some(Node {
                id: "n1".into(),
                component: "section".into(),
                props: json!({}),
                children: vec![
                    Node {
                        id: "n2".into(),
                        component: "heading".into(),
                        props: json!({ "text": "A" }),
                        children: vec![],
                    },
                    Node {
                        id: "n3".into(),
                        component: "heading".into(),
                        props: json!({ "text": "B" }),
                        children: vec![],
                    },
                ],
            }),
            zones: Default::default(),
        };
        let registry = registry_with_samples();
        let tokens = DesignTokens::default();
        let html = render_document_html(&doc, &registry, &tokens).unwrap();
        assert_eq!(html, "<section><h1>A</h1><h1>B</h1></section>");
    }

    #[test]
    fn unknown_component_errors() {
        let doc = BuilderDocument {
            root: Some(Node {
                id: "n1".into(),
                component: "not-registered".into(),
                props: json!({}),
                children: vec![],
            }),
            zones: Default::default(),
        };
        let registry = registry_with_samples();
        let tokens = DesignTokens::default();
        let err = render_document_html(&doc, &registry, &tokens).unwrap_err();
        assert!(matches!(err, RenderError::UnknownComponent(ref id) if id == "not-registered"));
    }

    #[test]
    fn default_html_impl_emits_div_wrapper() {
        // A component that doesn't override `render_html` should fall
        // back to `<div data-component="id">` with recursive children.
        struct Plain {
            id: ComponentId,
        }
        impl Component for Plain {
            fn id(&self) -> &ComponentId {
                &self.id
            }
            fn schema(&self) -> Value {
                json!({})
            }
        }

        let mut reg = ComponentRegistry::new();
        reg.register(Arc::new(Plain {
            id: "plain".into(),
        }))
        .unwrap();
        reg.register(Arc::new(Heading {
            id: "heading".into(),
        }))
        .unwrap();

        let doc = BuilderDocument {
            root: Some(Node {
                id: "n1".into(),
                component: "plain".into(),
                props: json!({}),
                children: vec![Node {
                    id: "n2".into(),
                    component: "heading".into(),
                    props: json!({ "text": "Inside" }),
                    children: vec![],
                }],
            }),
            zones: Default::default(),
        };
        let tokens = DesignTokens::default();
        let html = render_document_html(&doc, &reg, &tokens).unwrap();
        assert_eq!(
            html,
            r#"<div data-component="plain"><h1>Inside</h1></div>"#
        );
    }
}
