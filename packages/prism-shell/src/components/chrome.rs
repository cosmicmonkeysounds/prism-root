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
    bare_container, colored_text_node, hover_bg, parse_color, uniform_radius,
};
use prism_ui_runtime::command::Color;
use prism_ui_runtime::layout::{Direction, Node as UiNode, Padding, Semantic, Sizing};

/// Zero-size placeholder used by overlay blocks (command palette,
/// menu dropdown, context menu, component picker, help tooltip) when
/// their visibility prop is `false`. The skeleton authors every
/// overlay as a sibling of the app-window so they always reach the
/// resolver — visibility is host state, not skeleton state. Returning
/// a 0×0 container preserves structural symmetry while taking the
/// overlay out of the layout pass entirely.
///
/// `data-role` mirrors the live overlay's role so SSR consumers can
/// still detect "this overlay was authored but is currently hidden."
pub fn hidden_overlay(id: impl Into<String>, role: &'static str) -> UiNode {
    bare_container(id, vec![], move |props| {
        props.width = Sizing::Fixed(0.0);
        props.height = Sizing::Fixed(0.0);
        props.semantic = Semantic::tag("div")
            .with_attr("data-role", role)
            .with_attr("data-visible", "false")
            .with_attr("aria-hidden", "true");
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
pub const DRAG_NUMBER_HEIGHT: f32 = 28.0;
pub const DRAG_NUMBER_RADIUS: f32 = 4.0;
/// 16% black — visible as a distinct input pill against the panel's
/// near-white background, mirroring the Slint-era number-input chrome.
/// The previous 3% alpha was so subtle the input read as plain text
/// (the "monolithic property row" complaint).
pub const DRAG_NUMBER_RESTING_BG: &str = "#28000000";
pub const DRAG_NUMBER_HOVER_BG: &str = "#3c000000";
pub const DRAG_NUMBER_VALUE_COLOR: &str = "#000000";
pub const DRAG_NUMBER_LABEL_COLOR: &str = "#99000000";
pub const DRAG_NUMBER_LABEL_SIZE: f32 = 11.0;
pub const DRAG_NUMBER_VALUE_SIZE: f32 = 12.0;

pub fn drag_number_field_node(
    id: impl Into<String>,
    label: &str,
    label_color: &str,
    value_text: String,
    key: &str,
) -> UiNode {
    let _ = id; // outer container is hit-test-transparent — see below
    let style = StyleProperties::default();
    let mut row_children: Vec<UiNode> = Vec::with_capacity(2);
    if !label.is_empty() {
        row_children.push(colored_text_node(
            String::new(),
            label.into(),
            &style,
            DRAG_NUMBER_LABEL_SIZE,
            label_color,
        ));
    }
    row_children.push(colored_text_node(
        String::new(),
        value_text,
        &style,
        DRAG_NUMBER_VALUE_SIZE,
        DRAG_NUMBER_VALUE_COLOR,
    ));

    let row = bare_container(String::new(), row_children, |p| {
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

    // **Empty outer id** so this pill doesn't register its own hit
    // rect. The field-editor row above it carries the routing keys
    // (`data-role="field-edit"`, `data-target-id`, `data-key`,
    // `data-kind`, `data-value`); a non-empty id here would surface
    // a deeper hit, `Surface::hit_test_at` would return the pill,
    // and the click would land on a container with no `data-role` —
    // exactly the silent-failure mode the user reported as "clicking
    // and dragging does not work" on number fields.
    bare_container(String::new(), vec![row], |props| {
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
    fn color_or_transparent_falls_back_when_parse_fails() {
        let c = color_or_transparent("not-a-color");
        assert_eq!(c.a, 0);
    }

    #[test]
    fn hidden_overlay_emits_zero_size_container_with_role_attr() {
        let n = hidden_overlay("ov", "modifier-picker");
        let UiNode::Container { props, .. } = n else {
            panic!()
        };
        assert_eq!(props.width, Sizing::Fixed(0.0));
        assert_eq!(props.height, Sizing::Fixed(0.0));
        assert!(props
            .semantic
            .attrs
            .iter()
            .any(|(k, v)| k == "data-role" && v == "modifier-picker"));
    }
}
