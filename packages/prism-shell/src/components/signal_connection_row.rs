//! `shell.signal-connection-row` — one row in the signals panel
//! describing a single `Connection` (source signal → action kind →
//! target node). Reads typed props (`source-signal`, `action-kind`,
//! `target-label`, `selected`) and mirrors `shell.inspector-row`'s
//! 30px geometry.
//!
//! Slint origin: the connection rows in `ui/app.slint` panels-list
//! around line 3650 (signals panel content).

use prism_builder::{
    document::Node,
    registry::FieldSpec,
    signal::SignalDef,
    style::StyleProperties,
    ui_lower::{
        bare_container, colored_text_node, hover_bg, parse_color, prop_bool, prop_string,
        uniform_radius, LowerCtx,
    },
    with_common_signals,
};
use prism_ui_runtime::layout::{Direction, Node as UiNode, Padding, Semantic, Sizing};
use serde_json::Value;

use super::chrome::icon_button_node;

const ROW_HEIGHT: f32 = 30.0;
const ROW_RADIUS: f32 = 4.0;
const ROW_GAP: f32 = 8.0;
const HOVER_BG: &str = "#0a000000";
const SELECTED_BG: &str = "#26000000";
const LABEL_COLOR: &str = "#000000";
const SECONDARY_COLOR: &str = "#80000000";
const ACCENT_COLOR: &str = "#0060c0";

fn signal_connection_row_schema() -> Vec<FieldSpec> {
    vec![
        FieldSpec::text("source-signal", "Source signal"),
        FieldSpec::text("action-kind", "Action kind"),
        FieldSpec::text("target-label", "Target label"),
        FieldSpec::boolean("selected", "Selected").with_default(Value::Bool(false)),
        FieldSpec::boolean("show-delete", "Show delete affordance")
            .with_default(Value::Bool(false)),
    ]
}

fn signal_connection_row_signals() -> Vec<prism_builder::signal::SignalDef> {
    with_common_signals(vec![
        SignalDef::new(
            "row-clicked",
            "Row activated; host selects the bound connection.",
        ),
        SignalDef::new("delete-clicked", "Trash button pressed."),
    ])
}

fn signal_connection_row_lower(
    _ctx: &LowerCtx<'_>,
    node: &Node,
    _style: &StyleProperties,
) -> UiNode {
    let style = StyleProperties::default();
    let signal = prop_string(node, "source-signal");
    let kind = prop_string(node, "action-kind");
    let target = prop_string(node, "target-label");
    let selected = prop_bool(node, "selected", false);
    let show_delete = prop_bool(node, "show-delete", false);

    let signal_label = if signal.is_empty() {
        "(no signal)".into()
    } else {
        signal
    };

    let mut left: Vec<UiNode> = Vec::with_capacity(5);
    left.push(colored_text_node(
        format!("{}::signal", node.id),
        signal_label,
        &style,
        12.0,
        ACCENT_COLOR,
    ));
    if !kind.is_empty() {
        left.push(colored_text_node(
            format!("{}::arrow", node.id),
            "→".into(),
            &style,
            12.0,
            SECONDARY_COLOR,
        ));
        left.push(colored_text_node(
            format!("{}::kind", node.id),
            kind,
            &style,
            12.0,
            LABEL_COLOR,
        ));
    }
    if !target.is_empty() {
        left.push(colored_text_node(
            format!("{}::dot", node.id),
            "·".into(),
            &style,
            12.0,
            SECONDARY_COLOR,
        ));
        left.push(colored_text_node(
            format!("{}::target", node.id),
            target,
            &style,
            11.0,
            SECONDARY_COLOR,
        ));
    }

    let left_cluster = bare_container(format!("{}::left", node.id), left, |p| {
        p.direction = Direction::Row;
        p.gap = ROW_GAP;
        p.height = Sizing::Grow;
    });

    let mut row_kids = vec![left_cluster];
    if show_delete {
        row_kids.push(icon_button_node(
            format!("{}::delete", node.id),
            "icons/trash.svg",
            true,
            Some("Delete connection"),
        ));
    }

    bare_container(node.id.clone(), row_kids, |p| {
        p.direction = Direction::Row;
        p.gap = 0.0;
        p.height = Sizing::Fixed(ROW_HEIGHT);
        p.radius = uniform_radius(ROW_RADIUS);
        p.padding = Padding {
            left: 12.0,
            right: 6.0,
            top: 0.0,
            bottom: 0.0,
        };
        if selected {
            p.background = parse_color(SELECTED_BG);
        }
        p.hover = hover_bg(HOVER_BG);
        let mut s = Semantic::tag("div")
            .with_attr("role", "listitem")
            .with_attr("data-role", "signal-connection-row");
        if selected {
            s = s.with_attr("aria-selected", "true");
        }
        p.semantic = s;
    })
}

pub const SIGNAL_CONNECTION_ROW_SPEC: prism_builder::BlockSpec =
    prism_builder::BlockSpec::new("shell.signal-connection-row", signal_connection_row_schema)
        .lower(signal_connection_row_lower)
        .signals(signal_connection_row_signals);

#[cfg(test)]
mod tests {
    use super::*;

    use crate::components::testing::{lower_with, test_node};
    use serde_json::json;

    fn lower(props: Value) -> UiNode {
        let n = test_node("scr", "shell.signal-connection-row", props);
        lower_with(&n, signal_connection_row_lower)
    }

    #[test]
    fn renders_full_row() {
        let ui = lower(json!({
            "source-signal": "clicked",
            "action-kind": "NavigateTo",
            "target-label": "page-2",
            "selected": true,
        }));
        let UiNode::Container {
            children, props, ..
        } = ui
        else {
            panic!()
        };
        assert!(props.background.is_some(), "selected row tinted");
        assert_eq!(children.len(), 1, "no delete unless show-delete");
        let UiNode::Container { children: left, .. } = &children[0] else {
            panic!()
        };
        // signal + arrow + kind + dot + target = 5
        assert_eq!(left.len(), 5);
    }

    #[test]
    fn show_delete_appends_trash_button() {
        let ui = lower(json!({
            "source-signal": "clicked",
            "action-kind": "SetProperty",
            "target-label": "x",
            "show-delete": true,
        }));
        let UiNode::Container { children, .. } = ui else {
            panic!()
        };
        assert_eq!(children.len(), 2);
    }

    #[test]
    fn empty_signal_falls_through_placeholder() {
        let ui = lower(json!({}));
        let UiNode::Container { children, .. } = ui else {
            panic!()
        };
        let UiNode::Container { children: left, .. } = &children[0] else {
            panic!()
        };
        assert_eq!(left.len(), 1, "only the (no signal) placeholder");
    }
}
