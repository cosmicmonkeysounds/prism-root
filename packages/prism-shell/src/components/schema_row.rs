//! `shell.schema-row` — single field entry in the schema designer.
//! Reads typed props (`field-name`, `field-kind`, `required`,
//! `selected`, `show-delete`) and produces a 32px-tall row with a
//! type badge, name, optional required marker and a trailing trash
//! button.
//!
//! Slint origin: per-field rows in the schema designer panel
//! (`ui/app.slint` line 3816).

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

const ROW_HEIGHT: f32 = 32.0;
const ROW_RADIUS: f32 = 4.0;
const HOVER_BG: &str = "#0a000000";
const SELECTED_BG: &str = "#26000000";
const NAME_COLOR: &str = "#000000";
const KIND_BADGE_BG: &str = "#160060c0";
const KIND_BADGE_COLOR: &str = "#0060c0";
const REQUIRED_COLOR: &str = "#b91c1c";

fn schema_row_schema() -> Vec<FieldSpec> {
    vec![
        FieldSpec::text("field-id", "Field id (cursor key)"),
        FieldSpec::text("field-name", "Field name"),
        FieldSpec::text("field-kind", "Field kind"),
        FieldSpec::boolean("required", "Required").with_default(Value::Bool(false)),
        FieldSpec::boolean("selected", "Selected").with_default(Value::Bool(false)),
        FieldSpec::boolean("show-delete", "Show delete button").with_default(Value::Bool(false)),
    ]
}

fn schema_row_signals() -> Vec<prism_builder::signal::SignalDef> {
    with_common_signals(vec![
        SignalDef::new(
            "row-clicked",
            "Row activated; host selects the bound field.",
        ),
        SignalDef::new("delete-clicked", "Trash button pressed."),
    ])
}

fn schema_row_lower(_ctx: &LowerCtx<'_>, node: &Node, _style: &StyleProperties) -> UiNode {
    let style = StyleProperties::default();
    let field_id = prop_string(node, "field-id");
    let name = prop_string(node, "field-name");
    let kind = prop_string(node, "field-kind");
    let required = prop_bool(node, "required", false);
    let selected = prop_bool(node, "selected", false);
    let show_delete = prop_bool(node, "show-delete", false);

    let kind_badge = bare_container(
        format!("{}::badge", node.id),
        vec![colored_text_node(
            format!("{}::badge::label", node.id),
            if kind.is_empty() { "any".into() } else { kind },
            &style,
            10.0,
            KIND_BADGE_COLOR,
        )],
        |p| {
            p.padding = Padding {
                left: 6.0,
                right: 6.0,
                top: 2.0,
                bottom: 2.0,
            };
            p.radius = uniform_radius(3.0);
            p.background = parse_color(KIND_BADGE_BG);
        },
    );

    let mut left: Vec<UiNode> = vec![
        kind_badge,
        colored_text_node(
            format!("{}::name", node.id),
            if name.is_empty() {
                "(unnamed)".into()
            } else {
                name
            },
            &style,
            12.0,
            NAME_COLOR,
        ),
    ];
    if required {
        left.push(colored_text_node(
            format!("{}::required", node.id),
            "*".into(),
            &style,
            12.0,
            REQUIRED_COLOR,
        ));
    }

    let left_cluster = bare_container(format!("{}::left", node.id), left, |p| {
        p.direction = Direction::Row;
        p.gap = 8.0;
        p.height = Sizing::Grow;
    });

    let mut row_kids = vec![left_cluster];
    if show_delete {
        // Trash dispatches via the command table (no per-row target):
        // the `BuilderSlot::schema.selected_field` cursor is the
        // canonical sink — `schema.delete-selected-field` reads it.
        // The row click route moves the cursor onto this field's
        // `field-id` before the trash is visible, so a single command
        // body handles every row.
        row_kids.push(icon_button_node(
            format!("{}::delete", node.id),
            "icons/trash.svg",
            true,
            Some("Delete field"),
            Some("schema.delete-selected-field"),
        ));
    }

    bare_container(node.id.clone(), row_kids, |p| {
        p.direction = Direction::Row;
        p.gap = 0.0;
        p.height = Sizing::Fixed(ROW_HEIGHT);
        p.padding = Padding {
            left: 10.0,
            right: 6.0,
            top: 0.0,
            bottom: 0.0,
        };
        p.radius = uniform_radius(ROW_RADIUS);
        if selected {
            p.background = parse_color(SELECTED_BG);
        }
        p.hover = hover_bg(HOVER_BG);
        let mut s = Semantic::tag("div")
            .with_attr("role", "listitem")
            .with_attr("data-role", "schema-row");
        if !field_id.is_empty() {
            s = s.with_attr("data-target-id", field_id.clone());
        }
        if selected {
            s = s.with_attr("aria-selected", "true");
        }
        p.semantic = s;
    })
}

pub const SCHEMA_ROW_SPEC: prism_builder::BlockSpec =
    prism_builder::BlockSpec::new("shell.schema-row", schema_row_schema)
        .lower(schema_row_lower)
        .signals(schema_row_signals);

#[cfg(test)]
mod tests {
    use super::*;

    use crate::components::testing::{lower_with, test_node};
    use serde_json::json;

    fn lower(props: Value) -> UiNode {
        let n = test_node("sr", "shell.schema-row", props);
        lower_with(&n, schema_row_lower)
    }

    #[test]
    fn renders_badge_and_name() {
        let ui = lower(json!({ "field-name": "title", "field-kind": "text" }));
        let UiNode::Container { children, .. } = ui else {
            panic!()
        };
        let UiNode::Container { children: left, .. } = &children[0] else {
            panic!()
        };
        // badge + name
        assert_eq!(left.len(), 2);
    }

    #[test]
    fn required_marker_appended_when_required() {
        let ui = lower(json!({
            "field-name": "id",
            "field-kind": "text",
            "required": true
        }));
        let UiNode::Container { children, .. } = ui else {
            panic!()
        };
        let UiNode::Container { children: left, .. } = &children[0] else {
            panic!()
        };
        assert_eq!(left.len(), 3);
    }

    #[test]
    fn show_delete_appends_trash_button() {
        let ui = lower(json!({
            "field-name": "title",
            "field-kind": "text",
            "show-delete": true,
        }));
        let UiNode::Container { children, .. } = ui else {
            panic!()
        };
        assert_eq!(children.len(), 2);
    }

    #[test]
    fn field_id_prop_surfaces_as_data_target_id_for_routing() {
        let ui = lower(json!({
            "field-id": "title",
            "field-name": "Title",
            "field-kind": "text"
        }));
        let UiNode::Container { props, .. } = ui else {
            panic!()
        };
        assert!(props
            .semantic
            .attrs
            .iter()
            .any(|(k, v)| k == "data-role" && v == "schema-row"));
        assert!(props
            .semantic
            .attrs
            .iter()
            .any(|(k, v)| k == "data-target-id" && v == "title"));
    }

    #[test]
    fn trash_button_threads_delete_selected_field_command() {
        let ui = lower(json!({
            "field-id": "title",
            "field-name": "Title",
            "field-kind": "text",
            "show-delete": true,
        }));
        let UiNode::Container { children, .. } = ui else {
            panic!()
        };
        let UiNode::Container { props, .. } = &children[1] else {
            panic!("trash icon button expected when show-delete is true")
        };
        assert!(props
            .semantic
            .attrs
            .iter()
            .any(|(k, v)| k == "data-on-click" && v == "cmd schema.delete-selected-field"));
    }
}
