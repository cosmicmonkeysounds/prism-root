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

use std::collections::{HashMap, HashSet};

use glam::{Affine2, Vec2};
use prism_core::foundation::geometry::{Edges, Point2, Rect, Size2};
use prism_core::foundation::spatial::ComputedTransform;
use serde::{Deserialize, Serialize};
use taffy::prelude::*;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum GridEditError {
    #[error("index out of bounds: {0}")]
    IndexOutOfBounds(usize),
    #[error("cannot remove last track")]
    CannotRemoveLastTrack,
}

use crate::document::{BuilderDocument, Node, NodeId};

// ── Page layout ──────────────────────────────────────────────────────

/// Physical page dimensions.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum PageSize {
    /// Standard paper sizes.
    A4,
    A3,
    A5,
    Letter,
    Legal,
    Tabloid,
    /// Custom size in pixels.
    Custom {
        width: f32,
        height: f32,
    },
    /// Responsive — fills the viewport. No fixed dimensions.
    #[default]
    Responsive,
}

impl PageSize {
    /// Returns the page dimensions in pixels at 96 DPI, or `None` for
    /// `Responsive` (viewport-dependent).
    pub fn to_pixels(self) -> Option<Size2> {
        match self {
            Self::A4 => Some(Size2::new(794.0, 1123.0)),
            Self::A3 => Some(Size2::new(1123.0, 1587.0)),
            Self::A5 => Some(Size2::new(559.0, 794.0)),
            Self::Letter => Some(Size2::new(816.0, 1056.0)),
            Self::Legal => Some(Size2::new(816.0, 1344.0)),
            Self::Tabloid => Some(Size2::new(1056.0, 1632.0)),
            Self::Custom { width, height } => Some(Size2::new(width, height)),
            Self::Responsive => None,
        }
    }
}

/// Page orientation.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Orientation {
    #[default]
    Portrait,
    Landscape,
}

/// A single track definition in the grid template (column or row).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum TrackSize {
    /// Fixed size in pixels.
    Fixed { value: f32 },
    /// Fractional unit (`1fr`, `2fr`, etc.).
    Fr { value: f32 },
    /// Size to content.
    Auto,
    /// Minimum and maximum bounds.
    MinMax { min: f32, max: f32 },
    /// Percentage of the available space.
    Percent { value: f32 },
}

impl TrackSize {
    fn to_taffy(self) -> taffy::style::TrackSizingFunction {
        match self {
            Self::Fixed { value } => minmax(length(value), length(value)),
            Self::Fr { value } => minmax(length(0.0), fr(value)),
            Self::Auto => minmax(auto(), auto()),
            Self::MinMax { min, max } => minmax(length(min), length(max)),
            Self::Percent { value } => minmax(percent(value / 100.0), percent(value / 100.0)),
        }
    }
}

/// Structural layout properties of a page. Every page in a
/// `BuilderDocument` has one of these — it's not a component.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageLayout {
    #[serde(default)]
    pub size: PageSize,
    #[serde(default)]
    pub orientation: Orientation,
    #[serde(default)]
    pub margins: Edges<f32>,
    #[serde(default)]
    pub bleed: f32,
    #[serde(default)]
    pub columns: Vec<TrackSize>,
    #[serde(default)]
    pub rows: Vec<TrackSize>,
    #[serde(default)]
    pub column_gap: f32,
    #[serde(default)]
    pub row_gap: f32,
}

impl PageLayout {
    pub fn insert_column(&mut self, index: usize, track: TrackSize) -> usize {
        if index >= self.columns.len() {
            self.columns.push(track);
        } else {
            self.columns.insert(index, track);
        }
        self.columns.len()
    }

    pub fn remove_column(&mut self, index: usize) -> Result<(), GridEditError> {
        if self.columns.len() <= 1 {
            return Err(GridEditError::CannotRemoveLastTrack);
        }
        if index >= self.columns.len() {
            return Err(GridEditError::IndexOutOfBounds(index));
        }
        self.columns.remove(index);
        Ok(())
    }

    pub fn resize_column(&mut self, index: usize, track: TrackSize) -> Result<(), GridEditError> {
        if index >= self.columns.len() {
            return Err(GridEditError::IndexOutOfBounds(index));
        }
        self.columns[index] = track;
        Ok(())
    }

    pub fn insert_row(&mut self, index: usize, track: TrackSize) -> usize {
        if index >= self.rows.len() {
            self.rows.push(track);
        } else {
            self.rows.insert(index, track);
        }
        self.rows.len()
    }

    pub fn remove_row(&mut self, index: usize) -> Result<(), GridEditError> {
        if self.rows.len() <= 1 {
            return Err(GridEditError::CannotRemoveLastTrack);
        }
        if index >= self.rows.len() {
            return Err(GridEditError::IndexOutOfBounds(index));
        }
        self.rows.remove(index);
        Ok(())
    }

    pub fn resize_row(&mut self, index: usize, track: TrackSize) -> Result<(), GridEditError> {
        if index >= self.rows.len() {
            return Err(GridEditError::IndexOutOfBounds(index));
        }
        self.rows[index] = track;
        Ok(())
    }

    pub fn cell_positions(&self) -> Vec<(usize, usize)> {
        let cols = self.columns.len().max(1);
        let rows = self.rows.len().max(1);
        let mut cells = Vec::with_capacity(cols * rows);
        for r in 0..rows {
            for c in 0..cols {
                cells.push((c, r));
            }
        }
        cells
    }

    pub fn empty_cells(&self, occupied: &[(GridPlacement, GridPlacement)]) -> Vec<(usize, usize)> {
        let all = self.cell_positions();
        let mut taken = HashSet::new();
        for (col_p, row_p) in occupied {
            let col = col_p.resolved_index().unwrap_or(0);
            let row = row_p.resolved_index().unwrap_or(0);
            taken.insert((col, row));
        }
        all.into_iter().filter(|c| !taken.contains(c)).collect()
    }

    /// Resolve the page dimensions, applying orientation. Returns
    /// `None` for `Responsive` pages.
    pub fn resolved_size(&self) -> Option<Size2> {
        self.size.to_pixels().map(|s| match self.orientation {
            Orientation::Portrait => s,
            Orientation::Landscape => Size2::new(s.height, s.width),
        })
    }

    /// The content area after subtracting margins from the page size.
    pub fn content_rect(&self, page_size: Size2) -> Rect {
        Rect::new(0.0, 0.0, page_size.width, page_size.height).inset(&self.margins)
    }

    /// The bleed area — extends outward from the page edges.
    pub fn bleed_rect(&self, page_size: Size2) -> Rect {
        let bleed_edges = Edges::all(self.bleed);
        Rect::new(0.0, 0.0, page_size.width, page_size.height).outset(&bleed_edges)
    }

    fn build_taffy_style(&self, page_size: Size2) -> Style {
        let content = self.content_rect(page_size);

        let grid_template_columns: Vec<TrackSizingFunction> = if self.columns.is_empty() {
            vec![minmax(auto(), fr(1.0))]
        } else {
            self.columns
                .iter()
                .copied()
                .map(TrackSize::to_taffy)
                .collect()
        };

        let grid_template_rows: Vec<TrackSizingFunction> = if self.rows.is_empty() {
            vec![minmax(auto(), auto())]
        } else {
            self.rows.iter().copied().map(TrackSize::to_taffy).collect()
        };

        Style {
            display: Display::Grid,
            size: taffy::Size {
                width: length(content.width()),
                height: length(content.height()),
            },
            grid_template_columns,
            grid_template_rows,
            gap: taffy::Size {
                width: length(self.column_gap),
                height: length(self.row_gap),
            },
            ..Default::default()
        }
    }
}

impl Default for PageLayout {
    fn default() -> Self {
        Self {
            size: PageSize::default(),
            orientation: Orientation::default(),
            margins: Edges::ZERO,
            bleed: 0.0,
            columns: Vec::new(),
            rows: Vec::new(),
            column_gap: 0.0,
            row_gap: 0.0,
        }
    }
}

// ── Per-node layout mode ─────────────────────────────────────────────

/// How a node participates in its parent's layout.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "kebab-case")]
pub enum LayoutMode {
    /// Positioned by the parent's flow (flex/grid/block).
    Flow(FlowProps),
    /// Removed from flow — positioned by `Transform2D` alone.
    Free,
}

impl Default for LayoutMode {
    fn default() -> Self {
        Self::Flow(FlowProps::default())
    }
}

/// CSS-like properties for nodes in flow layout. Maps to `taffy::Style`
/// fields.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FlowProps {
    #[serde(default = "FlowProps::default_display")]
    pub display: FlowDisplay,

    #[serde(default)]
    pub width: Dimension,
    #[serde(default)]
    pub height: Dimension,
    #[serde(default)]
    pub min_width: Dimension,
    #[serde(default)]
    pub min_height: Dimension,
    #[serde(default)]
    pub max_width: Dimension,
    #[serde(default)]
    pub max_height: Dimension,

    #[serde(default)]
    pub padding: Edges<f32>,
    #[serde(default)]
    pub margin: Edges<f32>,

    #[serde(default)]
    pub flex_grow: f32,
    #[serde(default)]
    pub flex_shrink: f32,
    #[serde(default)]
    pub flex_basis: Dimension,
    #[serde(default)]
    pub flex_direction: FlexDirection,

    #[serde(default)]
    pub align_self: AlignOption,
    #[serde(default)]
    pub align_items: AlignOption,
    #[serde(default)]
    pub justify_content: JustifyOption,

    #[serde(default)]
    pub grid_column: GridPlacement,
    #[serde(default)]
    pub grid_row: GridPlacement,

    #[serde(default)]
    pub gap: f32,
}

impl FlowProps {
    /// True when all fields are at their defaults — no layout wrapper needed.
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }

    fn default_display() -> FlowDisplay {
        FlowDisplay::Block
    }

    pub fn to_taffy_style(&self) -> Style {
        Style {
            display: self.display.to_taffy(),
            size: taffy::Size {
                width: self.width.to_taffy(),
                height: self.height.to_taffy(),
            },
            min_size: taffy::Size {
                width: self.min_width.to_taffy(),
                height: self.min_height.to_taffy(),
            },
            max_size: taffy::Size {
                width: self.max_width.to_taffy(),
                height: self.max_height.to_taffy(),
            },
            padding: taffy::Rect {
                left: length(self.padding.left),
                right: length(self.padding.right),
                top: length(self.padding.top),
                bottom: length(self.padding.bottom),
            },
            margin: taffy::Rect {
                left: length(self.margin.left),
                right: length(self.margin.right),
                top: length(self.margin.top),
                bottom: length(self.margin.bottom),
            },
            flex_grow: self.flex_grow,
            flex_shrink: self.flex_shrink,
            flex_basis: self.flex_basis.to_taffy(),
            flex_direction: self.flex_direction.to_taffy(),
            align_self: self.align_self.to_taffy_align(),
            align_items: self.align_items.to_taffy_align(),
            justify_content: self.justify_content.to_taffy_justify(),
            grid_column: Line {
                start: self.grid_column.to_taffy(),
                end: taffy::style::GridPlacement::AUTO,
            },
            grid_row: Line {
                start: self.grid_row.to_taffy(),
                end: taffy::style::GridPlacement::AUTO,
            },
            gap: taffy::Size {
                width: length(self.gap),
                height: length(self.gap),
            },
            ..Default::default()
        }
    }
}

impl Default for FlowProps {
    fn default() -> Self {
        Self {
            display: FlowDisplay::Block,
            width: Dimension::Auto,
            height: Dimension::Auto,
            min_width: Dimension::Auto,
            min_height: Dimension::Auto,
            max_width: Dimension::Auto,
            max_height: Dimension::Auto,
            padding: Edges::ZERO,
            margin: Edges::ZERO,
            flex_grow: 0.0,
            flex_shrink: 1.0,
            flex_basis: Dimension::Auto,
            flex_direction: FlexDirection::Column,
            align_self: AlignOption::Auto,
            align_items: AlignOption::Auto,
            justify_content: JustifyOption::Start,
            grid_column: GridPlacement::Auto,
            grid_row: GridPlacement::Auto,
            gap: 0.0,
        }
    }
}

/// Display mode for flow nodes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FlowDisplay {
    Block,
    Flex,
    Grid,
    None,
}

impl FlowDisplay {
    fn to_taffy(self) -> Display {
        match self {
            Self::Block => Display::Block,
            Self::Flex => Display::Flex,
            Self::Grid => Display::Grid,
            Self::None => Display::None,
        }
    }
}

/// A length dimension — auto, fixed, or percentage.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum Dimension {
    #[default]
    Auto,
    Px {
        value: f32,
    },
    Percent {
        value: f32,
    },
}

impl Dimension {
    fn to_taffy(self) -> taffy::style::Dimension {
        match self {
            Self::Auto => taffy::style::Dimension::Auto,
            Self::Px { value } => taffy::style::Dimension::Length(value),
            Self::Percent { value } => taffy::style::Dimension::Percent(value / 100.0),
        }
    }
}

/// Flex direction.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FlexDirection {
    Row,
    #[default]
    Column,
    RowReverse,
    ColumnReverse,
}

impl FlexDirection {
    fn to_taffy(self) -> taffy::style::FlexDirection {
        match self {
            Self::Row => taffy::style::FlexDirection::Row,
            Self::Column => taffy::style::FlexDirection::Column,
            Self::RowReverse => taffy::style::FlexDirection::RowReverse,
            Self::ColumnReverse => taffy::style::FlexDirection::ColumnReverse,
        }
    }
}

/// Alignment option (maps to CSS align-items / align-self).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AlignOption {
    #[default]
    Auto,
    Start,
    End,
    Center,
    Stretch,
    Baseline,
}

impl AlignOption {
    fn to_taffy_align(self) -> Option<taffy::style::AlignItems> {
        match self {
            Self::Auto => None,
            Self::Start => Some(taffy::style::AlignItems::Start),
            Self::End => Some(taffy::style::AlignItems::End),
            Self::Center => Some(taffy::style::AlignItems::Center),
            Self::Stretch => Some(taffy::style::AlignItems::Stretch),
            Self::Baseline => Some(taffy::style::AlignItems::Baseline),
        }
    }
}

/// Justify-content option.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum JustifyOption {
    #[default]
    Start,
    End,
    Center,
    SpaceBetween,
    SpaceAround,
    SpaceEvenly,
    Stretch,
}

impl JustifyOption {
    fn to_taffy_justify(self) -> Option<taffy::style::JustifyContent> {
        match self {
            Self::Start => Some(taffy::style::JustifyContent::Start),
            Self::End => Some(taffy::style::JustifyContent::End),
            Self::Center => Some(taffy::style::JustifyContent::Center),
            Self::SpaceBetween => Some(taffy::style::JustifyContent::SpaceBetween),
            Self::SpaceAround => Some(taffy::style::JustifyContent::SpaceAround),
            Self::SpaceEvenly => Some(taffy::style::JustifyContent::SpaceEvenly),
            Self::Stretch => Some(taffy::style::JustifyContent::Stretch),
        }
    }
}

/// Grid placement for a node within a CSS Grid parent.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum GridPlacement {
    #[default]
    Auto,
    Line {
        index: i16,
    },
    Span {
        count: u16,
    },
}

impl GridPlacement {
    fn to_taffy(self) -> taffy::style::GridPlacement {
        match self {
            Self::Auto => taffy::style::GridPlacement::AUTO,
            Self::Line { index } => taffy::style::GridPlacement::from_line_index(index),
            Self::Span { count } => taffy::style::GridPlacement::from_span(count),
        }
    }

    pub fn resolved_index(&self) -> Option<usize> {
        match self {
            Self::Line { index } => Some((*index - 1).max(0) as usize),
            _ => None,
        }
    }
}

// ── Layout computation ───────────────────────────────────────────────

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

    fn collect_layouts(
        tree: &TaffyTree<NodeId>,
        taffy_map: &HashMap<NodeId, taffy::NodeId>,
        node: &Node,
        parent_offset: Vec2,
        parent_global: Affine2,
        content_offset: Vec2,
        result: &mut ComputedLayout,
    ) {
        let taffy_node = match taffy_map.get(&node.id) {
            Some(n) => *n,
            None => return,
        };

        let taffy_layout = tree.layout(taffy_node).expect("layout lookup");
        let layout_pos = Vec2::new(taffy_layout.location.x, taffy_layout.location.y)
            + parent_offset
            + content_offset;
        let layout_size = Vec2::new(taffy_layout.size.width, taffy_layout.size.height);

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
                Vec2::ZERO,
                result,
            );
        }
    }

    let content_offset = Vec2::new(content_rect.x(), content_rect.y());
    collect_layouts(
        &tree,
        &taffy_map,
        root,
        Vec2::ZERO,
        Affine2::IDENTITY,
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
            columns: vec![TrackSize::Fr { value: 1.0 }, TrackSize::Fr { value: 2.0 }],
            rows: vec![TrackSize::Auto, TrackSize::Fixed { value: 200.0 }],
            column_gap: 16.0,
            row_gap: 12.0,
        };
        let json = serde_json::to_string(&layout).unwrap();
        let layout2: PageLayout = serde_json::from_str(&json).unwrap();
        assert_eq!(layout.bleed, layout2.bleed);
        assert_eq!(layout.column_gap, layout2.column_gap);
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

    // ── Grid manipulation tests ─────────────────────────────────────

    #[test]
    fn insert_column_appends() {
        let mut layout = PageLayout::default();
        layout.insert_column(0, TrackSize::Fr { value: 1.0 });
        layout.insert_column(100, TrackSize::Fr { value: 2.0 });
        assert_eq!(layout.columns.len(), 2);
    }

    #[test]
    fn insert_column_at_index() {
        let mut layout = PageLayout {
            columns: vec![TrackSize::Fr { value: 1.0 }, TrackSize::Fr { value: 3.0 }],
            ..Default::default()
        };
        layout.insert_column(1, TrackSize::Fr { value: 2.0 });
        assert_eq!(layout.columns.len(), 3);
        assert_eq!(layout.columns[1], TrackSize::Fr { value: 2.0 });
    }

    #[test]
    fn remove_column_last_errors() {
        let mut layout = PageLayout {
            columns: vec![TrackSize::Fr { value: 1.0 }],
            ..Default::default()
        };
        assert!(layout.remove_column(0).is_err());
    }

    #[test]
    fn remove_column_valid() {
        let mut layout = PageLayout {
            columns: vec![TrackSize::Fr { value: 1.0 }, TrackSize::Fr { value: 2.0 }],
            ..Default::default()
        };
        layout.remove_column(0).unwrap();
        assert_eq!(layout.columns.len(), 1);
        assert_eq!(layout.columns[0], TrackSize::Fr { value: 2.0 });
    }

    #[test]
    fn resize_column_valid() {
        let mut layout = PageLayout {
            columns: vec![TrackSize::Fr { value: 1.0 }],
            ..Default::default()
        };
        layout
            .resize_column(0, TrackSize::Fixed { value: 300.0 })
            .unwrap();
        assert_eq!(layout.columns[0], TrackSize::Fixed { value: 300.0 });
    }

    #[test]
    fn resize_column_oob() {
        let mut layout = PageLayout::default();
        assert!(layout
            .resize_column(5, TrackSize::Fr { value: 1.0 })
            .is_err());
    }

    #[test]
    fn insert_remove_row() {
        let mut layout = PageLayout::default();
        layout.insert_row(0, TrackSize::Auto);
        layout.insert_row(1, TrackSize::Fr { value: 1.0 });
        assert_eq!(layout.rows.len(), 2);
        layout.remove_row(0).unwrap();
        assert_eq!(layout.rows.len(), 1);
    }

    #[test]
    fn cell_positions_2x3() {
        let layout = PageLayout {
            columns: vec![TrackSize::Fr { value: 1.0 }, TrackSize::Fr { value: 1.0 }],
            rows: vec![TrackSize::Auto, TrackSize::Auto, TrackSize::Auto],
            ..Default::default()
        };
        let cells = layout.cell_positions();
        assert_eq!(cells.len(), 6);
        assert_eq!(cells[0], (0, 0));
        assert_eq!(cells[1], (1, 0));
        assert_eq!(cells[2], (0, 1));
        assert_eq!(cells[5], (1, 2));
    }

    #[test]
    fn cell_positions_empty_layout() {
        let layout = PageLayout::default();
        let cells = layout.cell_positions();
        assert_eq!(cells.len(), 1);
        assert_eq!(cells[0], (0, 0));
    }

    #[test]
    fn empty_cells_computation() {
        let layout = PageLayout {
            columns: vec![TrackSize::Fr { value: 1.0 }, TrackSize::Fr { value: 1.0 }],
            rows: vec![TrackSize::Auto, TrackSize::Auto],
            ..Default::default()
        };
        let occupied = vec![(
            GridPlacement::Line { index: 1 },
            GridPlacement::Line { index: 1 },
        )];
        let empty = layout.empty_cells(&occupied);
        assert_eq!(empty.len(), 3);
        assert!(!empty.contains(&(0, 0)));
        assert!(empty.contains(&(1, 0)));
        assert!(empty.contains(&(0, 1)));
        assert!(empty.contains(&(1, 1)));
    }

    #[test]
    fn grid_placement_resolved_index() {
        assert_eq!(GridPlacement::Auto.resolved_index(), None);
        assert_eq!(GridPlacement::Line { index: 1 }.resolved_index(), Some(0));
        assert_eq!(GridPlacement::Line { index: 3 }.resolved_index(), Some(2));
        assert_eq!(GridPlacement::Span { count: 2 }.resolved_index(), None);
    }
}
