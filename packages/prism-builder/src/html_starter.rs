//! HTML starter catalog — the 17 built-in blocks for Sovereign Portal SSR.
//!
//! Mirrors `starter.rs` but implements `HtmlBlock` instead of `Component`.
//! The relay calls `register_html_builtins` on boot; the shell never
//! touches this module.

use std::sync::Arc;

use serde_json::Value;

use crate::component::{ComponentId, RenderError};
use crate::document::Node;
use crate::html::Html;
use crate::html_block::{HtmlBlock, HtmlRegistry, HtmlRenderContext};
use crate::registry::{FieldSpec, NumericBounds, RegistryError, SelectOption};

pub fn register_html_builtins(reg: &mut HtmlRegistry) -> Result<(), RegistryError> {
    reg.register(Arc::new(HtmlHeading {
        id: "heading".into(),
    }))?;
    reg.register(Arc::new(HtmlText { id: "text".into() }))?;
    reg.register(Arc::new(HtmlLink { id: "link".into() }))?;
    reg.register(Arc::new(HtmlImage { id: "image".into() }))?;
    reg.register(Arc::new(HtmlContainer {
        id: "container".into(),
    }))?;
    reg.register(Arc::new(HtmlForm { id: "form".into() }))?;
    reg.register(Arc::new(HtmlInput { id: "input".into() }))?;
    reg.register(Arc::new(HtmlButton {
        id: "button".into(),
    }))?;
    reg.register(Arc::new(HtmlCard { id: "card".into() }))?;
    reg.register(Arc::new(HtmlCode { id: "code".into() }))?;
    reg.register(Arc::new(HtmlDivider {
        id: "divider".into(),
    }))?;
    reg.register(Arc::new(HtmlSpacer {
        id: "spacer".into(),
    }))?;
    reg.register(Arc::new(HtmlColumns {
        id: "columns".into(),
    }))?;
    reg.register(Arc::new(HtmlList { id: "list".into() }))?;
    reg.register(Arc::new(HtmlTable { id: "table".into() }))?;
    reg.register(Arc::new(HtmlTabs { id: "tabs".into() }))?;
    reg.register(Arc::new(HtmlAccordion {
        id: "accordion".into(),
    }))?;
    Ok(())
}

struct HtmlHeading {
    id: ComponentId,
}

impl HtmlBlock for HtmlHeading {
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
        _ctx: &HtmlRenderContext<'_>,
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

struct HtmlText {
    id: ComponentId,
}

impl HtmlBlock for HtmlText {
    fn id(&self) -> &ComponentId {
        &self.id
    }
    fn schema(&self) -> Vec<FieldSpec> {
        vec![FieldSpec::textarea("body", "Body")]
    }
    fn render_html(
        &self,
        _ctx: &HtmlRenderContext<'_>,
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

struct HtmlLink {
    id: ComponentId,
}

impl HtmlBlock for HtmlLink {
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
        ctx: &HtmlRenderContext<'_>,
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

struct HtmlImage {
    id: ComponentId,
}

impl HtmlBlock for HtmlImage {
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
        _ctx: &HtmlRenderContext<'_>,
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

struct HtmlContainer {
    id: ComponentId,
}

impl HtmlBlock for HtmlContainer {
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
        ctx: &HtmlRenderContext<'_>,
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

struct HtmlForm {
    id: ComponentId,
}

impl HtmlBlock for HtmlForm {
    fn id(&self) -> &ComponentId {
        &self.id
    }
    fn schema(&self) -> Vec<FieldSpec> {
        vec![
            FieldSpec::text("action", "Form action URL"),
            FieldSpec::select(
                "method",
                "HTTP method",
                vec![
                    SelectOption::new("post", "POST"),
                    SelectOption::new("get", "GET"),
                ],
            )
            .with_default(Value::from("post")),
        ]
    }
    fn render_html(
        &self,
        ctx: &HtmlRenderContext<'_>,
        props: &Value,
        children: &[Node],
        out: &mut Html,
    ) -> Result<(), RenderError> {
        let method = props
            .get("method")
            .and_then(|v| v.as_str())
            .unwrap_or("post");
        let mut attrs: Vec<(&str, &str)> = vec![("method", method)];
        let action = props.get("action").and_then(|v| v.as_str()).unwrap_or("");
        if !action.is_empty() {
            attrs.push(("action", action));
        }
        out.open_attrs("form", &attrs);
        ctx.render_children(children, out)?;
        out.close("form");
        Ok(())
    }
}

struct HtmlInput {
    id: ComponentId,
}

impl HtmlBlock for HtmlInput {
    fn id(&self) -> &ComponentId {
        &self.id
    }
    fn schema(&self) -> Vec<FieldSpec> {
        vec![
            FieldSpec::text("name", "Field name").required(),
            FieldSpec::select(
                "type",
                "Input type",
                vec![
                    SelectOption::new("text", "Text"),
                    SelectOption::new("email", "Email"),
                    SelectOption::new("password", "Password"),
                    SelectOption::new("number", "Number"),
                    SelectOption::new("hidden", "Hidden"),
                ],
            )
            .with_default(Value::from("text")),
            FieldSpec::text("placeholder", "Placeholder"),
            FieldSpec::text("value", "Default value"),
            FieldSpec::boolean("required", "Required"),
            FieldSpec::text("label", "Label text"),
        ]
    }
    fn render_html(
        &self,
        _ctx: &HtmlRenderContext<'_>,
        props: &Value,
        _children: &[Node],
        out: &mut Html,
    ) -> Result<(), RenderError> {
        let name = props.get("name").and_then(|v| v.as_str()).unwrap_or("");
        let input_type = props.get("type").and_then(|v| v.as_str()).unwrap_or("text");
        let placeholder = props
            .get("placeholder")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let value = props.get("value").and_then(|v| v.as_str()).unwrap_or("");
        let required = props
            .get("required")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let label = props.get("label").and_then(|v| v.as_str()).unwrap_or("");

        if !label.is_empty() {
            out.open("label");
            out.text(label);
        }
        let mut attrs = vec![("type", input_type), ("name", name)];
        if !placeholder.is_empty() {
            attrs.push(("placeholder", placeholder));
        }
        if !value.is_empty() {
            attrs.push(("value", value));
        }
        if required {
            attrs.push(("required", "required"));
        }
        out.void("input", &attrs);
        if !label.is_empty() {
            out.close("label");
        }
        Ok(())
    }
}

struct HtmlButton {
    id: ComponentId,
}

impl HtmlBlock for HtmlButton {
    fn id(&self) -> &ComponentId {
        &self.id
    }
    fn schema(&self) -> Vec<FieldSpec> {
        vec![
            FieldSpec::text("text", "Button label").required(),
            FieldSpec::select(
                "type",
                "Button type",
                vec![
                    SelectOption::new("submit", "Submit"),
                    SelectOption::new("button", "Button"),
                    SelectOption::new("reset", "Reset"),
                ],
            )
            .with_default(Value::from("submit")),
            FieldSpec::boolean("disabled", "Disabled"),
        ]
    }
    fn render_html(
        &self,
        _ctx: &HtmlRenderContext<'_>,
        props: &Value,
        _children: &[Node],
        out: &mut Html,
    ) -> Result<(), RenderError> {
        let text = props
            .get("text")
            .and_then(|v| v.as_str())
            .unwrap_or("Submit");
        let btn_type = props
            .get("type")
            .and_then(|v| v.as_str())
            .unwrap_or("submit");
        let disabled = props
            .get("disabled")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let mut attrs = vec![("type", btn_type)];
        if disabled {
            attrs.push(("disabled", "disabled"));
        }
        out.open_attrs("button", &attrs);
        out.text(text);
        out.close("button");
        Ok(())
    }
}

struct HtmlCard {
    id: ComponentId,
}

impl HtmlBlock for HtmlCard {
    fn id(&self) -> &ComponentId {
        &self.id
    }
    fn schema(&self) -> Vec<FieldSpec> {
        vec![
            FieldSpec::text("title", "Card title").required(),
            FieldSpec::textarea("body", "Card body"),
            FieldSpec::select(
                "variant",
                "Style variant",
                vec![
                    SelectOption::new("default", "Default"),
                    SelectOption::new("outlined", "Outlined"),
                ],
            )
            .with_default(Value::from("default")),
        ]
    }
    fn render_html(
        &self,
        ctx: &HtmlRenderContext<'_>,
        props: &Value,
        children: &[Node],
        out: &mut Html,
    ) -> Result<(), RenderError> {
        let title = props.get("title").and_then(|v| v.as_str()).unwrap_or("");
        let body = props.get("body").and_then(|v| v.as_str()).unwrap_or("");
        out.open("article");
        out.open("h3");
        out.text(title);
        out.close("h3");
        if !body.is_empty() {
            out.open("p");
            out.text(body);
            out.close("p");
        }
        ctx.render_children(children, out)?;
        out.close("article");
        Ok(())
    }
}

struct HtmlCode {
    id: ComponentId,
}

impl HtmlBlock for HtmlCode {
    fn id(&self) -> &ComponentId {
        &self.id
    }
    fn schema(&self) -> Vec<FieldSpec> {
        vec![
            FieldSpec::textarea("code", "Code").required(),
            FieldSpec::text("language", "Language"),
        ]
    }
    fn render_html(
        &self,
        _ctx: &HtmlRenderContext<'_>,
        props: &Value,
        _children: &[Node],
        out: &mut Html,
    ) -> Result<(), RenderError> {
        let code = props.get("code").and_then(|v| v.as_str()).unwrap_or("");
        let lang = props.get("language").and_then(|v| v.as_str()).unwrap_or("");
        out.open("pre");
        if lang.is_empty() {
            out.open("code");
        } else {
            out.open_attrs("code", &[("class", &format!("language-{lang}"))]);
        }
        out.text(code);
        out.close("code");
        out.close("pre");
        Ok(())
    }
}

struct HtmlDivider {
    id: ComponentId,
}

impl HtmlBlock for HtmlDivider {
    fn id(&self) -> &ComponentId {
        &self.id
    }
    fn schema(&self) -> Vec<FieldSpec> {
        vec![]
    }
    fn render_html(
        &self,
        _ctx: &HtmlRenderContext<'_>,
        _props: &Value,
        _children: &[Node],
        out: &mut Html,
    ) -> Result<(), RenderError> {
        out.void("hr", &[]);
        Ok(())
    }
}

struct HtmlSpacer {
    id: ComponentId,
}

impl HtmlBlock for HtmlSpacer {
    fn id(&self) -> &ComponentId {
        &self.id
    }
    fn schema(&self) -> Vec<FieldSpec> {
        vec![
            FieldSpec::integer("height", "Height (px)", NumericBounds::min_max(4.0, 128.0))
                .with_default(Value::from(24)),
        ]
    }
    fn render_html(
        &self,
        _ctx: &HtmlRenderContext<'_>,
        props: &Value,
        _children: &[Node],
        out: &mut Html,
    ) -> Result<(), RenderError> {
        let height = props.get("height").and_then(|v| v.as_u64()).unwrap_or(24);
        let style = format!("height:{height}px");
        out.open_attrs("div", &[("style", &style), ("aria-hidden", "true")]);
        out.close("div");
        Ok(())
    }
}

struct HtmlColumns {
    id: ComponentId,
}

impl HtmlBlock for HtmlColumns {
    fn id(&self) -> &ComponentId {
        &self.id
    }
    fn schema(&self) -> Vec<FieldSpec> {
        vec![
            FieldSpec::integer("gap", "Column gap (px)", NumericBounds::min_max(0.0, 64.0))
                .with_default(Value::from(16)),
        ]
    }
    fn render_html(
        &self,
        ctx: &HtmlRenderContext<'_>,
        props: &Value,
        children: &[Node],
        out: &mut Html,
    ) -> Result<(), RenderError> {
        let gap = props.get("gap").and_then(|v| v.as_u64()).unwrap_or(16);
        let style = format!("display:flex;gap:{gap}px");
        out.open_attrs("div", &[("style", &style)]);
        for child in children {
            out.open_attrs("div", &[("style", "flex:1")]);
            ctx.render_child(child, out)?;
            out.close("div");
        }
        out.close("div");
        Ok(())
    }
}

struct HtmlList {
    id: ComponentId,
}

impl HtmlBlock for HtmlList {
    fn id(&self) -> &ComponentId {
        &self.id
    }
    fn schema(&self) -> Vec<FieldSpec> {
        vec![FieldSpec::boolean("ordered", "Ordered (numbered)")]
    }
    fn render_html(
        &self,
        ctx: &HtmlRenderContext<'_>,
        props: &Value,
        children: &[Node],
        out: &mut Html,
    ) -> Result<(), RenderError> {
        let ordered = props
            .get("ordered")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let tag = if ordered { "ol" } else { "ul" };
        out.open(tag);
        for child in children {
            out.open("li");
            ctx.render_child(child, out)?;
            out.close("li");
        }
        out.close(tag);
        Ok(())
    }
}

struct HtmlTable {
    id: ComponentId,
}

impl HtmlBlock for HtmlTable {
    fn id(&self) -> &ComponentId {
        &self.id
    }
    fn schema(&self) -> Vec<FieldSpec> {
        vec![
            FieldSpec::text("headers", "Column headers (comma-separated)").required(),
            FieldSpec::text("caption", "Table caption"),
        ]
    }
    fn render_html(
        &self,
        _ctx: &HtmlRenderContext<'_>,
        props: &Value,
        _children: &[Node],
        out: &mut Html,
    ) -> Result<(), RenderError> {
        let headers = props.get("headers").and_then(|v| v.as_str()).unwrap_or("");
        let caption = props.get("caption").and_then(|v| v.as_str()).unwrap_or("");
        out.open("table");
        if !caption.is_empty() {
            out.open("caption");
            out.text(caption);
            out.close("caption");
        }
        out.open("thead");
        out.open("tr");
        for col in headers.split(',') {
            let col = col.trim();
            if !col.is_empty() {
                out.open("th");
                out.text(col);
                out.close("th");
            }
        }
        out.close("tr");
        out.close("thead");
        out.close("table");
        Ok(())
    }
}

struct HtmlTabs {
    id: ComponentId,
}

impl HtmlBlock for HtmlTabs {
    fn id(&self) -> &ComponentId {
        &self.id
    }
    fn schema(&self) -> Vec<FieldSpec> {
        vec![FieldSpec::text("labels", "Tab labels (comma-separated)").required()]
    }
    fn render_html(
        &self,
        ctx: &HtmlRenderContext<'_>,
        props: &Value,
        children: &[Node],
        out: &mut Html,
    ) -> Result<(), RenderError> {
        let labels = props.get("labels").and_then(|v| v.as_str()).unwrap_or("");
        out.open_attrs("div", &[("role", "tablist")]);
        for (i, label) in labels.split(',').enumerate() {
            let label = label.trim();
            if !label.is_empty() {
                let selected = if i == 0 { "true" } else { "false" };
                out.open_attrs("button", &[("role", "tab"), ("aria-selected", selected)]);
                out.text(label);
                out.close("button");
            }
        }
        out.close("div");
        for (i, child) in children.iter().enumerate() {
            let hidden = if i == 0 { "" } else { "true" };
            if hidden.is_empty() {
                out.open_attrs("div", &[("role", "tabpanel")]);
            } else {
                out.open_attrs("div", &[("role", "tabpanel"), ("hidden", hidden)]);
            }
            ctx.render_child(child, out)?;
            out.close("div");
        }
        Ok(())
    }
}

struct HtmlAccordion {
    id: ComponentId,
}

impl HtmlBlock for HtmlAccordion {
    fn id(&self) -> &ComponentId {
        &self.id
    }
    fn schema(&self) -> Vec<FieldSpec> {
        vec![
            FieldSpec::text("title", "Section title").required(),
            FieldSpec::boolean("open", "Initially open"),
        ]
    }
    fn render_html(
        &self,
        ctx: &HtmlRenderContext<'_>,
        props: &Value,
        children: &[Node],
        out: &mut Html,
    ) -> Result<(), RenderError> {
        let title = props.get("title").and_then(|v| v.as_str()).unwrap_or("");
        let open = props.get("open").and_then(|v| v.as_bool()).unwrap_or(false);
        if open {
            out.open_attrs("details", &[("open", "open")]);
        } else {
            out.open("details");
        }
        out.open("summary");
        out.text(title);
        out.close("summary");
        ctx.render_children(children, out)?;
        out.close("details");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::{BuilderDocument, Node};
    use crate::render::render_document_html;
    use prism_core::design_tokens::DesignTokens;
    use serde_json::json;

    fn setup() -> (HtmlRegistry, DesignTokens) {
        let mut reg = HtmlRegistry::new();
        register_html_builtins(&mut reg).expect("register html builtins");
        (reg, DesignTokens::default())
    }

    #[test]
    fn heading_renders_correct_tag() {
        let (reg, tokens) = setup();
        let doc = BuilderDocument {
            root: Some(Node {
                id: "n".into(),
                component: "heading".into(),
                props: json!({ "text": "Prism", "level": 3 }),
                children: vec![],
                ..Default::default()
            }),
            ..Default::default()
        };
        assert_eq!(
            render_document_html(&doc, &reg, &tokens).unwrap(),
            "<h3>Prism</h3>"
        );
    }

    #[test]
    fn container_walks_children() {
        let (reg, tokens) = setup();
        let doc = BuilderDocument {
            root: Some(Node {
                id: "root".into(),
                component: "container".into(),
                props: json!({}),
                children: vec![
                    Node {
                        id: "n1".into(),
                        component: "heading".into(),
                        props: json!({ "text": "A", "level": 2 }),
                        children: vec![],
                        ..Default::default()
                    },
                    Node {
                        id: "n2".into(),
                        component: "text".into(),
                        props: json!({ "body": "B" }),
                        children: vec![],
                        ..Default::default()
                    },
                ],
                ..Default::default()
            }),
            ..Default::default()
        };
        assert_eq!(
            render_document_html(&doc, &reg, &tokens).unwrap(),
            "<section><h2>A</h2><p>B</p></section>"
        );
    }

    #[test]
    fn xss_escaped() {
        let (reg, tokens) = setup();
        let doc = BuilderDocument {
            root: Some(Node {
                id: "n".into(),
                component: "heading".into(),
                props: json!({ "text": "<script>alert(1)</script>" }),
                children: vec![],
                ..Default::default()
            }),
            ..Default::default()
        };
        assert_eq!(
            render_document_html(&doc, &reg, &tokens).unwrap(),
            "<h1>&lt;script&gt;alert(1)&lt;/script&gt;</h1>"
        );
    }

    #[test]
    fn register_html_builtins_seeds_seventeen_blocks() {
        let (reg, _) = setup();
        for id in [
            "heading",
            "text",
            "link",
            "image",
            "container",
            "form",
            "input",
            "button",
            "card",
            "code",
            "divider",
            "spacer",
            "columns",
            "list",
            "table",
            "tabs",
            "accordion",
        ] {
            assert!(reg.get(id).is_some(), "missing html block: {id}");
        }
    }

    #[test]
    fn card_renders_article() {
        let (reg, tokens) = setup();
        let doc = BuilderDocument {
            root: Some(Node {
                id: "c".into(),
                component: "card".into(),
                props: json!({ "title": "Title", "body": "Body" }),
                children: vec![],
                ..Default::default()
            }),
            ..Default::default()
        };
        let html = render_document_html(&doc, &reg, &tokens).unwrap();
        assert!(html.starts_with("<article>"));
        assert!(html.contains("<h3>Title</h3>"));
        assert!(html.contains("<p>Body</p>"));
    }

    #[test]
    fn code_renders_pre_code() {
        let (reg, tokens) = setup();
        let doc = BuilderDocument {
            root: Some(Node {
                id: "c".into(),
                component: "code".into(),
                props: json!({ "code": "let x = 1;", "language": "rust" }),
                children: vec![],
                ..Default::default()
            }),
            ..Default::default()
        };
        let html = render_document_html(&doc, &reg, &tokens).unwrap();
        assert!(html.contains(r#"<code class="language-rust">"#));
        assert!(html.contains("let x = 1;"));
    }

    #[test]
    fn divider_renders_hr() {
        let (reg, tokens) = setup();
        let doc = BuilderDocument {
            root: Some(Node {
                id: "d".into(),
                component: "divider".into(),
                props: json!({}),
                children: vec![],
                ..Default::default()
            }),
            ..Default::default()
        };
        assert_eq!(render_document_html(&doc, &reg, &tokens).unwrap(), "<hr>");
    }

    #[test]
    fn spacer_renders_div_with_height() {
        let (reg, tokens) = setup();
        let doc = BuilderDocument {
            root: Some(Node {
                id: "s".into(),
                component: "spacer".into(),
                props: json!({ "height": 48 }),
                children: vec![],
                ..Default::default()
            }),
            ..Default::default()
        };
        let html = render_document_html(&doc, &reg, &tokens).unwrap();
        assert!(html.contains("height:48px"));
        assert!(html.contains("aria-hidden"));
    }

    #[test]
    fn columns_renders_flex() {
        let (reg, tokens) = setup();
        let doc = BuilderDocument {
            root: Some(Node {
                id: "cols".into(),
                component: "columns".into(),
                props: json!({ "gap": 24 }),
                children: vec![
                    Node {
                        id: "c1".into(),
                        component: "text".into(),
                        props: json!({ "body": "left" }),
                        children: vec![],
                        ..Default::default()
                    },
                    Node {
                        id: "c2".into(),
                        component: "text".into(),
                        props: json!({ "body": "right" }),
                        children: vec![],
                        ..Default::default()
                    },
                ],
                ..Default::default()
            }),
            ..Default::default()
        };
        let html = render_document_html(&doc, &reg, &tokens).unwrap();
        assert!(html.contains("display:flex;gap:24px"));
        assert!(html.contains("<p>left</p>"));
        assert!(html.contains("<p>right</p>"));
    }

    #[test]
    fn list_renders_ul_with_li() {
        let (reg, tokens) = setup();
        let doc = BuilderDocument {
            root: Some(Node {
                id: "l".into(),
                component: "list".into(),
                props: json!({}),
                children: vec![
                    Node {
                        id: "i1".into(),
                        component: "text".into(),
                        props: json!({ "body": "item 1" }),
                        children: vec![],
                        ..Default::default()
                    },
                    Node {
                        id: "i2".into(),
                        component: "text".into(),
                        props: json!({ "body": "item 2" }),
                        children: vec![],
                        ..Default::default()
                    },
                ],
                ..Default::default()
            }),
            ..Default::default()
        };
        let html = render_document_html(&doc, &reg, &tokens).unwrap();
        assert!(html.starts_with("<ul>"));
        assert!(html.contains("<li><p>item 1</p></li>"));
    }

    #[test]
    fn list_ordered_renders_ol() {
        let (reg, tokens) = setup();
        let doc = BuilderDocument {
            root: Some(Node {
                id: "l".into(),
                component: "list".into(),
                props: json!({ "ordered": true }),
                children: vec![Node {
                    id: "i1".into(),
                    component: "text".into(),
                    props: json!({ "body": "first" }),
                    children: vec![],
                    ..Default::default()
                }],
                ..Default::default()
            }),
            ..Default::default()
        };
        let html = render_document_html(&doc, &reg, &tokens).unwrap();
        assert!(html.starts_with("<ol>"));
    }

    #[test]
    fn table_renders_thead() {
        let (reg, tokens) = setup();
        let doc = BuilderDocument {
            root: Some(Node {
                id: "t".into(),
                component: "table".into(),
                props: json!({ "headers": "Name, Age", "caption": "Users" }),
                children: vec![],
                ..Default::default()
            }),
            ..Default::default()
        };
        let html = render_document_html(&doc, &reg, &tokens).unwrap();
        assert!(html.contains("<caption>Users</caption>"));
        assert!(html.contains("<th>Name</th>"));
        assert!(html.contains("<th>Age</th>"));
    }

    #[test]
    fn tabs_renders_tablist() {
        let (reg, tokens) = setup();
        let doc = BuilderDocument {
            root: Some(Node {
                id: "t".into(),
                component: "tabs".into(),
                props: json!({ "labels": "Tab A, Tab B" }),
                children: vec![Node {
                    id: "p1".into(),
                    component: "text".into(),
                    props: json!({ "body": "panel" }),
                    children: vec![],
                    ..Default::default()
                }],
                ..Default::default()
            }),
            ..Default::default()
        };
        let html = render_document_html(&doc, &reg, &tokens).unwrap();
        assert!(html.contains(r#"role="tablist"#));
        assert!(html.contains("Tab A"));
        assert!(html.contains(r#"role="tabpanel"#));
    }

    #[test]
    fn accordion_renders_details() {
        let (reg, tokens) = setup();
        let doc = BuilderDocument {
            root: Some(Node {
                id: "a".into(),
                component: "accordion".into(),
                props: json!({ "title": "FAQ", "open": true }),
                children: vec![Node {
                    id: "c".into(),
                    component: "text".into(),
                    props: json!({ "body": "answer" }),
                    children: vec![],
                    ..Default::default()
                }],
                ..Default::default()
            }),
            ..Default::default()
        };
        let html = render_document_html(&doc, &reg, &tokens).unwrap();
        assert!(html.contains(r#"<details open="open">"#));
        assert!(html.contains("<summary>FAQ</summary>"));
        assert!(html.contains("<p>answer</p>"));
    }

    #[test]
    fn accordion_closed_by_default() {
        let (reg, tokens) = setup();
        let doc = BuilderDocument {
            root: Some(Node {
                id: "a".into(),
                component: "accordion".into(),
                props: json!({ "title": "Closed" }),
                children: vec![],
                ..Default::default()
            }),
            ..Default::default()
        };
        let html = render_document_html(&doc, &reg, &tokens).unwrap();
        assert!(html.starts_with("<details>"));
        assert!(!html.contains("open="));
    }

    #[test]
    fn form_with_input_and_button() {
        let (reg, tokens) = setup();
        let doc = BuilderDocument {
            root: Some(Node {
                id: "f".into(),
                component: "form".into(),
                props: json!({}),
                children: vec![
                    Node {
                        id: "i".into(),
                        component: "input".into(),
                        props: json!({ "name": "email", "type": "email" }),
                        children: vec![],
                        ..Default::default()
                    },
                    Node {
                        id: "b".into(),
                        component: "button".into(),
                        props: json!({ "text": "Send" }),
                        children: vec![],
                        ..Default::default()
                    },
                ],
                ..Default::default()
            }),
            ..Default::default()
        };
        let html = render_document_html(&doc, &reg, &tokens).unwrap();
        assert!(html.starts_with(r#"<form method="post">"#));
        assert!(html.contains(r#"<button type="submit">Send</button>"#));
    }
}
