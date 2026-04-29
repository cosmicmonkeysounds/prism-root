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
use prism_core::foundation::geometry::{Edges, Point2, Rect, Size2};
use prism_core::foundation::spatial::ComputedTransform;
use serde::{Deserialize, Serialize};
use taffy::prelude::*;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum GridEditError {
    #[error("index out of bounds: {0}")]
    IndexOutOfBounds(usize),
    #[error("cannot remove last cell")]
    CannotRemoveLastCell,
    #[error("no grid defined")]
    NoGrid,
    #[error("target is not a leaf cell")]
    NotALeaf,
}

// ── Recursive grid cell tree ────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SplitDirection {
    Horizontal,
    Vertical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CellEdge {
    Top,
    Bottom,
    Left,
    Right,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum GridCell {
    Leaf {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        node_id: Option<String>,
    },
    Split {
        direction: SplitDirection,
        tracks: Vec<TrackSize>,
        #[serde(default)]
        gap: f32,
        children: Vec<GridCell>,
    },
}

impl GridCell {
    pub fn leaf() -> Self {
        Self::Leaf { node_id: None }
    }

    pub fn leaf_with(id: impl Into<String>) -> Self {
        Self::Leaf {
            node_id: Some(id.into()),
        }
    }

    pub fn split(
        direction: SplitDirection,
        tracks: Vec<TrackSize>,
        gap: f32,
        children: Vec<GridCell>,
    ) -> Self {
        Self::Split {
            direction,
            tracks,
            gap,
            children,
        }
    }

    pub fn is_leaf(&self) -> bool {
        matches!(self, Self::Leaf { .. })
    }

    pub fn node_id(&self) -> Option<&str> {
        match self {
            Self::Leaf { node_id } => node_id.as_deref(),
            Self::Split { .. } => None,
        }
    }

    pub fn at(&self, path: &[usize]) -> Option<&GridCell> {
        if path.is_empty() {
            return Some(self);
        }
        match self {
            Self::Split { children, .. } => children.get(path[0]).and_then(|c| c.at(&path[1..])),
            Self::Leaf { .. } => None,
        }
    }

    pub fn at_mut(&mut self, path: &[usize]) -> Option<&mut GridCell> {
        if path.is_empty() {
            return Some(self);
        }
        match self {
            Self::Split { children, .. } => {
                children.get_mut(path[0]).and_then(|c| c.at_mut(&path[1..]))
            }
            Self::Leaf { .. } => None,
        }
    }

    pub fn leaf_count(&self) -> usize {
        match self {
            Self::Leaf { .. } => 1,
            Self::Split { children, .. } => children.iter().map(|c| c.leaf_count()).sum(),
        }
    }

    pub fn collect_node_ids(&self) -> Vec<String> {
        let mut ids = Vec::new();
        self.walk_leaves(&mut |_, nid| {
            if let Some(id) = nid {
                ids.push(id.to_string());
            }
        });
        ids
    }

    pub fn walk_leaves(&self, f: &mut impl FnMut(&[usize], Option<&str>)) {
        self.walk_leaves_inner(&mut Vec::new(), f);
    }

    fn walk_leaves_inner(&self, path: &mut Vec<usize>, f: &mut impl FnMut(&[usize], Option<&str>)) {
        match self {
            Self::Leaf { node_id } => f(path, node_id.as_deref()),
            Self::Split { children, .. } => {
                for (i, child) in children.iter().enumerate() {
                    path.push(i);
                    child.walk_leaves_inner(path, f);
                    path.pop();
                }
            }
        }
    }
}

pub fn path_to_string(path: &[usize]) -> String {
    path.iter()
        .map(|i| i.to_string())
        .collect::<Vec<_>>()
        .join(".")
}

pub fn path_from_string(s: &str) -> Vec<usize> {
    if s.is_empty() {
        return Vec::new();
    }
    s.split('.').filter_map(|p| p.parse().ok()).collect()
}

pub struct FlatCell {
    pub path: Vec<usize>,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub node_id: Option<String>,
}

pub struct EdgeHandle {
    pub cell_path: Vec<usize>,
    pub edge: CellEdge,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub is_gap: bool,
    pub parent_path: Vec<usize>,
    pub gap_index: usize,
    pub orientation: SplitDirection,
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

/// Structural layout properties of a page. The grid is a recursive
/// `GridCell` tree where each cell can be independently split
/// horizontally (columns) or vertically (rows).
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grid: Option<GridCell>,
    #[serde(default)]
    pub column_gap: f32,
    #[serde(default)]
    pub row_gap: f32,
}

impl PageLayout {
    pub fn has_grid(&self) -> bool {
        self.grid.is_some()
    }

    pub fn leaf_count(&self) -> usize {
        self.grid.as_ref().map_or(0, |g| g.leaf_count())
    }

    /// Resolve the page dimensions, applying orientation. Returns
    /// `None` for `Responsive` pages.
    pub fn resolved_size(&self) -> Option<Size2> {
        self.size.to_pixels().map(|s| match self.orientation {
            Orientation::Portrait => s,
            Orientation::Landscape => Size2::new(s.height, s.width),
        })
    }

    pub fn content_rect(&self, page_size: Size2) -> Rect {
        Rect::new(0.0, 0.0, page_size.width, page_size.height).inset(&self.margins)
    }

    pub fn bleed_rect(&self, page_size: Size2) -> Rect {
        let bleed_edges = Edges::all(self.bleed);
        Rect::new(0.0, 0.0, page_size.width, page_size.height).outset(&bleed_edges)
    }

    pub fn flatten_cells(&self, content_w: f32, content_h: f32) -> Vec<FlatCell> {
        let mut out = Vec::new();
        if let Some(grid) = &self.grid {
            let mut path = Vec::new();
            flatten_cells_rec(grid, 0.0, 0.0, content_w, content_h, &mut path, &mut out);
        }
        out
    }

    pub fn flatten_edge_handles(&self, content_w: f32, content_h: f32) -> Vec<EdgeHandle> {
        let mut out = Vec::new();
        if let Some(grid) = &self.grid {
            let mut path = Vec::new();
            flatten_edges_rec(
                grid,
                0.0,
                0.0,
                content_w,
                content_h,
                &mut path,
                EdgeSuppression::default(),
                &mut out,
            );
        }
        out
    }

    pub fn resize_gap(
        &mut self,
        parent_path: &[usize],
        gap_index: usize,
        delta: f32,
        available: f32,
    ) -> Result<(), GridEditError> {
        let grid = self.grid.as_mut().ok_or(GridEditError::NoGrid)?;
        let parent = if parent_path.is_empty() {
            grid
        } else {
            grid.at_mut(parent_path)
                .ok_or(GridEditError::IndexOutOfBounds(0))?
        };

        match parent {
            GridCell::Split { tracks, gap, .. } => {
                if gap_index + 1 >= tracks.len() {
                    return Err(GridEditError::IndexOutOfBounds(gap_index));
                }

                let sizes = compute_track_sizes(tracks, *gap, available);
                let size_a = sizes[gap_index];
                let size_b = sizes[gap_index + 1];
                let min_size = 20.0_f32;
                let total = size_a + size_b;

                let new_a = (size_a + delta).clamp(min_size, total - min_size);
                let new_b = total - new_a;

                let total_fr = match (&tracks[gap_index], &tracks[gap_index + 1]) {
                    (TrackSize::Fr { value: a }, TrackSize::Fr { value: b }) => a + b,
                    _ => 2.0,
                };

                tracks[gap_index] = TrackSize::Fr {
                    value: total_fr * new_a / total,
                };
                tracks[gap_index + 1] = TrackSize::Fr {
                    value: total_fr * new_b / total,
                };

                Ok(())
            }
            _ => Err(GridEditError::IndexOutOfBounds(0)),
        }
    }

    pub fn insert_at_edge(
        &mut self,
        cell_path: &[usize],
        edge: CellEdge,
    ) -> Result<(), GridEditError> {
        let desired_dir = match edge {
            CellEdge::Left | CellEdge::Right => SplitDirection::Horizontal,
            CellEdge::Top | CellEdge::Bottom => SplitDirection::Vertical,
        };
        let insert_before = matches!(edge, CellEdge::Left | CellEdge::Top);

        let grid = self.grid.as_mut().ok_or(GridEditError::NoGrid)?;

        if cell_path.is_empty() {
            let old = std::mem::replace(grid, GridCell::leaf());
            let gap = match desired_dir {
                SplitDirection::Horizontal => self.column_gap,
                SplitDirection::Vertical => self.row_gap,
            };
            let children = if insert_before {
                vec![GridCell::leaf(), old]
            } else {
                vec![old, GridCell::leaf()]
            };
            *grid = GridCell::split(
                desired_dir,
                vec![TrackSize::Fr { value: 1.0 }; 2],
                gap,
                children,
            );
            return Ok(());
        }

        let (parent_path, tail) = cell_path.split_at(cell_path.len() - 1);
        let child_idx = tail[0];
        let default_gap = match desired_dir {
            SplitDirection::Horizontal => self.column_gap,
            SplitDirection::Vertical => self.row_gap,
        };

        let parent = grid
            .at_mut(parent_path)
            .ok_or(GridEditError::IndexOutOfBounds(0))?;

        match parent {
            GridCell::Split {
                direction,
                tracks,
                children,
                ..
            } => {
                if *direction == desired_dir {
                    let ins = if insert_before {
                        child_idx
                    } else {
                        child_idx + 1
                    };
                    children.insert(ins, GridCell::leaf());
                    tracks.insert(ins, TrackSize::Fr { value: 1.0 });
                } else {
                    let old = children[child_idx].clone();
                    let new_children = if insert_before {
                        vec![GridCell::leaf(), old]
                    } else {
                        vec![old, GridCell::leaf()]
                    };
                    children[child_idx] = GridCell::split(
                        desired_dir,
                        vec![TrackSize::Fr { value: 1.0 }; 2],
                        default_gap,
                        new_children,
                    );
                }
            }
            GridCell::Leaf { .. } => return Err(GridEditError::IndexOutOfBounds(0)),
        }
        Ok(())
    }

    pub fn remove_cell(&mut self, cell_path: &[usize]) -> Result<(), GridEditError> {
        let grid = self.grid.as_mut().ok_or(GridEditError::NoGrid)?;

        if cell_path.is_empty() {
            return Err(GridEditError::CannotRemoveLastCell);
        }

        let (parent_path, tail) = cell_path.split_at(cell_path.len() - 1);
        let child_idx = tail[0];

        let parent = grid
            .at_mut(parent_path)
            .ok_or(GridEditError::IndexOutOfBounds(0))?;

        match parent {
            GridCell::Split {
                tracks, children, ..
            } => {
                if children.len() <= 1 {
                    return Err(GridEditError::CannotRemoveLastCell);
                }
                if child_idx >= children.len() {
                    return Err(GridEditError::IndexOutOfBounds(child_idx));
                }
                children.remove(child_idx);
                if child_idx < tracks.len() {
                    tracks.remove(child_idx);
                }
                if children.len() == 1 {
                    let remaining = children.remove(0);
                    *parent = remaining;
                }
            }
            _ => return Err(GridEditError::IndexOutOfBounds(0)),
        }
        Ok(())
    }

    pub fn place_node_at(&mut self, path: &[usize], node_id: String) -> Result<(), GridEditError> {
        let grid = self.grid.as_mut().ok_or(GridEditError::NoGrid)?;
        let cell = grid
            .at_mut(path)
            .ok_or(GridEditError::IndexOutOfBounds(0))?;
        match cell {
            GridCell::Leaf { node_id: nid } => {
                *nid = Some(node_id);
                Ok(())
            }
            GridCell::Split { .. } => Err(GridEditError::NotALeaf),
        }
    }

    pub fn clear_cell(&mut self, path: &[usize]) -> Result<(), GridEditError> {
        let grid = self.grid.as_mut().ok_or(GridEditError::NoGrid)?;
        let cell = grid
            .at_mut(path)
            .ok_or(GridEditError::IndexOutOfBounds(0))?;
        match cell {
            GridCell::Leaf { node_id } => {
                *node_id = None;
                Ok(())
            }
            GridCell::Split { .. } => Err(GridEditError::NotALeaf),
        }
    }

    fn build_taffy_style(&self, page_size: Size2) -> Style {
        let content = self.content_rect(page_size);
        Style {
            display: Display::Grid,
            size: taffy::Size {
                width: length(content.width()),
                height: length(content.height()),
            },
            grid_template_columns: vec![minmax(auto(), fr(1.0))],
            grid_template_rows: vec![minmax(auto(), auto())],
            gap: taffy::Size {
                width: length(self.column_gap),
                height: length(self.row_gap),
            },
            ..Default::default()
        }
    }
}

fn flatten_cells_rec(
    cell: &GridCell,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    path: &mut Vec<usize>,
    out: &mut Vec<FlatCell>,
) {
    match cell {
        GridCell::Leaf { node_id } => {
            out.push(FlatCell {
                path: path.clone(),
                x,
                y,
                width: w,
                height: h,
                node_id: node_id.clone(),
            });
        }
        GridCell::Split {
            direction,
            tracks,
            gap,
            children,
        } => {
            let available = match direction {
                SplitDirection::Horizontal => w,
                SplitDirection::Vertical => h,
            };
            let sizes = compute_track_sizes(tracks, *gap, available);
            let mut offset = 0.0;
            for (i, child) in children.iter().enumerate() {
                let sz = sizes.get(i).copied().unwrap_or(0.0);
                path.push(i);
                match direction {
                    SplitDirection::Horizontal => {
                        flatten_cells_rec(child, x + offset, y, sz, h, path, out);
                    }
                    SplitDirection::Vertical => {
                        flatten_cells_rec(child, x, y + offset, w, sz, path, out);
                    }
                }
                path.pop();
                offset += sz + gap;
            }
        }
    }
}

#[derive(Default, Clone, Copy)]
struct EdgeSuppression {
    top: bool,
    bottom: bool,
    left: bool,
    right: bool,
}

impl EdgeSuppression {
    fn is_suppressed(self, edge: CellEdge) -> bool {
        match edge {
            CellEdge::Top => self.top,
            CellEdge::Bottom => self.bottom,
            CellEdge::Left => self.left,
            CellEdge::Right => self.right,
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn flatten_edges_rec(
    cell: &GridCell,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    path: &mut Vec<usize>,
    suppress: EdgeSuppression,
    out: &mut Vec<EdgeHandle>,
) {
    let handle_zone = 12.0_f32;
    match cell {
        GridCell::Leaf { .. } => {
            for &edge in &[
                CellEdge::Top,
                CellEdge::Bottom,
                CellEdge::Left,
                CellEdge::Right,
            ] {
                if suppress.is_suppressed(edge) {
                    continue;
                }
                let (hx, hy, hw, hh) = match edge {
                    CellEdge::Top => (x, y - handle_zone / 2.0, w, handle_zone),
                    CellEdge::Bottom => (x, y + h - handle_zone / 2.0, w, handle_zone),
                    CellEdge::Left => (x - handle_zone / 2.0, y, handle_zone, h),
                    CellEdge::Right => (x + w - handle_zone / 2.0, y, handle_zone, h),
                };
                out.push(EdgeHandle {
                    cell_path: path.clone(),
                    edge,
                    x: hx,
                    y: hy,
                    width: hw,
                    height: hh,
                    is_gap: false,
                    parent_path: Vec::new(),
                    gap_index: 0,
                    orientation: SplitDirection::Horizontal,
                });
            }
        }
        GridCell::Split {
            direction,
            tracks,
            gap,
            children,
        } => {
            let available = match direction {
                SplitDirection::Horizontal => w,
                SplitDirection::Vertical => h,
            };
            let sizes = compute_track_sizes(tracks, *gap, available);
            let mut offset = 0.0;
            let gap_val = *gap;

            for (i, child) in children.iter().enumerate() {
                let sz = sizes.get(i).copied().unwrap_or(0.0);

                let mut child_suppress = suppress;
                match direction {
                    SplitDirection::Horizontal => {
                        if i > 0 {
                            child_suppress.left = true;
                        }
                        if i < children.len() - 1 {
                            child_suppress.right = true;
                        }
                    }
                    SplitDirection::Vertical => {
                        if i > 0 {
                            child_suppress.top = true;
                        }
                        if i < children.len() - 1 {
                            child_suppress.bottom = true;
                        }
                    }
                }

                path.push(i);
                match direction {
                    SplitDirection::Horizontal => {
                        flatten_edges_rec(child, x + offset, y, sz, h, path, child_suppress, out);
                    }
                    SplitDirection::Vertical => {
                        flatten_edges_rec(child, x, y + offset, w, sz, path, child_suppress, out);
                    }
                }
                path.pop();

                if i < children.len() - 1 {
                    let mut add_path = path.clone();
                    add_path.push(i);
                    let (add_edge, gx, gy, gw, gh) = match direction {
                        SplitDirection::Horizontal => {
                            let center_x = x + offset + sz + gap_val / 2.0;
                            let zone_w = gap_val.max(handle_zone);
                            (CellEdge::Right, center_x - zone_w / 2.0, y, zone_w, h)
                        }
                        SplitDirection::Vertical => {
                            let center_y = y + offset + sz + gap_val / 2.0;
                            let zone_h = gap_val.max(handle_zone);
                            (CellEdge::Bottom, x, center_y - zone_h / 2.0, w, zone_h)
                        }
                    };

                    out.push(EdgeHandle {
                        cell_path: add_path,
                        edge: add_edge,
                        x: gx,
                        y: gy,
                        width: gw,
                        height: gh,
                        is_gap: true,
                        parent_path: path.clone(),
                        gap_index: i,
                        orientation: *direction,
                    });
                }

                offset += sz + gap_val;
            }
        }
    }
}

pub fn compute_track_sizes(tracks: &[TrackSize], gap: f32, available: f32) -> Vec<f32> {
    if tracks.is_empty() {
        return vec![available];
    }
    let num_gaps = if tracks.len() > 1 {
        tracks.len() - 1
    } else {
        0
    };
    let total_gap = gap * num_gaps as f32;
    let track_space = (available - total_gap).max(0.0);

    let total_fr: f32 = tracks
        .iter()
        .map(|t| match t {
            TrackSize::Fr { value } => *value,
            _ => 0.0,
        })
        .sum();

    let fixed_space: f32 = tracks
        .iter()
        .map(|t| match t {
            TrackSize::Fixed { value } => *value,
            TrackSize::Percent { value } => available * value / 100.0,
            TrackSize::MinMax { min, .. } => *min,
            _ => 0.0,
        })
        .sum();

    let fr_available = (track_space - fixed_space).max(0.0);
    let fr_unit = if total_fr > 0.0 {
        fr_available / total_fr
    } else {
        0.0
    };

    tracks
        .iter()
        .map(|t| match t {
            TrackSize::Fixed { value } => *value,
            TrackSize::Fr { value } => fr_unit * value,
            TrackSize::Auto => {
                if total_fr > 0.0 {
                    0.0
                } else {
                    track_space / tracks.len() as f32
                }
            }
            TrackSize::MinMax { min, max } => fr_unit.clamp(*min, *max),
            TrackSize::Percent { value } => available * value / 100.0,
        })
        .collect()
}

impl Default for PageLayout {
    fn default() -> Self {
        Self {
            size: PageSize::default(),
            orientation: Orientation::default(),
            margins: Edges::ZERO,
            bleed: 0.0,
            grid: None,
            column_gap: 0.0,
            row_gap: 0.0,
        }
    }
}

// ── Per-node layout mode ─────────────────────────────────────────────

/// How a node participates in its parent's layout.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "kebab-case")]
pub enum LayoutMode {
    /// Positioned by the parent's flow (flex/grid/block).
    Flow(FlowProps),
    /// Removed from flow — positioned by `Transform2D` alone.
    /// Legacy mode: no anchor resolution, no explicit size.
    Free,
    /// Removed from flow. Positioned by `Transform2D.position` +
    /// `Transform2D.anchor` relative to the parent's rect. The parent
    /// is the positioning anchor — whether that's a grid cell, a
    /// container component, or the page itself.
    Absolute(AbsoluteProps),
    /// Participates in the parent's flow layout (like `Flow`), but
    /// `Transform2D.position` is applied as an offset *after* the
    /// flow-computed position (like CSS `position: relative`).
    Relative(FlowProps),
}

impl LayoutMode {
    pub fn is_in_flow(&self) -> bool {
        matches!(self, Self::Flow(_) | Self::Relative(_))
    }

    pub fn is_positioned(&self) -> bool {
        matches!(self, Self::Absolute(_) | Self::Relative(_) | Self::Free)
    }

    pub fn flow_props(&self) -> Option<&FlowProps> {
        match self {
            Self::Flow(f) | Self::Relative(f) => Some(f),
            _ => None,
        }
    }
}

impl Default for LayoutMode {
    fn default() -> Self {
        Self::Flow(FlowProps::default())
    }
}

/// Properties for absolutely-positioned nodes. The node is removed
/// from the parent's flow and positioned by its `Transform2D.position`
/// + `Transform2D.anchor` relative to the parent's rect.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AbsoluteProps {
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
}

impl Default for AbsoluteProps {
    fn default() -> Self {
        Self {
            width: Dimension::Auto,
            height: Dimension::Auto,
            min_width: Dimension::Auto,
            min_height: Dimension::Auto,
            max_width: Dimension::Auto,
            max_height: Dimension::Auto,
        }
    }
}

impl AbsoluteProps {
    pub fn to_taffy_style(&self) -> Style {
        Style {
            position: Position::Absolute,
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
            ..Default::default()
        }
    }

    pub fn fixed(width: f32, height: f32) -> Self {
        Self {
            width: Dimension::Px { value: width },
            height: Dimension::Px { value: height },
            ..Default::default()
        }
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
    pub fn to_taffy(self) -> taffy::style::Dimension {
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
