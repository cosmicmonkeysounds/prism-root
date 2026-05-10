//! `shell.icon-button` — 28×28 icon-only button used by toolbars,
//! dock tabs, inspector rows, toasts, and every other slot in the
//! shell that needs "one icon, one click".
//!
//! Slint origin: `IconButton` in `ui/app.slint` (lines 334-373).
//!
//! Lowering: a fixed-size 28×28 container with a 6px corner radius,
//! holding a centred 16×16 [`prism_ui_runtime::layout::Node::Image`].
//! Cascade resolves the resting background; the hover-active background
//! is intentionally left to a follow-up runtime change (the layout
//! `Node` model has no hover-state vocabulary yet — see
//! `clay-migration-plan.md` Phase 4 runtime gaps).
//!
//! Signals: `clicked`, `hover-start { x, y }`, `hover-end` plus the 12
//! universal common signals from `prism_builder::common_signals`.

use prism_builder::{
    common_signals,
    document::Node,
    registry::{FieldSpec, NumericBounds},
    schemas, // unused — reserved for shared field factories as we grow
    signal::SignalDef,
    style::StyleProperties,
    ui_lower::{parse_color, prop_bool, prop_str, LowerCtx},
};
use prism_ui_runtime::layout::Node as UiNode;
use serde_json::Value;

use super::chrome::icon_button_node_tinted;

/// `shell.icon-button` block. Schema mirrors the four `in property`
/// declarations on the original Slint component.
fn icon_button_schema() -> Vec<FieldSpec> {
    vec![
        FieldSpec::text("icon", "Icon").required(),
        FieldSpec::boolean("enabled", "Enabled").with_default(Value::Bool(true)),
        FieldSpec::text("tooltip-text", "Tooltip text"),
        FieldSpec::text("help-id", "Help ID"),
        // Optional glyph tint — lowers to `Node::Image::tint`.
        // Mirrors the original Slint `colorize` property on the
        // icon's `Image` element.
        FieldSpec::text("tint", "Glyph tint"),
    ]
}

fn icon_button_signals() -> Vec<prism_builder::signal::SignalDef> {
    // `with_common_signals` would dedup component-specific names
    // against the 12 universals, but `hover-start`/`hover-end` are
    // additive vocabulary the IconButton emits with positional
    // payload (the Slint version takes `(string, length, length)`).
    let mut signals = common_signals();
    signals.push(
        SignalDef::new(
            "hover-start",
            "Pointer entered the button — positional payload for tooltip placement.",
        )
        .with_payload(vec![
            FieldSpec::text("help_id", "Help ID"),
            FieldSpec::number("x", "X (px)", NumericBounds::default()),
            FieldSpec::number("y", "Y (px)", NumericBounds::default()),
        ]),
    );
    signals.push(SignalDef::new("hover-end", "Pointer left the button."));
    signals
}

fn icon_button_lower(_ctx: &LowerCtx<'_>, node: &Node, _style: &StyleProperties) -> UiNode {
    // Visual recipe lives in `chrome::icon_button_node` so it's
    // reusable from any chrome primitive that embeds a chevron /
    // trash / move button (InspectorRow, Tab pills, MenuBar items).
    // The Block layer's job is just prop → helper-arg translation.
    let enabled = prop_bool(node, "enabled", true);
    let tooltip = Some(prop_str(node, "tooltip-text")).filter(|s| !s.is_empty());
    let tint = parse_color(prop_str(node, "tint"));
    icon_button_node_tinted(
        node.id.clone(),
        prop_str(node, "icon").to_string(),
        enabled,
        tooltip,
        tint,
    )
}

pub const ICON_BUTTON_SPEC: prism_builder::BlockSpec =
    prism_builder::BlockSpec::new("shell.icon-button", icon_button_schema)
        .lower(icon_button_lower)
        .signals(icon_button_signals);

// `schemas` is imported up top but the IconButton schema is hand-rolled
// (four shell-only fields, no overlap with `prism_builder::schemas`).
// Suppress the unused-import nag without dropping the symbol — once we
// grow shared shell field factories they'll live here.
#[allow(dead_code)]
fn _schemas_anchor() -> usize {
    schemas::button().len()
}

#[cfg(test)]
mod tests {
    use super::*;
    use prism_builder::document::Node as BuilderNode;
    use prism_builder::layout::LayoutMode;
    use prism_builder::style::StyleProperties as Cascade;
    use prism_builder::ui_lower::LowerCtx;
    use prism_builder::Block;
    use prism_core::foundation::spatial::Transform2D;
    use prism_ui_runtime::layout::Sizing;
    use serde_json::json;

    fn lower_one(node: &BuilderNode) -> UiNode {
        let cascade = Cascade::default();
        let ctx = LowerCtx::new(None, &cascade);
        icon_button_lower(&ctx, node, &cascade)
    }

    fn icon_node(props: Value) -> BuilderNode {
        BuilderNode {
            id: "ib".into(),
            component: "shell.icon-button".into(),
            props,
            children: vec![],
            layout_mode: LayoutMode::default(),
            transform: Transform2D::default(),
            modifiers: vec![],
            style: Cascade::default(),
        }
    }

    #[test]
    fn lowers_to_28x28_container_with_image_child() {
        let node = icon_node(json!({ "icon": "icons/box.svg", "enabled": true }));
        let ui = lower_one(&node);
        match ui {
            UiNode::Container {
                props, children, ..
            } => {
                assert_eq!(props.width, Sizing::Fixed(28.0));
                assert_eq!(props.height, Sizing::Fixed(28.0));
                assert_eq!(props.radius.tl, 6.0);
                assert_eq!(props.semantic.tag.as_deref(), Some("button"));
                assert_eq!(children.len(), 1);
                match &children[0] {
                    UiNode::Image {
                        source,
                        width,
                        height,
                        ..
                    } => {
                        assert_eq!(source, "icons/box.svg");
                        assert_eq!(*width, Sizing::Fixed(16.0));
                        assert_eq!(*height, Sizing::Fixed(16.0));
                    }
                    other => panic!("expected Image glyph, got {other:?}"),
                }
            }
            other => panic!("expected Container, got {other:?}"),
        }
    }

    #[test]
    fn tooltip_text_becomes_aria_label() {
        let node = icon_node(json!({ "icon": "icons/x.svg", "tooltip-text": "Close" }));
        let ui = lower_one(&node);
        if let UiNode::Container { props, .. } = ui {
            assert_eq!(props.semantic.aria_label.as_deref(), Some("Close"));
        } else {
            panic!("not a container")
        }
    }

    #[test]
    fn disabled_propagates_to_semantic_attrs() {
        let node = icon_node(json!({ "icon": "icons/x.svg", "enabled": false }));
        let ui = lower_one(&node);
        if let UiNode::Container { props, .. } = ui {
            assert!(props
                .semantic
                .attrs
                .iter()
                .any(|(k, v)| k == "disabled" && v == "disabled"));
        } else {
            panic!("not a container")
        }
    }

    #[test]
    fn schema_declares_five_fields() {
        let block = prism_builder::SpecBlock::new(&super::ICON_BUTTON_SPEC);
        let schema = block.schema();
        let keys: Vec<&str> = schema.iter().map(|f| f.key.as_str()).collect();
        assert_eq!(
            keys,
            vec!["icon", "enabled", "tooltip-text", "help-id", "tint"]
        );
    }

    #[test]
    fn tint_prop_propagates_to_glyph_image_tint() {
        let node = icon_node(json!({ "icon": "icons/x.svg", "tint": "#ff0000" }));
        let UiNode::Container { children, .. } = lower_one(&node) else {
            panic!()
        };
        let UiNode::Image { tint, .. } = &children[0] else {
            panic!("expected Image glyph")
        };
        let c = tint.expect("tint propagates");
        assert_eq!((c.r, c.g, c.b), (255, 0, 0));
    }

    #[test]
    fn missing_tint_prop_leaves_glyph_untinted() {
        let node = icon_node(json!({ "icon": "icons/x.svg" }));
        let UiNode::Container { children, .. } = lower_one(&node) else {
            panic!()
        };
        let UiNode::Image { tint, .. } = &children[0] else {
            panic!()
        };
        assert!(tint.is_none());
    }

    #[test]
    fn enabled_button_declares_hover_background() {
        let node = icon_node(json!({ "icon": "icons/x.svg", "enabled": true }));
        if let UiNode::Container { props, .. } = lower_one(&node) {
            let hover = props.hover.expect("enabled button declares hover");
            assert!(hover.background.is_some());
        } else {
            panic!("not a container")
        }
    }

    #[test]
    fn disabled_button_omits_hover_overrides() {
        let node = icon_node(json!({ "icon": "icons/x.svg", "enabled": false }));
        if let UiNode::Container { props, .. } = lower_one(&node) {
            assert!(
                props.hover.is_none(),
                "disabled buttons stay static under the pointer"
            );
        } else {
            panic!("not a container")
        }
    }

    #[test]
    fn signals_include_clicked_and_hover_pair() {
        let block = prism_builder::SpecBlock::new(&super::ICON_BUTTON_SPEC);
        let names: Vec<String> = block.signals().into_iter().map(|s| s.name).collect();
        assert!(names.contains(&"clicked".into()));
        assert!(names.contains(&"hover-start".into()));
        assert!(names.contains(&"hover-end".into()));
    }
}
