//! Builder→runtime lowering: `Component::lower_ui` plumbing + the
//! shared helpers every built-in block reuses.
//!
//! This is the seam called out in the Clay/Taffy migration plan
//! (`docs/dev/clay-migration-plan.md` §3, §6). The old `ui_runtime`
//! translator dispatched on `node.component` with a hard-coded string
//! match — every new block had to teach that match about itself.
//! Now each [`crate::component::Component`] knows how to lower itself
//! to a `prism_ui_runtime::layout::Node`, and the translator is just
//! "look the component up in the registry, call `lower_ui`".
//!
//! The helpers below are the *single source of truth* for the
//! translation primitives blocks reuse:
//!
//! - [`LowerCtx`] — passed to every `lower_ui` impl. Carries the
//!   inherited style cascade and the registry, exposes
//!   [`LowerCtx::lower_children`] for recursion, and
//!   [`LowerCtx::default_container`] as the generic fallback.
//! - [`container_props_from`] — turns a node's `FlowProps` + cascaded
//!   `StyleProperties` into runtime `ContainerProps`. Containers
//!   share this; bespoke blocks (cards, columns) layer on top.
//! - [`parse_color`] — `#rgb` / `#rrggbb` / `#rrggbbaa`. The cascade
//!   resolves to strings; this is where they become runtime `Color`.
//! - [`text_node`] / [`spacer_node`] — convenience constructors so
//!   `TextBlock` / `SpacerBlock` don't reimplement the same shape.
//!
//! Blocks that don't override `lower_ui` get the default container
//! lowering automatically — same behaviour the legacy `ui_runtime`
//! translator gave for unknown component ids.

use prism_ui_runtime::command::{Color, CornerRadius};
use prism_ui_runtime::layout::{
    ContainerProps, Direction, Node as UiNode, Padding, Sizing, TextProps,
};

use crate::document::Node;
use crate::layout::{Dimension, FlexDirection, FlowProps, LayoutMode};
use crate::registry::ComponentRegistry;
use crate::style::{resolve_cascade, StyleProperties};

/// Context threaded through `Component::lower_ui` impls during the
/// `BuilderDocument` → `prism_ui_runtime::layout::Node` walk.
///
/// `parent_style` is *the cascade output for the node currently being
/// lowered* — i.e. for a block authoring its own children, calling
/// [`Self::lower_children`] cascades correctly without the block
/// knowing anything about the cascade.
pub struct LowerCtx<'a> {
    registry: Option<&'a ComponentRegistry>,
    parent_style: &'a StyleProperties,
}

impl<'a> LowerCtx<'a> {
    /// Build a context anchored at a specific parent cascade. Callers
    /// that don't have a meaningful parent style (i.e. the root of a
    /// document) pass a borrow to a `StyleProperties::default()`.
    pub fn new(registry: Option<&'a ComponentRegistry>, parent_style: &'a StyleProperties) -> Self {
        Self {
            registry,
            parent_style,
        }
    }

    /// Lower a single node. The cascade is resolved internally and a
    /// fresh child-scope `LowerCtx` is handed to whichever
    /// `Component::lower_ui` impl owns this node's component id.
    /// Unknown ids fall back to [`Self::default_container`].
    pub fn lower(&self, node: &Node) -> UiNode {
        let style = resolve_cascade(self.parent_style, &StyleProperties::default(), &node.style);
        let child = LowerCtx {
            registry: self.registry,
            parent_style: &style,
        };
        if let Some(reg) = self.registry {
            if let Some(comp) = reg.get(&node.component) {
                return comp.lower_ui(&child, node, &style);
            }
        }
        child.default_container(node, &style)
    }

    /// Recurse into a slice of children with this context's cascade
    /// as their parent. Blocks that wrap their children call this.
    pub fn lower_children(&self, children: &[Node]) -> Vec<UiNode> {
        children.iter().map(|c| self.lower(c)).collect()
    }

    /// Generic container lowering — what `Component::lower_ui` falls
    /// back to when a block doesn't override the method. Mirrors the
    /// pre-migration `translate_container` behaviour exactly.
    pub fn default_container(&self, node: &Node, style: &StyleProperties) -> UiNode {
        let flow = match &node.layout_mode {
            LayoutMode::Flow(f) | LayoutMode::Relative(f) => Some(f),
            _ => None,
        };
        UiNode::Container {
            id: node.id.clone(),
            props: container_props_from(flow, style),
            children: self.lower_children(&node.children),
        }
    }

    /// Cascade output the *current scope* sees as its inherited
    /// style. Useful for blocks that need to peek at parent values
    /// without owning the cascade machinery.
    pub fn parent_style(&self) -> &StyleProperties {
        self.parent_style
    }
}

/// Build runtime `ContainerProps` from a node's `FlowProps` + cascade.
/// Single source of truth — every container-shaped block routes here.
pub fn container_props_from(flow: Option<&FlowProps>, style: &StyleProperties) -> ContainerProps {
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

/// Map a builder `Dimension` + `flex_grow` to a runtime `Sizing`.
pub fn sizing_from_dimension(dim: Dimension, flex_grow: f32) -> Sizing {
    match dim {
        Dimension::Px { value } => Sizing::Fixed(value),
        Dimension::Auto if flex_grow > 0.0 => Sizing::Grow,
        // Taffy handles percentages natively, but the runtime `Sizing`
        // vocabulary doesn't carry them yet — collapse to `Grow` as a
        // best-effort. Lands properly when Phase 4 grows the primitive.
        Dimension::Percent { .. } => Sizing::Grow,
        Dimension::Auto => Sizing::Fit,
    }
}

/// Construct a `UiNode::Text` with cascade-resolved size/color and a
/// per-block default font size (paragraph / heading / code differ).
pub fn text_node(
    node_id: String,
    content: String,
    style: &StyleProperties,
    default_size: f32,
) -> UiNode {
    let font_size = style.font_size.unwrap_or(default_size);
    let color = style
        .color
        .as_deref()
        .and_then(parse_color)
        .unwrap_or(DEFAULT_TEXT_COLOR);
    UiNode::Text {
        id: node_id,
        content,
        props: TextProps { font_size, color },
    }
}

/// Construct a `UiNode::Spacer`. Trivial wrapper, kept here so all
/// node constructors live in one place.
pub fn spacer_node(node_id: String, width: f32, height: f32) -> UiNode {
    UiNode::Spacer {
        id: node_id,
        width,
        height,
    }
}

const DEFAULT_TEXT_COLOR: Color = Color {
    r: 20,
    g: 20,
    b: 20,
    a: 255,
};

/// Tiny CSS-color parser — `#rgb`, `#rrggbb`, `#rrggbbaa`. Anything
/// else returns `None` and the caller falls back to a default. Richer
/// parsing (named colours, `rgb(...)`, `oklch(...)`) lands with the
/// design-tokens cascade wiring.
pub fn parse_color(s: &str) -> Option<Color> {
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
