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
    ui_lower::{hover_bg, image_node, uniform_radius, LowerCtx},
    Block,
    ComponentId,
    RenderError,
    RenderSlintContext,
};
use prism_ui_runtime::layout::{Node as UiNode, Semantic, Sizing};
use serde_json::Value;

const ICON_BUTTON_SIZE: f32 = 28.0;
const ICON_BUTTON_RADIUS: f32 = 6.0;
const ICON_GLYPH_SIZE: f32 = 16.0;
/// Hover background — `Palette.control-background` in the original
/// Slint version, hard-coded here until the design-tokens cascade
/// resolves the value at lower-time.
const ICON_BUTTON_HOVER_BG: &str = "#1f000000";

/// `shell.icon-button` block. Schema mirrors the four `in property`
/// declarations on the original Slint component.
pub struct IconButton {
    pub id: ComponentId,
}

impl Block for IconButton {
    fn id(&self) -> &ComponentId {
        &self.id
    }

    fn schema(&self) -> Vec<FieldSpec> {
        vec![
            FieldSpec::text("icon", "Icon").required(),
            FieldSpec::boolean("enabled", "Enabled").with_default(Value::Bool(true)),
            FieldSpec::text("tooltip-text", "Tooltip text"),
            FieldSpec::text("help-id", "Help ID"),
        ]
    }

    fn signals(&self) -> Vec<SignalDef> {
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

    fn render_slint(
        &self,
        _ctx: &RenderSlintContext<'_>,
        _props: &Value,
        _children: &[Node],
        out: &mut prism_builder::slint_source::SlintEmitter,
    ) -> Result<(), RenderError> {
        // The shell still emits Slint during the parallel-build period
        // (Phase 4 cargo feature). Once `ui/app.prism-ui` lands and the
        // shell stops compiling Slint, this method goes away.
        out.block("Rectangle", |out| {
            out.prop_px("width", ICON_BUTTON_SIZE as f64);
            out.prop_px("height", ICON_BUTTON_SIZE as f64);
            out.prop_px("border-radius", ICON_BUTTON_RADIUS as f64);
            Ok(())
        })
    }

    fn lower_ui(&self, ctx: &LowerCtx<'_>, node: &Node, style: &StyleProperties) -> UiNode {
        let icon = node
            .props
            .get("icon")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let glyph = image_node(
            format!("{}::glyph", node.id),
            icon,
            style,
            Sizing::Fixed(ICON_GLYPH_SIZE),
            Sizing::Fixed(ICON_GLYPH_SIZE),
        );

        // 28×28 container with a centred 16×16 glyph. `synthetic_container`
        // owns cascade resolution + flow props; we override only the
        // fields that make this an icon button rather than a generic box.
        let enabled = !matches!(node.props.get("enabled"), Some(Value::Bool(false)));
        ctx.synthetic_container(node, style, vec![glyph], |props| {
            props.width = Sizing::Fixed(ICON_BUTTON_SIZE);
            props.height = Sizing::Fixed(ICON_BUTTON_SIZE);
            props.radius = uniform_radius(ICON_BUTTON_RADIUS);
            // Resting bg stays at whatever the cascade resolved (typically
            // None → transparent). Disabled buttons stay static under
            // the pointer, so only enabled buttons declare a hover swap.
            if enabled {
                props.hover = hover_bg(ICON_BUTTON_HOVER_BG);
            }
            // SSR semantic: <button>. ARIA label is filled from the
            // tooltip prop so screen readers get the same text the
            // pointer-hover tooltip shows.
            let tooltip = node
                .props
                .get("tooltip-text")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty());
            props.semantic = Semantic::button()
                .with_aria_label_opt(tooltip)
                .with_attr_if(!enabled, "disabled", "disabled");
        })
    }
}

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
    use prism_core::foundation::spatial::Transform2D;
    use serde_json::json;

    fn lower_one(node: &BuilderNode) -> UiNode {
        let block = IconButton {
            id: "shell.icon-button".into(),
        };
        let cascade = Cascade::default();
        let ctx = LowerCtx::new(None, &cascade);
        block.lower_ui(&ctx, node, &cascade)
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
    fn schema_declares_four_fields() {
        let block = IconButton {
            id: "shell.icon-button".into(),
        };
        let schema = block.schema();
        let keys: Vec<&str> = schema.iter().map(|f| f.key.as_str()).collect();
        assert_eq!(keys, vec!["icon", "enabled", "tooltip-text", "help-id"]);
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
        let block = IconButton {
            id: "shell.icon-button".into(),
        };
        let names: Vec<String> = block.signals().into_iter().map(|s| s.name).collect();
        assert!(names.contains(&"clicked".into()));
        assert!(names.contains(&"hover-start".into()));
        assert!(names.contains(&"hover-end".into()));
    }
}
