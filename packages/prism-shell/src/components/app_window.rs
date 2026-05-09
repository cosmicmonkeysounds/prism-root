//! `shell.app-window` — the top-level Studio shell scaffolding. Hosts
//! the menu bar at the top, the activity bar on the left edge, the
//! main content area (which receives the document's children), and
//! the status bar at the bottom.
//!
//! Slint origin: `AppWindow` in `ui/app.slint` (line 1477+).
//!
//! Smart pattern: this primitive is *purely structural* — it owns the
//! row/column scaffold and slot positions, and forwards every
//! interactive child (menu pills, nav buttons, status text, the
//! document tree itself) through the registry's normal lowering. The
//! activity-bar buttons and menu-bar items are encoded as JSON arrays
//! in props (matching `MenuBarRow` / `NavButton`). The main content
//! area uses `LowerCtx::lower_children` so any document subtree the
//! host hands AppWindow flows through registered blocks unchanged —
//! AppWindow does not know what content blocks exist.
//!
//! There is no host-coupling shortcut here: every nested chrome
//! primitive is lowered by re-using its existing `lower_ui` recipe.
//! AppWindow is the "compose every primitive at once" capstone for
//! the Phase-4 chrome scoreboard.

use prism_builder::{
    common_signals,
    document::Node,
    registry::FieldSpec,
    signal::SignalDef,
    style::StyleProperties,
    ui_lower::{bare_container, colored_text_node, parse_color, prop_str, LowerCtx},
    Block, ComponentId,
};
use prism_ui_runtime::layout::{Direction, Node as UiNode, Padding, Semantic, Sizing};
use serde_json::Value;

use super::{menu_bar_row::MenuBarRow, nav_button::NavButton};

const ACTIVITY_BAR_WIDTH: f32 = 40.0;
const STATUS_BAR_HEIGHT: f32 = 26.0;
const ACTIVITY_BAR_BG: &str = "#0d000000";
const STATUS_BAR_BG: &str = "#08000000";
const STATUS_TEXT_COLOR: &str = "#99000000";
const STATUS_FONT_SIZE: f32 = 11.0;
const CONTENT_BG: &str = "#ffffff";

pub struct AppWindow {
    pub id: ComponentId,
}

impl Block for AppWindow {
    fn id(&self) -> &ComponentId {
        &self.id
    }

    fn schema(&self) -> Vec<FieldSpec> {
        vec![
            FieldSpec::text("title", "Window title"),
            FieldSpec::text("status", "Status bar text"),
            FieldSpec::text("app-name", "Active app name"),
            // `menus`, `tabs`, `nav-buttons` are JSON arrays, untyped
            // at the schema layer — the host populates them from
            // workspace state.
        ]
    }

    fn signals(&self) -> Vec<SignalDef> {
        let mut signals = common_signals();
        signals.push(SignalDef::new(
            "nav-clicked",
            "Activity-bar button clicked — payload is the nav id.",
        ));
        signals
    }

    fn lower_ui(&self, ctx: &LowerCtx<'_>, node: &Node, _style: &StyleProperties) -> UiNode {
        let menu_bar = synth_menu_bar(node);
        let activity_bar = synth_activity_bar(node);
        let status_bar = synth_status_bar(node);

        // Content area — the document's own children flow through
        // here. AppWindow does not pre-stylise them; they get the same
        // cascade resolution every other Block does. When the resolver
        // (`RegistryTagResolver`) has already pre-lowered AST children
        // for us — i.e. the block was reached via `<shell.app-window>
        // …</shell.app-window>` from `.prism-ui` source — we adopt that
        // slice directly and skip the builder-Node walk. The fallback
        // chain keeps the host-driven path (`Shell` constructs builder
        // Nodes by hand) working unchanged.
        let content_children = ctx
            .host_children()
            .map(|s| s.to_vec())
            .unwrap_or_else(|| ctx.lower_children(&node.children));
        let content = bare_container(format!("{}::content", node.id), content_children, |p| {
            p.width = Sizing::Grow;
            p.height = Sizing::Grow;
            p.background = parse_color(CONTENT_BG);
            p.semantic = Semantic::tag("main").with_attr("role", "main");
        });

        let body = bare_container(
            format!("{}::body", node.id),
            vec![activity_bar, content],
            |p| {
                p.direction = Direction::Row;
                p.width = Sizing::Grow;
                p.height = Sizing::Grow;
            },
        );

        bare_container(node.id.clone(), vec![menu_bar, body, status_bar], |props| {
            props.direction = Direction::Column;
            props.width = Sizing::Grow;
            props.height = Sizing::Grow;
            props.semantic = Semantic::tag("div").with_attr("data-role", "app-window");
        })
    }
}

/// Synthesise the menu-bar row by delegating to `MenuBarRow::lower_ui`
/// over a derived child Node. Single source of truth: AppWindow does
/// not duplicate menu-pill rendering; it produces a virtual MenuBarRow
/// node and runs that block's lowering.
fn synth_menu_bar(node: &Node) -> UiNode {
    let derived = derived_node(
        format!("{}::menu", node.id),
        "shell.menu-bar-row",
        json_object_with_keys(
            node,
            &["menus", "tabs", "show-tabs", "app-name", "active-menu"],
        ),
    );
    let block = MenuBarRow {
        id: "shell.menu-bar-row".into(),
    };
    let cascade = StyleProperties::default();
    let ctx = LowerCtx::new(None, &cascade);
    block.lower_ui(&ctx, &derived, &cascade)
}

/// Activity bar = vertical column of `shell.nav-button` instances
/// (lowered through `NavButton::lower_ui`), centred horizontally in a
/// 40px wide column.
fn synth_activity_bar(node: &Node) -> UiNode {
    let cascade = StyleProperties::default();
    let ctx = LowerCtx::new(None, &cascade);
    let block = NavButton {
        id: "shell.nav-button".into(),
    };
    let buttons: Vec<UiNode> = node
        .props
        .get("nav-buttons")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .enumerate()
                .map(|(idx, item)| {
                    let derived = derived_node(
                        format!("{}::nav::{}", node.id, idx),
                        "shell.nav-button",
                        item.clone(),
                    );
                    block.lower_ui(&ctx, &derived, &cascade)
                })
                .collect()
        })
        .unwrap_or_default();

    bare_container(format!("{}::activity-bar", node.id), buttons, |p| {
        p.direction = Direction::Column;
        p.width = Sizing::Fixed(ACTIVITY_BAR_WIDTH);
        p.height = Sizing::Grow;
        p.background = parse_color(ACTIVITY_BAR_BG);
        p.semantic = Semantic::tag("nav")
            .with_attr("role", "navigation")
            .with_attr("aria-label", "Activity bar");
    })
}

fn synth_status_bar(node: &Node) -> UiNode {
    let status_text = prop_str(node, "status");
    let style = StyleProperties::default();
    let label = colored_text_node(
        format!("{}::status::label", node.id),
        status_text.into(),
        &style,
        STATUS_FONT_SIZE,
        STATUS_TEXT_COLOR,
    );
    bare_container(format!("{}::status", node.id), vec![label], |p| {
        p.direction = Direction::Row;
        p.height = Sizing::Fixed(STATUS_BAR_HEIGHT);
        p.padding = Padding {
            left: 12.0,
            right: 12.0,
            top: 0.0,
            bottom: 0.0,
        };
        p.background = parse_color(STATUS_BAR_BG);
        p.semantic = Semantic::tag("footer").with_attr("role", "contentinfo");
    })
}

/// Build a synthetic builder Node for the embedded MenuBarRow /
/// NavButton lowerings. We don't mutate the source document; this is a
/// transient wrapper that gives the embedded block a `node.props`
/// shape it expects.
fn derived_node(id: String, component: &str, props: Value) -> Node {
    Node {
        id,
        component: component.into(),
        props,
        children: vec![],
        layout_mode: prism_builder::layout::LayoutMode::default(),
        transform: prism_core::foundation::spatial::Transform2D::default(),
        modifiers: vec![],
        style: StyleProperties::default(),
    }
}

fn json_object_with_keys(node: &Node, keys: &[&str]) -> Value {
    let mut map = serde_json::Map::new();
    for k in keys {
        if let Some(v) = node.props.get(*k) {
            map.insert((*k).into(), v.clone());
        }
    }
    Value::Object(map)
}

#[cfg(test)]
mod tests {
    use super::*;
    use prism_builder::document::Node as BuilderNode;
    use prism_builder::layout::LayoutMode;
    use prism_core::foundation::spatial::Transform2D;
    use serde_json::json;

    fn lower(props: Value, children: Vec<BuilderNode>) -> UiNode {
        let block = AppWindow {
            id: "shell.app-window".into(),
        };
        let n = BuilderNode {
            id: "aw".into(),
            component: "shell.app-window".into(),
            props,
            children,
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
    fn lowers_to_three_sections_in_a_column() {
        let ui = lower(json!({}), vec![]);
        let UiNode::Container {
            children, props, ..
        } = ui
        else {
            panic!()
        };
        assert_eq!(props.direction, Direction::Column);
        assert_eq!(children.len(), 3, "menu + body + status");
    }

    #[test]
    fn body_row_contains_activity_bar_and_content() {
        let ui = lower(json!({}), vec![]);
        let UiNode::Container { children, .. } = ui else {
            panic!()
        };
        let body = &children[1];
        let UiNode::Container {
            children: body_kids,
            props,
            ..
        } = body
        else {
            panic!()
        };
        assert_eq!(props.direction, Direction::Row);
        assert_eq!(body_kids.len(), 2, "activity-bar + content");
        let UiNode::Container { props, .. } = &body_kids[0] else {
            panic!()
        };
        assert_eq!(props.width, Sizing::Fixed(ACTIVITY_BAR_WIDTH));
    }

    #[test]
    fn menu_bar_includes_menus_from_props() {
        let ui = lower(json!({ "menus": [{ "id": "f", "label": "File" }] }), vec![]);
        let UiNode::Container { children, .. } = ui else {
            panic!()
        };
        // The menu bar should have at least one pill child (plus
        // trailing flex spacer the MenuBarRow lowering adds).
        let UiNode::Container {
            children: menu_kids,
            ..
        } = &children[0]
        else {
            panic!()
        };
        assert!(menu_kids.len() >= 2);
    }

    #[test]
    fn status_bar_paints_text_when_set() {
        let ui = lower(json!({ "status": "Saved." }), vec![]);
        let UiNode::Container { children, .. } = ui else {
            panic!()
        };
        let UiNode::Container {
            children: status_kids,
            props,
            ..
        } = &children[2]
        else {
            panic!()
        };
        assert_eq!(props.height, Sizing::Fixed(STATUS_BAR_HEIGHT));
        let UiNode::Text { content, .. } = &status_kids[0] else {
            panic!()
        };
        assert_eq!(content, "Saved.");
    }

    #[test]
    fn content_area_lowers_document_children() {
        // Synthesise a child node — it will fall through to default
        // container since we have no registry, but it should still
        // appear in the content area's children.
        let child = BuilderNode {
            id: "child".into(),
            component: "container".into(),
            props: json!({}),
            children: vec![],
            layout_mode: LayoutMode::default(),
            transform: Transform2D::default(),
            modifiers: vec![],
            style: StyleProperties::default(),
        };
        let ui = lower(json!({}), vec![child]);
        let UiNode::Container { children, .. } = ui else {
            panic!()
        };
        let UiNode::Container {
            children: body_kids,
            ..
        } = &children[1]
        else {
            panic!()
        };
        let UiNode::Container {
            children: content_kids,
            ..
        } = &body_kids[1]
        else {
            panic!("content area not a container")
        };
        assert_eq!(content_kids.len(), 1);
    }

    #[test]
    fn activity_bar_lowers_each_nav_button_from_props() {
        let ui = lower(
            json!({
                "nav-buttons": [
                    { "icon": "icons/home.svg", "selected": true, "help-id": "home" },
                    { "icon": "icons/search.svg", "selected": false, "help-id": "search" },
                ]
            }),
            vec![],
        );
        let UiNode::Container { children, .. } = ui else {
            panic!()
        };
        let UiNode::Container {
            children: body_kids,
            ..
        } = &children[1]
        else {
            panic!()
        };
        let UiNode::Container {
            children: act_kids, ..
        } = &body_kids[0]
        else {
            panic!()
        };
        assert_eq!(act_kids.len(), 2);
    }

    #[test]
    fn outer_semantic_data_role() {
        let ui = lower(json!({}), vec![]);
        let UiNode::Container { props, .. } = ui else {
            panic!()
        };
        assert!(props
            .semantic
            .attrs
            .iter()
            .any(|(k, v)| k == "data-role" && v == "app-window"));
    }
}
