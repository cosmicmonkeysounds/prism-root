//! Layout engine types and computation pass (ADR-003).
//!
//! Three concepts:
//!
//! - [`PageLayout`] — structural properties of a page (size, margins,
//!   bleed, CSS Grid template). Not a component you drag; a property
//!   of the page itself.
//! - [`LayoutMode`] — how a node participates in its parent's layout.
//!   `Flow` nodes are positioned by Taffy; `Free` nodes are positioned
//!   by their `Transform2D` alone.
//! - [`ComputedLayout`] — the output of the layout pass: a per-node
//!   map of resolved rectangles + composed transforms.


use std::collections::HashMap;

use glam::{Affine2, Vec2};
use prism_core::foundation::geometry::{Point2, Rect, Size2};
#[cfg(test)]
use prism_core::foundation::geometry::Edges;
use prism_core::foundation::spatial::ComputedTransform;
use taffy::{AvailableSpace, Position, Style, TaffyTree};

mod grid;
mod mode;

pub use grid::*;
pub use mode::*;

use crate::document::{BuilderDocument, Node, NodeId};

/// Per-node result of the layout + transform propagation pass.
#[derive(Debug, Clone)]
pub struct NodeLayout {
    pub rect: Rect,
    pub transform: ComputedTransform,
}

/// Output of [`compute_layout`] — maps node IDs to their resolved
/// rectangles and composed transforms.
#[derive(Debug, Clone, Default)]
pub struct ComputedLayout {
    pub page_size: Size2,
    pub nodes: HashMap<NodeId, NodeLayout>,
}

/// Run the Taffy layout pass on a document and propagate transforms.
///
/// `viewport_size` is used when `PageLayout::size` is `Responsive`.
pub fn compute_layout(doc: &BuilderDocument, viewport_size: Size2) -> ComputedLayout {
    let root = match &doc.root {
        Some(r) => r,
        None => return ComputedLayout::default(),
    };

    let page_layout = &doc.page_layout;
    let page_size = page_layout.resolved_size().unwrap_or(viewport_size);

    let mut tree: TaffyTree<NodeId> = TaffyTree::new();
    let mut taffy_map: HashMap<NodeId, taffy::NodeId> = HashMap::new();

    let page_style = page_layout.build_taffy_style(page_size);
    let content_rect = page_layout.content_rect(page_size);

    fn build_taffy_subtree(
        tree: &mut TaffyTree<NodeId>,
        map: &mut HashMap<NodeId, taffy::NodeId>,
        node: &Node,
    ) -> taffy::NodeId {
        let style = match &node.layout_mode {
            LayoutMode::Flow(flow) => flow.to_taffy_style(),
            LayoutMode::Free => Style {
                position: Position::Absolute,
                ..Default::default()
            },
            LayoutMode::Absolute(abs) => abs.to_taffy_style(),
            LayoutMode::Relative(flow) => {
                let mut style = flow.to_taffy_style();
                style.position = Position::Relative;
                style
            }
        };

        let child_taffy_ids: Vec<taffy::NodeId> = node
            .children
            .iter()
            .map(|child| build_taffy_subtree(tree, map, child))
            .collect();

        let taffy_node = tree
            .new_with_children(style, &child_taffy_ids)
            .expect("taffy node creation");
        tree.set_node_context(taffy_node, Some(node.id.clone()))
            .expect("set node context");
        map.insert(node.id.clone(), taffy_node);
        taffy_node
    }

    let root_children = vec![build_taffy_subtree(&mut tree, &mut taffy_map, root)];
    let page_node = tree
        .new_with_children(page_style, &root_children)
        .expect("page node");

    tree.compute_layout(
        page_node,
        taffy::Size {
            width: AvailableSpace::Definite(page_size.width),
            height: AvailableSpace::Definite(page_size.height),
        },
    )
    .expect("layout computation");

    let mut result = ComputedLayout {
        page_size,
        nodes: HashMap::new(),
    };

    #[allow(clippy::too_many_arguments)]
    fn collect_layouts(
        tree: &TaffyTree<NodeId>,
        taffy_map: &HashMap<NodeId, taffy::NodeId>,
        node: &Node,
        parent_offset: Vec2,
        parent_global: Affine2,
        parent_size: Vec2,
        content_offset: Vec2,
        result: &mut ComputedLayout,
    ) {
        let taffy_node = match taffy_map.get(&node.id) {
            Some(n) => *n,
            None => return,
        };

        let taffy_layout = tree.layout(taffy_node).expect("layout lookup");
        let mut layout_pos = Vec2::new(taffy_layout.location.x, taffy_layout.location.y)
            + parent_offset
            + content_offset;
        let layout_size = Vec2::new(taffy_layout.size.width, taffy_layout.size.height);

        // Anchor resolution for Absolute nodes: the transform.position
        // is an offset from the anchor point within the parent rect.
        if let LayoutMode::Absolute(_) = &node.layout_mode {
            let anchor_frac = node.transform.anchor.as_fraction();
            let anchor_origin = parent_size * anchor_frac;
            layout_pos = anchor_origin + node.transform.position_vec2() + content_offset;
        }

        let local_affine = node.transform.to_local_affine(layout_size);
        let position_affine = Affine2::from_translation(layout_pos);
        let full_local = position_affine * local_affine;
        let computed = ComputedTransform::new(full_local, parent_global);

        result.nodes.insert(
            node.id.clone(),
            NodeLayout {
                rect: Rect::from_origin_size(
                    Point2::new(layout_pos.x, layout_pos.y),
                    Size2::new(layout_size.x, layout_size.y),
                ),
                transform: computed,
            },
        );

        for child in &node.children {
            collect_layouts(
                tree,
                taffy_map,
                child,
                Vec2::ZERO,
                computed.global,
                layout_size,
                Vec2::ZERO,
                result,
            );
        }
    }

    let content_offset = Vec2::new(content_rect.x(), content_rect.y());
    let page_vec = Vec2::new(page_size.width, page_size.height);
    collect_layouts(
        &tree,
        &taffy_map,
        root,
        Vec2::ZERO,
        Affine2::IDENTITY,
        page_vec,
        content_offset,
        &mut result,
    );

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::{BuilderDocument, Node};
    use serde_json::json;

    fn make_node(id: &str) -> Node {
        Node {
            id: id.to_string(),
            component: "container".to_string(),
            props: json!({}),
            children: Vec::new(),
            ..Default::default()
        }
    }

    #[test]
    fn page_size_a4() {
        let s = PageSize::A4.to_pixels().unwrap();
        assert_eq!(s.width, 794.0);
        assert_eq!(s.height, 1123.0);
    }

    #[test]
    fn page_size_landscape() {
        let layout = PageLayout {
            size: PageSize::A4,
            orientation: Orientation::Landscape,
            ..Default::default()
        };
        let s = layout.resolved_size().unwrap();
        assert_eq!(s.width, 1123.0);
        assert_eq!(s.height, 794.0);
    }

    #[test]
    fn responsive_returns_none() {
        assert!(PageSize::Responsive.to_pixels().is_none());
    }

    #[test]
    fn content_rect_with_margins() {
        let layout = PageLayout {
            margins: Edges::all(20.0),
            ..Default::default()
        };
        let page = Size2::new(800.0, 600.0);
        let content = layout.content_rect(page);
        assert_eq!(content.width(), 760.0);
        assert_eq!(content.height(), 560.0);
        assert_eq!(content.x(), 20.0);
        assert_eq!(content.y(), 20.0);
    }

    #[test]
    fn bleed_rect_expands() {
        let layout = PageLayout {
            bleed: 10.0,
            ..Default::default()
        };
        let page = Size2::new(800.0, 600.0);
        let bleed = layout.bleed_rect(page);
        assert_eq!(bleed.x(), -10.0);
        assert_eq!(bleed.y(), -10.0);
        assert_eq!(bleed.width(), 820.0);
        assert_eq!(bleed.height(), 620.0);
    }

    #[test]
    fn dimension_serde_roundtrip() {
        let dims = vec![
            Dimension::Auto,
            Dimension::Px { value: 100.0 },
            Dimension::Percent { value: 50.0 },
        ];
        for d in &dims {
            let json = serde_json::to_string(d).unwrap();
            let d2: Dimension = serde_json::from_str(&json).unwrap();
            assert_eq!(*d, d2);
        }
    }

    #[test]
    fn layout_mode_default_is_flow() {
        let lm = LayoutMode::default();
        assert!(matches!(lm, LayoutMode::Flow(_)));
    }

    #[test]
    fn compute_layout_empty_doc() {
        let doc = BuilderDocument::default();
        let result = compute_layout(&doc, Size2::new(1280.0, 800.0));
        assert!(result.nodes.is_empty());
    }

    #[test]
    fn compute_layout_single_node() {
        let doc = BuilderDocument {
            root: Some(make_node("root")),
            ..Default::default()
        };
        let result = compute_layout(&doc, Size2::new(1280.0, 800.0));
        assert!(result.nodes.contains_key("root"));
    }

    #[test]
    fn compute_layout_with_children() {
        let mut parent = make_node("parent");
        parent.layout_mode = LayoutMode::Flow(FlowProps {
            display: FlowDisplay::Flex,
            flex_direction: FlexDirection::Column,
            ..Default::default()
        });

        let mut child_a = make_node("a");
        child_a.layout_mode = LayoutMode::Flow(FlowProps {
            height: Dimension::Px { value: 100.0 },
            ..Default::default()
        });

        let mut child_b = make_node("b");
        child_b.layout_mode = LayoutMode::Flow(FlowProps {
            height: Dimension::Px { value: 200.0 },
            ..Default::default()
        });

        parent.children = vec![child_a, child_b];

        let doc = BuilderDocument {
            root: Some(parent),
            ..Default::default()
        };

        let result = compute_layout(&doc, Size2::new(800.0, 600.0));
        assert!(result.nodes.contains_key("parent"));
        assert!(result.nodes.contains_key("a"));
        assert!(result.nodes.contains_key("b"));

        let a_rect = &result.nodes["a"].rect;
        let b_rect = &result.nodes["b"].rect;
        assert!((a_rect.height() - 100.0).abs() < 1.0);
        assert!((b_rect.height() - 200.0).abs() < 1.0);
        assert!(b_rect.y() >= a_rect.bottom() - 1.0);
    }

    #[test]
    fn compute_layout_free_node() {
        let mut parent = make_node("parent");
        parent.layout_mode = LayoutMode::Flow(FlowProps {
            display: FlowDisplay::Flex,
            ..Default::default()
        });

        let mut free_child = make_node("free");
        free_child.layout_mode = LayoutMode::Free;
        free_child.transform.position = [50.0, 75.0];

        parent.children = vec![free_child];

        let doc = BuilderDocument {
            root: Some(parent),
            ..Default::default()
        };

        let result = compute_layout(&doc, Size2::new(800.0, 600.0));
        assert!(result.nodes.contains_key("free"));
    }

    #[test]
    fn grid_placement_serde() {
        let placements = vec![
            GridPlacement::Auto,
            GridPlacement::Line { index: 2 },
            GridPlacement::Span { count: 3 },
        ];
        for p in &placements {
            let json = serde_json::to_string(p).unwrap();
            let p2: GridPlacement = serde_json::from_str(&json).unwrap();
            assert_eq!(*p, p2);
        }
    }

    #[test]
    fn page_layout_serde_roundtrip() {
        let layout = PageLayout {
            size: PageSize::A4,
            orientation: Orientation::Landscape,
            margins: Edges::new(20.0, 15.0, 20.0, 15.0),
            bleed: 3.0,
            grid: Some(GridCell::split(
                SplitDirection::Horizontal,
                vec![TrackSize::Fr { value: 1.0 }, TrackSize::Fr { value: 2.0 }],
                16.0,
                vec![GridCell::leaf(), GridCell::leaf()],
            )),
            column_gap: 16.0,
            row_gap: 12.0,
        };
        let json = serde_json::to_string(&layout).unwrap();
        let layout2: PageLayout = serde_json::from_str(&json).unwrap();
        assert_eq!(layout.bleed, layout2.bleed);
        assert_eq!(layout.column_gap, layout2.column_gap);
        assert!(layout2.grid.is_some());
    }

    #[test]
    fn compute_layout_with_page_margins() {
        let mut root = make_node("root");
        root.layout_mode = LayoutMode::Flow(FlowProps {
            width: Dimension::Percent { value: 100.0 },
            height: Dimension::Px { value: 100.0 },
            ..Default::default()
        });

        let doc = BuilderDocument {
            root: Some(root),
            page_layout: PageLayout {
                size: PageSize::Custom {
                    width: 800.0,
                    height: 600.0,
                },
                margins: Edges::all(50.0),
                ..Default::default()
            },
            ..Default::default()
        };

        let result = compute_layout(&doc, Size2::new(800.0, 600.0));
        let root_layout = &result.nodes["root"];
        assert!(root_layout.rect.x() >= 49.0);
        assert!(root_layout.rect.y() >= 49.0);
    }

    #[test]
    fn track_size_serde_roundtrip() {
        let tracks = vec![
            TrackSize::Fixed { value: 100.0 },
            TrackSize::Fr { value: 2.0 },
            TrackSize::Auto,
            TrackSize::MinMax {
                min: 50.0,
                max: 200.0,
            },
            TrackSize::Percent { value: 33.3 },
        ];
        for t in &tracks {
            let json = serde_json::to_string(t).unwrap();
            let t2: TrackSize = serde_json::from_str(&json).unwrap();
            assert_eq!(*t, t2);
        }
    }

    // ── Grid cell tree tests ────────────────────────────────────────

    #[test]
    fn grid_cell_at_navigates() {
        let grid = GridCell::split(
            SplitDirection::Horizontal,
            vec![TrackSize::Fr { value: 1.0 }; 2],
            0.0,
            vec![
                GridCell::leaf_with("a"),
                GridCell::split(
                    SplitDirection::Vertical,
                    vec![TrackSize::Fr { value: 1.0 }; 2],
                    0.0,
                    vec![GridCell::leaf_with("b"), GridCell::leaf()],
                ),
            ],
        );
        assert_eq!(grid.at(&[0]).unwrap().node_id(), Some("a"));
        assert_eq!(grid.at(&[1, 0]).unwrap().node_id(), Some("b"));
        assert_eq!(grid.at(&[1, 1]).unwrap().node_id(), None);
        assert!(grid.at(&[2]).is_none());
    }

    #[test]
    fn insert_at_edge_adds_sibling() {
        let mut layout = PageLayout {
            grid: Some(GridCell::split(
                SplitDirection::Horizontal,
                vec![TrackSize::Fr { value: 1.0 }; 2],
                16.0,
                vec![GridCell::leaf_with("a"), GridCell::leaf_with("b")],
            )),
            column_gap: 16.0,
            ..Default::default()
        };
        layout.insert_at_edge(&[1], CellEdge::Right).unwrap();
        let grid = layout.grid.as_ref().unwrap();
        match grid {
            GridCell::Split { children, .. } => {
                assert_eq!(children.len(), 3);
                assert_eq!(children[1].node_id(), Some("b"));
                assert!(children[2].is_leaf());
            }
            _ => panic!("expected split"),
        }
    }

    #[test]
    fn insert_at_edge_subdivides_perpendicular() {
        let mut layout = PageLayout {
            grid: Some(GridCell::split(
                SplitDirection::Horizontal,
                vec![TrackSize::Fr { value: 1.0 }; 2],
                16.0,
                vec![GridCell::leaf_with("a"), GridCell::leaf_with("b")],
            )),
            row_gap: 12.0,
            ..Default::default()
        };
        layout.insert_at_edge(&[1], CellEdge::Bottom).unwrap();
        let grid = layout.grid.as_ref().unwrap();
        match grid {
            GridCell::Split { children, .. } => {
                assert_eq!(children.len(), 2);
                assert_eq!(children[0].node_id(), Some("a"));
                match &children[1] {
                    GridCell::Split {
                        direction,
                        children: inner,
                        ..
                    } => {
                        assert_eq!(*direction, SplitDirection::Vertical);
                        assert_eq!(inner.len(), 2);
                        assert_eq!(inner[0].node_id(), Some("b"));
                        assert!(inner[1].is_leaf());
                    }
                    _ => panic!("expected vertical split"),
                }
            }
            _ => panic!("expected split"),
        }
    }

    #[test]
    fn remove_cell_collapses_parent() {
        let mut layout = PageLayout {
            grid: Some(GridCell::split(
                SplitDirection::Horizontal,
                vec![TrackSize::Fr { value: 1.0 }; 2],
                0.0,
                vec![GridCell::leaf_with("a"), GridCell::leaf_with("b")],
            )),
            ..Default::default()
        };
        layout.remove_cell(&[0]).unwrap();
        assert_eq!(layout.grid.as_ref().unwrap().node_id(), Some("b"));
    }

    #[test]
    fn flatten_cells_computes_positions() {
        let layout = PageLayout {
            grid: Some(GridCell::split(
                SplitDirection::Horizontal,
                vec![TrackSize::Fr { value: 1.0 }; 2],
                0.0,
                vec![GridCell::leaf_with("a"), GridCell::leaf_with("b")],
            )),
            ..Default::default()
        };
        let cells = layout.flatten_cells(200.0, 100.0);
        assert_eq!(cells.len(), 2);
        assert_eq!(cells[0].x, 0.0);
        assert_eq!(cells[0].width, 100.0);
        assert_eq!(cells[1].x, 100.0);
        assert_eq!(cells[1].width, 100.0);
    }

    #[test]
    fn place_node_at_and_clear() {
        let mut layout = PageLayout {
            grid: Some(GridCell::leaf()),
            ..Default::default()
        };
        layout.place_node_at(&[], "n0".into()).unwrap();
        assert_eq!(layout.grid.as_ref().unwrap().node_id(), Some("n0"));
        layout.clear_cell(&[]).unwrap();
        assert_eq!(layout.grid.as_ref().unwrap().node_id(), None);
    }

    #[test]
    fn path_string_round_trips() {
        let path = vec![1, 0, 2];
        assert_eq!(path_from_string(&path_to_string(&path)), path);
        assert!(path_from_string("").is_empty());
        assert_eq!(path_to_string(&[]), "");
    }

    #[test]
    fn grid_placement_resolved_index() {
        assert_eq!(GridPlacement::Auto.resolved_index(), None);
        assert_eq!(GridPlacement::Line { index: 1 }.resolved_index(), Some(0));
        assert_eq!(GridPlacement::Line { index: 3 }.resolved_index(), Some(2));
        assert_eq!(GridPlacement::Span { count: 2 }.resolved_index(), None);
    }

    // ── Absolute / Relative positioning tests ──────────────────────

    #[test]
    fn absolute_props_serde_roundtrip() {
        let abs = AbsoluteProps {
            width: Dimension::Px { value: 200.0 },
            height: Dimension::Px { value: 100.0 },
            ..Default::default()
        };
        let json = serde_json::to_string(&abs).unwrap();
        let abs2: AbsoluteProps = serde_json::from_str(&json).unwrap();
        assert_eq!(abs, abs2);
    }

    #[test]
    fn layout_mode_absolute_serde_roundtrip() {
        let mode = LayoutMode::Absolute(AbsoluteProps::fixed(300.0, 150.0));
        let json = serde_json::to_string(&mode).unwrap();
        let mode2: LayoutMode = serde_json::from_str(&json).unwrap();
        assert!(matches!(mode2, LayoutMode::Absolute(_)));
        if let LayoutMode::Absolute(abs) = mode2 {
            assert_eq!(abs.width, Dimension::Px { value: 300.0 });
            assert_eq!(abs.height, Dimension::Px { value: 150.0 });
        }
    }

    #[test]
    fn layout_mode_relative_serde_roundtrip() {
        let mode = LayoutMode::Relative(FlowProps {
            display: FlowDisplay::Flex,
            gap: 8.0,
            ..Default::default()
        });
        let json = serde_json::to_string(&mode).unwrap();
        let mode2: LayoutMode = serde_json::from_str(&json).unwrap();
        assert!(matches!(mode2, LayoutMode::Relative(_)));
    }

    #[test]
    fn layout_mode_helpers() {
        assert!(LayoutMode::Flow(FlowProps::default()).is_in_flow());
        assert!(LayoutMode::Relative(FlowProps::default()).is_in_flow());
        assert!(!LayoutMode::Absolute(AbsoluteProps::default()).is_in_flow());
        assert!(!LayoutMode::Free.is_in_flow());

        assert!(!LayoutMode::Flow(FlowProps::default()).is_positioned());
        assert!(LayoutMode::Relative(FlowProps::default()).is_positioned());
        assert!(LayoutMode::Absolute(AbsoluteProps::default()).is_positioned());
        assert!(LayoutMode::Free.is_positioned());

        assert!(LayoutMode::Flow(FlowProps::default())
            .flow_props()
            .is_some());
        assert!(LayoutMode::Relative(FlowProps::default())
            .flow_props()
            .is_some());
        assert!(LayoutMode::Absolute(AbsoluteProps::default())
            .flow_props()
            .is_none());
        assert!(LayoutMode::Free.flow_props().is_none());
    }

    #[test]
    fn compute_layout_absolute_node_top_left() {
        use prism_core::foundation::spatial::Anchor;

        let mut parent = make_node("parent");
        parent.layout_mode = LayoutMode::Flow(FlowProps {
            display: FlowDisplay::Flex,
            width: Dimension::Px { value: 400.0 },
            height: Dimension::Px { value: 300.0 },
            ..Default::default()
        });

        let mut abs_child = make_node("abs");
        abs_child.layout_mode = LayoutMode::Absolute(AbsoluteProps::fixed(100.0, 50.0));
        abs_child.transform.position = [20.0, 30.0];
        abs_child.transform.anchor = Anchor::TopLeft;

        parent.children = vec![abs_child];

        let doc = BuilderDocument {
            root: Some(parent),
            ..Default::default()
        };

        let result = compute_layout(&doc, Size2::new(800.0, 600.0));
        let abs_layout = &result.nodes["abs"];
        assert!((abs_layout.rect.x() - 20.0).abs() < 2.0);
        assert!((abs_layout.rect.y() - 30.0).abs() < 2.0);
    }

    #[test]
    fn compute_layout_absolute_node_center_anchor() {
        use prism_core::foundation::spatial::Anchor;

        let mut parent = make_node("parent");
        parent.layout_mode = LayoutMode::Flow(FlowProps {
            display: FlowDisplay::Flex,
            width: Dimension::Px { value: 400.0 },
            height: Dimension::Px { value: 300.0 },
            ..Default::default()
        });

        let mut abs_child = make_node("centered");
        abs_child.layout_mode = LayoutMode::Absolute(AbsoluteProps::fixed(100.0, 50.0));
        abs_child.transform.position = [0.0, 0.0];
        abs_child.transform.anchor = Anchor::Center;

        parent.children = vec![abs_child];

        let doc = BuilderDocument {
            root: Some(parent),
            ..Default::default()
        };

        let result = compute_layout(&doc, Size2::new(800.0, 600.0));
        let layout = &result.nodes["centered"];
        // Center of a 400x300 parent = (200, 150), offset by (0,0)
        assert!((layout.rect.x() - 200.0).abs() < 2.0);
        assert!((layout.rect.y() - 150.0).abs() < 2.0);
    }

    #[test]
    fn compute_layout_absolute_node_bottom_right() {
        use prism_core::foundation::spatial::Anchor;

        let mut parent = make_node("parent");
        parent.layout_mode = LayoutMode::Flow(FlowProps {
            display: FlowDisplay::Flex,
            width: Dimension::Px { value: 400.0 },
            height: Dimension::Px { value: 300.0 },
            ..Default::default()
        });

        let mut abs_child = make_node("br");
        abs_child.layout_mode = LayoutMode::Absolute(AbsoluteProps::fixed(80.0, 40.0));
        abs_child.transform.position = [-10.0, -5.0];
        abs_child.transform.anchor = Anchor::BottomRight;

        parent.children = vec![abs_child];

        let doc = BuilderDocument {
            root: Some(parent),
            ..Default::default()
        };

        let result = compute_layout(&doc, Size2::new(800.0, 600.0));
        let layout = &result.nodes["br"];
        // Bottom-right of 400x300 = (400, 300), offset by (-10, -5)
        assert!((layout.rect.x() - 390.0).abs() < 2.0);
        assert!((layout.rect.y() - 295.0).abs() < 2.0);
    }

    #[test]
    fn compute_layout_relative_node_offset() {
        let mut parent = make_node("parent");
        parent.layout_mode = LayoutMode::Flow(FlowProps {
            display: FlowDisplay::Flex,
            flex_direction: FlexDirection::Column,
            width: Dimension::Px { value: 400.0 },
            height: Dimension::Px { value: 300.0 },
            ..Default::default()
        });

        let mut flow_child = make_node("flow");
        flow_child.layout_mode = LayoutMode::Flow(FlowProps {
            height: Dimension::Px { value: 50.0 },
            ..Default::default()
        });

        let mut rel_child = make_node("rel");
        rel_child.layout_mode = LayoutMode::Relative(FlowProps {
            height: Dimension::Px { value: 60.0 },
            ..Default::default()
        });
        rel_child.transform.position = [10.0, 5.0];

        parent.children = vec![flow_child, rel_child];

        let doc = BuilderDocument {
            root: Some(parent),
            ..Default::default()
        };

        let result = compute_layout(&doc, Size2::new(800.0, 600.0));
        assert!(result.nodes.contains_key("rel"));
        let rel_layout = &result.nodes["rel"];
        // Relative node is in flow after the 50px flow child, but
        // its transform position (10, 5) is composed into its transform.
        assert!((rel_layout.rect.height() - 60.0).abs() < 1.0);
    }

    #[test]
    fn absolute_mixed_with_flow_children() {
        let mut parent = make_node("parent");
        parent.layout_mode = LayoutMode::Flow(FlowProps {
            display: FlowDisplay::Flex,
            flex_direction: FlexDirection::Column,
            width: Dimension::Px { value: 400.0 },
            height: Dimension::Px { value: 300.0 },
            ..Default::default()
        });

        let mut flow_a = make_node("a");
        flow_a.layout_mode = LayoutMode::Flow(FlowProps {
            height: Dimension::Px { value: 100.0 },
            ..Default::default()
        });

        let mut abs_b = make_node("b");
        abs_b.layout_mode = LayoutMode::Absolute(AbsoluteProps::fixed(50.0, 50.0));
        abs_b.transform.position = [10.0, 10.0];

        let mut flow_c = make_node("c");
        flow_c.layout_mode = LayoutMode::Flow(FlowProps {
            height: Dimension::Px { value: 80.0 },
            ..Default::default()
        });

        parent.children = vec![flow_a, abs_b, flow_c];

        let doc = BuilderDocument {
            root: Some(parent),
            ..Default::default()
        };

        let result = compute_layout(&doc, Size2::new(800.0, 600.0));
        let a_rect = &result.nodes["a"].rect;
        let c_rect = &result.nodes["c"].rect;
        // Flow children should be stacked: a at top, c right after a.
        // The absolute child b does NOT take up flow space.
        assert!((a_rect.height() - 100.0).abs() < 1.0);
        assert!((c_rect.y() - a_rect.bottom()).abs() < 1.0);
    }

    #[test]
    fn absolute_props_default() {
        let abs = AbsoluteProps::default();
        assert_eq!(abs.width, Dimension::Auto);
        assert_eq!(abs.height, Dimension::Auto);
    }

    #[test]
    fn absolute_props_fixed_constructor() {
        let abs = AbsoluteProps::fixed(200.0, 100.0);
        assert_eq!(abs.width, Dimension::Px { value: 200.0 });
        assert_eq!(abs.height, Dimension::Px { value: 100.0 });
    }

    // ── Gap handle deduplication tests ─────────────────────────────

    #[test]
    fn edge_handles_single_leaf_has_four_outer() {
        let layout = PageLayout {
            grid: Some(GridCell::leaf()),
            ..Default::default()
        };
        let handles = layout.flatten_edge_handles(200.0, 100.0);
        assert_eq!(handles.len(), 4);
        assert!(handles.iter().all(|h| !h.is_gap));
    }

    #[test]
    fn edge_handles_horizontal_split_deduplicates() {
        let layout = PageLayout {
            grid: Some(GridCell::split(
                SplitDirection::Horizontal,
                vec![TrackSize::Fr { value: 1.0 }; 2],
                16.0,
                vec![GridCell::leaf(), GridCell::leaf()],
            )),
            column_gap: 16.0,
            ..Default::default()
        };
        let handles = layout.flatten_edge_handles(200.0, 100.0);
        let gaps: Vec<_> = handles.iter().filter(|h| h.is_gap).collect();
        let outer: Vec<_> = handles.iter().filter(|h| !h.is_gap).collect();
        assert_eq!(gaps.len(), 1);
        assert_eq!(gaps[0].gap_index, 0);
        assert_eq!(gaps[0].orientation, SplitDirection::Horizontal);
        // 2 cells × 4 edges = 8, minus 2 suppressed (A.right, B.left) = 6 outer
        assert_eq!(outer.len(), 6);
    }

    #[test]
    fn edge_handles_three_columns_has_two_gaps() {
        let layout = PageLayout {
            grid: Some(GridCell::split(
                SplitDirection::Horizontal,
                vec![TrackSize::Fr { value: 1.0 }; 3],
                8.0,
                vec![GridCell::leaf(), GridCell::leaf(), GridCell::leaf()],
            )),
            ..Default::default()
        };
        let handles = layout.flatten_edge_handles(300.0, 100.0);
        let gaps: Vec<_> = handles.iter().filter(|h| h.is_gap).collect();
        assert_eq!(gaps.len(), 2);
        assert_eq!(gaps[0].gap_index, 0);
        assert_eq!(gaps[1].gap_index, 1);
    }

    #[test]
    fn edge_handles_vertical_split_gap_orientation() {
        let layout = PageLayout {
            grid: Some(GridCell::split(
                SplitDirection::Vertical,
                vec![TrackSize::Fr { value: 1.0 }; 2],
                12.0,
                vec![GridCell::leaf(), GridCell::leaf()],
            )),
            ..Default::default()
        };
        let handles = layout.flatten_edge_handles(200.0, 200.0);
        let gaps: Vec<_> = handles.iter().filter(|h| h.is_gap).collect();
        assert_eq!(gaps.len(), 1);
        assert_eq!(gaps[0].orientation, SplitDirection::Vertical);
    }

    #[test]
    fn edge_handles_nested_suppresses_inner() {
        let layout = PageLayout {
            grid: Some(GridCell::split(
                SplitDirection::Horizontal,
                vec![TrackSize::Fr { value: 1.0 }; 2],
                16.0,
                vec![
                    GridCell::split(
                        SplitDirection::Vertical,
                        vec![TrackSize::Fr { value: 1.0 }; 2],
                        8.0,
                        vec![GridCell::leaf(), GridCell::leaf()],
                    ),
                    GridCell::leaf(),
                ],
            )),
            ..Default::default()
        };
        let handles = layout.flatten_edge_handles(400.0, 200.0);
        let gaps: Vec<_> = handles.iter().filter(|h| h.is_gap).collect();
        // 1 horizontal gap (between the vertical split and the leaf)
        // + 1 vertical gap (between the two leaves inside the vertical split)
        assert_eq!(gaps.len(), 2);
        // No leaf should have a Right edge on child 0 or Left edge on child 1
        let outer: Vec<_> = handles.iter().filter(|h| !h.is_gap).collect();
        for h in &outer {
            let path_str = path_to_string(&h.cell_path);
            if path_str == "0.0" || path_str == "0.1" {
                assert_ne!(
                    h.edge,
                    CellEdge::Right,
                    "inner right edge should be suppressed"
                );
            }
            if path_str == "1" {
                assert_ne!(
                    h.edge,
                    CellEdge::Left,
                    "inner left edge should be suppressed"
                );
            }
        }
    }

    // ── Resize gap tests ───────────────────────────────────────────

    #[test]
    fn resize_gap_adjusts_fr_tracks() {
        let mut layout = PageLayout {
            grid: Some(GridCell::split(
                SplitDirection::Horizontal,
                vec![TrackSize::Fr { value: 1.0 }, TrackSize::Fr { value: 1.0 }],
                0.0,
                vec![GridCell::leaf(), GridCell::leaf()],
            )),
            ..Default::default()
        };
        // Each track is 100px. Move 20px to the right.
        layout.resize_gap(&[], 0, 20.0, 200.0).unwrap();
        match &layout.grid {
            Some(GridCell::Split { tracks, .. }) => {
                if let (TrackSize::Fr { value: a }, TrackSize::Fr { value: b }) =
                    (&tracks[0], &tracks[1])
                {
                    assert!((a - 1.2).abs() < 0.01, "track 0 should grow: {a}");
                    assert!((b - 0.8).abs() < 0.01, "track 1 should shrink: {b}");
                } else {
                    panic!("expected Fr tracks");
                }
            }
            _ => panic!("expected split"),
        }
    }

    #[test]
    fn resize_gap_respects_minimum_size() {
        let mut layout = PageLayout {
            grid: Some(GridCell::split(
                SplitDirection::Horizontal,
                vec![TrackSize::Fr { value: 1.0 }, TrackSize::Fr { value: 1.0 }],
                0.0,
                vec![GridCell::leaf(), GridCell::leaf()],
            )),
            ..Default::default()
        };
        // Try to push 90px in a 200px space — second track would go to 10px,
        // but minimum is 20px.
        layout.resize_gap(&[], 0, 90.0, 200.0).unwrap();
        match &layout.grid {
            Some(GridCell::Split { tracks, .. }) => {
                let sizes = compute_track_sizes(tracks, 0.0, 200.0);
                assert!(
                    sizes[1] >= 19.9,
                    "track 1 should respect minimum: {}",
                    sizes[1]
                );
            }
            _ => panic!("expected split"),
        }
    }

    #[test]
    fn resize_gap_nested_split() {
        let mut layout = PageLayout {
            grid: Some(GridCell::split(
                SplitDirection::Horizontal,
                vec![TrackSize::Fr { value: 1.0 }; 2],
                0.0,
                vec![
                    GridCell::split(
                        SplitDirection::Vertical,
                        vec![TrackSize::Fr { value: 1.0 }, TrackSize::Fr { value: 1.0 }],
                        0.0,
                        vec![GridCell::leaf(), GridCell::leaf()],
                    ),
                    GridCell::leaf(),
                ],
            )),
            ..Default::default()
        };
        // Resize the vertical gap inside the first child
        layout.resize_gap(&[0], 0, 10.0, 100.0).unwrap();
        let inner = layout.grid.as_ref().unwrap().at(&[0]).unwrap();
        match inner {
            GridCell::Split { tracks, .. } => {
                let sizes = compute_track_sizes(tracks, 0.0, 100.0);
                assert!(
                    (sizes[0] - 60.0).abs() < 1.0,
                    "top track should be ~60px: {}",
                    sizes[0]
                );
                assert!(
                    (sizes[1] - 40.0).abs() < 1.0,
                    "bottom track should be ~40px: {}",
                    sizes[1]
                );
            }
            _ => panic!("expected split"),
        }
    }
}
