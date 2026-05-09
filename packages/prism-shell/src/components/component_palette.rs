//! `shell.component-palette` — left rail palette listing draggable
//! component types. Reads an `items` JSON array (`{ id, label,
//! icon? }`).

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

const ROW_HEIGHT: f32 = 32.0;
const ROW_HOVER: &str = "#0f000000";
const ROW_SELECTED: &str = "#190060c0";
const ROW_RADIUS: f32 = 4.0;
const LABEL_COLOR: &str = "#000000";
const ICON_SIZE: f32 = 16.0;

pub struct ComponentPalette {
    pub id: ComponentId,
}

impl Block for ComponentPalette {
    fn id(&self) -> &ComponentId {
        &self.id
    }

    fn schema(&self) -> Vec<FieldSpec> {
        vec![
            FieldSpec::text("items", "Items (JSON array)"),
            FieldSpec::text("selected-id", "Selected item id"),
        ]
    }

    fn signals(&self) -> Vec<SignalDef> {
        let mut s = common_signals();
        s.push(SignalDef::new("item-activated", "Palette item picked."));
        s
    }

    fn lower_ui(&self, _ctx: &LowerCtx<'_>, node: &Node, _style: &StyleProperties) -> UiNode {
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
}

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
        p.semantic = Semantic::tag("div")
            .with_attr("role", "option")
            .with_attr_if(is_selected, "aria-selected", "true");
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use prism_builder::document::Node as BuilderNode;
    use prism_builder::layout::LayoutMode;
    use prism_core::foundation::spatial::Transform2D;
    use serde_json::json;

    fn lower(props: Value) -> UiNode {
        let block = ComponentPalette {
            id: "shell.component-palette".into(),
        };
        let n = BuilderNode {
            id: "cp".into(),
            component: "shell.component-palette".into(),
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
}
