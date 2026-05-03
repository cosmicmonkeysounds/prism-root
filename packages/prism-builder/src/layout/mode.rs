//! Per-node layout mode and its supporting field types.
//!
//! Submodule of [`crate::layout`]. Re-exported from there.

use prism_core::foundation::geometry::Edges;
use serde::{Deserialize, Serialize};
use taffy::prelude::*;

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
