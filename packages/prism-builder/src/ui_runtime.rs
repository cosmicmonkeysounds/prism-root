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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block::register_block;
    use crate::document::Node;
    use crate::html_block::HtmlRegistry;
    use crate::layout::{FlexDirection, FlowDisplay, FlowProps, LayoutMode};
    use crate::starter::{SpacerBlock, TextBlock};
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
        let mut html = HtmlRegistry::new();
        register_block(
            &mut comps,
            &mut html,
            Arc::new(TextBlock { id: text_id.into() }),
        )
        .unwrap();
        register_block(
            &mut comps,
            &mut html,
            Arc::new(SpacerBlock {
                id: spacer_id.into(),
            }),
        )
        .unwrap();
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
