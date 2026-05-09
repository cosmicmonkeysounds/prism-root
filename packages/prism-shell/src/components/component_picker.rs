//! `shell.component-picker` — popup overlay for adding a component
//! to a grid cell. Reads a `categories` JSON array (`[{ label,
//! items: [{ id, label, icon? }] }]`) plus an optional `query`
//! string. The result is a vertical column of section headers and
//! item rows; the host owns positioning relative to the canvas
//! cell.
//!
//! Slint origin: component picker overlay around `ui/app.slint:4314`.

use prism_builder::{
    common_signals,
    document::Node,
    registry::FieldSpec,
    signal::SignalDef,
    style::StyleProperties,
    ui_lower::{
        bare_container, colored_text_node, hover_bg, image_node, parse_color, prop_string,
        uniform_radius, LowerCtx,
    },
    Block, ComponentId,
};
use prism_ui_runtime::layout::{Direction, Node as UiNode, Padding, Semantic, Sizing};
use serde_json::Value;

const POPUP_WIDTH: f32 = 280.0;
const POPUP_BG: &str = "#f0ffffff";
const POPUP_RADIUS: f32 = 8.0;
const ROW_HEIGHT: f32 = 32.0;
const ROW_HOVER: &str = "#0f000000";
const ROW_RADIUS: f32 = 4.0;
const SECTION_COLOR: &str = "#80000000";
const LABEL_COLOR: &str = "#000000";
const ICON_SIZE: f32 = 16.0;

pub struct ComponentPicker {
    pub id: ComponentId,
}

impl Block for ComponentPicker {
    fn id(&self) -> &ComponentId {
        &self.id
    }

    fn schema(&self) -> Vec<FieldSpec> {
        vec![
            FieldSpec::text("query", "Query string"),
            FieldSpec::text(
                "categories",
                "Categories (JSON array of {label, items: [{id, label, icon?}]})",
            ),
        ]
    }

    fn signals(&self) -> Vec<SignalDef> {
        let mut s = common_signals();
        s.push(SignalDef::new("item-picked", "Item activated."));
        s.push(SignalDef::new("query-changed", "Query string changed."));
        s.push(SignalDef::new("dismissed", "Popup dismissed."));
        s
    }

    fn lower_ui(&self, _ctx: &LowerCtx<'_>, node: &Node, _style: &StyleProperties) -> UiNode {
        let style = StyleProperties::default();
        let query = prop_string(node, "query");

        let mut kids: Vec<UiNode> = Vec::new();
        if !query.is_empty() {
            kids.push(colored_text_node(
                format!("{}::query", node.id),
                format!("filter · {query}"),
                &style,
                10.0,
                SECTION_COLOR,
            ));
        }

        if let Some(arr) = node.props.get("categories").and_then(|v| v.as_array()) {
            for (cidx, cat) in arr.iter().enumerate() {
                let label = cat.get("label").and_then(|v| v.as_str()).unwrap_or("");
                if !label.is_empty() {
                    kids.push(colored_text_node(
                        format!("{}::cat::{}::label", node.id, cidx),
                        label.into(),
                        &style,
                        11.0,
                        SECTION_COLOR,
                    ));
                }
                if let Some(items) = cat.get("items").and_then(|v| v.as_array()) {
                    for (iidx, item) in items.iter().enumerate() {
                        kids.push(build_item_row(node, &style, cidx, iidx, item));
                    }
                }
            }
        }

        bare_container(node.id.clone(), kids, |p| {
            p.direction = Direction::Column;
            p.gap = 4.0;
            p.padding = Padding {
                left: 8.0,
                right: 8.0,
                top: 8.0,
                bottom: 8.0,
            };
            p.width = Sizing::Fixed(POPUP_WIDTH);
            p.background = parse_color(POPUP_BG);
            p.radius = uniform_radius(POPUP_RADIUS);
            p.semantic = Semantic::tag("div")
                .with_attr("role", "dialog")
                .with_attr("aria-modal", "true")
                .with_attr("aria-label", "Component picker")
                .with_attr("data-role", "component-picker");
        })
    }
}

fn build_item_row(
    node: &Node,
    style: &StyleProperties,
    cidx: usize,
    iidx: usize,
    item: &Value,
) -> UiNode {
    let id = item.get("id").and_then(|v| v.as_str()).unwrap_or("");
    let label = item.get("label").and_then(|v| v.as_str()).unwrap_or(id);
    let icon = item.get("icon").and_then(|v| v.as_str()).unwrap_or("");

    let mut row_kids: Vec<UiNode> = Vec::with_capacity(2);
    if !icon.is_empty() {
        row_kids.push(image_node(
            format!("{}::cat::{}::item::{}::icon", node.id, cidx, iidx),
            icon.into(),
            style,
            Sizing::Fixed(ICON_SIZE),
            Sizing::Fixed(ICON_SIZE),
        ));
    }
    row_kids.push(colored_text_node(
        format!("{}::cat::{}::item::{}::label", node.id, cidx, iidx),
        label.into(),
        style,
        12.0,
        LABEL_COLOR,
    ));

    bare_container(
        format!("{}::cat::{}::item::{}", node.id, cidx, iidx),
        row_kids,
        |p| {
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
            p.hover = hover_bg(ROW_HOVER);
            p.semantic = Semantic::tag("div")
                .with_attr("role", "option")
                .with_attr("data-role", "component-picker-item")
                .with_attr("data-id", id);
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use prism_builder::document::Node as BuilderNode;
    use prism_builder::layout::LayoutMode;
    use prism_core::foundation::spatial::Transform2D;
    use serde_json::json;

    fn lower(props: Value) -> UiNode {
        let block = ComponentPicker {
            id: "shell.component-picker".into(),
        };
        let n = BuilderNode {
            id: "cp".into(),
            component: "shell.component-picker".into(),
            props,
            children: vec![],
            layout_mode: LayoutMode::default(),
            transform: Transform2D::default(),
            modifiers: vec![],
            style: StyleProperties::default(),
        };
        let cascade = StyleProperties::default();
        let ctx = LowerCtx::new(None, &cascade);
        block.lower_ui(&ctx, &n, &cascade)
    }

    #[test]
    fn empty_picker_renders_dialog() {
        let ui = lower(json!({}));
        let UiNode::Container {
            props, children, ..
        } = ui
        else {
            panic!()
        };
        assert!(children.is_empty());
        assert!(props
            .semantic
            .attrs
            .iter()
            .any(|(k, v)| k == "role" && v == "dialog"));
    }

    #[test]
    fn categories_and_items_render() {
        let ui = lower(json!({
            "query": "tx",
            "categories": [
                { "label": "Layout", "items": [
                    { "id": "container", "label": "Container", "icon": "icons/box.svg" },
                    { "id": "grid", "label": "Grid" },
                ]},
                { "label": "Text", "items": [
                    { "id": "heading", "label": "Heading" }
                ]},
            ]
        }));
        let UiNode::Container { children, .. } = ui else {
            panic!()
        };
        // query · Layout label · 2 items · Text label · 1 item = 6
        assert_eq!(children.len(), 6);
    }
}
