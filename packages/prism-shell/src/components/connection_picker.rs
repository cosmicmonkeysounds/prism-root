//! `shell.connection-picker` — overlay form that builds a fresh
//! `SignalConnection`. Three rows (source signal / action kind /
//! target label) plus Add + Cancel buttons.
//!
//! See `docs/dev/composable-builder-plan.md` Wave 4.3.
//!
//! Routing attrs:
//! - `data-role="connection-picker-field"` + `data-field` ∈
//!   {`source-signal`, `action-kind`, `target-label`} — click to
//!   cycle (action-kind today) or focus (source / target text
//!   fields, deferred to Wave 10's text-input primitive).
//! - `data-role="connection-picker-add"` — confirm.
//! - `data-role="connection-picker-cancel"` — close without
//!   inserting.
//!
//! Visibility follows the §43-A2 hidden-overlay shape when `open`
//! is false — the block paints nothing but stays in the skeleton
//! so route dispatch never goes through a missing tag.

use prism_builder::{
    document::Node,
    registry::FieldSpec,
    style::StyleProperties,
    ui_lower::{
        bare_container, colored_text_node, hover_bg, parse_color, uniform_radius, LowerCtx,
    },
};
use prism_ui_runtime::layout::{Direction, Node as UiNode, Padding, Semantic, Sizing};
use serde_json::Value;

use crate::components::chrome::hidden_overlay;

const POPUP_WIDTH: f32 = 320.0;
const POPUP_BG: &str = "#f0ffffff";
const POPUP_RADIUS: f32 = 8.0;
const ROW_HEIGHT: f32 = 32.0;
const ROW_HOVER: &str = "#0f000000";
const ROW_RADIUS: f32 = 4.0;
const LABEL_COLOR: &str = "#000000";
const SECONDARY_COLOR: &str = "#7f000000";
const LABEL_FONT: f32 = 12.0;
const KEY_FONT: f32 = 10.0;
const BUTTON_BG_ADD: &str = "#0060c0";
const BUTTON_BG_CANCEL: &str = "#11000000";
const BUTTON_TEXT_ADD: &str = "#ffffff";
const BUTTON_TEXT_CANCEL: &str = "#000000";

fn connection_picker_schema() -> Vec<FieldSpec> {
    vec![
        FieldSpec::boolean("open", "Open").with_default(Value::Bool(false)),
        FieldSpec::text("source-signal", "Source signal"),
        FieldSpec::text("action-kind", "Action kind"),
        FieldSpec::text("target-label", "Target label"),
    ]
}

fn connection_picker_lower(ctx: &LowerCtx<'_>, node: &Node, style: &StyleProperties) -> UiNode {
    let open = ctx.prop_bool(node, "open", false);
    if !open {
        return hidden_overlay(node.id.clone(), "connection-picker");
    }
    let source = ctx.prop_str(node, "source-signal");
    let kind = ctx.prop_str(node, "action-kind");
    let target = ctx.prop_str(node, "target-label");

    let mut kids: Vec<UiNode> = Vec::with_capacity(4);
    kids.push(picker_row(
        node,
        "source",
        "Signal",
        &source,
        "source-signal",
        style,
    ));
    kids.push(picker_row(
        node,
        "kind",
        "Action",
        &kind,
        "action-kind",
        style,
    ));
    kids.push(picker_row(
        node,
        "target",
        "Target",
        &target,
        "target-label",
        style,
    ));
    kids.push(button_row(node, style));

    ctx.synthetic_container(node, style, kids, |p| {
        p.direction = Direction::Column;
        p.gap = 6.0;
        p.padding = Padding {
            left: 12.0,
            right: 12.0,
            top: 12.0,
            bottom: 12.0,
        };
        p.width = Sizing::Fixed(POPUP_WIDTH);
        p.background = parse_color(POPUP_BG);
        p.radius = uniform_radius(POPUP_RADIUS);
        let sem = Semantic::tag("div")
            .with_attr("data-role", "connection-picker")
            .with_attr("data-visible", "true")
            .with_attr("role", "dialog")
            .with_attr("aria-label", "Add connection");
        p.semantic = sem;
    })
}

/// One picker form row — a small "Signal" / "Action" / "Target"
/// label on the left, the current value on the right, click cycles
/// (action-kind) or focuses (source / target — text-input lands
/// with Wave 10's primitive registry).
fn picker_row(
    parent: &Node,
    suffix: &str,
    label: &str,
    value: &str,
    field: &str,
    style: &StyleProperties,
) -> UiNode {
    let label_text = colored_text_node(
        format!("{}::{suffix}::label", parent.id),
        label.into(),
        style,
        KEY_FONT,
        SECONDARY_COLOR,
    );
    let value_str = if value.is_empty() {
        "—".into()
    } else {
        value.to_string()
    };
    let value_text = colored_text_node(
        format!("{}::{suffix}::value", parent.id),
        value_str,
        style,
        LABEL_FONT,
        LABEL_COLOR,
    );
    bare_container(
        format!("{}::{suffix}", parent.id),
        vec![label_text, value_text],
        |p| {
            p.direction = Direction::Row;
            p.gap = 8.0;
            p.padding = Padding {
                left: 8.0,
                right: 8.0,
                top: 4.0,
                bottom: 4.0,
            };
            p.height = Sizing::Fixed(ROW_HEIGHT);
            p.radius = uniform_radius(ROW_RADIUS);
            p.hover = hover_bg(ROW_HOVER);
            let sem = Semantic::tag("button")
                .with_attr("data-role", "connection-picker-field")
                .with_attr("data-field", field)
                .with_attr("role", "button")
                .with_attr("aria-label", format!("Edit {label}"));
            p.semantic = sem;
        },
    )
}

/// Bottom button row — `Cancel` + `Add`. Add is the primary
/// affordance (filled blue); Cancel is ghost so the visual
/// hierarchy reads naturally.
fn button_row(parent: &Node, style: &StyleProperties) -> UiNode {
    let cancel = bare_container(
        format!("{}::cancel", parent.id),
        vec![colored_text_node(
            format!("{}::cancel::label", parent.id),
            "Cancel".into(),
            style,
            LABEL_FONT,
            BUTTON_TEXT_CANCEL,
        )],
        |p| {
            p.direction = Direction::Row;
            p.padding = Padding {
                left: 16.0,
                right: 16.0,
                top: 0.0,
                bottom: 0.0,
            };
            p.height = Sizing::Fixed(ROW_HEIGHT);
            p.background = parse_color(BUTTON_BG_CANCEL);
            p.radius = uniform_radius(ROW_RADIUS);
            let sem = Semantic::tag("button")
                .with_attr("data-role", "connection-picker-cancel")
                .with_attr("aria-label", "Cancel");
            p.semantic = sem;
        },
    );
    let add = bare_container(
        format!("{}::add", parent.id),
        vec![colored_text_node(
            format!("{}::add::label", parent.id),
            "Add".into(),
            style,
            LABEL_FONT,
            BUTTON_TEXT_ADD,
        )],
        |p| {
            p.direction = Direction::Row;
            p.padding = Padding {
                left: 16.0,
                right: 16.0,
                top: 0.0,
                bottom: 0.0,
            };
            p.height = Sizing::Fixed(ROW_HEIGHT);
            p.background = parse_color(BUTTON_BG_ADD);
            p.radius = uniform_radius(ROW_RADIUS);
            let sem = Semantic::tag("button")
                .with_attr("data-role", "connection-picker-add")
                .with_attr("aria-label", "Add connection");
            p.semantic = sem;
        },
    );
    bare_container(format!("{}::buttons", parent.id), vec![cancel, add], |p| {
        p.direction = Direction::Row;
        p.gap = 8.0;
        p.padding = Padding {
            left: 0.0,
            right: 0.0,
            top: 8.0,
            bottom: 0.0,
        };
    })
}

pub const CONNECTION_PICKER_SPEC: prism_builder::BlockSpec =
    prism_builder::BlockSpec::new("shell.connection-picker", connection_picker_schema)
        .lower(connection_picker_lower);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::testing::{lower_with, test_node};
    use serde_json::json;

    fn lower_one(props: Value) -> UiNode {
        lower_with(
            &test_node("cp", "shell.connection-picker", props),
            connection_picker_lower,
        )
    }

    fn outer_attrs(ui: &UiNode) -> &Vec<(String, String)> {
        match ui {
            UiNode::Container { props, .. } => &props.semantic.attrs,
            _ => panic!(),
        }
    }

    #[test]
    fn closed_picker_collapses_to_hidden_overlay() {
        let ui = lower_one(json!({ "open": false }));
        let attrs = outer_attrs(&ui);
        assert!(attrs
            .iter()
            .any(|(k, v)| k == "data-visible" && v == "false"));
        assert!(attrs.iter().any(|(k, v)| k == "aria-hidden" && v == "true"));
    }

    #[test]
    fn open_picker_renders_three_field_rows_and_two_buttons() {
        let ui = lower_one(json!({
            "open": true,
            "source-signal": "clicked",
            "action-kind": "SetProperty",
            "target-label": "demo-button",
        }));
        let UiNode::Container { children, .. } = &ui else {
            panic!()
        };
        // 3 field rows + 1 button row
        assert_eq!(children.len(), 4);
    }

    #[test]
    fn field_rows_carry_field_routing_attrs() {
        let ui = lower_one(json!({
            "open": true,
            "source-signal": "",
            "action-kind": "SetProperty",
            "target-label": "",
        }));
        let UiNode::Container { children, .. } = &ui else {
            panic!()
        };
        let fields: Vec<&str> = children
            .iter()
            .take(3)
            .filter_map(|c| {
                let UiNode::Container { props, .. } = c else {
                    return None;
                };
                let attrs = &props.semantic.attrs;
                let role = attrs.iter().find(|(k, _)| k == "data-role").map(|(_, v)| v);
                let field = attrs
                    .iter()
                    .find(|(k, _)| k == "data-field")
                    .map(|(_, v)| v);
                if role.map(String::as_str) == Some("connection-picker-field") {
                    field.map(String::as_str)
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(fields, vec!["source-signal", "action-kind", "target-label"]);
    }

    #[test]
    fn add_and_cancel_buttons_carry_distinct_routes() {
        let ui = lower_one(json!({
            "open": true,
            "source-signal": "clicked",
            "action-kind": "SetProperty",
            "target-label": "x",
        }));
        let UiNode::Container { children, .. } = &ui else {
            panic!()
        };
        let UiNode::Container {
            children: buttons, ..
        } = &children[3]
        else {
            panic!()
        };
        assert_eq!(buttons.len(), 2);
        let role_of = |ui: &UiNode| -> Option<String> {
            let UiNode::Container { props, .. } = ui else {
                return None;
            };
            props
                .semantic
                .attrs
                .iter()
                .find(|(k, _)| k == "data-role")
                .map(|(_, v)| v.clone())
        };
        assert_eq!(
            role_of(&buttons[0]).as_deref(),
            Some("connection-picker-cancel")
        );
        assert_eq!(
            role_of(&buttons[1]).as_deref(),
            Some("connection-picker-add")
        );
    }

    #[test]
    fn empty_field_value_renders_em_dash_placeholder() {
        let ui = lower_one(json!({
            "open": true,
            "source-signal": "",
            "action-kind": "",
            "target-label": "",
        }));
        let UiNode::Container { children, .. } = &ui else {
            panic!()
        };
        // The first field row's value text is the second child of
        // the row container.
        let UiNode::Container {
            children: row_kids, ..
        } = &children[0]
        else {
            panic!()
        };
        let UiNode::Text { content, .. } = &row_kids[1] else {
            panic!("value cell should be a text leaf")
        };
        assert_eq!(content, "—");
    }
}
