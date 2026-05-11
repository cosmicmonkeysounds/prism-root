//! `shell.menu-bar-row` — 28px top chrome row carrying menu pills,
//! optional app-name pill, and the workflow-page tab strip.
//!
//! Slint origin: `MenuBarRow` in `ui/app.slint` (lines 383-479).
//!
//! Smart pattern: the menu list and tab list are driven by JSON arrays
//! in `node.props["menus"]` / `node.props["tabs"]` (the runtime's
//! `<for>` lowering already handles array-driven repetition; this
//! Block consumes the resolved data shape). Each pill kind has a
//! single constructor (`menu_pill_node`, `app_name_pill_node`,
//! `tab_pill_node`) so the lowering body is a flat sequence of
//! `extend()` calls — no branching on item identity.
//!
//! The "+" add-page button reuses [`super::chrome::icon_button_node`]
//! at its native 28×28 size (the original Slint `28x28` cell with a
//! 12×12 glyph is one helper call).

use prism_builder::{
    document::Node,
    registry::FieldSpec,
    signal::SignalDef,
    style::StyleProperties,
    ui_lower::{
        bare_container, colored_text_node, hover_bg, parse_color, prop_str, uniform_radius,
        LowerCtx,
    },
    with_common_signals,
};
use prism_ui_runtime::layout::{Direction, Node as UiNode, Padding, Semantic, Sizing};
use serde_json::Value;

use super::chrome::icon_button_node;

const ROW_HEIGHT: f32 = 28.0;
const MENU_GAP: f32 = 0.0;
const MENU_PILL_PADDING_X: f32 = 10.0;
const MENU_PILL_RADIUS: f32 = 4.0;
const MENU_FONT_SIZE: f32 = 12.0;
const MENU_HOVER_BG: &str = "#14000000";
const MENU_ACTIVE_BG: &str = "#14336699";
const APP_NAME_FONT_SIZE: f32 = 11.0;
const APP_NAME_COLOR: &str = "#336699";
const TAB_FONT_SIZE: f32 = 11.0;
const TAB_ACTIVE_BG: &str = "#ffffff";
const TAB_INACTIVE_TEXT: &str = "#99000000";
const TAB_ACTIVE_TEXT: &str = "#000000";
const TAB_ACCENT: &str = "#336699";
const ROW_BG: &str = "#08000000";
const HAIRLINE_COLOR: &str = "#1a000000";
const SEPARATOR_COLOR: &str = "#26000000";

fn menu_bar_row_schema() -> Vec<FieldSpec> {
    vec![
        FieldSpec::text("app-name", "App name"),
        FieldSpec::boolean("show-tabs", "Show tabs").with_default(Value::Bool(false)),
        // `menus` and `tabs` are JSON arrays whose item shape is
        // intentionally untyped at the schema layer — runtime data
        // resolution (facets / Luau) feeds them directly.
    ]
}

fn menu_bar_row_signals() -> Vec<prism_builder::signal::SignalDef> {
    with_common_signals(vec![
        SignalDef::new(
            "item-clicked",
            "Menu pill clicked — payload is the menu id.",
        ),
        SignalDef::new("tab-activated", "Tab clicked — payload is the tab index."),
        SignalDef::new("add-page", "The trailing + button was clicked."),
    ])
}

fn menu_bar_row_lower(_ctx: &LowerCtx<'_>, node: &Node, _style: &StyleProperties) -> UiNode {
    let menus = node
        .props
        .get("menus")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let tabs = node
        .props
        .get("tabs")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let app_name = prop_str(node, "app-name");
    let show_tabs = node
        .props
        .get("show-tabs")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let active_menu = node
        .props
        .get("active-menu")
        .and_then(|v| v.as_i64())
        .unwrap_or(-1);

    let mut row_children: Vec<UiNode> = Vec::new();

    for (idx, item) in menus.iter().enumerate() {
        let label = item.get("label").and_then(|v| v.as_str()).unwrap_or("");
        let id = item.get("id").and_then(|v| v.as_str()).unwrap_or(label);
        row_children.push(menu_pill_node(
            format!("{}::menu::{}", node.id, id),
            label,
            idx as i64 == active_menu,
        ));
    }

    if show_tabs {
        row_children.push(separator_node(format!("{}::sep", node.id)));
        if !app_name.is_empty() {
            row_children.push(app_name_pill_node(
                format!("{}::app-name", node.id),
                app_name,
            ));
        }
        for (idx, tab) in tabs.iter().enumerate() {
            let title = tab.get("title").and_then(|v| v.as_str()).unwrap_or("");
            let active = tab.get("active").and_then(|v| v.as_bool()).unwrap_or(false);
            row_children.push(tab_pill_node(
                format!("{}::tab::{}", node.id, idx),
                title,
                active,
            ));
        }
        row_children.push(icon_button_node(
            format!("{}::add-page", node.id),
            "icons/plus.svg",
            true,
            Some("Add page"),
            Some("navigation.add-page"),
        ));
    }

    // Trailing flex spacer pushes everything left.
    row_children.push(bare_container(format!("{}::flex", node.id), vec![], |p| {
        p.width = Sizing::Grow;
        p.height = Sizing::Grow;
    }));

    bare_container(node.id.clone(), row_children, |props| {
        props.direction = Direction::Row;
        props.gap = MENU_GAP;
        // `Sizing::Grow` so the row spans the full app-window width.
        // Without this, the row was `Sizing::Fit` (tight to children),
        // which Taffy lays out by squeezing child widths under the
        // anonymous wrapper container — long labels like "Window"
        // wrapped to two lines because the pill's intrinsic width
        // was being shrunk below `chars * font_size * 0.55`.
        props.width = Sizing::Grow;
        props.height = Sizing::Fixed(ROW_HEIGHT);
        props.padding = Padding {
            left: 8.0,
            right: 0.0,
            top: 0.0,
            bottom: 0.0,
        };
        props.background = parse_color(ROW_BG);
        props.semantic = Semantic::tag("nav")
            .with_attr("role", "menubar")
            .with_attr("aria-label", "Application menu");
    })
}

pub const MENU_BAR_ROW_SPEC: prism_builder::BlockSpec =
    prism_builder::BlockSpec::new("shell.menu-bar-row", menu_bar_row_schema)
        .lower(menu_bar_row_lower)
        .signals(menu_bar_row_signals);

fn menu_pill_node(id: impl Into<String>, label: &str, active: bool) -> UiNode {
    let id = id.into();
    let style = StyleProperties::default();
    let text = colored_text_node(
        format!("{id}::label"),
        label.into(),
        &style,
        MENU_FONT_SIZE,
        "#000000",
    );
    bare_container(id, vec![text], |p| {
        p.height = Sizing::Fixed(ROW_HEIGHT);
        p.padding = Padding {
            left: MENU_PILL_PADDING_X,
            right: MENU_PILL_PADDING_X,
            top: 0.0,
            bottom: 0.0,
        };
        p.radius = uniform_radius(MENU_PILL_RADIUS);
        if active {
            p.background = parse_color(MENU_ACTIVE_BG);
        }
        p.hover = hover_bg(MENU_HOVER_BG);
        p.semantic = Semantic::tag("button")
            .with_attr("type", "button")
            .with_attr("role", "menuitem")
            .with_attr_if(active, "aria-expanded", "true");
    })
}

fn app_name_pill_node(id: impl Into<String>, name: &str) -> UiNode {
    let style = StyleProperties::default();
    let text = colored_text_node(
        format!("{}::label", id.into()),
        name.into(),
        &style,
        APP_NAME_FONT_SIZE,
        APP_NAME_COLOR,
    );
    bare_container(format!("{name}-pill"), vec![text], |p| {
        p.height = Sizing::Fixed(ROW_HEIGHT);
        p.padding = Padding {
            left: 8.0,
            right: 8.0,
            top: 0.0,
            bottom: 0.0,
        };
        p.semantic = Semantic::tag("strong").with_attr("data-role", "app-name");
    })
}

fn tab_pill_node(id: impl Into<String>, title: &str, active: bool) -> UiNode {
    let id = id.into();
    let style = StyleProperties::default();
    let label_color = if active {
        TAB_ACTIVE_TEXT
    } else {
        TAB_INACTIVE_TEXT
    };
    let label = colored_text_node(
        format!("{id}::label"),
        title.into(),
        &style,
        TAB_FONT_SIZE,
        label_color,
    );
    let body = bare_container(format!("{id}::body"), vec![label], |p| {
        p.direction = Direction::Row;
        p.padding = Padding {
            left: 10.0,
            right: 10.0,
            top: 0.0,
            bottom: 0.0,
        };
        p.height = Sizing::Grow;
    });
    // 2px accent stripe at the bottom (when active).
    let accent_color = if active { TAB_ACCENT } else { HAIRLINE_COLOR };
    let accent = bare_container(format!("{id}::accent"), vec![], |p| {
        p.height = Sizing::Fixed(2.0);
        p.background = parse_color(accent_color);
    });
    bare_container(id, vec![body, accent], |p| {
        p.direction = Direction::Column;
        p.height = Sizing::Fixed(ROW_HEIGHT);
        if active {
            p.background = parse_color(TAB_ACTIVE_BG);
        }
        p.hover = hover_bg(MENU_HOVER_BG);
        p.semantic = Semantic::tag("button")
            .with_attr("type", "button")
            .with_attr("role", "tab")
            .with_attr_if(active, "aria-selected", "true");
    })
}

fn separator_node(id: impl Into<String>) -> UiNode {
    bare_container(id, vec![], |p| {
        p.width = Sizing::Fixed(1.0);
        p.height = Sizing::Fixed(16.0);
        p.background = parse_color(SEPARATOR_COLOR);
        p.semantic = Semantic::tag("hr")
            .with_attr("role", "separator")
            .with_attr("aria-orientation", "vertical");
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::components::testing::{lower_with, test_node};
    use serde_json::json;

    fn lower(props: Value) -> UiNode {
        let n = test_node("mb", "shell.menu-bar-row", props);
        lower_with(&n, menu_bar_row_lower)
    }

    #[test]
    fn empty_row_has_only_trailing_flex_spacer() {
        let ui = lower(json!({}));
        let UiNode::Container { children, .. } = ui else {
            panic!()
        };
        assert_eq!(children.len(), 1, "just the flex spacer");
    }

    #[test]
    fn menu_pills_lay_out_in_order() {
        let ui = lower(json!({
            "menus": [
                { "id": "file", "label": "File" },
                { "id": "edit", "label": "Edit" },
            ]
        }));
        let UiNode::Container { children, .. } = ui else {
            panic!()
        };
        // 2 pills + spacer
        assert_eq!(children.len(), 3);
    }

    #[test]
    fn active_menu_paints_background() {
        let ui = lower(json!({
            "active-menu": 0,
            "menus": [{ "id": "file", "label": "File" }]
        }));
        let UiNode::Container { children, .. } = ui else {
            panic!()
        };
        let UiNode::Container { props, .. } = &children[0] else {
            panic!()
        };
        assert!(props.background.is_some());
        assert!(props
            .semantic
            .attrs
            .iter()
            .any(|(k, v)| k == "aria-expanded" && v == "true"));
    }

    #[test]
    fn show_tabs_inserts_separator_app_name_tabs_and_add_button() {
        let ui = lower(json!({
            "show-tabs": true,
            "app-name": "Lattice",
            "menus": [{ "id": "f", "label": "F" }],
            "tabs": [
                { "title": "Page 1", "active": true },
                { "title": "Page 2", "active": false },
            ]
        }));
        let UiNode::Container { children, .. } = ui else {
            panic!()
        };
        // 1 menu + sep + app-name + 2 tabs + add + spacer = 7
        assert_eq!(children.len(), 7);
    }

    #[test]
    fn tab_active_carries_aria_selected() {
        let ui = lower(json!({
            "show-tabs": true,
            "menus": [],
            "tabs": [{ "title": "P1", "active": true }]
        }));
        let UiNode::Container { children, .. } = ui else {
            panic!()
        };
        // children: sep + tab + add + spacer (no app-name)
        let UiNode::Container { props, .. } = &children[1] else {
            panic!()
        };
        assert_eq!(props.semantic.tag.as_deref(), Some("button"));
        assert!(props
            .semantic
            .attrs
            .iter()
            .any(|(k, v)| k == "aria-selected" && v == "true"));
    }

    #[test]
    fn outer_semantic_is_nav_with_menubar_role() {
        let ui = lower(json!({}));
        let UiNode::Container { props, .. } = ui else {
            panic!()
        };
        assert_eq!(props.semantic.tag.as_deref(), Some("nav"));
        assert!(props
            .semantic
            .attrs
            .iter()
            .any(|(k, v)| k == "role" && v == "menubar"));
        assert_eq!(props.height, Sizing::Fixed(ROW_HEIGHT));
    }
}
