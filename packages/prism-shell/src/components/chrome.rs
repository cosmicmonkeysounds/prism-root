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

use prism_builder::document::Node as BuilderNode;
use prism_builder::style::StyleProperties;
use prism_builder::ui_lower::{
    bare_container, colored_text_node, hover_bg, image_node, parse_color, tinted_image_node,
    uniform_radius, LowerCtx,
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
    command: Option<&str>,
) -> UiNode {
    icon_button_node_tinted(id, icon, enabled, aria_label, None, command)
}

/// Tinted variant of [`icon_button_node`] — paints the glyph through
/// `tint` as a mask. The original Slint shell drove icon colour via
/// the `colorize` property; in the runtime that's a `Node::Image`
/// `tint`. `None` falls back to the as-authored monochrome render.
///
/// `command` is an optional command-table id; when set and the
/// button is enabled, the lowered container carries
/// `data-on-click="cmd <id>"` so the shell's `route_on_click`
/// dispatches the click through the command table. Disabled buttons
/// drop the attr so the rest of the pointer-down chain still gets
/// to run.
pub fn icon_button_node_tinted(
    id: impl Into<String>,
    icon: impl Into<String>,
    enabled: bool,
    aria_label: Option<&str>,
    tint: Option<Color>,
    command: Option<&str>,
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

    let command = command
        .filter(|s| !s.is_empty() && enabled)
        .map(String::from);

    bare_container(id, vec![glyph], |props| {
        props.width = Sizing::Fixed(ICON_BUTTON_SIZE);
        props.height = Sizing::Fixed(ICON_BUTTON_SIZE);
        props.radius = uniform_radius(ICON_BUTTON_RADIUS);
        if enabled {
            props.hover = hover_bg(ICON_BUTTON_HOVER_BG);
        }
        let mut s = Semantic::button()
            .with_aria_label_opt(aria_label)
            .with_attr_if(!enabled, "disabled", "disabled");
        if let Some(cmd) = command {
            s = s.with_attr("data-on-click", format!("cmd {cmd}"));
        }
        props.semantic = s;
    })
}

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

/// Static visual recipe for an "active-underline tab": a column with a
/// label on top and a 2px underline at the bottom; active state paints
/// a tinted background, resting state swaps to a hover-bg. Shared
/// between `shell.dock-tab` and `shell.workflow-page-button` (and any
/// future tab-shaped chrome). The variation between consumers is
/// purely metric/colour — captured here as a `&'static TabStyle`.
///
/// `data_role` is the hit-test routing key the shell's event router
/// reads off the lowered container's semantic attrs (see
/// `prism-shell/src/events.rs::POINTER_ROUTES`). Each consumer pins
/// its own role string so a click on a workflow-page tab can be told
/// apart from a click on a dock-panel tab even though the lowered
/// shape is identical.
pub struct TabStyle {
    pub height: f32,
    pub padding: Padding,
    pub label_size: f32,
    pub label_active: &'static str,
    pub label_resting: &'static str,
    pub active_bg: &'static str,
    pub hover_bg: &'static str,
    pub underline_height: f32,
    pub underline_active: &'static str,
    pub data_role: &'static str,
}

pub fn active_underline_tab(
    ctx: &LowerCtx<'_>,
    node: &BuilderNode,
    style: &StyleProperties,
    label_text: String,
    active: bool,
    target_id: &str,
    spec: &TabStyle,
) -> UiNode {
    let label = colored_text_node(
        format!("{}::label", node.id),
        label_text,
        style,
        spec.label_size,
        if active {
            spec.label_active
        } else {
            spec.label_resting
        },
    );
    let underline = bare_container(format!("{}::underline", node.id), vec![], |p| {
        p.width = Sizing::Grow;
        p.height = Sizing::Fixed(spec.underline_height);
        if active {
            p.background = parse_color(spec.underline_active);
        }
    });
    ctx.synthetic_container(node, style, vec![label, underline], |p| {
        p.direction = Direction::Column;
        p.height = Sizing::Fixed(spec.height);
        p.padding = spec.padding;
        if active {
            p.background = parse_color(spec.active_bg);
        } else {
            p.hover = hover_bg(spec.hover_bg);
        }
        let mut s = Semantic::button()
            .with_attr("role", "tab")
            .with_attr("data-role", spec.data_role);
        if !target_id.is_empty() {
            s = s.with_attr("data-target-id", target_id);
        }
        if active {
            s = s.with_attr("aria-selected", "true");
        }
        p.semantic = s;
    })
}

/// Single coloured rectangle representing one axis arm of a gizmo
/// (move / scale). Renders as a `<span role="presentation">` with
/// `data-role="gizmo-axis"` and a `data-axis` attr the painter / hit
/// tester picks up. `rounded` controls the half-thickness pill radius
/// (move-gizmo arms are rounded, scale-gizmo arms are square).
pub fn gizmo_axis_arm(
    id: String,
    axis: char,
    length: f32,
    thick: f32,
    color: &str,
    rounded: bool,
) -> UiNode {
    let (w, h) = if axis == 'x' {
        (length, thick)
    } else {
        (thick, length)
    };
    let axis_str = if axis == 'x' { "x" } else { "y" };
    bare_container(id, vec![], |p| {
        p.width = Sizing::Fixed(w);
        p.height = Sizing::Fixed(h);
        p.background = parse_color(color);
        if rounded {
            p.radius = uniform_radius(thick / 2.0);
        }
        p.semantic = Semantic::tag("span")
            .with_attr("role", "presentation")
            .with_attr("data-role", "gizmo-axis")
            .with_attr("data-axis", axis_str);
    })
}

/// Coloured square / circle handle inside a gizmo (hub, cap, rotate
/// handle). `data_role` differentiates `gizmo-hub` / `gizmo-cap` /
/// `gizmo-handle`. `axis` opt-adds `data-axis` for cap-style handles.
/// `radius` controls the corner round (0 = square, size/2 = circle).
pub fn gizmo_handle(
    id: String,
    size: f32,
    radius: f32,
    color: &str,
    aria: &str,
    data_role: &str,
    axis: Option<char>,
) -> UiNode {
    bare_container(id, vec![], |p| {
        p.width = Sizing::Fixed(size);
        p.height = Sizing::Fixed(size);
        p.background = parse_color(color);
        if radius > 0.0 {
            p.radius = uniform_radius(radius);
        }
        let mut s = Semantic::tag("span")
            .with_attr("role", "button")
            .with_attr("aria-label", aria)
            .with_attr("data-role", data_role);
        if let Some(a) = axis {
            s = s.with_attr("data-axis", if a == 'x' { "x" } else { "y" });
        }
        p.semantic = s;
    })
}

/// Outer `<div role="group">` wrapper shared by every gizmo-tool block.
/// `tool` populates the `data-tool` attr the hit tester routes through.
pub fn gizmo_root(id: String, children: Vec<UiNode>, aria: &str, tool: &str, row: bool) -> UiNode {
    bare_container(id, children, |p| {
        if row {
            p.direction = Direction::Row;
        }
        p.semantic = Semantic::tag("div")
            .with_attr("role", "group")
            .with_attr("aria-label", aria)
            .with_attr("data-role", "gizmo")
            .with_attr("data-tool", tool);
    })
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
        let n = icon_button_node("ib", "icons/x.svg", true, None, None);
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
        let n = icon_button_node("ib", "icons/x.svg", false, Some("Close"), None);
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

    #[test]
    fn icon_button_node_command_arg_emits_data_on_click() {
        let n = icon_button_node("ib", "icons/x.svg", true, Some("Save"), Some("file.save"));
        let UiNode::Container { props, .. } = n else {
            panic!()
        };
        assert!(props
            .semantic
            .attrs
            .iter()
            .any(|(k, v)| k == "data-on-click" && v == "cmd file.save"));
    }
}
