//! `shell.component-palette` — left rail palette listing draggable
//! component types. Reads an `items` JSON array (`{ id, label,
//! icon? }`).

use prism_builder::{
    document::Node,
    registry::FieldSpec,
    signal::SignalDef,
    style::StyleProperties,
    ui_lower::{
        bare_container, colored_text_node, hover_bg, image_node, parse_color, prop_string,
        uniform_radius, LowerCtx,
    },
    with_common_signals,
};
use prism_ui_runtime::layout::{Direction, Node as UiNode, Padding, Semantic, Sizing};
use serde_json::Value;

const ROW_HEIGHT: f32 = 32.0;
const ROW_HOVER: &str = "#0f000000";
const ROW_SELECTED: &str = "#190060c0";
const ROW_RADIUS: f32 = 4.0;
const LABEL_COLOR: &str = "#000000";
const ICON_SIZE: f32 = 16.0;

fn component_palette_schema() -> Vec<FieldSpec> {
    vec![
        FieldSpec::text("items", "Items (JSON array)"),
        FieldSpec::text("selected-id", "Selected item id"),
    ]
}

fn component_palette_signals() -> Vec<prism_builder::signal::SignalDef> {
    with_common_signals(vec![SignalDef::new(
        "item-activated",
        "Palette item picked.",
    )])
}

fn component_palette_lower(_ctx: &LowerCtx<'_>, node: &Node, _style: &StyleProperties) -> UiNode {
    let selected = prop_string(node, "selected-id");
    let style = StyleProperties::default();

    let rows: Vec<UiNode> = node
        .props
        .get("items")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .enumerate()
                .map(|(idx, item)| build_row(node, idx, item, &style, &selected))
                .collect()
        })
        .unwrap_or_default();

    bare_container(node.id.clone(), rows, |p| {
        p.direction = Direction::Column;
        p.gap = 2.0;
        p.padding = Padding {
            left: 6.0,
            right: 6.0,
            top: 6.0,
            bottom: 6.0,
        };
        p.width = Sizing::Grow;
        p.height = Sizing::Grow;
        p.semantic = Semantic::tag("aside")
            .with_attr("aria-label", "Component palette")
            .with_attr("data-role", "component-palette");
    })
}

pub const COMPONENT_PALETTE_SPEC: prism_builder::BlockSpec =
    prism_builder::BlockSpec::new("shell.component-palette", component_palette_schema)
        .lower(component_palette_lower)
        .signals(component_palette_signals);

fn build_row(
    node: &Node,
    idx: usize,
    item: &Value,
    style: &StyleProperties,
    selected: &str,
) -> UiNode {
    let id = item.get("id").and_then(|v| v.as_str()).unwrap_or("");
    let label = item.get("label").and_then(|v| v.as_str()).unwrap_or(id);
    let icon = item.get("icon").and_then(|v| v.as_str()).unwrap_or("");
    let is_selected = !selected.is_empty() && selected == id;

    let mut row_kids: Vec<UiNode> = Vec::with_capacity(2);
    if !icon.is_empty() {
        row_kids.push(image_node(
            format!("{}::row::{}::icon", node.id, idx),
            icon.into(),
            style,
            Sizing::Fixed(ICON_SIZE),
            Sizing::Fixed(ICON_SIZE),
        ));
    }
    row_kids.push(colored_text_node(
        format!("{}::row::{}::label", node.id, idx),
        label.into(),
        style,
        12.0,
        LABEL_COLOR,
    ));

    bare_container(format!("{}::row::{}", node.id, idx), row_kids, |p| {
        p.direction = Direction::Row;
        p.gap = 8.0;
        p.padding = Padding {
            left: 8.0,
            right: 8.0,
            top: 6.0,
            bottom: 6.0,
        };
        p.height = Sizing::Fixed(ROW_HEIGHT);
        p.radius = uniform_radius(ROW_RADIUS);
        if is_selected {
            p.background = parse_color(ROW_SELECTED);
        } else {
            p.hover = hover_bg(ROW_HOVER);
        }
        let mut s = Semantic::tag("div")
            .with_attr("role", "option")
            .with_attr("data-role", "palette-item");
        if !id.is_empty() {
            s = s.with_attr("data-target-id", id);
        }
        if is_selected {
            s = s.with_attr("aria-selected", "true");
        }
        p.semantic = s;
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::components::testing::{lower_with, test_node};
    use serde_json::json;

    fn lower(props: Value) -> UiNode {
        let n = test_node("cp", "shell.component-palette", props);
        lower_with(&n, component_palette_lower)
    }

    #[test]
    fn rows_render() {
        let ui = lower(json!({
            "items": [
                { "id": "container", "label": "Container", "icon": "icons/box.svg" },
                { "id": "text", "label": "Text" },
            ],
            "selected-id": "text",
        }));
        let UiNode::Container { children, .. } = ui else {
            panic!()
        };
        assert_eq!(children.len(), 2);
        let UiNode::Container { props: row, .. } = &children[1] else {
            panic!()
        };
        assert!(row.background.is_some(), "selected row tinted");
    }

    #[test]
    fn rows_carry_data_role_and_target_id_for_click_routing() {
        let ui = lower(json!({
            "items": [
                { "id": "container", "label": "Container" },
                { "id": "text", "label": "Text" },
            ],
        }));
        let UiNode::Container { children, .. } = ui else {
            panic!()
        };
        let UiNode::Container { props: row, .. } = &children[0] else {
            panic!()
        };
        assert!(row
            .semantic
            .attrs
            .iter()
            .any(|(k, v)| k == "data-role" && v == "palette-item"));
        assert!(row
            .semantic
            .attrs
            .iter()
            .any(|(k, v)| k == "data-target-id" && v == "container"));
    }
}
