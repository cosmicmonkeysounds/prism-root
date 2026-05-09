//! Host-side bridge: typed panel data → JSON props for shell blocks.
//!
//! The shell-component vocabulary in [`crate::components`] defines a `Block`
//! impl per primitive that reads a JSON-shaped prop bag. This module is the
//! seam that turns the live typed data on `AppState` / `panels::*` into the
//! prop bags those blocks consume — the close-out work flagged in
//! `docs/dev/clay-migration-plan.md` §16.
//!
//! Each function is a pure mapping from typed data → `serde_json::Value`. No
//! mutation, no I/O, no allocation tricks beyond the JSON build itself. The
//! consumer composes these into a `BuilderDocument` Node (or a `host_children`
//! slot) so the runtime resolver dispatches each entry through the registered
//! shell block.
//!
//! Wiring overview (left = JSON shape, right = block that consumes it):
//!
//! - `inspector_rows(doc, selected)`        → `shell.inspector-row` × N
//! - `signals_panel_props(doc)`              → `shell.signals-panel`
//! - `nav_page_list_props(app)`              → `shell.nav-page-list`
//! - `nav_graph_props(app)`                  → `shell.nav-graph`
//! - `schema_designer_props(doc, sel)`       → `shell.schema-designer`
//! - `properties_panel_props(sections)`      → `shell.properties-panel`
//! - `command_palette_props(reg, q, sel)`    → `shell.command-palette`
//! - `toast_stack_entries(toasts)`           → `shell.toast` × N (host_children)
//! - `dock_tab_bar_props(workspace)`         → `shell.dock-tab-bar`
//! - `workflow_page_bar_props(workspace)`    → `shell.workflow-page-bar`
//! - `menu_bar_row_props(workspace, name)`   → `shell.menu-bar-row`
//! - `app_window_props(workspace, name, st)` → `shell.app-window`

use prism_builder::{app::PrismApp, document::BuilderDocument, FacetSchema, NodeId};
use prism_dock::DockWorkspace;
use serde_json::{json, Map, Value};

use crate::app::ToastData;
use crate::command::CommandRegistry;
use crate::panels::navigation::NavigationPanel;
use crate::panels::properties::{FieldRowData, PropertySection};
use crate::panels::schema::SchemaDesignerPanel;
use crate::panels::signals::SignalsPanel as SignalsPanelData;

// ── Inspector tree ────────────────────────────────────────────────

/// One JSON entry per visible row, ready to dispatch through
/// `shell.inspector-row`. Walks the document tree depth-first.
pub fn inspector_rows(doc: &BuilderDocument, selected: Option<&str>) -> Vec<Value> {
    let mut out = Vec::new();
    if let Some(root) = &doc.root {
        walk_inspector(root, 0, selected, &mut out);
    }
    out
}

fn walk_inspector(
    node: &prism_builder::Node,
    depth: usize,
    selected: Option<&str>,
    out: &mut Vec<Value>,
) {
    let is_sel = selected == Some(node.id.as_str());
    out.push(json!({
        "node-id": node.id,
        "component-id": node.component,
        "kind": "node",
        "depth": depth as f64,
        "selected": is_sel,
        "show-delete": is_sel,
    }));
    for child in &node.children {
        walk_inspector(child, depth + 1, selected, out);
    }
}

// ── Signals panel ─────────────────────────────────────────────────

/// Props for `shell.signals-panel` — scoped to a selected node when given,
/// otherwise lists every connection in the document.
pub fn signals_panel_props(doc: &BuilderDocument, selected: Option<&str>) -> Value {
    let rows = match selected {
        Some(id) => SignalsPanelData::connections_for_node(doc, id),
        None => SignalsPanelData::connection_rows(doc),
    };
    let connections: Vec<Value> = rows
        .into_iter()
        .map(|r| {
            json!({
                "source-signal": r.signal,
                "action-kind": r.action_kind,
                "target-label": r.target_label,
                "selected": false,
                "show-delete": false,
            })
        })
        .collect();
    json!({
        "title": match selected {
            Some(_) => "Signals (selected node)",
            None => "Signals",
        },
        "connections": connections,
    })
}

// ── Navigation panel ──────────────────────────────────────────────

/// Per-row props for `shell.nav-page-row`, ready to dispatch as host_children
/// through `shell.nav-page-list`.
pub fn nav_page_row_entries(app: &PrismApp) -> Vec<Value> {
    NavigationPanel::page_rows(app)
        .into_iter()
        .map(|r| {
            json!({
                "page-title": r.title,
                "route": r.route,
                "is-active": r.is_active,
                "node-count": r.node_count as f64,
                "link-count": r.link_count as f64,
                "selected": false,
                "show-delete": !r.is_active,
            })
        })
        .collect()
}

/// Props for `shell.nav-graph` — positioned cards + computed edge endpoints.
pub fn nav_graph_props(app: &PrismApp) -> Value {
    let nodes = NavigationPanel::graph_nodes(app);
    let pages: Vec<Value> = nodes
        .iter()
        .map(|n| {
            json!({
                "x": n.x,
                "y": n.y,
                "label": n.title,
                "route": n.route,
                "is-active": n.is_active,
            })
        })
        .collect();

    let edges_raw = NavigationPanel::graph_edges(app);
    let edges: Vec<Value> = edges_raw
        .iter()
        .filter_map(|e| {
            let src = nodes.get(e.source_page_index)?;
            let tgt = nodes.get(e.target_page_index)?;
            let x1 = src.x + src.width / 2.0;
            let y1 = src.y + src.height / 2.0;
            let x2 = tgt.x + tgt.width / 2.0;
            let y2 = tgt.y + tgt.height / 2.0;
            Some(json!({
                "x1": x1, "y1": y1, "x2": x2, "y2": y2,
                "kind": e.kind,
            }))
        })
        .collect();

    json!({
        "title": "Navigation",
        "pages": pages,
        "edges": edges,
    })
}

// ── Schema designer ───────────────────────────────────────────────

/// Props for `shell.schema-designer` — the selected schema's fields, mapped
/// into the per-row prop shape `shell.schema-row` consumes.
pub fn schema_designer_props(doc: &BuilderDocument, selected_id: Option<&str>) -> Value {
    let schema: Option<&FacetSchema> = selected_id
        .and_then(|id| doc.facet_schemas.get(id))
        .or_else(|| doc.facet_schemas.values().next());

    let (name, fields): (String, Vec<Value>) = match schema {
        Some(s) => {
            let fields: Vec<Value> = s
                .fields
                .iter()
                .map(|f| {
                    json!({
                        "field-name": if f.label.is_empty() { f.key.clone() } else { f.label.clone() },
                        "field-kind": kind_label(&f.kind),
                        "required": f.required,
                        "selected": false,
                        "show-delete": false,
                    })
                })
                .collect();
            (s.label.clone(), fields)
        }
        None => (String::new(), Vec::new()),
    };

    json!({
        "title": "Schema",
        "schema-name": name,
        "fields": fields,
    })
}

fn kind_label(k: &prism_core::widget::FieldKind) -> String {
    use prism_core::widget::FieldKind;
    match k {
        FieldKind::Text | FieldKind::TextArea => "text",
        FieldKind::Number(_) | FieldKind::Currency { .. } => "number",
        FieldKind::Integer(_) | FieldKind::Duration => "integer",
        FieldKind::Boolean => "boolean",
        FieldKind::Date | FieldKind::DateTime => "date",
        FieldKind::Color => "color",
        FieldKind::File(_) => "file",
        FieldKind::Select(_) => "select",
        FieldKind::Calculation { .. } => "calculation",
        FieldKind::Custom { tag, .. } => return tag.clone(),
    }
    .into()
}

/// Per-schema entries for the schema-list strip (`shell.inspector-row` reuse).
pub fn schema_list_entries(doc: &BuilderDocument, selected_id: Option<&str>) -> Vec<Value> {
    SchemaDesignerPanel::schema_list_rows(doc)
        .into_iter()
        .map(|r| {
            let is_sel = selected_id == Some(r.id.as_str());
            json!({
                "node-id": r.id,
                "component-id": format!("{} fields", r.field_count),
                "kind": "row",
                "depth": 0.0,
                "selected": is_sel,
                "show-delete": false,
            })
        })
        .collect()
}

// ── Properties panel ──────────────────────────────────────────────

/// Build the `rows` JSON the `shell.properties-panel` block dispatches via
/// `lower_as`. Each section becomes a `shell.section-header` entry followed by
/// one `shell.field-editor` per field row.
pub fn properties_panel_props(sections: &[PropertySection]) -> Value {
    let mut rows: Vec<Value> = Vec::with_capacity(sections.iter().map(|s| 1 + s.rows.len()).sum());
    for s in sections {
        rows.push(json!({
            "component": "shell.section-header",
            "props": {
                "label": s.label,
                "collapsed": s.collapsed,
            },
        }));
        if !s.collapsed {
            for r in &s.rows {
                rows.push(json!({
                    "component": "shell.field-editor",
                    "props": field_row_props(r),
                }));
            }
        }
    }
    json!({ "rows": rows })
}

fn field_row_props(r: &FieldRowData) -> Value {
    let mut m = Map::new();
    m.insert("key".into(), r.key.clone().into());
    m.insert("label".into(), r.label.clone().into());
    m.insert("kind".into(), r.kind.clone().into());
    m.insert("value".into(), r.value.clone().into());
    m.insert("required".into(), r.required.into());
    if r.has_bounds {
        m.insert("min".into(), (r.min as f64).into());
        m.insert("max".into(), (r.max as f64).into());
    }
    if !r.options.is_empty() {
        m.insert(
            "options".into(),
            Value::Array(r.options.iter().map(|o| Value::String(o.clone())).collect()),
        );
    }
    Value::Object(m)
}

// ── Command palette ───────────────────────────────────────────────

/// Props for `shell.command-palette` — fuzzy-filtered against `query`.
pub fn command_palette_props(reg: &CommandRegistry, query: &str, selected_index: usize) -> Value {
    let matches = if query.is_empty() {
        reg.list().into_iter().collect()
    } else {
        reg.filter(query)
    };
    let results: Vec<Value> = matches
        .iter()
        .take(50)
        .map(|cmd| {
            let mut m = Map::new();
            m.insert("id".into(), Value::String(cmd.id.clone()));
            m.insert("label".into(), Value::String(cmd.label.clone()));
            if !cmd.category.is_empty() {
                m.insert("category".into(), Value::String(cmd.category.clone()));
            }
            if let Some(s) = &cmd.shortcut {
                m.insert("shortcut".into(), Value::String(s.clone()));
            }
            Value::Object(m)
        })
        .collect();
    json!({
        "query": query,
        "placeholder": "Search commands…",
        "results": results,
        "selected-index": selected_index as i64,
    })
}

// ── Toasts ────────────────────────────────────────────────────────

/// Per-toast props ready to dispatch as host_children of `shell.toast-stack`.
pub fn toast_stack_entries(toasts: &[ToastData]) -> Vec<Value> {
    toasts
        .iter()
        .map(|t| {
            json!({
                "title": t.title,
                "body": t.body,
                "kind": t.kind,
            })
        })
        .collect()
}

// ── Dock & workflow chrome ────────────────────────────────────────

/// Props for `shell.dock-tab-bar` derived from the active dock state. Each
/// tab carries `tab-id`/`label`/`active`.
pub fn dock_tab_bar_props(workspace: &DockWorkspace) -> Value {
    use prism_dock::PanelKind;
    let dock = workspace.active_dock();
    let panels = dock.panel_ids();
    let active_idx = 0usize; // visible-tab tracking lives in DockState; default to first.
    let tabs: Vec<Value> = panels
        .iter()
        .enumerate()
        .map(|(i, id)| {
            let label = PanelKind::from_id(id)
                .map(|k| k.meta().label.to_string())
                .unwrap_or_else(|| id.to_string());
            json!({
                "tab-id": id,
                "label": label,
                "active": i == active_idx,
            })
        })
        .collect();
    json!({ "tabs": tabs })
}

/// Props for `shell.workflow-page-bar`. Mirrors the legacy `WorkflowPageItem`
/// model: `pages: [{ id, label, icon-hint, active }]`.
pub fn workflow_page_bar_props(workspace: &DockWorkspace) -> Value {
    let active = workspace.active_index();
    let pages: Vec<Value> = workspace
        .pages()
        .iter()
        .enumerate()
        .map(|(i, p)| {
            json!({
                "page-id": p.id,
                "label": p.label,
                "icon-hint": p.icon_hint,
                "active": i == active,
            })
        })
        .collect();
    json!({ "pages": pages })
}

// ── Menu bar + app window ─────────────────────────────────────────

/// Props for `shell.menu-bar-row` — top chrome with menu pills, app name, and
/// the workflow page tabs. `menus` is the static File/Edit/View/Help bag.
pub fn menu_bar_row_props(workspace: &DockWorkspace, app_name: &str) -> Value {
    let menus = json!([
        { "label": "File" },
        { "label": "Edit" },
        { "label": "View" },
        { "label": "Help" },
    ]);
    let active = workspace.active_index();
    let tabs: Vec<Value> = workspace
        .pages()
        .iter()
        .enumerate()
        .map(|(i, p)| {
            json!({
                "tab-id": p.id,
                "label": p.label,
                "active": i == active,
            })
        })
        .collect();
    json!({
        "app-name": app_name,
        "menus": menus,
        "tabs": tabs,
    })
}

/// Props for `shell.app-window` — composes the menu-bar / activity-bar /
/// status-bar inputs into one bag. The runtime resolver dispatches the
/// embedded chrome through `lower_as` against this bag.
pub fn app_window_props(workspace: &DockWorkspace, app_name: &str, status: &str) -> Value {
    let menus = json!([
        { "label": "File" }, { "label": "Edit" }, { "label": "View" }, { "label": "Help" },
    ]);
    let active = workspace.active_index();
    let tabs: Vec<Value> = workspace
        .pages()
        .iter()
        .enumerate()
        .map(|(i, p)| json!({ "tab-id": p.id, "label": p.label, "active": i == active }))
        .collect();
    let nav_buttons = json!([
        { "icon": "icons/home.svg", "selected": true },
    ]);
    json!({
        "app-name": app_name,
        "status": status,
        "menus": menus,
        "tabs": tabs,
        "nav-buttons": nav_buttons,
    })
}

// ── Convenience: full snapshot for tests + supervisor wiring ──────

/// Selection helper for the inspector — returns the primary selection id.
pub fn primary_selection_id(sel: &crate::SelectionModel) -> Option<&str> {
    sel.primary().map(|n: &NodeId| n.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;
    use prism_builder::Node;
    use serde_json::json;

    fn doc_with_two_nodes() -> BuilderDocument {
        BuilderDocument {
            root: Some(Node {
                id: "root".into(),
                component: "container".into(),
                props: json!({}),
                children: vec![Node {
                    id: "a".into(),
                    component: "text".into(),
                    props: json!({}),
                    children: vec![],
                    ..Default::default()
                }],
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    #[test]
    fn inspector_rows_walks_tree_with_depth_and_selection() {
        let doc = doc_with_two_nodes();
        let rows = inspector_rows(&doc, Some("a"));
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0]["node-id"], "root");
        assert_eq!(rows[0]["depth"], 0.0);
        assert_eq!(rows[0]["selected"], false);
        assert_eq!(rows[1]["node-id"], "a");
        assert_eq!(rows[1]["depth"], 1.0);
        assert_eq!(rows[1]["selected"], true);
        assert_eq!(rows[1]["show-delete"], true);
    }

    #[test]
    fn inspector_rows_handles_empty_document() {
        assert!(inspector_rows(&BuilderDocument::default(), None).is_empty());
    }

    #[test]
    fn signals_panel_props_emits_connections_array() {
        let doc = BuilderDocument::default();
        let v = signals_panel_props(&doc, None);
        assert_eq!(v["title"], "Signals");
        assert!(v["connections"].is_array());
    }

    #[test]
    fn signals_panel_props_titles_per_selection() {
        let doc = BuilderDocument::default();
        let v = signals_panel_props(&doc, Some("a"));
        assert_eq!(v["title"], "Signals (selected node)");
    }

    #[test]
    fn workflow_page_bar_props_marks_active() {
        let ws = DockWorkspace::with_builtins();
        let v = workflow_page_bar_props(&ws);
        let pages = v["pages"].as_array().unwrap();
        assert!(!pages.is_empty());
        let active_count = pages.iter().filter(|p| p["active"] == true).count();
        assert_eq!(active_count, 1);
    }

    #[test]
    fn menu_bar_row_props_includes_menus_and_tabs() {
        let ws = DockWorkspace::with_builtins();
        let v = menu_bar_row_props(&ws, "Studio");
        assert_eq!(v["app-name"], "Studio");
        assert_eq!(v["menus"].as_array().unwrap().len(), 4);
        assert!(!v["tabs"].as_array().unwrap().is_empty());
    }

    #[test]
    fn app_window_props_carries_status_and_chrome_inputs() {
        let ws = DockWorkspace::with_builtins();
        let v = app_window_props(&ws, "Studio", "Ready");
        assert_eq!(v["status"], "Ready");
        assert_eq!(v["app-name"], "Studio");
        assert!(v["menus"].is_array());
        assert!(v["tabs"].is_array());
        assert!(v["nav-buttons"].is_array());
    }

    #[test]
    fn dock_tab_bar_props_emits_tab_per_panel() {
        let ws = DockWorkspace::with_builtins();
        let v = dock_tab_bar_props(&ws);
        assert!(v["tabs"].is_array());
        let tabs = v["tabs"].as_array().unwrap();
        for t in tabs {
            assert!(t["tab-id"].is_string());
            assert!(t["label"].is_string());
        }
        assert_eq!(tabs.iter().filter(|t| t["active"] == true).count(), 1);
    }

    #[test]
    fn command_palette_props_filters_by_query() {
        let reg = CommandRegistry::with_builtins();
        let v = command_palette_props(&reg, "", 0);
        assert!(!v["results"].as_array().unwrap().is_empty());
        assert_eq!(v["query"], "");
        assert_eq!(v["selected-index"], 0);
    }

    #[test]
    fn toast_stack_entries_passes_through_kind() {
        let toasts = vec![ToastData {
            id: 1,
            title: "Hi".into(),
            body: "yo".into(),
            kind: "info".into(),
            created_at: None,
        }];
        let v = toast_stack_entries(&toasts);
        assert_eq!(v.len(), 1);
        assert_eq!(v[0]["title"], "Hi");
        assert_eq!(v[0]["kind"], "info");
    }

    #[test]
    fn properties_panel_props_emits_section_header_then_field_rows() {
        let sections = vec![PropertySection {
            id: "appearance".into(),
            label: "Appearance".into(),
            icon: "".into(),
            collapsed: false,
            rows: vec![FieldRowData {
                key: "title".into(),
                label: "Title".into(),
                kind: "text".into(),
                value: "Hello".into(),
                required: false,
                min: 0.0,
                max: 0.0,
                has_bounds: false,
                options: vec![],
            }],
        }];
        let v = properties_panel_props(&sections);
        let rows = v["rows"].as_array().unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0]["component"], "shell.section-header");
        assert_eq!(rows[0]["props"]["label"], "Appearance");
        assert_eq!(rows[1]["component"], "shell.field-editor");
        assert_eq!(rows[1]["props"]["key"], "title");
        assert_eq!(rows[1]["props"]["value"], "Hello");
    }

    #[test]
    fn properties_panel_props_skips_collapsed_section_rows() {
        let sections = vec![PropertySection {
            id: "x".into(),
            label: "X".into(),
            icon: "".into(),
            collapsed: true,
            rows: vec![FieldRowData {
                key: "k".into(),
                label: "K".into(),
                kind: "text".into(),
                value: "v".into(),
                required: false,
                min: 0.0,
                max: 0.0,
                has_bounds: false,
                options: vec![],
            }],
        }];
        let v = properties_panel_props(&sections);
        let rows = v["rows"].as_array().unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["component"], "shell.section-header");
    }

    #[test]
    fn schema_designer_props_falls_back_to_first_schema_when_unselected() {
        let v = schema_designer_props(&BuilderDocument::default(), None);
        // No schemas → empty name, empty fields, but still a shaped object.
        assert_eq!(v["title"], "Schema");
        assert_eq!(v["fields"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn nav_page_row_entries_marks_active_page_with_show_delete_off() {
        let app = PrismApp {
            id: "a".into(),
            name: "Test".into(),
            description: String::new(),
            icon: prism_builder::app::AppIcon::default(),
            pages: vec![prism_builder::app::Page {
                id: "p1".into(),
                title: "Home".into(),
                route: "/".into(),
                source: String::new(),
                document: BuilderDocument::default(),
                style: Default::default(),
            }],
            active_page: 0,
            navigation: prism_builder::app::NavigationConfig::default(),
            style: Default::default(),
        };
        let rows = nav_page_row_entries(&app);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["is-active"], true);
        assert_eq!(rows[0]["show-delete"], false);
    }

    /// End-to-end: prove the bridge actually feeds the registered shell
    /// blocks. Builds a builder Node carrying `app_window_props` JSON, runs
    /// it through the resolver, and asserts the lowered tree matches the
    /// expected AppWindow structure. This is the "host-side wiring works"
    /// proof for the §16 close-out work.
    #[test]
    fn app_window_props_lower_through_registry_resolver() {
        use crate::components::registry::{register_shell_builtins, ShellComponentRegistry};
        use prism_builder::layout::LayoutMode;
        use prism_builder::style::StyleProperties;
        use prism_builder::ui_lower::LowerCtx;
        use prism_core::foundation::spatial::Transform2D;
        use prism_ui_runtime::layout::Node as UiNode;

        let ws = DockWorkspace::with_builtins();
        let props = app_window_props(&ws, "Studio", "Ready");

        let mut reg = ShellComponentRegistry::new();
        register_shell_builtins(&mut reg).expect("register");

        let block = reg.get("shell.app-window").expect("app-window registered");

        let n = Node {
            id: "root".into(),
            component: "shell.app-window".into(),
            props,
            children: vec![],
            layout_mode: LayoutMode::default(),
            transform: Transform2D::default(),
            modifiers: vec![],
            style: StyleProperties::default(),
        };
        let cascade = StyleProperties::default();
        let ctx = LowerCtx::new(Some(reg.as_component_registry()), &cascade);

        // Component::lower_ui dispatches to AppWindow::lower_ui.
        let ui = block.lower_ui(&ctx, &n, &cascade);
        let UiNode::Container { id, children, .. } = ui else {
            panic!("expected container");
        };
        assert_eq!(id, "root");
        // AppWindow lowers to menu / body-row / status — three sections.
        assert_eq!(children.len(), 3);
    }

    #[test]
    fn workflow_page_bar_props_lower_through_registry_resolver() {
        use crate::components::registry::{register_shell_builtins, ShellComponentRegistry};
        use prism_builder::layout::LayoutMode;
        use prism_builder::style::StyleProperties;
        use prism_builder::ui_lower::LowerCtx;
        use prism_core::foundation::spatial::Transform2D;
        use prism_ui_runtime::layout::Node as UiNode;

        let ws = DockWorkspace::with_builtins();
        let props = workflow_page_bar_props(&ws);

        let mut reg = ShellComponentRegistry::new();
        register_shell_builtins(&mut reg).expect("register");
        let block = reg
            .get("shell.workflow-page-bar")
            .expect("workflow-page-bar registered");

        let n = Node {
            id: "wf".into(),
            component: "shell.workflow-page-bar".into(),
            props,
            children: vec![],
            layout_mode: LayoutMode::default(),
            transform: Transform2D::default(),
            modifiers: vec![],
            style: StyleProperties::default(),
        };
        let cascade = StyleProperties::default();
        let ctx = LowerCtx::new(Some(reg.as_component_registry()), &cascade);

        let UiNode::Container { id, children, .. } = block.lower_ui(&ctx, &n, &cascade) else {
            panic!("expected container");
        };
        assert_eq!(id, "wf");
        // One UiNode child per workflow page from `with_builtins`.
        assert_eq!(children.len(), ws.pages().len());
    }

    #[test]
    fn nav_graph_props_pairs_pages_and_edges() {
        let app = PrismApp {
            id: "a".into(),
            name: "Test".into(),
            description: String::new(),
            icon: prism_builder::app::AppIcon::default(),
            pages: vec![
                prism_builder::app::Page {
                    id: "p1".into(),
                    title: "Home".into(),
                    route: "/".into(),
                    source: String::new(),
                    document: BuilderDocument::default(),
                    style: Default::default(),
                },
                prism_builder::app::Page {
                    id: "p2".into(),
                    title: "About".into(),
                    route: "/about".into(),
                    source: String::new(),
                    document: BuilderDocument::default(),
                    style: Default::default(),
                },
            ],
            active_page: 0,
            navigation: prism_builder::app::NavigationConfig::default(),
            style: Default::default(),
        };
        let v = nav_graph_props(&app);
        let pages = v["pages"].as_array().unwrap();
        assert_eq!(pages.len(), 2);
        assert!(pages[0]["x"].as_f64().unwrap() >= 0.0);
        // No href / NavigateTo connections => empty edges.
        assert_eq!(v["edges"].as_array().unwrap().len(), 0);
    }
}
