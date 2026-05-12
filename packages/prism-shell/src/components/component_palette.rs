//! `shell.component-palette` — left rail palette listing draggable
//! component types. Reads an `items` JSON array (`{ id, label,
//! icon? }`).

use prism_builder::{
    document::Node,
    registry::FieldSpec,
    signal::SignalDef,
    style::StyleProperties,
    ui_lower::{
        bare_container, colored_text_node, hover_bg, image_node, parse_color, uniform_radius,
        LowerCtx,
    },
    with_common_signals,
};
use prism_ui_runtime::layout::{Direction, Node as UiNode, Padding, Semantic, Sizing};
use serde_json::Value;

const ROW_HEIGHT: f32 = 32.0;
/// Same tint intensity the canvas-document preview nodes use
/// (`builder_canvas::CANVAS_NODE_HOVER_BG`). The previous black tint
/// at 6% alpha was too subtle to register against the panel's near-
/// white background — users couldn't tell hover was wired. Matching
/// the canvas tint keeps "hovered chrome" reading the same across
/// the whole window.
const ROW_HOVER: &str = "#1a0060c0";
const ROW_SELECTED: &str = "#330060c0";
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

fn component_palette_lower(ctx: &LowerCtx<'_>, node: &Node, _style: &StyleProperties) -> UiNode {
    let selected = ctx.prop_str(node, "selected-id");
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
    // The binding emits `item-id` to match the kebab-prefixed convention
    // every other row-block uses (`tab-id`, `page-id`, `node-id`,
    // `app-id`). Reading the wrong key was the load-bearing bug behind
    // "palette clicks do nothing in production": the row container got
    // built with an empty id, so the lowering's
    // `if !id.is_empty() { s.with_attr("data-target-id", id) }` guard
    // skipped, and `handle_palette_item_click`'s
    // `data-target-id`-required short-circuit fired on every click.
    let id = item.get("item-id").and_then(|v| v.as_str()).unwrap_or("");
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
        // `item-id` matches the binding emission shape (see
        // `CatalogSlot::palette_json`) — using plain `id` here would
        // silently render rows with empty `data-target-id`, mirroring
        // the production bug the kebab-prefix convention was supposed
        // to prevent. Tests follow the binding's shape, not a
        // convenient shorthand.
        let ui = lower(json!({
            "items": [
                { "item-id": "container", "label": "Container", "icon": "icons/box.svg" },
                { "item-id": "text", "label": "Text" },
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
                { "item-id": "container", "label": "Container" },
                { "item-id": "text", "label": "Text" },
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

    #[test]
    fn binding_shape_matches_block_read() {
        // Keystone parity pin: the binding emits `item-id` (see
        // `CatalogSlot::palette_json`), and the block reads `item-id`.
        // If a future refactor renames either side, this assertion
        // fails before the bug ships. The previous mismatch (binding
        // emits `item-id`, block reads `id`) silently broke every
        // palette-item click in production while every unit test in
        // this file passed.
        use crate::state::{CatalogSlot, PaletteItem};
        let catalog = CatalogSlot {
            palette: vec![PaletteItem {
                id: "container".into(),
                label: "Container".into(),
                icon: "icons/box.svg".into(),
                category: "Layout".into(),
            }],
            ..Default::default()
        };
        let props = catalog.component_palette_props();
        let ui = lower(props);
        let UiNode::Container { children, .. } = ui else {
            panic!()
        };
        let UiNode::Container { props: row, .. } = &children[0] else {
            panic!()
        };
        assert!(
            row.semantic
                .attrs
                .iter()
                .any(|(k, v)| k == "data-target-id" && v == "container"),
            "binding-emitted item-id must surface as data-target-id; \
             palette clicks are dead without this"
        );
    }
}
