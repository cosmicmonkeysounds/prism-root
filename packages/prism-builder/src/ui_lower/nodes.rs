//! Node-construction helpers — the container / text / image / input
//! primitive constructors every built-in block reuses, split out of
//! `ui_lower/mod.rs` (Phase B.5). `mod.rs` owns `LowerCtx` +
//! `BlockInvalidator` (the lowering context); this module owns the
//! stateless `UiNode`-shaping helpers. Re-exported `pub use nodes::*`
//! so `crate::ui_lower::<helper>` paths stay stable.

use crate::layout::{Dimension, FlexDirection, FlowProps};
use crate::style::StyleProperties;
use prism_ui_runtime::command::{Color, CornerRadius};
use prism_ui_runtime::layout::{
    ContainerProps, Direction, HoverOverrides, Node as UiNode, Padding, Semantic, Sizing, TextProps,
};

pub fn bare_container(
    id: impl Into<String>,
    children: Vec<UiNode>,
    customize: impl FnOnce(&mut ContainerProps),
) -> UiNode {
    let mut props = ContainerProps::default();
    customize(&mut props);
    UiNode::Container {
        id: id.into(),
        props,
        children,
    }
}

/// Attach a [`Semantic`] hint to whichever variant carries one. Used
/// by blocks to declare SSR markup (`<h1>`, `<section>`, alt text)
/// alongside layout vocabulary, in the same `lower_ui` impl, with no
/// per-block walker. `Spacer` ignores the hint (no semantic field).
pub fn with_semantic(node: UiNode, semantic: Semantic) -> UiNode {
    match node {
        UiNode::Container {
            id,
            mut props,
            children,
        } => {
            props.semantic = semantic;
            UiNode::Container {
                id,
                props,
                children,
            }
        }
        UiNode::Text {
            id,
            content,
            mut props,
        } => {
            props.semantic = semantic;
            UiNode::Text { id, content, props }
        }
        UiNode::Image {
            id,
            source,
            width,
            height,
            radius,
            tint,
            ..
        } => UiNode::Image {
            id,
            source,
            width,
            height,
            radius,
            tint,
            semantic,
        },
        UiNode::TextInput {
            id,
            value,
            placeholder,
            props,
            width,
            height,
            radius,
            focused,
            multiline,
            caret_byte,
            selection,
            spans,
            scroll_x,
            scroll_y,
            underline,
            bracket_match,
            highlight_current_line,
            ..
        } => UiNode::TextInput {
            id,
            value,
            placeholder,
            props,
            width,
            height,
            radius,
            semantic,
            focused,
            multiline,
            caret_byte,
            selection,
            spans,
            scroll_x,
            scroll_y,
            underline,
            bracket_match,
            highlight_current_line,
        },
        UiNode::Spacer { .. } => node,
    }
}

/// Build a [`UiNode::Text`] with the cascade colour overridden by an
/// explicit per-block colour string. The "clone the cascade and stamp
/// `color`" dance shows up at every chrome primitive that paints a
/// label in a non-cascade tint (section-header label/badge, toast
/// title/body, future docs/app-card text). Centralising it keeps
/// every call site to one line and frees blocks from owning a tiny
/// private helper for the same shape.
///
/// The colour string follows the same vocabulary as [`parse_color`]
/// (`#rgb` / `#rrggbb` / `#rrggbbaa`); unparseable values silently
/// fall through to the cascade (same shape `text_node` itself uses
/// when `style.color` doesn't parse).
pub fn colored_text_node(
    node_id: String,
    content: String,
    style: &StyleProperties,
    default_size: f32,
    color: &str,
) -> UiNode {
    let mut scoped = style.clone();
    scoped.color = Some(color.into());
    text_node(node_id, content, &scoped, default_size)
}

/// One-line constructor for the most common interactive-primitive
/// hover shape: "swap the background only". Returns `None` when the
/// colour string fails to parse so the caller can `props.hover = ...`
/// unconditionally without a `parse_color`/`HoverOverrides` two-liner
/// at every call site. The full `HoverOverrides` struct stays
/// available for primitives that animate radius / future fields too.
pub fn hover_bg(color: &str) -> Option<HoverOverrides> {
    parse_color(color).map(|c| HoverOverrides {
        background: Some(c),
        radius: None,
    })
}

/// Convenience: equal corner radius on all four corners. Most blocks
/// want this; the long-form struct literal is noise.
pub fn uniform_radius(r: f32) -> CornerRadius {
    CornerRadius {
        tl: r,
        tr: r,
        br: r,
        bl: r,
    }
}

/// Shared "this surface responds to a pointer" tint. Every clickable
/// chrome surface uses the same intensity so hover reads identically
/// across the whole window — palette rows, inspector rows, canvas-doc
/// nodes, field-editor rows, menu pills. Authors who need a stronger
/// or softer tint can still pass any colour to [`hover_bg`] directly.
pub const POINTER_HOVER_TINT: &str = "#1a0060c0";

/// One-liner for the "clickable chrome surface" recipe — bundles
/// the three things every routable container needs into a single
/// call: a hover-bg tint, `data-role`, and (optional) `data-target-id`.
///
/// ```ignore
/// bare_container(node.id.clone(), kids, |p| {
///     p.padding = Padding::all(8.0);
///     p.radius = uniform_radius(4.0);
///     pointer_routing(p, "my-row", target_id);
/// });
/// ```
///
/// The caller still owns the rest of the `Semantic` shape (`tag`,
/// `aria-*`, custom `data-*`) — `pointer_routing` only appends the
/// two routing attrs and the hover tint, so existing builder calls
/// compose cleanly. The function deliberately takes `&mut props` so
/// it threads naturally through `bare_container`'s closure shape.
pub fn pointer_routing(props: &mut ContainerProps, role: &'static str, target_id: &str) {
    props.hover = hover_bg(POINTER_HOVER_TINT);
    let semantic = std::mem::take(&mut props.semantic);
    let mut s = semantic.with_attr("data-role", role);
    if !target_id.is_empty() {
        s = s.with_attr("data-target-id", target_id.to_string());
    }
    props.semantic = s;
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
        ..Default::default()
    }
}

/// Map a builder `Dimension` + `flex_grow` to a runtime `Sizing`.
pub fn sizing_from_dimension(dim: Dimension, flex_grow: f32) -> Sizing {
    match dim {
        Dimension::Px { value } => Sizing::Fixed(value),
        Dimension::Auto if flex_grow > 0.0 => Sizing::Grow,
        Dimension::Percent { value } => Sizing::Percent((value / 100.0).clamp(0.0, 1.0)),
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
        props: TextProps {
            font_size,
            color,
            ..Default::default()
        },
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

/// Construct a `UiNode::TextInput` — the editable single-line input
/// leaf. Same builder shape as [`text_node`] / [`image_node`]: the
/// caller hands over the few fields that vary, and cascade-resolved
/// font_size / colour are pulled from `style`. `value` is what the
/// user has typed (often empty); `placeholder` paints when the value
/// is empty. Width / height drive the runtime `Sizing` policy — most
/// inputs want `Sizing::Grow` along the parent's main axis.
pub fn text_input_node(
    node_id: String,
    value: String,
    placeholder: String,
    style: &StyleProperties,
    width: Sizing,
    height: Sizing,
    default_size: f32,
) -> UiNode {
    text_input_node_with_focus(
        node_id,
        value,
        placeholder,
        style,
        width,
        height,
        default_size,
        false,
    )
}

/// Variant of [`text_input_node`] that lets the caller mark the input
/// as the active focus target. The paint pass uses this to bump the
/// border to the accent colour and draw a 1-px caret bar after the
/// rendered text. Field-editor rows wire this through their
/// `focused` prop so users can see where their keystrokes will land.
#[allow(clippy::too_many_arguments)]
pub fn text_input_node_with_focus(
    node_id: String,
    value: String,
    placeholder: String,
    style: &StyleProperties,
    width: Sizing,
    height: Sizing,
    default_size: f32,
    focused: bool,
) -> UiNode {
    let font_size = style.font_size.unwrap_or(default_size);
    let color = style
        .color
        .as_deref()
        .and_then(parse_color)
        .unwrap_or(DEFAULT_TEXT_COLOR);
    UiNode::TextInput {
        id: node_id,
        value,
        placeholder,
        props: TextProps {
            font_size,
            color,
            ..Default::default()
        },
        width,
        height,
        radius: style.border_radius.map(uniform_radius).unwrap_or_default(),
        semantic: Semantic::default(),
        focused,
        multiline: false,
        caret_byte: None,
        selection: None,
        spans: Vec::new(),
        scroll_x: 0.0,
        scroll_y: 0.0,
        underline: None,
        bracket_match: Vec::new(),
        highlight_current_line: false,
    }
}

/// Editor-grade variant of [`text_input_node_with_focus`]. Threads
/// the host's editor model (caret byte offset, optional selection,
/// multi-line flag, syntax-highlight spans) straight into the
/// runtime node so a single editable primitive backs both single-
/// line string-property rows and multi-line code editors. The
/// shell calls this from `code_editor_props` / the field-focus path.
#[allow(clippy::too_many_arguments)]
pub fn text_input_editor_node(
    node_id: String,
    value: String,
    placeholder: String,
    style: &StyleProperties,
    width: Sizing,
    height: Sizing,
    default_size: f32,
    focused: bool,
    multiline: bool,
    caret_byte: Option<usize>,
    selection: Option<(usize, usize)>,
    spans: Vec<prism_ui_runtime::command::TextSpan>,
    scroll_x: f32,
    scroll_y: f32,
    underline: Option<(usize, usize)>,
) -> UiNode {
    let font_size = style.font_size.unwrap_or(default_size);
    let color = style
        .color
        .as_deref()
        .and_then(parse_color)
        .unwrap_or(DEFAULT_TEXT_COLOR);
    UiNode::TextInput {
        id: node_id,
        value,
        placeholder,
        props: TextProps {
            font_size,
            color,
            ..Default::default()
        },
        width,
        height,
        radius: style.border_radius.map(uniform_radius).unwrap_or_default(),
        semantic: Semantic::default(),
        focused,
        multiline,
        caret_byte,
        selection,
        spans,
        scroll_x,
        scroll_y,
        underline,
        bracket_match: Vec::new(),
        highlight_current_line: false,
    }
}

/// Construct a `UiNode::Image`. Width/height default to `Grow` so an
/// image inside a sized container fills its slot.
pub fn image_node(
    node_id: String,
    source: String,
    style: &StyleProperties,
    width: Sizing,
    height: Sizing,
) -> UiNode {
    let radius = style.border_radius.map(uniform_radius).unwrap_or_default();
    UiNode::Image {
        id: node_id,
        source,
        width,
        height,
        radius,
        tint: None,
        semantic: prism_ui_runtime::layout::Semantic::default(),
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
