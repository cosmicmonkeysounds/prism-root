//! Translation from `BuilderDocument` → `prism_ui_runtime::layout::Node`.
//!
//! This is the Phase 1 seam from the Clay migration plan
//! (`docs/dev/clay-migration-plan.md` §3): the existing typed
//! `BuilderDocument` tree is the runtime representation that feeds the
//! new layout engine. The translator is intentionally narrow — it
//! covers the structural primitives (containers + text + spacers) plus
//! the subset of `StyleProperties` and `FlowProps` that map cleanly
//! onto Clay's vocabulary. Anything richer (modifiers, transforms,
//! grid placement, facets) is dropped on the floor for now and lands
//! in later phases as the runtime grows the matching primitives.
//!
//! Direction of dependency is **builder → runtime**, never the other
//! way: `prism-ui-runtime` ships under `MIT OR Apache-2.0` and stays
//! free of any builder-side knowledge so the post-cutover crate graph
//! is clean. The translator lives here, in the GPL-3 builder, until
//! Phase 5 collapses the two render paths.

use prism_ui_runtime::command::{Color, CornerRadius, RenderCommand};
use prism_ui_runtime::layout::{
    compute, ContainerProps, Direction, Node as UiNode, Padding, Sizing, TextProps, Viewport,
};

use crate::document::{BuilderDocument, Node};
use crate::layout::{Dimension, FlexDirection, FlowProps, LayoutMode};
use crate::style::{resolve_cascade, StyleProperties};

/// Translate a whole document. Returns `None` if the document has no
/// root — caller decides whether that's fatal.
pub fn document_to_ui_tree(doc: &BuilderDocument) -> Option<UiNode> {
    let root = doc.root.as_ref()?;
    Some(translate_node(root, &StyleProperties::default()))
}

/// End-to-end: `BuilderDocument` → `UiNode` → Taffy layout pass →
/// render-command stream. The single chokepoint Phase 3 wires the
/// shell, web build, and relay through. Empty docs return an empty
/// stream so callers can lower it unconditionally.
pub fn render_commands(doc: &BuilderDocument, viewport: Viewport) -> Vec<RenderCommand> {
    let Some(tree) = document_to_ui_tree(doc) else {
        return Vec::new();
    };
    compute(&tree, viewport)
}

/// `BuilderDocument` → HTML/CSS string via the unified pipeline. This
/// is the function `prism-relay` will call once the Phase 5 cutover
/// retires `Component::render_html` + `HtmlRegistry`. Available now so
/// the relay can switch incrementally during Phase 3.
pub fn lower_html(doc: &BuilderDocument, viewport: Viewport) -> String {
    let cmds = render_commands(doc, viewport);
    prism_ui_runtime::backends::html::lower(&cmds)
}

/// Translate a single node against an inherited style cascade.
///
/// Cascade rule: callers pass the parent-level resolved style; we
/// merge the node's own style on top before lowering. This mirrors
/// `style::resolve_cascade` but operates two-level (parent + node)
/// because the page/app layers are folded in at the top of the walk.
pub fn translate_node(node: &Node, parent_style: &StyleProperties) -> UiNode {
    let style = resolve_cascade(parent_style, &StyleProperties::default(), &node.style);

    if is_text_component(&node.component) {
        return translate_text(node, &style);
    }
    if node.component == "spacer" {
        return translate_spacer(node);
    }

    translate_container(node, &style)
}

fn is_text_component(component: &str) -> bool {
    matches!(
        component,
        "text" | "heading" | "label" | "link" | "code" | "paragraph"
    )
}

fn translate_text(node: &Node, style: &StyleProperties) -> UiNode {
    let content = text_content(node).unwrap_or_default();
    let font_size = style
        .font_size
        .unwrap_or_else(|| default_font_size_for(&node.component));
    let color = style
        .color
        .as_deref()
        .and_then(parse_color)
        .unwrap_or(Color {
            r: 20,
            g: 20,
            b: 20,
            a: 255,
        });
    UiNode::Text {
        id: node.id.clone(),
        content,
        props: TextProps { font_size, color },
    }
}

fn default_font_size_for(component: &str) -> f32 {
    match component {
        "heading" => 24.0,
        "code" => 13.0,
        _ => 14.0,
    }
}

fn text_content(node: &Node) -> Option<String> {
    let value = node
        .props
        .get("text")
        .or_else(|| node.props.get("content"))?;
    value.as_str().map(str::to_owned)
}

fn translate_spacer(node: &Node) -> UiNode {
    let width = node
        .props
        .get("width")
        .and_then(|v| v.as_f64())
        .map(|f| f as f32)
        .unwrap_or(0.0);
    let height = node
        .props
        .get("height")
        .and_then(|v| v.as_f64())
        .map(|f| f as f32)
        .unwrap_or(0.0);
    UiNode::Spacer {
        id: node.id.clone(),
        width,
        height,
    }
}

fn translate_container(node: &Node, style: &StyleProperties) -> UiNode {
    let flow = match &node.layout_mode {
        LayoutMode::Flow(f) | LayoutMode::Relative(f) => Some(f),
        _ => None,
    };
    let props = container_props(flow, style);

    let children = node
        .children
        .iter()
        .map(|c| translate_node(c, style))
        .collect();

    UiNode::Container {
        id: node.id.clone(),
        props,
        children,
    }
}

fn container_props(flow: Option<&FlowProps>, style: &StyleProperties) -> ContainerProps {
    let direction = flow
        .map(|f| match f.flex_direction {
            FlexDirection::Row | FlexDirection::RowReverse => Direction::Row,
            FlexDirection::Column | FlexDirection::ColumnReverse => Direction::Column,
        })
        .unwrap_or_default();

    let gap = flow.map(|f| f.gap).unwrap_or(0.0);

    let padding = flow
        .map(|f| Padding {
            left: f.padding.left,
            right: f.padding.right,
            top: f.padding.top,
            bottom: f.padding.bottom,
        })
        .unwrap_or_default();

    let width = flow
        .map(|f| sizing_from_dimension(f.width, f.flex_grow))
        .unwrap_or_default();
    let height = flow
        .map(|f| sizing_from_dimension(f.height, f.flex_grow))
        .unwrap_or_default();

    let background = style.background.as_deref().and_then(parse_color);
    let radius = style
        .border_radius
        .map(|r| CornerRadius {
            tl: r,
            tr: r,
            br: r,
            bl: r,
        })
        .unwrap_or_default();

    ContainerProps {
        direction,
        gap,
        padding,
        width,
        height,
        background,
        radius,
    }
}

fn sizing_from_dimension(dim: Dimension, flex_grow: f32) -> Sizing {
    match dim {
        Dimension::Px { value } => Sizing::Fixed(value),
        Dimension::Auto if flex_grow > 0.0 => Sizing::Grow,
        // Clay has no first-class percentage today; the percentage
        // case collapses to Grow as a best-effort. Real Clay handles
        // percentages natively once the C binding is wired up.
        Dimension::Percent { .. } => Sizing::Grow,
        Dimension::Auto => Sizing::Fit,
    }
}

/// Tiny CSS-color parser — `#rgb`, `#rrggbb`, `#rrggbbaa`. Anything
/// else returns `None` and the caller falls back to a default. This
/// keeps the translator self-contained; richer parsing (named colours,
/// `rgb(...)`, `oklch(...)`) lives in the design-tokens module and
/// will replace this once the cascade/token wiring lands.
fn parse_color(s: &str) -> Option<Color> {
    let s = s.trim();
    let hex = s.strip_prefix('#')?;
    let bytes = match hex.len() {
        3 => {
            let r = u8::from_str_radix(&hex[0..1], 16).ok()?;
            let g = u8::from_str_radix(&hex[1..2], 16).ok()?;
            let b = u8::from_str_radix(&hex[2..3], 16).ok()?;
            [r * 17, g * 17, b * 17, 255]
        }
        6 => {
            let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
            [r, g, b, 255]
        }
        8 => {
            let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
            let a = u8::from_str_radix(&hex[6..8], 16).ok()?;
            [r, g, b, a]
        }
        _ => return None,
    };
    Some(Color {
        r: bytes[0],
        g: bytes[1],
        b: bytes[2],
        a: bytes[3],
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::{FlowDisplay, FlowProps, LayoutMode};
    use prism_core::foundation::geometry::Edges;
    use serde_json::json;

    fn flow(direction: FlexDirection, gap: f32) -> LayoutMode {
        LayoutMode::Flow(FlowProps {
            display: FlowDisplay::Flex,
            flex_direction: direction,
            gap,
            ..Default::default()
        })
    }

    #[test]
    fn empty_doc_translates_to_none() {
        let doc = BuilderDocument::default();
        assert!(document_to_ui_tree(&doc).is_none());
    }

    #[test]
    fn heading_becomes_text_node_with_default_font_size() {
        let node = Node {
            id: "h".into(),
            component: "heading".into(),
            props: json!({ "text": "Hello" }),
            ..Default::default()
        };
        match translate_node(&node, &StyleProperties::default()) {
            UiNode::Text { content, props, .. } => {
                assert_eq!(content, "Hello");
                assert_eq!(props.font_size, 24.0);
            }
            other => panic!("expected Text, got {other:?}"),
        }
    }

    #[test]
    fn container_with_flow_props_carries_direction_and_gap() {
        let node = Node {
            id: "c".into(),
            component: "container".into(),
            layout_mode: flow(FlexDirection::Row, 8.0),
            ..Default::default()
        };
        match translate_node(&node, &StyleProperties::default()) {
            UiNode::Container { props, .. } => {
                assert_eq!(props.direction, Direction::Row);
                assert_eq!(props.gap, 8.0);
            }
            other => panic!("expected Container, got {other:?}"),
        }
    }

    #[test]
    fn style_cascade_resolves_color_from_parent() {
        let parent_style = StyleProperties {
            color: Some("#112233".into()),
            ..Default::default()
        };
        let node = Node {
            id: "t".into(),
            component: "text".into(),
            props: json!({ "text": "hi" }),
            ..Default::default()
        };
        match translate_node(&node, &parent_style) {
            UiNode::Text { props, .. } => {
                assert_eq!(props.color.r, 0x11);
                assert_eq!(props.color.g, 0x22);
                assert_eq!(props.color.b, 0x33);
            }
            other => panic!("expected Text, got {other:?}"),
        }
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
        match translate_node(&node, &StyleProperties::default()) {
            UiNode::Container { props, .. } => {
                assert_eq!(props.padding.top, 4.0);
                assert_eq!(props.padding.left, 8.0);
            }
            other => panic!("expected Container, got {other:?}"),
        }
    }

    #[test]
    fn full_document_round_trips_through_translation() {
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
                        component: "heading".into(),
                        props: json!({ "text": "Prism" }),
                        ..Default::default()
                    },
                    Node {
                        id: "spacer".into(),
                        component: "spacer".into(),
                        props: json!({ "width": 16, "height": 8 }),
                        ..Default::default()
                    },
                ],
                ..Default::default()
            }),
            ..Default::default()
        };
        let tree = document_to_ui_tree(&doc).expect("root present");
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
    fn unknown_component_falls_back_to_container() {
        let node = Node {
            id: "x".into(),
            component: "image".into(),
            ..Default::default()
        };
        assert!(matches!(
            translate_node(&node, &StyleProperties::default()),
            UiNode::Container { .. }
        ));
    }
}
