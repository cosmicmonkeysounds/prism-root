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
    ui_lower::{bare_container, parse_color, LowerCtx},
};
use prism_ui_runtime::layout::{Direction, Node as UiNode, Semantic, Sizing};
use serde_json::{json, Value};

const ACTIVITY_BAR_WIDTH: f32 = 40.0;
const ACTIVITY_BAR_BG: &str = "#0d000000";
const CONTENT_BG: &str = "#ffffff";

fn app_window_schema() -> Vec<FieldSpec> {
    vec![
        FieldSpec::text("title", "Window title"),
        FieldSpec::text("status", "Status bar text"),
        FieldSpec::text("app-name", "Active app name"),
        // `menus`, `tabs`, `nav-buttons` are JSON arrays, untyped
        // at the schema layer — the host populates them from
        // workspace state.
    ]
}

fn app_window_signals() -> Vec<prism_builder::signal::SignalDef> {
    let mut signals = common_signals();
    signals.push(SignalDef::new(
        "nav-clicked",
        "Activity-bar button clicked — payload is the nav id.",
    ));
    signals
}

fn app_window_lower(ctx: &LowerCtx<'_>, node: &Node, _style: &StyleProperties) -> UiNode {
    let menu_bar = synth_menu_bar(ctx, node);
    let activity_bar = synth_activity_bar(ctx, node);
    let status_bar = synth_status_bar(ctx, node);

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

pub const APP_WINDOW_SPEC: prism_builder::BlockSpec =
    prism_builder::BlockSpec::new("shell.app-window", app_window_schema)
        .lower(app_window_lower)
        .signals(app_window_signals);

/// Synthesise the menu-bar row by resolving `shell.menu-bar-row`
/// through the registry on `ctx`. Single source of truth: AppWindow
/// does not import or instantiate `MenuBarRow` directly; the dispatch
/// goes through the same registry that `RegistryTagResolver` uses, so
/// a host-supplied override transparently takes effect. When no
/// registry is attached (headless tests, isolated lowering) the
/// section is rendered as a placeholder bare container — structural
/// shape is preserved.
fn synth_menu_bar(ctx: &LowerCtx<'_>, node: &Node) -> UiNode {
    let props = json_object_with_keys(
        node,
        &["menus", "tabs", "show-tabs", "app-name", "active-menu"],
    );
    ctx.lower_as("shell.menu-bar-row", format!("{}::menu", node.id), props)
        .unwrap_or_else(|| placeholder(format!("{}::menu", node.id), "menu-bar"))
}

/// Activity bar = vertical column of `shell.nav-button` instances,
/// each resolved through the same registry seam. The `nav-buttons`
/// JSON array is the *declarative* source of truth — adding a
/// button is one entry in the prop, with zero changes to the
/// lowering body.
fn synth_activity_bar(ctx: &LowerCtx<'_>, node: &Node) -> UiNode {
    let buttons: Vec<UiNode> = node
        .props
        .get("nav-buttons")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .enumerate()
                .filter_map(|(idx, item)| {
                    ctx.lower_as(
                        "shell.nav-button",
                        format!("{}::nav::{}", node.id, idx),
                        item.clone(),
                    )
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

/// Headless / no-registry placeholder. The structural-shape tests
/// (column with three sections, body row with activity-bar + content)
/// exercise this path; production paths always have a registry and
/// dispatch through `lower_as`.
fn placeholder(id: String, role: &str) -> UiNode {
    bare_container(id, vec![], |p| {
        p.semantic = Semantic::tag("div").with_attr("data-role", role);
    })
}

/// Status bar dispatches through the same registry seam as the menu
/// bar and activity bar (§15). AppWindow forwards its `status` prop
/// as `text` on the synthesised `shell.status-bar` node — no other
/// fields, since the §16 v1 leaf renders a single label. Headless /
/// no-registry tests fall through to a placeholder; production paths
/// always have the registry attached.
fn synth_status_bar(ctx: &LowerCtx<'_>, node: &Node) -> UiNode {
    let status = node
        .props
        .get("status")
        .cloned()
        .unwrap_or_else(|| Value::String(String::new()));
    let props = json!({ "text": status });
    ctx.lower_as("shell.status-bar", format!("{}::status", node.id), props)
        .unwrap_or_else(|| placeholder(format!("{}::status", node.id), "status-bar"))
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
        lower_with(props, children, None)
    }

    /// Lower with the full shell registry attached so embedded chrome
    /// (menu bar, nav buttons) dispatches through the same registry
    /// the resolver path uses. Used by tests that assert on the
    /// rendered content of those sections.
    fn lower_with_full_registry(props: Value, children: Vec<BuilderNode>) -> UiNode {
        use crate::components::registry::{register_shell_builtins, ShellComponentRegistry};
        let mut reg = ShellComponentRegistry::new();
        register_shell_builtins(&mut reg).expect("register");
        // Borrow the inner ComponentRegistry through `as_component_registry`.
        // We have to keep the registry alive for the LowerCtx lifetime;
        // build it locally and pass a borrow.
        let owned = reg;
        let cr = owned.as_component_registry();
        lower_with(props, children, Some(cr))
    }

    fn lower_with(
        props: Value,
        children: Vec<BuilderNode>,
        registry: Option<&prism_builder::ComponentRegistry>,
    ) -> UiNode {
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
        let ctx = LowerCtx::new(registry, &cascade);
        app_window_lower(&ctx, &n, &cascade)
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
        // Embedded MenuBarRow lowering goes through the registry on
        // the LowerCtx, so this test attaches the full shell registry.
        let ui =
            lower_with_full_registry(json!({ "menus": [{ "id": "f", "label": "File" }] }), vec![]);
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
        // Status-bar lowering dispatches through the registry seam, so
        // this test attaches the full shell registry. Without one the
        // section falls through to the no-registry placeholder.
        let ui = lower_with_full_registry(json!({ "status": "Saved." }), vec![]);
        let UiNode::Container { children, .. } = ui else {
            panic!()
        };
        let UiNode::Container {
            children: status_kids,
            ..
        } = &children[2]
        else {
            panic!()
        };
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
        // Embedded NavButton lowering dispatches through the registry,
        // so this test attaches the full shell registry.
        let ui = lower_with_full_registry(
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
