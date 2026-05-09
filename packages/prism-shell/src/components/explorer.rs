//! `shell.explorer` — left rail file/object explorer. Hosts a column
//! of [`shell.inspector-row`] entries (or any registered row block)
//! via the `nodes` JSON prop.

use prism_builder::{
    document::Node,
    registry::FieldSpec,
    style::StyleProperties,
    ui_lower::{bare_container, LowerCtx},
    Block, ComponentId,
};
use prism_ui_runtime::layout::{Direction, Node as UiNode, Padding, Semantic, Sizing};

pub struct Explorer {
    pub id: ComponentId,
}

impl Block for Explorer {
    fn id(&self) -> &ComponentId {
        &self.id
    }

    fn schema(&self) -> Vec<FieldSpec> {
        vec![FieldSpec::text("nodes", "Nodes (JSON array of row props)")]
    }

    fn lower_ui(&self, ctx: &LowerCtx<'_>, node: &Node, _style: &StyleProperties) -> UiNode {
        let rows: Vec<UiNode> = node
            .props
            .get("nodes")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .enumerate()
                    .filter_map(|(idx, item)| {
                        ctx.lower_as(
                            "shell.inspector-row",
                            format!("{}::row::{}", node.id, idx),
                            item.clone(),
                        )
                    })
                    .collect()
            })
            .unwrap_or_default();

        bare_container(node.id.clone(), rows, |p| {
            p.direction = Direction::Column;
            p.padding = Padding {
                left: 4.0,
                right: 4.0,
                top: 4.0,
                bottom: 4.0,
            };
            p.width = Sizing::Grow;
            p.height = Sizing::Grow;
            p.semantic = Semantic::tag("aside")
                .with_attr("aria-label", "Explorer")
                .with_attr("data-role", "explorer");
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::registry::{register_shell_builtins, ShellComponentRegistry};
    use prism_builder::document::Node as BuilderNode;
    use prism_builder::layout::LayoutMode;
    use prism_core::foundation::spatial::Transform2D;
    use serde_json::json;

    #[test]
    fn rows_dispatch_through_inspector_row() {
        let block = Explorer {
            id: "shell.explorer".into(),
        };
        let n = BuilderNode {
            id: "ex".into(),
            component: "shell.explorer".into(),
            props: json!({
                "nodes": [
                    { "label": "src", "kind": "node", "depth": 0 },
                    { "label": "lib.rs", "kind": "row", "depth": 1 },
                ]
            }),
            children: vec![],
            layout_mode: LayoutMode::default(),
            transform: Transform2D::default(),
            modifiers: vec![],
            style: StyleProperties::default(),
        };
        let mut reg = ShellComponentRegistry::new();
        register_shell_builtins(&mut reg).expect("register");
        let owned = reg;
        let cascade = StyleProperties::default();
        let ctx = LowerCtx::new(Some(owned.as_component_registry()), &cascade);
        let UiNode::Container { children, .. } = block.lower_ui(&ctx, &n, &cascade) else {
            panic!()
        };
        assert_eq!(children.len(), 2);
    }
}
