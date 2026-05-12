//! `shell.schema-designer` — Data workflow page panel composing
//! `shell.schema-row` entries from a `fields` JSON array.
//! Optional title/header text is exposed via the `title` prop.
//!
//! Slint origin: schema designer header + list around `ui/app.slint`
//! lines 3800–3900.

use prism_builder::{
    document::Node,
    registry::FieldSpec,
    style::StyleProperties,
    ui_lower::{bare_container, colored_text_node, LowerCtx},
};
use prism_ui_runtime::layout::{Direction, Node as UiNode, Padding, Semantic, Sizing};

const HEADER_COLOR: &str = "#80000000";

fn schema_designer_schema() -> Vec<FieldSpec> {
    vec![
        FieldSpec::text("title", "Section title"),
        FieldSpec::text("schema-name", "Schema name"),
        FieldSpec::text("fields", "Fields (JSON array of schema-row props)"),
    ]
}

fn schema_designer_lower(ctx: &LowerCtx<'_>, node: &Node, _style: &StyleProperties) -> UiNode {
    let style = StyleProperties::default();
    let title = ctx.prop_str(node, "title");
    let schema_name = ctx.prop_str(node, "schema-name");

    let mut kids: Vec<UiNode> = Vec::new();

    if !title.is_empty() {
        kids.push(colored_text_node(
            format!("{}::title", node.id),
            title,
            &style,
            14.0,
            "#000000",
        ));
    }
    if !schema_name.is_empty() {
        kids.push(colored_text_node(
            format!("{}::schema", node.id),
            format!("schema · {schema_name}"),
            &style,
            11.0,
            HEADER_COLOR,
        ));
    }

    // Author-driven children (e.g. an actions toolbar) come first
    // after the header, then the fields-array dispatch.
    let host = ctx
        .host_children()
        .map(|s| s.to_vec())
        .unwrap_or_else(|| ctx.lower_children(&node.children));
    kids.extend(host);

    if let Some(arr) = node.props.get("fields").and_then(|v| v.as_array()) {
        for (idx, item) in arr.iter().enumerate() {
            if let Some(child) = ctx.lower_as(
                "shell.schema-row",
                format!("{}::row::{}", node.id, idx),
                item.clone(),
            ) {
                kids.push(child);
            }
        }
    }

    bare_container(node.id.clone(), kids, |p| {
        p.direction = Direction::Column;
        p.gap = 6.0;
        p.padding = Padding {
            left: 16.0,
            right: 16.0,
            top: 14.0,
            bottom: 14.0,
        };
        p.width = Sizing::Grow;
        p.height = Sizing::Grow;
        p.semantic = Semantic::tag("section")
            .with_attr("aria-label", "Schema designer")
            .with_attr("data-role", "schema-designer");
    })
}

pub const SCHEMA_DESIGNER_SPEC: prism_builder::BlockSpec =
    prism_builder::BlockSpec::new("shell.schema-designer", schema_designer_schema)
        .lower(schema_designer_lower);

#[cfg(test)]
mod tests {
    use super::*;

    use crate::components::registry::{register_full_shell_chrome, ShellComponentRegistry};
    use crate::components::testing::test_node;
    use serde_json::json;

    fn lower(props: serde_json::Value) -> UiNode {
        let n = test_node("sd", "shell.schema-designer", props);
        let mut reg = ShellComponentRegistry::new();
        // Wave 11.2: `shell.schema-row` migrated to `.prism-ui` source,
        // so the full chrome bootstrap (native + DSL) is required for
        // the dispatch to resolve.
        register_full_shell_chrome(&mut reg).expect("register");
        let cascade = StyleProperties::default();
        let ctx = LowerCtx::new(Some(reg.as_component_registry()), &cascade);
        schema_designer_lower(&ctx, &n, &cascade)
    }

    #[test]
    fn header_only_when_no_fields() {
        let ui = lower(json!({ "title": "Posts", "schema-name": "post" }));
        let UiNode::Container { children, .. } = ui else {
            panic!()
        };
        assert_eq!(children.len(), 2);
    }

    #[test]
    fn fields_dispatch_to_rows() {
        let ui = lower(json!({
            "title": "Posts",
            "fields": [
                { "field-name": "title", "field-kind": "text", "required": true },
                { "field-name": "body", "field-kind": "rich-text" },
                { "field-name": "tags", "field-kind": "list" },
            ]
        }));
        let UiNode::Container { children, .. } = ui else {
            panic!()
        };
        // title + 3 rows
        assert_eq!(children.len(), 4);
    }
}
