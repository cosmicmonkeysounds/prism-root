//! `shell.nav-button` — 48×48 activity-bar icon used to switch between
//! workflow modes (Home, Edit, Code, Preview, …). Selected state shows
//! an accent rail on the left edge plus a tinted background; hover
//! lights up the resting bg through the runtime's hover-overrides
//! vocabulary.
//!
//! Slint origin: `NavButton` in `ui/app.slint` (lines 485-522).
//!
//! The selection state is a *prop* (`selected`) — its visuals fold in
//! at lower-time. The hover state goes through `props.hover` —
//! [`prism_ui_runtime::layout::Surface`] swaps in the override when its
//! `hovered_id` matches.

use prism_builder::{
    document::Node,
    registry::{FieldSpec, NumericBounds},
    signal::SignalDef,
    style::StyleProperties,
    ui_lower::{
        bare_container, hover_bg, image_node, parse_color, prop_bool, prop_string, LowerCtx,
    },
    with_common_signals,
};
use prism_ui_runtime::layout::{Node as UiNode, Padding, Semantic, Sizing};
use serde_json::Value;

const NAV_SIZE: f32 = 48.0;
const RAIL_WIDTH: f32 = 3.0;
const ICON_SIZE: f32 = 20.0;
/// `Palette.accent-background.transparentize(85%)` — selection bg.
const SELECTED_BG: &str = "#260060c0";
/// Translucent foreground tint used for the resting hover state.
const HOVER_BG: &str = "#1f000000";
/// Accent rail colour when selected.
const RAIL_ACCENT: &str = "#0060c0";

fn nav_button_schema() -> Vec<FieldSpec> {
    vec![
        FieldSpec::text("icon", "Icon").required(),
        FieldSpec::boolean("selected", "Selected").with_default(Value::Bool(false)),
        FieldSpec::text("help-id", "Help ID"),
    ]
}

fn nav_button_signals() -> Vec<prism_builder::signal::SignalDef> {
    with_common_signals(vec![
        SignalDef::new(
            "hover-start",
            "Pointer entered the button — positional payload for tooltip placement.",
        )
        .with_payload(vec![
            FieldSpec::text("help_id", "Help ID"),
            FieldSpec::number("x", "X (px)", NumericBounds::default()),
            FieldSpec::number("y", "Y (px)", NumericBounds::default()),
        ]),
        SignalDef::new("hover-end", "Pointer left the button."),
    ])
}

fn nav_button_lower(ctx: &LowerCtx<'_>, node: &Node, style: &StyleProperties) -> UiNode {
    let icon = prop_string(node, "icon");
    let selected = prop_bool(node, "selected", false);

    // Left accent rail — 3px-wide vertical stroke that's only painted
    // when selected. We always emit the container to keep the layout
    // deterministic; transparent fills compose to no draw call.
    let rail = bare_container(format!("{}::rail", node.id), vec![], |props| {
        props.width = Sizing::Fixed(RAIL_WIDTH);
        props.height = Sizing::Grow;
        if selected {
            props.background = parse_color(RAIL_ACCENT);
        }
    });

    let glyph = image_node(
        format!("{}::glyph", node.id),
        icon,
        style,
        Sizing::Fixed(ICON_SIZE),
        Sizing::Fixed(ICON_SIZE),
    );

    // Right side hosts the centred glyph in its own grow container so
    // the rail floats at x=0 and the icon sits in the remaining 45px.
    let body = bare_container(format!("{}::body", node.id), vec![glyph], |props| {
        props.width = Sizing::Grow;
        props.height = Sizing::Grow;
        props.padding = Padding {
            left: (NAV_SIZE - RAIL_WIDTH - ICON_SIZE) / 2.0,
            right: (NAV_SIZE - RAIL_WIDTH - ICON_SIZE) / 2.0,
            top: (NAV_SIZE - ICON_SIZE) / 2.0,
            bottom: (NAV_SIZE - ICON_SIZE) / 2.0,
        };
    });

    ctx.synthetic_container(node, style, vec![rail, body], |props| {
        props.direction = prism_ui_runtime::layout::Direction::Row;
        props.width = Sizing::Fixed(NAV_SIZE);
        props.height = Sizing::Fixed(NAV_SIZE);
        if selected {
            props.background = parse_color(SELECTED_BG);
        }
        // Selected buttons keep their accent bg under hover (no
        // double-state); resting buttons gain the foreground tint.
        if !selected {
            props.hover = hover_bg(HOVER_BG);
        }
        props.semantic = Semantic::button().with_attr_if(selected, "aria-pressed", "true");
    })
}

pub const NAV_BUTTON_SPEC: prism_builder::BlockSpec =
    prism_builder::BlockSpec::new("shell.nav-button", nav_button_schema)
        .lower(nav_button_lower)
        .signals(nav_button_signals);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::testing::{lower_with, test_node};
    use prism_builder::document::Node as BuilderNode;
    use prism_builder::Block;
    use serde_json::json;

    fn lower_one(node: &BuilderNode) -> UiNode {
        lower_with(node, nav_button_lower)
    }

    fn nav(props: Value) -> BuilderNode {
        test_node("n", "shell.nav-button", props)
    }

    #[test]
    fn selected_button_paints_rail_and_accent_bg() {
        let ui = lower_one(&nav(json!({ "icon": "icons/home.svg", "selected": true })));
        if let UiNode::Container {
            props, children, ..
        } = ui
        {
            assert!(props.background.is_some(), "selected has accent bg");
            // No hover override — selected stays accent under pointer.
            assert!(props.hover.is_none());
            // Rail is the first child and has its own bg.
            if let UiNode::Container { props: rail, .. } = &children[0] {
                assert!(rail.background.is_some(), "rail painted when selected");
            }
            assert_eq!(props.semantic.tag.as_deref(), Some("button"));
            assert!(props
                .semantic
                .attrs
                .iter()
                .any(|(k, v)| k == "aria-pressed" && v == "true"));
        } else {
            panic!("not a container")
        }
    }

    #[test]
    fn unselected_button_declares_hover_overrides() {
        let ui = lower_one(&nav(json!({ "icon": "icons/home.svg", "selected": false })));
        if let UiNode::Container {
            props, children, ..
        } = ui
        {
            assert!(props.background.is_none());
            assert!(props.hover.is_some(), "resting button hovers on tint");
            // Rail container exists but transparent.
            if let UiNode::Container { props: rail, .. } = &children[0] {
                assert!(rail.background.is_none());
            }
            assert!(props
                .semantic
                .attrs
                .iter()
                .all(|(k, _)| k != "aria-pressed"));
        } else {
            panic!("not a container")
        }
    }

    #[test]
    fn schema_declares_three_fields() {
        let block = prism_builder::SpecBlock::new(&super::NAV_BUTTON_SPEC);
        let keys: Vec<String> = block.schema().into_iter().map(|f| f.key).collect();
        assert_eq!(keys, vec!["icon", "selected", "help-id"]);
    }
}
