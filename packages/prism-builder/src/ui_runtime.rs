//! `BuilderDocument` → `prism_ui_runtime::layout::Node` translator.
//!
//! Phase-3 seam from the Clay/Taffy migration plan
//! (`docs/dev/clay-migration-plan.md` §3, §6). The walker no longer
//! string-matches on `node.component` to decide how to lower it —
//! every component implements `Component::lower_ui`, and this module
//! is just "look the component up in the registry, hand it a
//! [`crate::ui_lower::LowerCtx`], let it produce a `UiNode`". Built-in
//! lowerings live with their `Block` impls in `crate::starter`; the
//! shared helpers (`container_props_from`, `parse_color`, `text_node`,
//! `spacer_node`) live in `crate::ui_lower` so blocks compose them
//! without duplicating cascade / colour / sizing logic.
//!
//! Direction of dependency stays **builder → runtime** — the runtime
//! crate (`MIT OR Apache-2.0`) never learns anything about the
//! builder. The translator and registry indirection live here, in the
//! GPL-3 builder, until Phase 5 collapses the two render paths.
//!
//! Both registry-aware and registry-less APIs are kept during the
//! parallel-build period. Registry-less callers (`document_to_ui_tree`,
//! `render_commands`, `lower_html`) get the generic container fallback
//! for every node — useful for raw-fixture tests that don't want to
//! seed a registry. Registry-aware variants
//! (`*_with_registry`) dispatch each node through its block's
//! `lower_ui`, which is the path Phase 5 promotes to the only path.

use prism_ui_runtime::command::RenderCommand;
use prism_ui_runtime::layout::{compute, Node as UiNode, Viewport};

use crate::document::BuilderDocument;
use crate::registry::ComponentRegistry;
use crate::style::StyleProperties;
use crate::ui_lower::LowerCtx;

/// Translate a whole document with no registry — every node falls
/// through to the generic container lowering. Returns `None` if the
/// document has no root.
pub fn document_to_ui_tree(doc: &BuilderDocument) -> Option<UiNode> {
    let root = doc.root.as_ref()?;
    let parent = StyleProperties::default();
    let ctx = LowerCtx::new(None, &parent);
    Some(ctx.lower(root))
}

/// Registry-aware translation — each node is dispatched to its
/// `Component::lower_ui` impl. Unknown component ids fall through to
/// the same generic container the registry-less path produces.
pub fn document_to_ui_tree_with_registry(
    doc: &BuilderDocument,
    registry: &ComponentRegistry,
) -> Option<UiNode> {
    let root = doc.root.as_ref()?;
    let parent = StyleProperties::default();
    let ctx = LowerCtx::new(Some(registry), &parent);
    Some(ctx.lower(root))
}

/// End-to-end registry-less pipeline: document → tree → Taffy layout
/// → render-command stream. Empty docs return an empty stream so
/// callers can lower unconditionally.
pub fn render_commands(doc: &BuilderDocument, viewport: Viewport) -> Vec<RenderCommand> {
    let Some(tree) = document_to_ui_tree(doc) else {
        return Vec::new();
    };
    compute(&tree, viewport)
}

/// Registry-aware variant of [`render_commands`] — Phase 5 promotes
/// this to the canonical entry point.
pub fn render_commands_with_registry(
    doc: &BuilderDocument,
    registry: &ComponentRegistry,
    viewport: Viewport,
) -> Vec<RenderCommand> {
    let Some(tree) = document_to_ui_tree_with_registry(doc, registry) else {
        return Vec::new();
    };
    compute(&tree, viewport)
}

/// `BuilderDocument` → HTML/CSS via the unified pipeline. The relay
/// switches to this once the Phase 5 cutover retires
/// `Component::render_html` + `HtmlRegistry`. Registry-less variant.
pub fn lower_html(doc: &BuilderDocument, viewport: Viewport) -> String {
    let cmds = render_commands(doc, viewport);
    prism_ui_runtime::backends::html::lower(&cmds)
}

/// Registry-aware HTML lowering — what the relay calls post-cutover.
pub fn lower_html_with_registry(
    doc: &BuilderDocument,
    registry: &ComponentRegistry,
    viewport: Viewport,
) -> String {
    let cmds = render_commands_with_registry(doc, registry, viewport);
    prism_ui_runtime::backends::html::lower(&cmds)
}

/// `BuilderDocument` → **semantic** HTML via the unified pipeline.
/// Walks the typed `Node` tree directly (no layout pass, no render
/// commands) and emits SEO/accessibility-friendly markup driven by
/// each block's `Semantic` hint declarations. This is the entry
/// point that obsoletes `Component::render_html` + `HtmlRegistry`
/// for SSR — every block's `lower_ui` impl is the single source of
/// truth for both layout vocabulary and HTML flavour.
pub fn lower_semantic_html(doc: &BuilderDocument) -> String {
    document_to_ui_tree(doc)
        .map(|tree| prism_ui_runtime::backends::semantic_html::lower(&tree))
        .unwrap_or_default()
}

/// Registry-aware semantic HTML lowering — what the relay calls
/// post-cutover. Picks up every block's `lower_ui` impl, including
/// its `Semantic` declarations.
pub fn lower_semantic_html_with_registry(
    doc: &BuilderDocument,
    registry: &ComponentRegistry,
) -> String {
    document_to_ui_tree_with_registry(doc, registry)
        .map(|tree| prism_ui_runtime::backends::semantic_html::lower(&tree))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block::register_block;
    use crate::document::Node;
    use crate::layout::{FlexDirection, FlowDisplay, FlowProps, LayoutMode};
    use crate::starter::{
        AccordionBlock, ButtonBlock, CodeBlock, ColumnsBlock, ContainerBlock, DividerBlock,
        FormBlock, ImageBlock, InputBlock, ListBlock, SpacerBlock, TableBlock, TabsBlock,
        TextBlock,
    };
    use prism_core::foundation::geometry::Edges;
    use prism_ui_runtime::layout::Direction;
    use serde_json::json;
    use std::sync::Arc;

    fn flow(direction: FlexDirection, gap: f32) -> LayoutMode {
        LayoutMode::Flow(FlowProps {
            display: FlowDisplay::Flex,
            flex_direction: direction,
            gap,
            ..Default::default()
        })
    }

    fn registry_with(text_id: &str, spacer_id: &str) -> ComponentRegistry {
        let mut comps = ComponentRegistry::new();
        register_block(&mut comps, Arc::new(TextBlock { id: text_id.into() })).unwrap();
        register_block(
            &mut comps,
            Arc::new(SpacerBlock {
                id: spacer_id.into(),
            }),
        )
        .unwrap();
        comps
    }

    /// Full builtin-flavoured registry covering the blocks with
    /// dedicated `lower_ui` impls. Phase 3 progress is gated on this
    /// stack producing the expected runtime nodes. New blocks join
    /// the table below — no per-block registration boilerplate.
    fn full_registry() -> ComponentRegistry {
        let mut comps = ComponentRegistry::new();
        // Macro keeps registration declarative — each row is just
        // `(component-id, BlockType)`. Adding a block is one line.
        macro_rules! register_all {
            ($($id:literal => $ty:ident),* $(,)?) => {
                $(register_block(&mut comps, Arc::new($ty { id: $id.into() })).unwrap();)*
            };
        }
        register_all! {
            "text" => TextBlock,
            "spacer" => SpacerBlock,
            "columns" => ColumnsBlock,
            "list" => ListBlock,
            "container" => ContainerBlock,
            "divider" => DividerBlock,
            "code" => CodeBlock,
            "button" => ButtonBlock,
            "form" => FormBlock,
            "input" => InputBlock,
            "table" => TableBlock,
            "tabs" => TabsBlock,
            "accordion" => AccordionBlock,
            "image" => ImageBlock,
        }
        comps
    }

    #[test]
    fn empty_doc_translates_to_none() {
        let doc = BuilderDocument::default();
        assert!(document_to_ui_tree(&doc).is_none());
    }

    #[test]
    fn registry_dispatch_lowers_text_block_via_block_impl() {
        let reg = registry_with("text", "spacer");
        let node = Node {
            id: "h".into(),
            component: "text".into(),
            props: json!({ "body": "Hello", "level": "h2" }),
            ..Default::default()
        };
        let doc = BuilderDocument {
            root: Some(node),
            ..Default::default()
        };
        let tree = document_to_ui_tree_with_registry(&doc, &reg).expect("root present");
        match tree {
            UiNode::Text { content, props, .. } => {
                assert_eq!(content, "Hello");
                // h2 default size = 26.0 (from level_font_size).
                assert_eq!(props.font_size, 26.0);
            }
            other => panic!("expected Text, got {other:?}"),
        }
    }

    #[test]
    fn legacy_text_prop_still_works_via_text_block_lowering() {
        let reg = registry_with("text", "spacer");
        let node = Node {
            id: "t".into(),
            component: "text".into(),
            props: json!({ "text": "Old fixture" }),
            ..Default::default()
        };
        let doc = BuilderDocument {
            root: Some(node),
            ..Default::default()
        };
        let tree = document_to_ui_tree_with_registry(&doc, &reg).expect("root present");
        let UiNode::Text { content, .. } = tree else {
            panic!("expected Text");
        };
        assert_eq!(content, "Old fixture");
    }

    #[test]
    fn spacer_block_lowering_reads_height_from_schema() {
        let reg = registry_with("text", "spacer");
        let node = Node {
            id: "s".into(),
            component: "spacer".into(),
            props: json!({ "height": 32 }),
            ..Default::default()
        };
        let doc = BuilderDocument {
            root: Some(node),
            ..Default::default()
        };
        let tree = document_to_ui_tree_with_registry(&doc, &reg).expect("root present");
        let UiNode::Spacer { height, .. } = tree else {
            panic!("expected Spacer");
        };
        assert_eq!(height, 32.0);
    }

    #[test]
    fn unknown_component_falls_back_to_container() {
        let reg = registry_with("text", "spacer");
        let node = Node {
            id: "x".into(),
            component: "image".into(),
            ..Default::default()
        };
        let doc = BuilderDocument {
            root: Some(node),
            ..Default::default()
        };
        let tree = document_to_ui_tree_with_registry(&doc, &reg).expect("root present");
        assert!(matches!(tree, UiNode::Container { .. }));
    }

    #[test]
    fn registry_less_path_falls_through_to_container_for_every_node() {
        // Without a registry, even `component = "text"` lowers as a
        // container — this is the "no registry seeded" parity path
        // for raw fixtures during the migration.
        let node = Node {
            id: "t".into(),
            component: "text".into(),
            props: json!({ "body": "hi" }),
            ..Default::default()
        };
        let doc = BuilderDocument {
            root: Some(node),
            ..Default::default()
        };
        let tree = document_to_ui_tree(&doc).expect("root present");
        assert!(matches!(tree, UiNode::Container { .. }));
    }

    #[test]
    fn container_with_flow_props_carries_direction_and_gap() {
        let node = Node {
            id: "c".into(),
            component: "container".into(),
            layout_mode: flow(FlexDirection::Row, 8.0),
            ..Default::default()
        };
        let doc = BuilderDocument {
            root: Some(node),
            ..Default::default()
        };
        let tree = document_to_ui_tree(&doc).expect("root present");
        match tree {
            UiNode::Container { props, .. } => {
                assert_eq!(props.direction, Direction::Row);
                assert_eq!(props.gap, 8.0);
            }
            other => panic!("expected Container, got {other:?}"),
        }
    }

    #[test]
    fn style_cascade_resolves_color_from_parent() {
        let reg = registry_with("text", "spacer");
        let parent = Node {
            id: "root".into(),
            component: "container".into(),
            style: StyleProperties {
                color: Some("#112233".into()),
                ..Default::default()
            },
            children: vec![Node {
                id: "t".into(),
                component: "text".into(),
                props: json!({ "body": "hi" }),
                ..Default::default()
            }],
            ..Default::default()
        };
        let doc = BuilderDocument {
            root: Some(parent),
            ..Default::default()
        };
        let tree = document_to_ui_tree_with_registry(&doc, &reg).expect("root present");
        let UiNode::Container { children, .. } = tree else {
            panic!("expected container root");
        };
        let UiNode::Text { props, .. } = &children[0] else {
            panic!("expected Text child");
        };
        assert_eq!(props.color.r, 0x11);
        assert_eq!(props.color.g, 0x22);
        assert_eq!(props.color.b, 0x33);
    }

    #[test]
    fn padding_propagates_from_flow_props() {
        let mut flow_props = FlowProps {
            display: FlowDisplay::Flex,
            flex_direction: FlexDirection::Column,
            ..Default::default()
        };
        flow_props.padding = Edges::new(4.0, 8.0, 4.0, 8.0);
        let node = Node {
            id: "c".into(),
            component: "container".into(),
            layout_mode: LayoutMode::Flow(flow_props),
            ..Default::default()
        };
        let doc = BuilderDocument {
            root: Some(node),
            ..Default::default()
        };
        let tree = document_to_ui_tree(&doc).expect("root present");
        match tree {
            UiNode::Container { props, .. } => {
                assert_eq!(props.padding.top, 4.0);
                assert_eq!(props.padding.left, 8.0);
            }
            other => panic!("expected Container, got {other:?}"),
        }
    }

    #[test]
    fn render_commands_walks_full_pipeline() {
        let doc = BuilderDocument {
            root: Some(Node {
                id: "root".into(),
                component: "container".into(),
                layout_mode: flow(FlexDirection::Column, 0.0),
                style: StyleProperties {
                    background: Some("#ffffff".into()),
                    ..Default::default()
                },
                ..Default::default()
            }),
            ..Default::default()
        };
        let cmds = render_commands(
            &doc,
            Viewport {
                width: 320.0,
                height: 240.0,
            },
        );
        assert!(!cmds.is_empty(), "non-empty doc must emit commands");
    }

    #[test]
    fn lower_semantic_html_emits_meaningful_tags_per_block() {
        let reg = full_registry();
        // text(level=h2) → <h2>; image with alt → <img alt=…>;
        // form → <form method=get>; list(ordered) → <ol>.
        let doc = BuilderDocument {
            root: Some(Node {
                id: "root".into(),
                component: "form".into(),
                props: json!({ "method": "get" }),
                children: vec![
                    Node {
                        id: "title".into(),
                        component: "text".into(),
                        props: json!({ "level": "h2", "body": "Sign up" }),
                        ..Default::default()
                    },
                    Node {
                        id: "logo".into(),
                        component: "image".into(),
                        props: json!({ "src": "/asset/abc", "alt": "Logo" }),
                        ..Default::default()
                    },
                    Node {
                        id: "items".into(),
                        component: "list".into(),
                        props: json!({ "ordered": true }),
                        ..Default::default()
                    },
                ],
                ..Default::default()
            }),
            ..Default::default()
        };
        let html = lower_semantic_html_with_registry(&doc, &reg);
        assert!(html.contains("<form "));
        assert!(html.contains("method=\"get\""));
        assert!(html.contains("<h2"));
        assert!(html.contains(">Sign up</h2>"));
        assert!(html.contains("<img src=\"/asset/abc\""));
        assert!(html.contains("alt=\"Logo\""));
        assert!(html.contains("<ol"));
        assert!(html.ends_with("</form>"));
    }

    #[test]
    fn lower_semantic_html_covers_the_phase3_block_catalog() {
        // Every non-prefab built-in declares its semantic shape inside
        // its own `lower_ui` impl — no walker fan-out, no per-block
        // fork in the relay. This test pins the contract: each block
        // emits the SSR markup `render_html` would have, just by
        // setting `props.semantic` declaratively in the lowering.
        let reg = full_registry();
        let mk = |id: &str, comp: &str, props: serde_json::Value, children: Vec<Node>| Node {
            id: id.into(),
            component: comp.into(),
            props,
            children,
            ..Default::default()
        };
        let doc = BuilderDocument {
            root: Some(mk(
                "root",
                "container",
                json!({}),
                vec![
                    mk("btn", "button", json!({ "text": "Save" }), vec![]),
                    mk(
                        "btn-link",
                        "button",
                        json!({ "text": "Help", "href": "/help" }),
                        vec![],
                    ),
                    mk("hr", "divider", json!({}), vec![]),
                    mk(
                        "code",
                        "code",
                        json!({ "code": "print(1)", "language": "lua" }),
                        vec![],
                    ),
                    mk(
                        "in",
                        "input",
                        json!({ "name": "email", "type": "email", "label": "Email" }),
                        vec![],
                    ),
                    mk("tab", "tabs", json!({ "labels": "One, Two" }), vec![]),
                    mk(
                        "acc",
                        "accordion",
                        json!({ "title": "More", "open": true }),
                        vec![],
                    ),
                    mk(
                        "tbl",
                        "table",
                        json!({ "headers": "Name, Email", "caption": "Users" }),
                        vec![],
                    ),
                ],
            )),
            ..Default::default()
        };
        let html = lower_semantic_html_with_registry(&doc, &reg);

        // Button (paired) and anchor variant.
        assert!(html.contains("<button"));
        assert!(html.contains("type=\"submit\""));
        // The button label is a child text node, so the closing
        // `</button>` lives after the inner text element — assert it
        // wraps the label rather than expecting an exact slice.
        assert!(html.contains("Save"));
        assert!(html.contains("</button>"));
        assert!(html.contains("<a"));
        assert!(html.contains("href=\"/help\""));
        // Divider — void.
        assert!(html.contains("<hr"));
        assert!(!html.contains("</hr>"));
        // Code — `<pre><code class="language-lua">`.
        assert!(html.contains("<pre"));
        assert!(html.contains("<code class=\"language-lua\""));
        assert!(html.contains("print(1)"));
        assert!(html.contains("</code>"));
        assert!(html.contains("</pre>"));
        // Input — outer `<label>`, inner `<input>` void with attrs.
        assert!(html.contains("<label"));
        assert!(html.contains("<input"));
        assert!(html.contains("type=\"email\""));
        assert!(html.contains("name=\"email\""));
        // Tabs — role=tablist + role=tab buttons + role=tabpanel.
        assert!(html.contains("role=\"tablist\""));
        assert!(html.contains("role=\"tab\""));
        assert!(html.contains("aria-selected=\"true\""));
        assert!(html.contains("role=\"tabpanel\""));
        // Accordion — `<details open>` with `<summary>`.
        assert!(html.contains("<details"));
        assert!(html.contains("open=\"open\""));
        assert!(html.contains("<summary"));
        // Table — `<table>`/`<caption>`/`<thead>`/`<tr>`/`<th>`.
        assert!(html.contains("<table"));
        assert!(html.contains("<caption"));
        assert!(html.contains("<thead"));
        assert!(html.contains("<tr"));
        assert!(html.contains("<th"));
    }

    #[test]
    fn lower_semantic_html_returns_empty_for_empty_doc() {
        assert_eq!(lower_semantic_html(&BuilderDocument::default()), "");
    }

    #[test]
    fn lower_html_round_trips_to_string() {
        let doc = BuilderDocument {
            root: Some(Node {
                id: "root".into(),
                component: "container".into(),
                layout_mode: flow(FlexDirection::Column, 0.0),
                style: StyleProperties {
                    background: Some("#abcdef".into()),
                    ..Default::default()
                },
                ..Default::default()
            }),
            ..Default::default()
        };
        let html = lower_html(
            &doc,
            Viewport {
                width: 100.0,
                height: 50.0,
            },
        );
        assert!(html.starts_with("<div"));
        assert!(html.contains("rgba(171,205,239"));
    }

    #[test]
    fn empty_doc_lowers_to_empty_root() {
        let html = lower_html(
            &BuilderDocument::default(),
            Viewport {
                width: 100.0,
                height: 50.0,
            },
        );
        assert!(html.starts_with("<div"));
        assert!(html.ends_with("</div>"));
    }

    #[test]
    fn columns_block_lowers_to_row_container() {
        let reg = full_registry();
        let node = Node {
            id: "c".into(),
            component: "columns".into(),
            props: json!({ "gap": 24 }),
            ..Default::default()
        };
        let doc = BuilderDocument {
            root: Some(node),
            ..Default::default()
        };
        let tree = document_to_ui_tree_with_registry(&doc, &reg).unwrap();
        let UiNode::Container { props, .. } = tree else {
            panic!("expected container");
        };
        assert_eq!(props.direction, Direction::Row);
        assert_eq!(props.gap, 24.0);
    }

    #[test]
    fn list_block_uses_item_spacing_as_gap() {
        let reg = full_registry();
        let node = Node {
            id: "l".into(),
            component: "list".into(),
            props: json!({ "item_spacing": 12 }),
            ..Default::default()
        };
        let doc = BuilderDocument {
            root: Some(node),
            ..Default::default()
        };
        let tree = document_to_ui_tree_with_registry(&doc, &reg).unwrap();
        let UiNode::Container { props, .. } = tree else {
            panic!("expected container");
        };
        assert_eq!(props.direction, Direction::Column);
        assert_eq!(props.gap, 12.0);
    }

    #[test]
    fn container_block_props_resolve_into_runtime_container() {
        let reg = full_registry();
        let node = Node {
            id: "c".into(),
            component: "container".into(),
            props: json!({ "spacing": 10, "padding": 16 }),
            ..Default::default()
        };
        let doc = BuilderDocument {
            root: Some(node),
            ..Default::default()
        };
        let tree = document_to_ui_tree_with_registry(&doc, &reg).unwrap();
        let UiNode::Container { props, .. } = tree else {
            panic!("expected container");
        };
        assert_eq!(props.gap, 10.0);
        assert_eq!(props.padding.left, 16.0);
        assert_eq!(props.padding.top, 16.0);
    }

    #[test]
    fn divider_block_lowers_to_one_pixel_bar() {
        let reg = full_registry();
        let node = Node {
            id: "d".into(),
            component: "divider".into(),
            ..Default::default()
        };
        let doc = BuilderDocument {
            root: Some(node),
            ..Default::default()
        };
        let tree = document_to_ui_tree_with_registry(&doc, &reg).unwrap();
        let UiNode::Container {
            props, children, ..
        } = tree
        else {
            panic!("expected container");
        };
        assert!(children.is_empty());
        assert!(matches!(
            props.height,
            prism_ui_runtime::layout::Sizing::Fixed(v) if v == 1.0
        ));
        assert!(props.background.is_some());
    }

    #[test]
    fn code_block_wraps_text_in_padded_container() {
        let reg = full_registry();
        let node = Node {
            id: "code".into(),
            component: "code".into(),
            props: json!({ "code": "fn main() {}" }),
            ..Default::default()
        };
        let doc = BuilderDocument {
            root: Some(node),
            ..Default::default()
        };
        let tree = document_to_ui_tree_with_registry(&doc, &reg).unwrap();
        let UiNode::Container {
            props, children, ..
        } = tree
        else {
            panic!("expected container");
        };
        assert_eq!(children.len(), 1);
        let UiNode::Text { content, .. } = &children[0] else {
            panic!("expected text child");
        };
        assert_eq!(content, "fn main() {}");
        assert!(props.background.is_some(), "code blocks have a backdrop");
        assert_eq!(props.padding.left, 12.0);
    }

    #[test]
    fn button_block_centers_label_in_container() {
        let reg = full_registry();
        let node = Node {
            id: "b".into(),
            component: "button".into(),
            props: json!({ "text": "Save" }),
            ..Default::default()
        };
        let doc = BuilderDocument {
            root: Some(node),
            ..Default::default()
        };
        let tree = document_to_ui_tree_with_registry(&doc, &reg).unwrap();
        let UiNode::Container {
            props, children, ..
        } = tree
        else {
            panic!("expected container");
        };
        assert_eq!(children.len(), 1);
        let UiNode::Text {
            content,
            props: tprops,
            ..
        } = &children[0]
        else {
            panic!("expected text child");
        };
        assert_eq!(content, "Save");
        assert_eq!(tprops.color.r, 0xff);
        assert!(matches!(
            props.height,
            prism_ui_runtime::layout::Sizing::Fixed(v) if v == 36.0
        ));
    }

    /// Build a single-node `BuilderDocument` and lower it through the
    /// full registry. Most lower_ui tests want the same skeleton —
    /// component id + props, single root, lower, expect a Container.
    /// Helper keeps each test focused on the assertions that matter.
    fn lower_single(component: &str, props: serde_json::Value) -> UiNode {
        let reg = full_registry();
        let doc = BuilderDocument {
            root: Some(Node {
                id: "root".into(),
                component: component.into(),
                props,
                ..Default::default()
            }),
            ..Default::default()
        };
        document_to_ui_tree_with_registry(&doc, &reg).unwrap()
    }

    fn expect_container(node: UiNode) -> (prism_ui_runtime::layout::ContainerProps, Vec<UiNode>) {
        match node {
            UiNode::Container {
                props, children, ..
            } => (props, children),
            _ => panic!("expected container"),
        }
    }

    #[test]
    fn form_block_lowers_to_vertical_container_with_gap_8() {
        let (props, _) = expect_container(lower_single("form", json!({})));
        assert_eq!(props.direction, Direction::Column);
        assert_eq!(props.gap, 8.0);
    }

    #[test]
    fn input_block_synthesises_label_and_field() {
        let (props, children) = expect_container(lower_single(
            "input",
            json!({ "label": "Email", "placeholder": "you@example.com" }),
        ));
        assert_eq!(props.gap, 4.0);
        // [label-text, field-rect-with-placeholder]
        assert_eq!(children.len(), 2);
        let UiNode::Text { content, .. } = &children[0] else {
            panic!("expected label text first");
        };
        assert_eq!(content, "Email");
        let UiNode::Container {
            props: field_props,
            children: field_kids,
            ..
        } = &children[1]
        else {
            panic!("expected field container");
        };
        assert!(matches!(
            field_props.height,
            prism_ui_runtime::layout::Sizing::Fixed(v) if v == 32.0
        ));
        assert!(field_props.background.is_some());
        let UiNode::Text { content, .. } = &field_kids[0] else {
            panic!("expected placeholder text");
        };
        assert_eq!(content, "you@example.com");
    }

    #[test]
    fn input_block_uses_ellipsis_when_placeholder_empty() {
        let (_, children) = expect_container(lower_single("input", json!({ "name": "email" })));
        // Without label, only the field row is emitted.
        assert_eq!(children.len(), 1);
        let UiNode::Container {
            children: field, ..
        } = &children[0]
        else {
            panic!();
        };
        let UiNode::Text { content, .. } = &field[0] else {
            panic!();
        };
        assert_eq!(content, "...");
    }

    #[test]
    fn table_block_lowers_to_caption_plus_header_row() {
        let (props, children) = expect_container(lower_single(
            "table",
            json!({ "headers": "Name, Age, Email", "caption": "Users" }),
        ));
        assert_eq!(props.padding.left, 8.0);
        assert!(props.radius.tl > 0.0);
        // [caption text, thead container > tr container > th cells]
        assert_eq!(children.len(), 2);
        let UiNode::Text { content, .. } = &children[0] else {
            panic!("caption first");
        };
        assert_eq!(content, "Users");
        let (_thead_props, thead_kids) = expect_container(children[1].clone());
        assert_eq!(thead_kids.len(), 1);
        let (row_props, cells) = expect_container(thead_kids[0].clone());
        assert_eq!(row_props.direction, Direction::Row);
        assert_eq!(row_props.gap, 16.0);
        assert_eq!(cells.len(), 3);
    }

    #[test]
    fn tabs_block_lowers_to_strip_and_panel() {
        let (_props, children) =
            expect_container(lower_single("tabs", json!({ "labels": "One, Two, Three" })));
        assert_eq!(children.len(), 2);
        let (strip_props, pills) = expect_container(children[0].clone());
        assert_eq!(strip_props.direction, Direction::Row);
        assert_eq!(pills.len(), 3);
        // First pill highlighted, others dim.
        let UiNode::Container {
            props: first_pill, ..
        } = &pills[0]
        else {
            panic!();
        };
        let UiNode::Container {
            props: second_pill, ..
        } = &pills[1]
        else {
            panic!();
        };
        assert_ne!(first_pill.background, second_pill.background);
        // Panel container is empty (no children passed in this fixture).
        let (panel_props, panel_kids) = expect_container(children[1].clone());
        assert_eq!(panel_props.padding.left, 12.0);
        assert!(panel_kids.is_empty());
    }

    #[test]
    fn image_block_lowers_to_image_node_with_url_source() {
        // External URL → straight pass-through. `to_html_src()` is the
        // single source of truth for the rendered string; both the SSR
        // walker and the runtime pipeline route through it, so no
        // duplication of resolution logic.
        let node = lower_single(
            "image",
            json!({ "src": "https://example.test/cat.png", "alt": "cat" }),
        );
        let UiNode::Image {
            source,
            width,
            height,
            ..
        } = node
        else {
            panic!("expected image, got {node:?}");
        };
        assert_eq!(source, "https://example.test/cat.png");
        assert!(matches!(width, prism_ui_runtime::layout::Sizing::Grow));
        assert!(matches!(height, prism_ui_runtime::layout::Sizing::Grow));
    }

    #[test]
    fn image_block_propagates_cascade_radius() {
        let reg = full_registry();
        let doc = BuilderDocument {
            root: Some(Node {
                id: "img".into(),
                component: "image".into(),
                props: json!({ "src": "https://example.test/x.png" }),
                style: StyleProperties {
                    border_radius: Some(8.0),
                    ..Default::default()
                },
                ..Default::default()
            }),
            ..Default::default()
        };
        let UiNode::Image { radius, .. } = document_to_ui_tree_with_registry(&doc, &reg).unwrap()
        else {
            panic!("expected image");
        };
        assert_eq!(radius.tl, 8.0);
        assert_eq!(radius.br, 8.0);
    }

    #[test]
    fn accordion_block_lowers_to_header_and_content() {
        let (props, children) = expect_container(lower_single(
            "accordion",
            json!({ "title": "Details", "section_gap": 6 }),
        ));
        assert_eq!(props.gap, 6.0);
        assert_eq!(children.len(), 2);
        let (header_props, header_kids) = expect_container(children[0].clone());
        assert!(matches!(
            header_props.height,
            prism_ui_runtime::layout::Sizing::Fixed(v) if v == 32.0
        ));
        let UiNode::Text { content, .. } = &header_kids[0] else {
            panic!();
        };
        assert!(content.contains("Details"));
        let (content_props, _) = expect_container(children[1].clone());
        assert_eq!(content_props.padding.left, 16.0);
    }

    #[test]
    fn full_document_round_trips_through_registry_dispatch() {
        let reg = registry_with("text", "spacer");
        let doc = BuilderDocument {
            root: Some(Node {
                id: "root".into(),
                component: "container".into(),
                layout_mode: flow(FlexDirection::Column, 16.0),
                style: StyleProperties {
                    background: Some("#ffffff".into()),
                    border_radius: Some(8.0),
                    ..Default::default()
                },
                children: vec![
                    Node {
                        id: "title".into(),
                        component: "text".into(),
                        props: json!({ "body": "Prism", "level": "h1" }),
                        ..Default::default()
                    },
                    Node {
                        id: "spacer".into(),
                        component: "spacer".into(),
                        props: json!({ "height": 8 }),
                        ..Default::default()
                    },
                ],
                ..Default::default()
            }),
            ..Default::default()
        };
        let tree = document_to_ui_tree_with_registry(&doc, &reg).expect("root present");
        let UiNode::Container {
            props, children, ..
        } = tree
        else {
            panic!("root must be container");
        };
        assert_eq!(children.len(), 2);
        assert_eq!(props.gap, 16.0);
        assert!(props.background.is_some());
        assert_eq!(props.radius.tl, 8.0);
        assert!(matches!(children[0], UiNode::Text { .. }));
        assert!(matches!(children[1], UiNode::Spacer { .. }));
    }
}
