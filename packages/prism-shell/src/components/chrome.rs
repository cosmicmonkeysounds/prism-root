//! Shared visual helpers for shell-chrome primitives.
//!
//! When a primitive declares the *same* visual shape another primitive
//! already owns — an icon button glyph, a separator hairline, an indent
//! dot — the recipe lives here exactly once and every consumer composes
//! through one call. This is the "rule of three" graduation point for
//! shell-chrome helpers; promotion happens when the second consumer
//! arrives and a third is on the punch list.
//!
//! Smart pattern: each helper returns a fully-formed [`UiNode`] built
//! through the existing `prism_builder::ui_lower` constructors
//! (`bare_container`, `image_node`, `parse_color`). No new abstraction
//! layer, no new builder type — just composition over the
//! already-shared `ui_lower` namespace.

use prism_builder::style::StyleProperties;
use prism_builder::ui_lower::{
    bare_container, colored_text_node, hover_bg, image_node, parse_color, tinted_image_node,
    uniform_radius,
};
use prism_ui_runtime::command::Color;
use prism_ui_runtime::layout::{Direction, Node as UiNode, Padding, Semantic, Sizing};

/// 28×28 fixed-size, 6px-radius button frame holding a 16×16 icon
/// glyph. Resting background transparent; hover swaps to a translucent
/// foreground tint when `enabled` is true. Disabled buttons stay
/// static under the pointer.
///
/// SSR semantic: `<button type="button">` with optional
/// `aria-label` and a `disabled` attr when not enabled.
///
/// Used by [`super::IconButton`] (the standalone Block) and embedded
/// inside [`super::InspectorRow`] for the move-up / move-down / trash
/// chevrons. Future button-shaped chrome primitives (Tab pills,
/// MenuBar items) compose through the same call.
pub const ICON_BUTTON_SIZE: f32 = 28.0;
pub const ICON_BUTTON_RADIUS: f32 = 6.0;
pub const ICON_GLYPH_SIZE: f32 = 16.0;
/// Hover background — `Palette.control-background` in the original
/// Slint. Hard-coded until the design-tokens cascade resolves the
/// value at lower-time.
pub const ICON_BUTTON_HOVER_BG: &str = "#1f000000";

pub fn icon_button_node(
    id: impl Into<String>,
    icon: impl Into<String>,
    enabled: bool,
    aria_label: Option<&str>,
) -> UiNode {
    icon_button_node_tinted(id, icon, enabled, aria_label, None)
}

/// Tinted variant of [`icon_button_node`] — paints the glyph through
/// `tint` as a mask. The original Slint shell drove icon colour via
/// the `colorize` property; in the runtime that's a `Node::Image`
/// `tint`. `None` falls back to the as-authored monochrome render.
pub fn icon_button_node_tinted(
    id: impl Into<String>,
    icon: impl Into<String>,
    enabled: bool,
    aria_label: Option<&str>,
    tint: Option<Color>,
) -> UiNode {
    let id = id.into();
    let glyph_id = format!("{id}::glyph");
    let glyph_style = StyleProperties::default();
    let glyph = match tint {
        Some(c) => tinted_image_node(
            glyph_id,
            icon.into(),
            &glyph_style,
            Sizing::Fixed(ICON_GLYPH_SIZE),
            Sizing::Fixed(ICON_GLYPH_SIZE),
            c,
        ),
        None => image_node(
            glyph_id,
            icon.into(),
            // No cascade context here — embedded buttons don't inherit
            // a parent text colour for their glyph. Image lowering only
            // reads `style.color` as a default tint, which we don't want.
            &glyph_style,
            Sizing::Fixed(ICON_GLYPH_SIZE),
            Sizing::Fixed(ICON_GLYPH_SIZE),
        ),
    };

    bare_container(id, vec![glyph], |props| {
        props.width = Sizing::Fixed(ICON_BUTTON_SIZE);
        props.height = Sizing::Fixed(ICON_BUTTON_SIZE);
        props.radius = uniform_radius(ICON_BUTTON_RADIUS);
        if enabled {
            props.hover = hover_bg(ICON_BUTTON_HOVER_BG);
        }
        props.semantic = Semantic::button()
            .with_aria_label_opt(aria_label)
            .with_attr_if(!enabled, "disabled", "disabled");
    })
}

/// Tiny indent dot — the 6×6 marker the inspector / outline / dock
/// list use to anchor each row visually. `radius_px` typically 1px
/// for "row" shapes (square-ish) and 3px for "node" shapes (circular).
pub fn indent_dot(id: impl Into<String>, color: &str, radius_px: f32) -> UiNode {
    bare_container(id, vec![], |props| {
        props.width = Sizing::Fixed(6.0);
        props.height = Sizing::Fixed(6.0);
        props.radius = uniform_radius(radius_px);
        props.background = parse_color(color);
    })
}

/// 24px-tall horizontal-drag-to-edit number scrubber. Visual recipe
/// shared between [`super::DragNumberField`] (the standalone Block)
/// and [`super::TransformEditor`] (which embeds 5+ instances per
/// rendered row). Outer 24px container with 3px radius and a hover-bg
/// swap; inner row carries the optional 11px label and the formatted
/// value, with `label_color` controlling the per-axis tint
/// (red/green/transparent etc.).
///
/// `key` populates the SSR `data-key` attr; aria-label is derived from
/// `label` when set.
pub const DRAG_NUMBER_HEIGHT: f32 = 24.0;
pub const DRAG_NUMBER_RADIUS: f32 = 3.0;
pub const DRAG_NUMBER_RESTING_BG: &str = "#08000000";
pub const DRAG_NUMBER_HOVER_BG: &str = "#14000000";
pub const DRAG_NUMBER_VALUE_COLOR: &str = "#000000";
pub const DRAG_NUMBER_LABEL_COLOR: &str = "#99000000";
pub const DRAG_NUMBER_LABEL_SIZE: f32 = 11.0;
pub const DRAG_NUMBER_VALUE_SIZE: f32 = 11.0;

pub fn drag_number_field_node(
    id: impl Into<String>,
    label: &str,
    label_color: &str,
    value_text: String,
    key: &str,
) -> UiNode {
    let id = id.into();
    let style = StyleProperties::default();
    let mut row_children: Vec<UiNode> = Vec::with_capacity(2);
    if !label.is_empty() {
        row_children.push(colored_text_node(
            format!("{id}::label"),
            label.into(),
            &style,
            DRAG_NUMBER_LABEL_SIZE,
            label_color,
        ));
    }
    row_children.push(colored_text_node(
        format!("{id}::value"),
        value_text,
        &style,
        DRAG_NUMBER_VALUE_SIZE,
        DRAG_NUMBER_VALUE_COLOR,
    ));

    let row = bare_container(format!("{id}::row"), row_children, |p| {
        p.direction = Direction::Row;
        p.gap = 4.0;
        p.padding = Padding {
            left: 6.0,
            right: 6.0,
            top: 0.0,
            bottom: 0.0,
        };
        p.height = Sizing::Grow;
    });

    bare_container(id, vec![row], |props| {
        props.height = Sizing::Fixed(DRAG_NUMBER_HEIGHT);
        props.radius = uniform_radius(DRAG_NUMBER_RADIUS);
        props.background = parse_color(DRAG_NUMBER_RESTING_BG);
        props.hover = hover_bg(DRAG_NUMBER_HOVER_BG);
        let mut semantic = Semantic::tag("label").with_attr("data-key", key);
        if !label.is_empty() {
            semantic = semantic.with_attr("aria-label", label);
        }
        props.semantic = semantic;
    })
}

/// Format a number Slint-DragNumberField-style: `Math.round(value * 100) / 100`,
/// trailing-zero / trailing-dot trimmed (so integers render as `"3"`,
/// not `"3.00"`). Shared between standalone `DragNumberField` and
/// the composite `TransformEditor`.
pub fn format_drag_value(v: f64) -> String {
    let rounded = (v * 100.0).round() / 100.0;
    let raw = format!("{rounded:.2}");
    let trimmed = raw.trim_end_matches('0').trim_end_matches('.');
    if trimmed.is_empty() {
        "0".into()
    } else {
        trimmed.to_string()
    }
}

/// Resolve a hex string to a [`Color`], falling back to fully
/// transparent when the parse fails. Used for chrome primitives whose
/// "no colour" branch should still produce a deterministic value.
pub fn color_or_transparent(hex: &str) -> Color {
    parse_color(hex).unwrap_or(Color {
        r: 0,
        g: 0,
        b: 0,
        a: 0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icon_button_node_has_28x28_frame_and_16x16_glyph() {
        let n = icon_button_node("ib", "icons/x.svg", true, None);
        let UiNode::Container {
            props, children, ..
        } = n
        else {
            panic!("not a container")
        };
        assert_eq!(props.width, Sizing::Fixed(28.0));
        assert_eq!(props.height, Sizing::Fixed(28.0));
        assert_eq!(props.radius.tl, 6.0);
        assert!(props.hover.is_some());
        let UiNode::Image { width, height, .. } = &children[0] else {
            panic!("expected glyph image")
        };
        assert_eq!(*width, Sizing::Fixed(16.0));
        assert_eq!(*height, Sizing::Fixed(16.0));
    }

    #[test]
    fn icon_button_disabled_omits_hover_and_propagates_attr() {
        let n = icon_button_node("ib", "icons/x.svg", false, Some("Close"));
        let UiNode::Container { props, .. } = n else {
            panic!()
        };
        assert!(props.hover.is_none());
        assert_eq!(props.semantic.aria_label.as_deref(), Some("Close"));
        assert!(props
            .semantic
            .attrs
            .iter()
            .any(|(k, v)| k == "disabled" && v == "disabled"));
    }

    #[test]
    fn indent_dot_renders_as_6x6_filled_square() {
        let n = indent_dot("dot", "#000000", 3.0);
        let UiNode::Container { props, .. } = n else {
            panic!()
        };
        assert_eq!(props.width, Sizing::Fixed(6.0));
        assert_eq!(props.height, Sizing::Fixed(6.0));
        assert_eq!(props.radius.tl, 3.0);
        assert!(props.background.is_some());
    }

    #[test]
    fn color_or_transparent_falls_back_when_parse_fails() {
        let c = color_or_transparent("not-a-color");
        assert_eq!(c.a, 0);
    }
}
