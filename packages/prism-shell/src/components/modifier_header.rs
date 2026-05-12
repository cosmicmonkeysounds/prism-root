//! `shell.modifier-header` — section divider for one attached
//! `Modifier` in the Inspector. Variant of [`shell.section-header`]
//! with three per-row affordances: an enable toggle (left of the
//! label), a remove × (right side), and a drag handle ≡ (right end).
//!
//! See `docs/dev/composable-builder-plan.md` Wave 1.4.
//!
//! Routing attrs carried on the toggle / remove / drag-handle
//! children (read by `events::POINTER_ROUTES`):
//!
//! - toggle:  `data-role="modifier-toggle"` + `data-target-id` + `data-modifier-idx`
//! - remove:  `data-role="modifier-remove"` + `data-target-id` + `data-modifier-idx`
//! - reorder: `data-role="modifier-reorder"` + `data-target-id` + `data-modifier-idx`
//!
//! The outer container is purely visual — clicks anywhere outside
//! the three affordances are no-ops.

use prism_builder::{
    document::Node,
    registry::FieldSpec,
    style::StyleProperties,
    ui_lower::{bare_container, colored_text_node, image_node, parse_color, LowerCtx},
};
use prism_ui_runtime::layout::{Direction, Node as UiNode, Padding, Semantic, Sizing};
use serde_json::Value;

const ROW_HEIGHT: f32 = 36.0;
const ICON_SIZE: f32 = 12.0;
const LABEL_FONT_SIZE: f32 = 11.0;
const HAIRLINE_COLOR: &str = "#19000000";
const LABEL_COLOR_ENABLED: &str = "#cc000000";
const LABEL_COLOR_DISABLED: &str = "#66000000";

fn modifier_header_schema() -> Vec<FieldSpec> {
    vec![
        FieldSpec::text("label", "Behaviour label").required(),
        FieldSpec::text("description", "Description"),
        FieldSpec::text("modifier-id", "Registered behaviour id").required(),
        FieldSpec::integer(
            "modifier-idx",
            "Index in node.modifiers",
            prism_builder::registry::NumericBounds::min(0.0),
        ),
        FieldSpec::boolean("enabled", "Enabled").with_default(Value::Bool(true)),
        FieldSpec::text("target-id", "Owning node id").required(),
    ]
}

fn modifier_header_lower(ctx: &LowerCtx<'_>, node: &Node, style: &StyleProperties) -> UiNode {
    let label = ctx.prop_str(node, "label");
    let description = ctx.prop_str(node, "description");
    let modifier_id = ctx.prop_str(node, "modifier-id");
    let modifier_idx = ctx
        .prop(node, "modifier-idx")
        .as_u64()
        .map(|n| n.to_string())
        .unwrap_or_else(|| "0".into());
    let enabled = ctx.prop_bool(node, "enabled", true);
    let target_id = ctx.prop_str(node, "target-id");

    // Enable toggle — left-side switch icon. `data-role` routes the
    // click through `handle_modifier_toggle` in events.rs.
    let toggle_icon_src = if enabled {
        "icons/check-square.svg"
    } else {
        "icons/square.svg"
    };
    let toggle_icon = image_node(
        format!("{}::toggle-icon", node.id),
        toggle_icon_src.into(),
        style,
        Sizing::Fixed(ICON_SIZE),
        Sizing::Fixed(ICON_SIZE),
    );
    let toggle = bare_container(format!("{}::toggle", node.id), vec![toggle_icon], |p| {
        p.width = Sizing::Fixed(20.0);
        p.height = Sizing::Fixed(20.0);
        p.padding = Padding {
            left: 4.0,
            right: 4.0,
            top: 4.0,
            bottom: 4.0,
        };
        let mut sem = Semantic::tag("button");
        sem = sem.with_attr("data-role", "modifier-toggle");
        sem = sem.with_attr("data-target-id", &target_id);
        sem = sem.with_attr("data-modifier-idx", &modifier_idx);
        sem = sem.with_attr("data-modifier-id", &modifier_id);
        sem = sem.with_attr("aria-label", if enabled { "Disable" } else { "Enable" });
        sem = sem.with_attr("aria-pressed", if enabled { "true" } else { "false" });
        p.semantic = sem;
    });

    // Label
    let label_color = if enabled {
        LABEL_COLOR_ENABLED
    } else {
        LABEL_COLOR_DISABLED
    };
    let mut label_text = colored_text_node(
        format!("{}::label", node.id),
        label,
        style,
        LABEL_FONT_SIZE,
        label_color,
    );
    if !description.is_empty() {
        // Attach the description as the SSR title attribute so
        // hovering the label shows the tooltip text. (Live tooltips
        // arrive with the Tooltip modifier in Wave 11; this is the
        // SSR-friendly fallback.)
        if let UiNode::Text { ref mut props, .. } = label_text {
            props.semantic = props.semantic.clone().with_attr("title", &description);
        }
    }

    // Remove × — right-side delete affordance.
    let remove_icon = image_node(
        format!("{}::remove-icon", node.id),
        "icons/x.svg".into(),
        style,
        Sizing::Fixed(ICON_SIZE),
        Sizing::Fixed(ICON_SIZE),
    );
    let remove = bare_container(format!("{}::remove", node.id), vec![remove_icon], |p| {
        p.width = Sizing::Fixed(20.0);
        p.height = Sizing::Fixed(20.0);
        p.padding = Padding {
            left: 4.0,
            right: 4.0,
            top: 4.0,
            bottom: 4.0,
        };
        let mut sem = Semantic::tag("button");
        sem = sem.with_attr("data-role", "modifier-remove");
        sem = sem.with_attr("data-target-id", &target_id);
        sem = sem.with_attr("data-modifier-idx", &modifier_idx);
        sem = sem.with_attr("aria-label", "Remove behaviour");
        p.semantic = sem;
    });

    // Drag handle ≡ — reorder grip. Pointer gestures on this attach
    // to the §43-D `selection-gizmo`-style capture path (Wave 3);
    // until that lands, the icon is purely visual and the
    // `modifier-reorder` route is wired in `POINTER_ROUTES`.
    let drag_icon = image_node(
        format!("{}::drag-icon", node.id),
        "icons/menu.svg".into(),
        style,
        Sizing::Fixed(ICON_SIZE),
        Sizing::Fixed(ICON_SIZE),
    );
    let drag = bare_container(format!("{}::drag", node.id), vec![drag_icon], |p| {
        p.width = Sizing::Fixed(20.0);
        p.height = Sizing::Fixed(20.0);
        p.padding = Padding {
            left: 4.0,
            right: 4.0,
            top: 4.0,
            bottom: 4.0,
        };
        let mut sem = Semantic::tag("button");
        sem = sem.with_attr("data-role", "modifier-reorder");
        sem = sem.with_attr("data-target-id", &target_id);
        sem = sem.with_attr("data-modifier-idx", &modifier_idx);
        sem = sem.with_attr("aria-label", "Reorder behaviour");
        p.semantic = sem;
    });

    let spacer = bare_container(format!("{}::spacer", node.id), vec![], |p| {
        p.width = Sizing::Grow;
    });

    let row = bare_container(
        format!("{}::row", node.id),
        vec![toggle, label_text, spacer, remove, drag],
        |p| {
            p.direction = Direction::Row;
            p.gap = 6.0;
            p.padding = Padding {
                left: 8.0,
                right: 4.0,
                top: 0.0,
                bottom: 0.0,
            };
            p.height = Sizing::Fixed(ROW_HEIGHT - 1.0);
        },
    );

    let hairline = bare_container(format!("{}::hairline", node.id), vec![], |p| {
        p.height = Sizing::Fixed(1.0);
        p.background = parse_color(HAIRLINE_COLOR);
    });

    ctx.synthetic_container(node, style, vec![row, hairline], |p| {
        p.direction = Direction::Column;
        p.height = Sizing::Fixed(ROW_HEIGHT);
        let mut sem = Semantic::tag("header");
        sem = sem.with_attr("data-role", "modifier-header");
        sem = sem.with_attr("data-target-id", &target_id);
        sem = sem.with_attr("data-modifier-id", &modifier_id);
        sem = sem.with_attr("data-modifier-idx", &modifier_idx);
        if !enabled {
            sem = sem.with_attr("data-disabled", "true");
        }
        p.semantic = sem;
    })
}

pub const MODIFIER_HEADER_SPEC: prism_builder::BlockSpec =
    prism_builder::BlockSpec::new("shell.modifier-header", modifier_header_schema)
        .lower(modifier_header_lower);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::testing::{lower_with, test_node};
    use serde_json::json;

    fn lower_one(props: Value) -> UiNode {
        lower_with(
            &test_node("mh", "shell.modifier-header", props),
            modifier_header_lower,
        )
    }

    fn descend(ui: &UiNode) -> &[UiNode] {
        match ui {
            UiNode::Container { children, .. } => children,
            _ => panic!("expected container"),
        }
    }

    #[test]
    fn header_carries_routing_attrs_on_outer_container() {
        let ui = lower_one(json!({
            "label": "Tooltip",
            "modifier-id": "tooltip",
            "modifier-idx": 2,
            "enabled": true,
            "target-id": "btn1",
        }));
        let UiNode::Container { props, .. } = &ui else {
            panic!()
        };
        let attrs = &props.semantic.attrs;
        assert!(attrs
            .iter()
            .any(|(k, v)| k == "data-role" && v == "modifier-header"));
        assert!(attrs
            .iter()
            .any(|(k, v)| k == "data-target-id" && v == "btn1"));
        assert!(attrs
            .iter()
            .any(|(k, v)| k == "data-modifier-id" && v == "tooltip"));
        assert!(attrs
            .iter()
            .any(|(k, v)| k == "data-modifier-idx" && v == "2"));
    }

    #[test]
    fn disabled_header_marks_data_disabled() {
        let ui = lower_one(json!({
            "label": "Tooltip",
            "modifier-id": "tooltip",
            "modifier-idx": 0,
            "enabled": false,
            "target-id": "btn1",
        }));
        let UiNode::Container { props, .. } = &ui else {
            panic!()
        };
        assert!(props
            .semantic
            .attrs
            .iter()
            .any(|(k, v)| k == "data-disabled" && v == "true"));
    }

    #[test]
    fn row_has_five_segments_toggle_label_spacer_remove_drag() {
        let ui = lower_one(json!({
            "label": "Tooltip",
            "modifier-id": "tooltip",
            "modifier-idx": 0,
            "enabled": true,
            "target-id": "btn1",
        }));
        let outer = descend(&ui);
        let row = &outer[0];
        let row_children = descend(row);
        assert_eq!(row_children.len(), 5);
    }

    #[test]
    fn toggle_icon_changes_with_enabled_flag() {
        let on = lower_one(json!({
            "label": "T",
            "modifier-id": "tooltip",
            "modifier-idx": 0,
            "enabled": true,
            "target-id": "x",
        }));
        let off = lower_one(json!({
            "label": "T",
            "modifier-id": "tooltip",
            "modifier-idx": 0,
            "enabled": false,
            "target-id": "x",
        }));
        let extract_icon = |ui: &UiNode| {
            let row_children = descend(&descend(ui)[0]);
            let toggle = &row_children[0];
            let toggle_kids = descend(toggle);
            match &toggle_kids[0] {
                UiNode::Image { source, .. } => source.clone(),
                _ => panic!(),
            }
        };
        assert!(extract_icon(&on).contains("check"));
        assert!(!extract_icon(&off).contains("check"));
    }

    #[test]
    fn toggle_remove_drag_each_carry_distinct_routes() {
        let ui = lower_one(json!({
            "label": "T",
            "modifier-id": "tooltip",
            "modifier-idx": 1,
            "enabled": true,
            "target-id": "n",
        }));
        let row_children = descend(&descend(&ui)[0]);
        let toggle = match &row_children[0] {
            UiNode::Container { props, .. } => &props.semantic.attrs,
            _ => panic!(),
        };
        let remove = match &row_children[3] {
            UiNode::Container { props, .. } => &props.semantic.attrs,
            _ => panic!(),
        };
        let drag = match &row_children[4] {
            UiNode::Container { props, .. } => &props.semantic.attrs,
            _ => panic!(),
        };
        let role_of = |attrs: &[(String, String)]| {
            attrs
                .iter()
                .find(|(k, _)| k == "data-role")
                .map(|(_, v)| v.as_str())
                .unwrap_or("")
                .to_string()
        };
        assert_eq!(role_of(toggle), "modifier-toggle");
        assert_eq!(role_of(remove), "modifier-remove");
        assert_eq!(role_of(drag), "modifier-reorder");
    }
}
