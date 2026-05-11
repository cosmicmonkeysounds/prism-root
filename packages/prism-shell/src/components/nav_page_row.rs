//! `shell.nav-page-row` — single-page row in the navigation panel's
//! page list. Reads typed props (`page-title`, `route`, `is-active`,
//! `node-count`, `link-count`, `selected`, `show-delete`).
//!
//! Slint origin: per-page rows in the navigation panel page list
//! around `ui/app.slint:3710`.

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
const ACTIVE_BG: &str = "#160060c0";
const TITLE_COLOR: &str = "#000000";
const SECONDARY_COLOR: &str = "#80000000";

fn nav_page_row_schema() -> Vec<FieldSpec> {
    vec![
        FieldSpec::text("page-id", "Page id"),
        FieldSpec::text("page-title", "Page title"),
        FieldSpec::text("route", "Route"),
        FieldSpec::boolean("is-active", "Active page").with_default(Value::Bool(false)),
        FieldSpec::number(
            "node-count",
            "Node count",
            prism_builder::registry::NumericBounds::min(0.0),
        )
        .with_default(Value::from(0.0)),
        FieldSpec::number(
            "link-count",
            "Inbound link count",
            prism_builder::registry::NumericBounds::min(0.0),
        )
        .with_default(Value::from(0.0)),
        FieldSpec::boolean("selected", "Selected").with_default(Value::Bool(false)),
        FieldSpec::boolean("show-delete", "Show delete").with_default(Value::Bool(false)),
    ]
}

fn nav_page_row_signals() -> Vec<prism_builder::signal::SignalDef> {
    with_common_signals(vec![
        SignalDef::new("row-clicked", "Row activated."),
        SignalDef::new("move-up", "Move up clicked."),
        SignalDef::new("move-down", "Move down clicked."),
        SignalDef::new("delete-clicked", "Trash clicked."),
    ])
}

fn nav_page_row_lower(_ctx: &LowerCtx<'_>, node: &Node, _style: &StyleProperties) -> UiNode {
    let style = StyleProperties::default();
    let page_id = prop_string(node, "page-id");
    let title = prop_string(node, "page-title");
    let route = prop_string(node, "route");
    let is_active = prop_bool(node, "is-active", false);
    let selected = prop_bool(node, "selected", false);
    let show_delete = prop_bool(node, "show-delete", false);
    let node_count = node
        .props
        .get("node-count")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0) as i64;
    let link_count = node
        .props
        .get("link-count")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0) as i64;

    let mut left: Vec<UiNode> = Vec::with_capacity(4);
    left.push(colored_text_node(
        format!("{}::title", node.id),
        if title.is_empty() {
            "(untitled)".into()
        } else {
            title
        },
        &style,
        12.0,
        TITLE_COLOR,
    ));
    if !route.is_empty() {
        left.push(colored_text_node(
            format!("{}::route", node.id),
            route,
            &style,
            11.0,
            SECONDARY_COLOR,
        ));
    }
    if node_count > 0 || link_count > 0 {
        left.push(colored_text_node(
            format!("{}::counts", node.id),
            format!("{node_count} nodes · {link_count} links"),
            &style,
            10.0,
            SECONDARY_COLOR,
        ));
    }

    let left_cluster = bare_container(format!("{}::left", node.id), left, |p| {
        p.direction = Direction::Row;
        p.gap = 8.0;
        p.height = Sizing::Grow;
    });

    let mut right_kids: Vec<UiNode> = Vec::new();
    if selected {
        // Page-row chevrons / trash stay command-less for now: nav
        // pages have no "selected page" cursor in `NavigationSlot`,
        // so a stateless `cmd <id>` dispatch has nothing to act on.
        // Follow-up adds a selected-page index + `navigation.move-*`
        // commands.
        right_kids.push(icon_button_node(
            format!("{}::move-up", node.id),
            "icons/chevron-up.svg",
            true,
            Some("Move up"),
            None,
        ));
        right_kids.push(icon_button_node(
            format!("{}::move-down", node.id),
            "icons/chevron-down.svg",
            true,
            Some("Move down"),
            None,
        ));
    }
    if show_delete {
        right_kids.push(icon_button_node(
            format!("{}::delete", node.id),
            "icons/trash.svg",
            true,
            Some("Delete page"),
            None,
        ));
    }

    let mut row_kids = vec![left_cluster];
    if !right_kids.is_empty() {
        row_kids.push(bare_container(
            format!("{}::right", node.id),
            right_kids,
            |p| {
                p.direction = Direction::Row;
                p.gap = 2.0;
                p.height = Sizing::Grow;
            },
        ));
    }

    bare_container(node.id.clone(), row_kids, |p| {
        p.direction = Direction::Row;
        p.gap = 0.0;
        p.height = Sizing::Fixed(ROW_HEIGHT);
        p.padding = Padding {
            left: 12.0,
            right: 6.0,
            top: 0.0,
            bottom: 0.0,
        };
        p.radius = uniform_radius(ROW_RADIUS);
        p.background = if selected {
            parse_color(SELECTED_BG)
        } else if is_active {
            parse_color(ACTIVE_BG)
        } else {
            None
        };
        p.hover = hover_bg(HOVER_BG);
        let mut s = Semantic::tag("div")
            .with_attr("role", "listitem")
            .with_attr("data-role", "nav-page-row");
        if !page_id.is_empty() {
            s = s.with_attr("data-target-id", page_id.clone());
        }
        if is_active {
            s = s.with_attr("aria-current", "page");
        }
        if selected {
            s = s.with_attr("aria-selected", "true");
        }
        p.semantic = s;
    })
}

pub const NAV_PAGE_ROW_SPEC: prism_builder::BlockSpec =
    prism_builder::BlockSpec::new("shell.nav-page-row", nav_page_row_schema)
        .lower(nav_page_row_lower)
        .signals(nav_page_row_signals);

#[cfg(test)]
mod tests {
    use super::*;

    use crate::components::testing::{lower_with, test_node};
    use serde_json::json;

    fn lower(props: Value) -> UiNode {
        let n = test_node("npr", "shell.nav-page-row", props);
        lower_with(&n, nav_page_row_lower)
    }

    #[test]
    fn renders_title_route_counts() {
        let ui = lower(json!({
            "page-title": "Home",
            "route": "/",
            "node-count": 4,
            "link-count": 2
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
    fn active_page_paints_accent_bg() {
        let ui = lower(json!({ "page-title": "Home", "is-active": true }));
        let UiNode::Container { props, .. } = ui else {
            panic!()
        };
        assert!(props.background.is_some());
        assert!(props
            .semantic
            .attrs
            .iter()
            .any(|(k, v)| k == "aria-current" && v == "page"));
    }

    #[test]
    fn selected_grows_chevron_cluster() {
        let ui = lower(json!({ "page-title": "Home", "selected": true }));
        let UiNode::Container { children, .. } = ui else {
            panic!()
        };
        assert_eq!(children.len(), 2);
        let UiNode::Container {
            children: right, ..
        } = &children[1]
        else {
            panic!()
        };
        assert_eq!(right.len(), 2);
    }

    #[test]
    fn page_id_prop_surfaces_as_data_target_id_for_click_routing() {
        let ui = lower(json!({ "page-id": "home", "page-title": "Home" }));
        let UiNode::Container { props, .. } = ui else {
            panic!()
        };
        assert!(props
            .semantic
            .attrs
            .iter()
            .any(|(k, v)| k == "data-role" && v == "nav-page-row"));
        assert!(props
            .semantic
            .attrs
            .iter()
            .any(|(k, v)| k == "data-target-id" && v == "home"));
    }
}
