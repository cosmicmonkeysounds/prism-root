//! Starter component catalog — the default registry every boot ships.
//!
//! Five blocks land here: `heading`, `text`, `link`, `image`, and
//! `container`. Each implements [`Component`] for *both* render
//! targets: `render_html` drives the Sovereign Portal SSR path, and
//! `render_slint` emits the matching `.slint` DSL via [`SlintEmitter`]
//! so the same catalog round-trips through Studio's live builder
//! panel and `prism-relay`'s HTTP response body.
//!
//! The catalog lives inside `prism-builder` (not `prism-relay`) because
//! both the relay *and* the Studio shell want the same starter set —
//! putting it behind the registry crate keeps the component inventory
//! single-sourced and stops downstream crates from reimplementing
//! heading/text/link/image/container every time they want a document
//! to render.

use std::sync::Arc;

use serde_json::Value;

use crate::component::{
    Component, ComponentId, RenderError, RenderHtmlContext, RenderSlintContext,
};
use crate::document::Node;
use crate::html::Html;
use crate::registry::{ComponentRegistry, FieldSpec, NumericBounds, RegistryError};
use crate::slint_source::SlintEmitter;

/// Register the starter catalog into `reg`. Call this once at boot
/// to get a registry with five ready-to-render components.
pub fn register_builtins(reg: &mut ComponentRegistry) -> Result<(), RegistryError> {
    reg.register(Arc::new(HeadingComponent {
        id: "heading".into(),
    }))?;
    reg.register(Arc::new(TextComponent { id: "text".into() }))?;
    reg.register(Arc::new(LinkComponent { id: "link".into() }))?;
    reg.register(Arc::new(ImageComponent { id: "image".into() }))?;
    reg.register(Arc::new(ContainerComponent {
        id: "container".into(),
    }))?;
    Ok(())
}

fn heading_font_size(level: u64) -> f64 {
    match level {
        1 => 32.0,
        2 => 26.0,
        3 => 22.0,
        4 => 18.0,
        5 => 16.0,
        _ => 14.0,
    }
}

/// `h1`–`h6` depending on `props.level` (clamped to 1..=6). Reads
/// its label from `props.text` and escapes it.
pub struct HeadingComponent {
    pub id: ComponentId,
}

impl Component for HeadingComponent {
    fn id(&self) -> &ComponentId {
        &self.id
    }
    fn schema(&self) -> Vec<FieldSpec> {
        vec![
            FieldSpec::text("text", "Text").required(),
            FieldSpec::integer("level", "Heading level", NumericBounds::min_max(1.0, 6.0))
                .with_default(Value::from(1)),
        ]
    }
    fn render_html(
        &self,
        _ctx: &RenderHtmlContext<'_>,
        props: &Value,
        _children: &[Node],
        out: &mut Html,
    ) -> Result<(), RenderError> {
        let level = props
            .get("level")
            .and_then(|v| v.as_u64())
            .unwrap_or(1)
            .clamp(1, 6);
        let text = props.get("text").and_then(|v| v.as_str()).unwrap_or("");
        let tag = match level {
            1 => "h1",
            2 => "h2",
            3 => "h3",
            4 => "h4",
            5 => "h5",
            _ => "h6",
        };
        out.open(tag);
        out.text(text);
        out.close(tag);
        Ok(())
    }

    fn render_slint(
        &self,
        _ctx: &RenderSlintContext<'_>,
        props: &Value,
        _children: &[Node],
        out: &mut SlintEmitter,
    ) -> Result<(), RenderError> {
        let level = props
            .get("level")
            .and_then(|v| v.as_u64())
            .unwrap_or(1)
            .clamp(1, 6);
        let text = props.get("text").and_then(|v| v.as_str()).unwrap_or("");
        out.block("Text", |out| {
            out.prop_string("text", text);
            out.prop_px("font-size", heading_font_size(level));
            out.line("font-weight: 700;");
            Ok(())
        })
    }
}

/// Paragraph. Reads `props.body` as the text content.
pub struct TextComponent {
    pub id: ComponentId,
}

impl Component for TextComponent {
    fn id(&self) -> &ComponentId {
        &self.id
    }
    fn schema(&self) -> Vec<FieldSpec> {
        vec![FieldSpec::textarea("body", "Body")]
    }
    fn render_html(
        &self,
        _ctx: &RenderHtmlContext<'_>,
        props: &Value,
        _children: &[Node],
        out: &mut Html,
    ) -> Result<(), RenderError> {
        let body = props.get("body").and_then(|v| v.as_str()).unwrap_or("");
        out.open("p");
        out.text(body);
        out.close("p");
        Ok(())
    }

    fn render_slint(
        &self,
        _ctx: &RenderSlintContext<'_>,
        props: &Value,
        _children: &[Node],
        out: &mut SlintEmitter,
    ) -> Result<(), RenderError> {
        let body = props.get("body").and_then(|v| v.as_str()).unwrap_or("");
        out.block("Text", |out| {
            out.prop_string("text", body);
            out.prop_px("font-size", 14.0);
            out.line("wrap: word-wrap;");
            Ok(())
        })
    }
}

/// Anchor tag. Emits `<a href="…">text</a>`; children (if any) are
/// rendered inside the anchor in addition to the `text` prop.
pub struct LinkComponent {
    pub id: ComponentId,
}

impl Component for LinkComponent {
    fn id(&self) -> &ComponentId {
        &self.id
    }
    fn schema(&self) -> Vec<FieldSpec> {
        vec![
            FieldSpec::text("href", "URL").required(),
            FieldSpec::text("text", "Label"),
        ]
    }
    fn render_html(
        &self,
        ctx: &RenderHtmlContext<'_>,
        props: &Value,
        children: &[Node],
        out: &mut Html,
    ) -> Result<(), RenderError> {
        let href = props.get("href").and_then(|v| v.as_str()).unwrap_or("#");
        out.open_attrs("a", &[("href", href)]);
        if let Some(text) = props.get("text").and_then(|v| v.as_str()) {
            out.text(text);
        }
        ctx.render_children(children, out)?;
        out.close("a");
        Ok(())
    }

    fn render_slint(
        &self,
        _ctx: &RenderSlintContext<'_>,
        props: &Value,
        _children: &[Node],
        out: &mut SlintEmitter,
    ) -> Result<(), RenderError> {
        // Slint has no built-in anchor element; show the label as
        // underlined text the way the Studio previews external links.
        let text = props
            .get("text")
            .and_then(|v| v.as_str())
            .or_else(|| props.get("href").and_then(|v| v.as_str()))
            .unwrap_or("");
        out.block("Text", |out| {
            out.prop_string("text", text);
            out.prop_px("font-size", 14.0);
            out.line("color: #5aa0ff;");
            Ok(())
        })
    }
}

/// Void `img` element. Reads `props.src` and `props.alt`.
pub struct ImageComponent {
    pub id: ComponentId,
}

impl Component for ImageComponent {
    fn id(&self) -> &ComponentId {
        &self.id
    }
    fn schema(&self) -> Vec<FieldSpec> {
        vec![
            FieldSpec::text("src", "Image source").required(),
            FieldSpec::text("alt", "Alt text"),
        ]
    }
    fn render_html(
        &self,
        _ctx: &RenderHtmlContext<'_>,
        props: &Value,
        _children: &[Node],
        out: &mut Html,
    ) -> Result<(), RenderError> {
        let src = props.get("src").and_then(|v| v.as_str()).unwrap_or("");
        let alt = props.get("alt").and_then(|v| v.as_str()).unwrap_or("");
        out.void("img", &[("src", src), ("alt", alt)]);
        Ok(())
    }

    fn render_slint(
        &self,
        _ctx: &RenderSlintContext<'_>,
        props: &Value,
        _children: &[Node],
        out: &mut SlintEmitter,
    ) -> Result<(), RenderError> {
        // Slint's `Image` element expects `@image-url(...)`; we can't
        // resolve user-supplied URLs at DSL emit time, so we fall back
        // to a sized placeholder rectangle with the alt text overlaid.
        let alt = props.get("alt").and_then(|v| v.as_str()).unwrap_or("image");
        out.block("Rectangle", |out| {
            out.prop_px("min-height", 120.0);
            out.line("background: #2a3140;");
            out.line("border-radius: 6px;");
            out.block("Text", |out| {
                out.prop_string("text", alt);
                out.prop_px("font-size", 12.0);
                out.line("color: #9ca4b4;");
                out.line("horizontal-alignment: center;");
                out.line("vertical-alignment: center;");
                Ok(())
            })
        })
    }
}

/// Semantic `<section>` wrapper with children rendered inside.
/// Useful as a layout block in the portal body.
pub struct ContainerComponent {
    pub id: ComponentId,
}

impl Component for ContainerComponent {
    fn id(&self) -> &ComponentId {
        &self.id
    }
    fn schema(&self) -> Vec<FieldSpec> {
        vec![FieldSpec::integer(
            "spacing",
            "Child spacing (px)",
            NumericBounds::min_max(0.0, 64.0),
        )
        .with_default(Value::from(12))]
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

    fn render_slint(
        &self,
        ctx: &RenderSlintContext<'_>,
        props: &Value,
        children: &[Node],
        out: &mut SlintEmitter,
    ) -> Result<(), RenderError> {
        let spacing = props
            .get("spacing")
            .and_then(|v| v.as_f64())
            .unwrap_or(12.0);
        out.block("VerticalLayout", |out| {
            out.prop_px("spacing", spacing);
            out.line("alignment: start;");
            ctx.render_children(children, out)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::{render_document_html, render_document_slint_source};
    use crate::BuilderDocument;
    use prism_core::design_tokens::DesignTokens;
    use serde_json::json;

    fn setup() -> (ComponentRegistry, DesignTokens) {
        let mut reg = ComponentRegistry::new();
        register_builtins(&mut reg).expect("register builtins");
        (reg, DesignTokens::default())
    }

    fn doc(node: Node) -> BuilderDocument {
        BuilderDocument {
            root: Some(node),
            zones: Default::default(),
        }
    }

    #[test]
    fn heading_respects_level_prop() {
        let (reg, tokens) = setup();
        let d = doc(Node {
            id: "n".into(),
            component: "heading".into(),
            props: json!({ "text": "Prism", "level": 3 }),
            children: vec![],
        });
        assert_eq!(
            render_document_html(&d, &reg, &tokens).unwrap(),
            "<h3>Prism</h3>"
        );
    }

    #[test]
    fn heading_defaults_to_h1() {
        let (reg, tokens) = setup();
        let d = doc(Node {
            id: "n".into(),
            component: "heading".into(),
            props: json!({ "text": "Prism" }),
            children: vec![],
        });
        assert_eq!(
            render_document_html(&d, &reg, &tokens).unwrap(),
            "<h1>Prism</h1>"
        );
    }

    #[test]
    fn text_renders_paragraph() {
        let (reg, tokens) = setup();
        let d = doc(Node {
            id: "n".into(),
            component: "text".into(),
            props: json!({ "body": "Hello world" }),
            children: vec![],
        });
        assert_eq!(
            render_document_html(&d, &reg, &tokens).unwrap(),
            "<p>Hello world</p>"
        );
    }

    #[test]
    fn link_href_is_attribute_escaped() {
        let (reg, tokens) = setup();
        let d = doc(Node {
            id: "n".into(),
            component: "link".into(),
            props: json!({ "href": "/q?a=1&b=2", "text": "search" }),
            children: vec![],
        });
        assert_eq!(
            render_document_html(&d, &reg, &tokens).unwrap(),
            r#"<a href="/q?a=1&amp;b=2">search</a>"#
        );
    }

    #[test]
    fn image_renders_void_tag() {
        let (reg, tokens) = setup();
        let d = doc(Node {
            id: "n".into(),
            component: "image".into(),
            props: json!({ "src": "/a.png", "alt": "banner" }),
            children: vec![],
        });
        assert_eq!(
            render_document_html(&d, &reg, &tokens).unwrap(),
            r#"<img src="/a.png" alt="banner">"#
        );
    }

    #[test]
    fn container_walks_children() {
        let (reg, tokens) = setup();
        let d = doc(Node {
            id: "root".into(),
            component: "container".into(),
            props: json!({}),
            children: vec![
                Node {
                    id: "n1".into(),
                    component: "heading".into(),
                    props: json!({ "text": "A", "level": 2 }),
                    children: vec![],
                },
                Node {
                    id: "n2".into(),
                    component: "text".into(),
                    props: json!({ "body": "B" }),
                    children: vec![],
                },
            ],
        });
        assert_eq!(
            render_document_html(&d, &reg, &tokens).unwrap(),
            "<section><h2>A</h2><p>B</p></section>"
        );
    }

    #[test]
    fn xss_in_text_prop_is_escaped() {
        let (reg, tokens) = setup();
        let d = doc(Node {
            id: "n".into(),
            component: "heading".into(),
            props: json!({ "text": "<script>alert(1)</script>" }),
            children: vec![],
        });
        assert_eq!(
            render_document_html(&d, &reg, &tokens).unwrap(),
            "<h1>&lt;script&gt;alert(1)&lt;/script&gt;</h1>"
        );
    }

    #[test]
    fn heading_schema_has_required_text_field() {
        let comp = HeadingComponent {
            id: "heading".into(),
        };
        let schema = comp.schema();
        assert_eq!(schema.len(), 2);
        assert_eq!(schema[0].key, "text");
        assert!(schema[0].required);
        assert_eq!(schema[1].key, "level");
    }

    #[test]
    fn slint_walker_covers_full_catalog() {
        let (reg, tokens) = setup();
        let d = doc(Node {
            id: "root".into(),
            component: "container".into(),
            props: json!({ "spacing": 16 }),
            children: vec![
                Node {
                    id: "h".into(),
                    component: "heading".into(),
                    props: json!({ "text": "Welcome", "level": 2 }),
                    children: vec![],
                },
                Node {
                    id: "p".into(),
                    component: "text".into(),
                    props: json!({ "body": "intro body" }),
                    children: vec![],
                },
                Node {
                    id: "l".into(),
                    component: "link".into(),
                    props: json!({ "href": "/x", "text": "Read" }),
                    children: vec![],
                },
                Node {
                    id: "i".into(),
                    component: "image".into(),
                    props: json!({ "src": "/a.png", "alt": "hero" }),
                    children: vec![],
                },
            ],
        });
        let source = render_document_slint_source(&d, &reg, &tokens).unwrap();
        assert!(source.contains("VerticalLayout {"));
        assert!(source.contains("spacing: 16"));
        assert!(source.contains(r#"text: "Welcome";"#));
        assert!(source.contains(r#"text: "intro body";"#));
        assert!(source.contains(r#"text: "Read";"#));
        assert!(source.contains(r#"text: "hero";"#));
    }
}
