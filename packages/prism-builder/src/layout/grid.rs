//! Page-level grid (cells, edges, page size, margins, tracks).
//!
//! Submodule of [`crate::layout`]. Re-exported from there; external
//! callers should import via the parent.

use prism_core::foundation::geometry::{Edges, Rect, Size2};
use prism_luau_derive::Editable;
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

// ── Page layout ──────────────────────────────────────────────────────

/// Physical page dimensions.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize, Editable)]
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
#[derive(Debug, Clone, Serialize, Deserialize, Editable)]
pub struct PageLayout {
    /// Tagged enum (with a `Custom { width, height }` struct-variant
    /// payload). Edited via `size.@kind = "<variant>"` to reseat, then
    /// `size.custom.width = "1280"` etc. for the payload fields.
    #[serde(default)]
    pub size: PageSize,
    #[edit(skip)]
    #[serde(default)]
    pub orientation: Orientation,
    /// Each edge addressable as `margins.top` / `margins.right` / …
    /// via `Edges::apply_path`.
    #[serde(default)]
    pub margins: Edges<f32>,
    #[edit(skip)]
    #[serde(default)]
    pub bleed: f32,
    #[edit(skip)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grid: Option<GridCell>,
    /// `column_gap` mirrors into `row_gap` so a single property-panel
    /// input drives both axes.
    #[edit(also = "row_gap")]
    #[serde(default)]
    pub column_gap: f32,
    #[edit(skip)]
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

    pub(super) fn build_taffy_style(&self, page_size: Size2) -> Style {
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

#[cfg(test)]
mod editable_tests {
    use super::*;

    #[test]
    fn margins_dispatch_each_edge() {
        let mut pl = PageLayout::default();
        pl.apply_path("margins.top", "8");
        pl.apply_path("margins.right", "12");
        pl.apply_path("margins.bottom", "16");
        pl.apply_path("margins.left", "4");
        assert_eq!(pl.margins.top, 8.0);
        assert_eq!(pl.margins.right, 12.0);
        assert_eq!(pl.margins.bottom, 16.0);
        assert_eq!(pl.margins.left, 4.0);
    }

    #[test]
    fn also_attribute_fans_column_gap_to_row_gap() {
        let mut pl = PageLayout::default();
        pl.apply_path("column_gap", "24");
        assert_eq!(pl.column_gap, 24.0);
        assert_eq!(pl.row_gap, 24.0);
    }

    #[test]
    fn page_size_terminal_value_reseats_unit_variant() {
        let mut pl = PageLayout::default();
        pl.apply_path("size", "a4");
        assert!(matches!(pl.size, PageSize::A4));
    }

    #[test]
    fn page_size_kind_reseats_then_payload_writes() {
        let mut pl = PageLayout::default();
        pl.apply_path("size.@kind", "custom");
        pl.apply_path("size.custom.width", "1280");
        pl.apply_path("size.custom.height", "800");
        match pl.size {
            PageSize::Custom { width, height } => {
                assert_eq!(width, 1280.0);
                assert_eq!(height, 800.0);
            }
            _ => panic!("expected Custom variant"),
        }
    }

    #[test]
    fn payload_write_to_wrong_variant_is_noop() {
        let mut pl = PageLayout::default();
        pl.apply_path("size", "a4");
        pl.apply_path("size.custom.width", "1280");
        assert!(matches!(pl.size, PageSize::A4));
    }

    #[test]
    fn unparseable_margin_is_silent_noop() {
        let mut pl = PageLayout::default();
        pl.margins.top = 5.0;
        pl.apply_path("margins.top", "not-a-number");
        assert_eq!(pl.margins.top, 5.0);
    }

    #[test]
    fn unknown_keys_are_noop() {
        let mut pl = PageLayout {
            column_gap: 3.0,
            ..PageLayout::default()
        };
        pl.apply_path("totally_unknown", "99");
        assert_eq!(pl.column_gap, 3.0);
    }
}
