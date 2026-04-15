//! Built-in components the portal surface ships out of the box.
//!
//! `prism-relay` owns a small starter catalog so portals can render
//! meaningful content the moment the server boots — heading, text,
//! link, image, and a layout container. Every component implements
//! [`prism_builder::Component`]; the Slint render path will be wired
//! in Phase 3 alongside Studio's interactive surface. These impls
//! exist to exercise the `render_html` contract end-to-end and give
//! the portal routes something real to serve.
//!
//! Legacy Puck documents that name these components (`"heading"`,
//! `"text"`, `"link"`, …) boot unchanged — [`prism_builder::puck_json`]
//! reads Puck JSON forever and the id strings here match.

use std::sync::Arc;

use prism_builder::{
    Component, ComponentId, ComponentRegistry, Html, Node, RegistryError, RenderError,
    RenderHtmlContext,
};
use serde_json::{json, Value};

/// Register the starter catalog into `reg`. Call this once at boot
/// — `AppState::new` already does it. Returns the first registry
/// error if any id is already taken.
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

/// `h1`–`h6` depending on `props.level` (clamped to 1..=6). Reads
/// its label from `props.text` and escapes it.
pub struct HeadingComponent {
    pub id: ComponentId,
}

impl Component for HeadingComponent {
    fn id(&self) -> &ComponentId {
        &self.id
    }
    fn schema(&self) -> Value {
        json!({
            "text": "string",
            "level": { "type": "integer", "min": 1, "max": 6, "default": 1 }
        })
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
}

/// Paragraph. Reads `props.body` as the text content.
pub struct TextComponent {
    pub id: ComponentId,
}

impl Component for TextComponent {
    fn id(&self) -> &ComponentId {
        &self.id
    }
    fn schema(&self) -> Value {
        json!({ "body": "string" })
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
    fn schema(&self) -> Value {
        json!({ "href": "string", "text": "string" })
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
}

/// Void `img` element. Reads `props.src` and `props.alt`.
pub struct ImageComponent {
    pub id: ComponentId,
}

impl Component for ImageComponent {
    fn id(&self) -> &ComponentId {
        &self.id
    }
    fn schema(&self) -> Value {
        json!({ "src": "string", "alt": "string" })
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

#[cfg(test)]
mod tests {
    use super::*;
    use prism_builder::{render_document_html, BuilderDocument};
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
}
