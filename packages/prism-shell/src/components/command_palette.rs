//! `shell.command-palette` — Ctrl+Shift+P fuzzy-find palette.
//!
//! Slint origin: the `command-palette-overlay` block in
//! `ui/app.slint` around line 4043.
//!
//! Smart pattern: leaf — reads a `query` string and a `results` JSON
//! array of `{ id, label, shortcut?, category? }` entries. Renders a
//! 480px-wide rounded card with a query input row and a result list.
//! Selection state lives on the host (`AppState::command_palette_*`);
//! the lowering renders only the visual structure.

use prism_builder::{
    document::Node,
    registry::FieldSpec,
    signal::SignalDef,
    style::StyleProperties,
    ui_lower::{
        bare_container, colored_text_node, hover_bg, parse_color, prop_bool, text_input_node,
        uniform_radius, LowerCtx,
    },
    with_common_signals,
};
use prism_ui_runtime::layout::{Direction, Node as UiNode, Padding, Semantic, Sizing};
use serde_json::Value;

use crate::components::chrome::hidden_overlay;

const PALETTE_WIDTH: f32 = 480.0;
const PALETTE_RADIUS: f32 = 8.0;
const PALETTE_BG: &str = "#ffffff";
const ROW_HEIGHT: f32 = 32.0;
const ROW_HOVER: &str = "#0f000000";
const ROW_SELECTED: &str = "#190060c0";
const LABEL_COLOR: &str = "#000000";
const SHORTCUT_COLOR: &str = "#88000000";

fn command_palette_schema() -> Vec<FieldSpec> {
    vec![
        FieldSpec::text("query", "Search query"),
        FieldSpec::text("placeholder", "Placeholder")
            .with_default(Value::String("Search commands…".into())),
        FieldSpec::text("results", "Results (JSON array)"),
        FieldSpec::integer(
            "selected-index",
            "Selected result",
            prism_core::widget::field::NumericBounds::min(0.0),
        ),
    ]
}

fn command_palette_signals() -> Vec<prism_builder::signal::SignalDef> {
    with_common_signals(vec![
        SignalDef::new("query-changed", "User typed in the input."),
        SignalDef::new("result-activated", "User picked a result."),
    ])
}

fn command_palette_lower(_ctx: &LowerCtx<'_>, node: &Node, _style: &StyleProperties) -> UiNode {
    // Visibility gate (§43 A2): closed palette renders a zero-size
    // placeholder so it occupies no layout space. The skeleton authors
    // it as a sibling of the app-window so it always reaches the
    // resolver — visibility is host state, not skeleton state.
    if !prop_bool(node, "open", false) {
        return hidden_overlay(node.id.clone(), "command-palette");
    }
    let query = node
        .props
        .get("query")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let placeholder = node
        .props
        .get("placeholder")
        .and_then(|v| v.as_str())
        .unwrap_or("Search commands…")
        .to_string();
    let selected = node
        .props
        .get("selected-index")
        .and_then(|v| v.as_i64())
        .unwrap_or(0) as usize;

    let style = StyleProperties::default();

    let input = text_input_node(
        format!("{}::input", node.id),
        query,
        placeholder,
        &style,
        Sizing::Grow,
        Sizing::Fixed(36.0),
        13.0,
    );

    let results_arr = node
        .props
        .get("results")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let result_rows: Vec<UiNode> = results_arr
        .iter()
        .enumerate()
        .map(|(idx, item)| build_row(node, idx, item, idx == selected))
        .collect();

    let results_list = bare_container(format!("{}::results", node.id), result_rows, |p| {
        p.direction = Direction::Column;
        p.width = Sizing::Grow;
    });

    bare_container(node.id.clone(), vec![input, results_list], |p| {
        p.direction = Direction::Column;
        p.gap = 8.0;
        p.padding = Padding {
            left: 12.0,
            right: 12.0,
            top: 12.0,
            bottom: 12.0,
        };
        p.width = Sizing::Fixed(PALETTE_WIDTH);
        p.radius = uniform_radius(PALETTE_RADIUS);
        p.background = parse_color(PALETTE_BG);
        p.semantic = Semantic::tag("div")
            .with_attr("role", "dialog")
            .with_attr("aria-label", "Command palette")
            .with_attr("data-role", "command-palette");
    })
}

pub const COMMAND_PALETTE_SPEC: prism_builder::BlockSpec =
    prism_builder::BlockSpec::new("shell.command-palette", command_palette_schema)
        .lower(command_palette_lower)
        .signals(command_palette_signals);

fn build_row(node: &Node, idx: usize, item: &Value, selected: bool) -> UiNode {
    let label = item.get("label").and_then(|v| v.as_str()).unwrap_or("");
    let shortcut = item.get("shortcut").and_then(|v| v.as_str()).unwrap_or("");
    let style = StyleProperties::default();
    let label_node = colored_text_node(
        format!("{}::row::{}::label", node.id, idx),
        label.into(),
        &style,
        13.0,
        LABEL_COLOR,
    );
    let mut kids = vec![label_node];
    if !shortcut.is_empty() {
        kids.push(colored_text_node(
            format!("{}::row::{}::shortcut", node.id, idx),
            shortcut.into(),
            &style,
            11.0,
            SHORTCUT_COLOR,
        ));
    }
    bare_container(format!("{}::row::{}", node.id, idx), kids, |p| {
        p.direction = Direction::Row;
        p.gap = 12.0;
        p.padding = Padding {
            left: 8.0,
            right: 8.0,
            top: 6.0,
            bottom: 6.0,
        };
        p.width = Sizing::Grow;
        p.height = Sizing::Fixed(ROW_HEIGHT);
        p.radius = uniform_radius(4.0);
        if selected {
            p.background = parse_color(ROW_SELECTED);
        } else {
            p.hover = hover_bg(ROW_HOVER);
        }
        p.semantic = Semantic::tag("div")
            .with_attr("role", "option")
            .with_attr_if(selected, "aria-selected", "true");
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::components::testing::{lower_with, test_node};
    use serde_json::json;

    fn lower(props: Value) -> UiNode {
        let n = test_node("cp", "shell.command-palette", props);
        lower_with(&n, command_palette_lower)
    }

    #[test]
    fn closed_palette_is_hidden_zero_size() {
        // §43 A2: visibility gate. With `open=false` (default) the
        // palette must not paint any dialog chrome.
        let ui = lower(json!({}));
        let UiNode::Container { props, .. } = ui else {
            panic!()
        };
        assert_eq!(props.width, Sizing::Fixed(0.0));
        assert_eq!(props.height, Sizing::Fixed(0.0));
        assert!(props
            .semantic
            .attrs
            .iter()
            .any(|(k, v)| k == "aria-hidden" && v == "true"));
    }

    #[test]
    fn open_palette_has_dialog_role() {
        let ui = lower(json!({ "open": true }));
        let UiNode::Container { props, .. } = ui else {
            panic!()
        };
        assert!(props
            .semantic
            .attrs
            .iter()
            .any(|(k, v)| k == "role" && v == "dialog"));
    }

    #[test]
    fn results_render_as_rows() {
        let ui = lower(json!({
            "open": true,
            "results": [
                { "id": "save", "label": "Save", "shortcut": "Ctrl+S" },
                { "id": "open", "label": "Open" },
            ],
            "selected-index": 1,
        }));
        let UiNode::Container { children, .. } = ui else {
            panic!()
        };
        let UiNode::Container {
            children: results, ..
        } = &children[1]
        else {
            panic!()
        };
        assert_eq!(results.len(), 2);
        let UiNode::Container { props: row, .. } = &results[1] else {
            panic!()
        };
        assert!(row.background.is_some(), "selected row tinted");
    }

    #[test]
    fn input_carries_query_value() {
        let ui = lower(json!({ "open": true, "query": "hello" }));
        let UiNode::Container { children, .. } = ui else {
            panic!()
        };
        let UiNode::TextInput { value, .. } = &children[0] else {
            panic!()
        };
        assert_eq!(value, "hello");
    }
}
