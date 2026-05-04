//! HTML SSR integration tests for the starter catalog.
//!
//! Historically this module owned a parallel `register_html_builtins`
//! function that registered the same 16 builtins into an
//! [`HtmlRegistry`]. The unified [`crate::starter::register_builtins`]
//! now seeds both registries in lockstep, so the only thing left here
//! is the HTML output test suite — kept as a sibling module so the
//! Slint and HTML test surfaces stay symmetric.

#[cfg(test)]
mod tests {
    use crate::document::{BuilderDocument, Node};
    use crate::html_block::HtmlRegistry;
    use crate::registry::ComponentRegistry;
    use crate::render::render_document_html;
    use crate::starter::register_builtins;
    use prism_core::design_tokens::DesignTokens;
    use serde_json::json;

    fn setup() -> (HtmlRegistry, DesignTokens) {
        let mut comp = ComponentRegistry::new();
        let mut html = HtmlRegistry::new();
        register_builtins(&mut comp, &mut html).expect("register builtins");
        (html, DesignTokens::default())
    }

    #[test]
    fn text_heading_renders_correct_tag() {
        let (reg, tokens) = setup();
        let doc = BuilderDocument {
            root: Some(Node {
                id: "n".into(),
                component: "text".into(),
                props: json!({ "body": "Prism", "level": "h3" }),
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
    fn text_link_renders_anchor() {
        let (reg, tokens) = setup();
        let doc = BuilderDocument {
            root: Some(Node {
                id: "n".into(),
                component: "text".into(),
                props: json!({ "body": "Click", "href": "/foo" }),
                children: vec![],
                ..Default::default()
            }),
            ..Default::default()
        };
        assert_eq!(
            render_document_html(&doc, &reg, &tokens).unwrap(),
            "<p><a href=\"/foo\">Click</a></p>"
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
                        component: "text".into(),
                        props: json!({ "body": "A", "level": "h2" }),
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
                component: "text".into(),
                props: json!({ "body": "<script>alert(1)</script>", "level": "h1" }),
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
    fn register_html_builtins_seeds_sixteen_blocks() {
        let (reg, _) = setup();
        for id in [
            "text",
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
            "facet",
        ] {
            assert!(reg.get(id).is_some(), "missing html block: {id}");
        }
    }

    #[test]
    fn card_renders_via_prefab() {
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
        assert!(html.contains("<section"));
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
