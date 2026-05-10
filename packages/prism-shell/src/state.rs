//! `AppState` — slot-typed root of every datum a binding can read.
//!
//! Each top-level field is a *slot*: a typed sub-state owned by one
//! domain (chrome, workspace, selection, overlay, builder, project,
//! …). Slots own their own typed accessors and JSON emitters, so:
//!
//! * The bindings table in [`crate::props`] never reaches across
//!   slot boundaries — every closure asks the slot it cares about
//!   for one fully-shaped JSON value.
//! * `panel_props.rs`-style helpers move *onto* the slot they
//!   describe; there is no separate "shape this state into a row"
//!   layer that the closures have to plumb through.
//! * Adding a new datum is one struct field on the right slot and
//!   one method that returns the JSON shape its block consumes.
//!   Existing bindings keep compiling.
//!
//! Cross-slot composition (e.g. `shell.app-window` mixes chrome data
//! with workspace tabs) takes the *secondary slot as a `&` argument*
//! to the *primary slot's* method — `ChromeSlot::app_window_props(&self,
//! ws: &WorkspaceSlot)`. The bindings table still reads as one row,
//! the JSON shape still lives on exactly one method, and the
//! data-dependency graph stays visible at the call site.
//!
//! See `docs/dev/clay-migration-plan.md` §19.

use prism_dock::DockWorkspace;
use serde_json::{json, Value};

/// Reloadable root state. `Default` returns the zero-data shell that
/// the §17 contract boots into; ports re-introduce real data slot by
/// slot.
#[derive(Default, Clone)]
pub struct AppState {
    pub chrome: ChromeSlot,
    pub workspace: WorkspaceSlot,
    pub overlay: OverlaySlot,
    pub builder: BuilderSlot,
    pub navigation: NavigationSlot,
    pub catalog: CatalogSlot,
    pub docs: DocsSlot,
    pub menus: MenuSlot,
}

// ── chrome ────────────────────────────────────────────────────────

/// Static chrome strings — app name in the menu row, status string
/// in the bottom bar. The four nav-buttons (Home/etc.) live here too;
/// they're pure chrome ornament with no data dependencies.
///
/// Three bindings read from this slot:
/// `shell.status-bar` (status only), `shell.menu-bar-row` (chrome +
/// workspace tabs), `shell.app-window` (chrome + workspace tabs +
/// nav-buttons).
#[derive(Clone, Debug)]
pub struct ChromeSlot {
    pub app_name: String,
    pub status: String,
    pub nav_buttons: Vec<NavButton>,
    pub menus: Vec<MenuLabel>,
}

/// Top-bar menu pill. Rendered in `shell.menu-bar-row` and reused by
/// `shell.app-window` for embedded chrome.
#[derive(Clone, Debug)]
pub struct MenuLabel {
    pub label: String,
}

/// Activity-bar button. The runtime block reads `icon`/`selected`;
/// the underlying `panel_id` (where the click would route) is not
/// emitted yet — wired up when the navigation slot lands.
#[derive(Clone, Debug)]
pub struct NavButton {
    pub icon: String,
    pub selected: bool,
}

impl Default for ChromeSlot {
    fn default() -> Self {
        Self {
            app_name: "Prism".into(),
            status: "Ready".into(),
            nav_buttons: vec![NavButton {
                icon: "icons/home.svg".into(),
                selected: true,
            }],
            menus: ["File", "Edit", "View", "Help"]
                .into_iter()
                .map(|l| MenuLabel { label: l.into() })
                .collect(),
        }
    }
}

impl ChromeSlot {
    /// JSON for `shell.app-window`. Composes chrome data with the
    /// workspace's tab list — the secondary-arg pattern from §19.
    /// The skeleton's structural attrs (`id`, `panel-id`) win over
    /// emissions, so this method emits only data attrs.
    pub fn app_window_props(&self, workspace: &WorkspaceSlot) -> Value {
        json!({
            "app-name": self.app_name,
            "status": self.status,
            "menus": self.menus_json(),
            "tabs": workspace.tabs_json(),
            "nav-buttons": self.nav_buttons_json(),
        })
    }

    /// JSON for `shell.menu-bar-row` — top chrome with menu pills,
    /// app name, and the workflow page tabs. Same secondary-arg
    /// composition as `app_window_props` (chrome owns the row, tabs
    /// flow in by reference).
    pub fn menu_bar_row_props(&self, workspace: &WorkspaceSlot) -> Value {
        json!({
            "app-name": self.app_name,
            "menus": self.menus_json(),
            "tabs": workspace.tabs_json(),
        })
    }

    /// JSON for `shell.status-bar`. Same shape contract: chrome data
    /// only, no structural keys.
    pub fn status_bar_props(&self) -> Value {
        json!({ "status": self.status })
    }

    fn menus_json(&self) -> Value {
        Value::Array(
            self.menus
                .iter()
                .map(|m| json!({ "label": m.label }))
                .collect(),
        )
    }

    fn nav_buttons_json(&self) -> Value {
        Value::Array(
            self.nav_buttons
                .iter()
                .map(|b| json!({ "icon": b.icon, "selected": b.selected }))
                .collect(),
        )
    }
}

// ── workspace ─────────────────────────────────────────────────────

/// Workflow-page state — the seven DaVinci-style pages and the
/// active index. Wraps `prism_dock::DockWorkspace` directly so dock
/// ports (panels, dividers, tab bars) all read from the same source.
///
/// Two bindings consume this slot today:
/// `shell.workflow-page-bar` (the bottom mode bar) and
/// `shell.menu-bar-row` (top tabs). Both go through `tabs_json` /
/// `workflow_page_bar_props` — never inline JSON construction.
#[derive(Clone, Debug)]
pub struct WorkspaceSlot {
    pub workspace: DockWorkspace,
}

impl Default for WorkspaceSlot {
    fn default() -> Self {
        Self {
            workspace: DockWorkspace::with_builtins(),
        }
    }
}

impl WorkspaceSlot {
    /// JSON for `shell.workflow-page-bar`: one row per page with the
    /// active flag pre-resolved.
    pub fn workflow_page_bar_props(&self) -> Value {
        json!({ "pages": self.pages_json() })
    }

    /// Tab list shape consumed by both `shell.menu-bar-row` and
    /// `shell.app-window` (top-bar tabs). Crate-public so the chrome
    /// slot's composition methods can borrow it without duplicating
    /// the JSON shape.
    pub(crate) fn tabs_json(&self) -> Value {
        let active = self.workspace.active_index();
        Value::Array(
            self.workspace
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
                .collect(),
        )
    }

    fn pages_json(&self) -> Value {
        let active = self.workspace.active_index();
        Value::Array(
            self.workspace
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
                .collect(),
        )
    }
}

// ── overlay ───────────────────────────────────────────────────────

/// Floating chrome — toasts, the command palette, and the help
/// tooltip. None of these own a dock panel; they paint on top of
/// the app-window via the parsed skeleton's overlay siblings.
///
/// Three bindings consume this slot:
/// `shell.toast-stack`, `shell.command-palette`, `shell.help-tooltip`.
/// Each method emits the JSON shape its block already speaks (see
/// `components/{toast,command_palette,help_tooltip}.rs`).
#[derive(Clone, Debug, Default)]
pub struct OverlaySlot {
    pub toasts: Vec<Toast>,
    pub command_palette: CommandPalette,
    pub help_tooltip: Option<HelpTooltip>,
}

#[derive(Clone, Debug)]
pub struct Toast {
    pub title: String,
    pub body: String,
    pub kind: ToastKind,
}

#[derive(Clone, Copy, Debug, Default)]
pub enum ToastKind {
    #[default]
    Info,
    Success,
    Warning,
    Error,
}

impl ToastKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Success => "success",
            Self::Warning => "warning",
            Self::Error => "error",
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct CommandPalette {
    pub open: bool,
    pub query: String,
    pub results: Vec<CommandResult>,
    pub selected_index: usize,
}

#[derive(Clone, Debug)]
pub struct CommandResult {
    pub id: String,
    pub label: String,
    pub shortcut: Option<String>,
}

#[derive(Clone, Debug)]
pub struct HelpTooltip {
    pub title: String,
    pub summary: String,
}

impl OverlaySlot {
    /// JSON for `shell.toast-stack`. Empty list is valid — the block
    /// renders the empty container.
    pub fn toast_stack_props(&self) -> Value {
        json!({ "toasts": self.toasts_json() })
    }

    /// JSON for `shell.command-palette`. The block reads `query`,
    /// `results`, and `selected-index`; visibility is gated by `open`
    /// (skeleton-side `visible="…"` author attr binds against it).
    pub fn command_palette_props(&self) -> Value {
        json!({
            "open": self.command_palette.open,
            "query": self.command_palette.query,
            "results": self.results_json(),
            "selected-index": self.command_palette.selected_index,
        })
    }

    /// JSON for `shell.help-tooltip`. When no tooltip is showing, all
    /// fields are empty strings — the block paints nothing.
    pub fn help_tooltip_props(&self) -> Value {
        match &self.help_tooltip {
            Some(t) => json!({ "title": t.title, "summary": t.summary, "visible": true }),
            None => json!({ "title": "", "summary": "", "visible": false }),
        }
    }

    fn toasts_json(&self) -> Value {
        Value::Array(
            self.toasts
                .iter()
                .map(|t| {
                    json!({
                        "title": t.title,
                        "body": t.body,
                        "kind": t.kind.as_str(),
                    })
                })
                .collect(),
        )
    }

    fn results_json(&self) -> Value {
        Value::Array(
            self.command_palette
                .results
                .iter()
                .map(|r| {
                    let mut o = json!({ "id": r.id, "label": r.label });
                    if let Some(sc) = &r.shortcut {
                        o["shortcut"] = json!(sc);
                    }
                    o
                })
                .collect(),
        )
    }
}

// ── builder ───────────────────────────────────────────────────────

/// Inspector / properties / signals / schema — the four panels that
/// describe the *currently selected* document node. Each panel is a
/// distinct binding, but every shape ultimately reads from the same
/// `selection` cursor + the document tree, so they all live on one
/// slot. Cross-binding consistency (selecting a node updates all four
/// panels) is enforced by the slot owning the resolution code path
/// once.
///
/// Four bindings consume this slot:
/// `shell.inspector-tree`, `shell.properties-panel`,
/// `shell.signals-panel`, `shell.schema-designer`. Per-row blocks
/// (`shell.signal-connection-row`, `shell.schema-row`,
/// `shell.inspector-row`, `shell.field-editor`) stay stubs — their
/// data flows down inside the parent's `rows` / `connections` /
/// `fields` JSON arrays, never through their own binding row.
#[derive(Clone, Debug, Default)]
pub struct BuilderSlot {
    pub inspector: Vec<InspectorNode>,
    pub property_rows: Vec<PropertyRow>,
    pub signal_connections: Vec<SignalConnection>,
    pub schema: SchemaDoc,
}

#[derive(Clone, Debug)]
pub struct InspectorNode {
    pub id: String,
    pub label: String,
    pub depth: u32,
    pub selected: bool,
}

/// One row in the properties panel. The block already speaks a
/// generic `{component, props}` shape (see `properties_panel.rs`'s
/// example), so the slot stores it as typed pairs and the JSON
/// emitter folds them.
#[derive(Clone, Debug)]
pub struct PropertyRow {
    pub component: String,
    pub props: Value,
}

#[derive(Clone, Debug)]
pub struct SignalConnection {
    pub source_signal: String,
    pub action_kind: String,
    pub target_label: String,
    pub selected: bool,
}

#[derive(Clone, Debug, Default)]
pub struct SchemaDoc {
    pub title: String,
    pub schema_name: String,
    pub fields: Vec<SchemaField>,
}

#[derive(Clone, Debug)]
pub struct SchemaField {
    pub name: String,
    pub kind: String,
    pub required: bool,
}

impl BuilderSlot {
    pub fn inspector_tree_props(&self) -> Value {
        json!({ "nodes": self.inspector_json() })
    }

    pub fn properties_panel_props(&self) -> Value {
        json!({ "rows": self.property_rows_json() })
    }

    pub fn signals_panel_props(&self) -> Value {
        json!({
            "title": "Signals",
            "connections": self.connections_json(),
        })
    }

    pub fn schema_designer_props(&self) -> Value {
        json!({
            "title": self.schema.title,
            "schema-name": self.schema.schema_name,
            "fields": self.schema_fields_json(),
        })
    }

    fn inspector_json(&self) -> Value {
        Value::Array(
            self.inspector
                .iter()
                .map(|n| {
                    json!({
                        "node-id": n.id,
                        "label": n.label,
                        "depth": n.depth,
                        "selected": n.selected,
                    })
                })
                .collect(),
        )
    }

    fn property_rows_json(&self) -> Value {
        Value::Array(
            self.property_rows
                .iter()
                .map(|r| json!({ "component": r.component, "props": r.props }))
                .collect(),
        )
    }

    fn connections_json(&self) -> Value {
        Value::Array(
            self.signal_connections
                .iter()
                .map(|c| {
                    json!({
                        "source-signal": c.source_signal,
                        "action-kind": c.action_kind,
                        "target-label": c.target_label,
                        "selected": c.selected,
                    })
                })
                .collect(),
        )
    }

    fn schema_fields_json(&self) -> Value {
        Value::Array(
            self.schema
                .fields
                .iter()
                .map(|f| {
                    json!({
                        "field-name": f.name,
                        "field-kind": f.kind,
                        "required": f.required,
                    })
                })
                .collect(),
        )
    }
}

// ── navigation ────────────────────────────────────────────────────

/// Page list + graph — the two lenses on the multi-page authoring
/// model. `pages_json` is the load-bearing helper consumed by both
/// bindings; the graph adds positions and cross-page edges on top.
///
/// Two bindings consume this slot:
/// `shell.nav-page-list`, `shell.nav-graph`. The per-row block
/// (`shell.nav-page-row`) stays a stub — page rows render inside the
/// parent list's `pages` array.
#[derive(Clone, Debug, Default)]
pub struct NavigationSlot {
    pub pages: Vec<NavPage>,
    pub edges: Vec<NavEdge>,
}

#[derive(Clone, Debug)]
pub struct NavPage {
    pub id: String,
    pub title: String,
    pub route: String,
    pub x: f32,
    pub y: f32,
    pub node_count: u32,
    pub link_count: u32,
    pub is_active: bool,
}

#[derive(Clone, Debug)]
pub struct NavEdge {
    pub from: usize,
    pub to: usize,
    pub kind: NavEdgeKind,
}

#[derive(Clone, Copy, Debug)]
pub enum NavEdgeKind {
    Href,
    Signal,
}

impl NavEdgeKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Href => "href",
            Self::Signal => "signal",
        }
    }
}

impl NavigationSlot {
    /// JSON for `shell.nav-page-list`. The block reads only the page
    /// list — graph positions and edges are dropped here.
    pub fn nav_page_list_props(&self) -> Value {
        json!({ "pages": self.pages_list_json() })
    }

    /// JSON for `shell.nav-graph`. Composes the same page list with
    /// graph positions + edges. The shared subset (page-title, route,
    /// is-active) flows through the same `iter().map()` shape — no
    /// duplicated emitter.
    pub fn nav_graph_props(&self) -> Value {
        json!({
            "title": "Pages",
            "pages": self.pages_graph_json(),
            "edges": self.edges_json(),
        })
    }

    fn pages_list_json(&self) -> Value {
        Value::Array(
            self.pages
                .iter()
                .map(|p| {
                    json!({
                        "page-id": p.id,
                        "page-title": p.title,
                        "route": p.route,
                        "node-count": p.node_count,
                        "link-count": p.link_count,
                        "is-active": p.is_active,
                    })
                })
                .collect(),
        )
    }

    fn pages_graph_json(&self) -> Value {
        Value::Array(
            self.pages
                .iter()
                .map(|p| {
                    json!({
                        "label": p.title,
                        "route": p.route,
                        "x": p.x,
                        "y": p.y,
                        "is-active": p.is_active,
                    })
                })
                .collect(),
        )
    }

    fn edges_json(&self) -> Value {
        Value::Array(
            self.edges
                .iter()
                .map(|e| {
                    json!({
                        "from": e.from,
                        "to": e.to,
                        "kind": e.kind.as_str(),
                    })
                })
                .collect(),
        )
    }
}

// ── catalog ───────────────────────────────────────────────────────

/// Browse-style panels — launchpad apps, explorer files, component
/// palette items. Three bindings, three disjoint shapes; no shared
/// item type because the underlying data is genuinely different
/// (app cards vs file rows vs draggable component descriptors). Each
/// emitter is a plain `iter().map()` fold (rule-of-three: zero
/// consumers in common, no helper).
///
/// Three bindings consume this slot:
/// `shell.launchpad`, `shell.explorer`, `shell.component-palette`.
/// The per-row block `shell.app-card` stays a stub — its data flows
/// down inside the launchpad's `apps` array.
#[derive(Clone, Debug, Default)]
pub struct CatalogSlot {
    pub launchpad_title: String,
    pub apps: Vec<AppCard>,
    pub files: Vec<FileNode>,
    pub palette: Vec<PaletteItem>,
    pub palette_selected: Option<String>,
}

#[derive(Clone, Debug)]
pub struct AppCard {
    pub id: String,
    pub label: String,
    pub icon: String,
    pub summary: String,
}

#[derive(Clone, Debug)]
pub struct FileNode {
    pub id: String,
    pub label: String,
    pub depth: u32,
    pub kind: FileKind,
}

#[derive(Clone, Copy, Debug)]
pub enum FileKind {
    Directory,
    File,
}

impl FileKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Directory => "directory",
            Self::File => "file",
        }
    }
}

#[derive(Clone, Debug)]
pub struct PaletteItem {
    pub id: String,
    pub label: String,
    pub icon: String,
    pub category: String,
}

impl CatalogSlot {
    /// JSON for `shell.launchpad`. The block reads `title` and an
    /// `apps` array of `AppCard`-shaped objects.
    pub fn launchpad_props(&self) -> Value {
        json!({
            "title": self.launchpad_title,
            "apps": self.apps_json(),
        })
    }

    /// JSON for `shell.explorer`. The block reads a `nodes` array of
    /// row props (label / depth / kind) — flat list, the tree shape
    /// is encoded in `depth`.
    pub fn explorer_props(&self) -> Value {
        json!({ "nodes": self.files_json() })
    }

    /// JSON for `shell.component-palette`. The block reads `items` and
    /// the optional `selected-id`. Selection flows here from the
    /// builder canvas's "place mode" (set when the user picks an item
    /// to place into a grid cell).
    pub fn component_palette_props(&self) -> Value {
        let mut props = json!({ "items": self.palette_json() });
        if let Some(id) = &self.palette_selected {
            props["selected-id"] = json!(id);
        }
        props
    }

    fn apps_json(&self) -> Value {
        Value::Array(
            self.apps
                .iter()
                .map(|a| {
                    json!({
                        "app-id": a.id,
                        "label": a.label,
                        "icon": a.icon,
                        "summary": a.summary,
                    })
                })
                .collect(),
        )
    }

    fn files_json(&self) -> Value {
        Value::Array(
            self.files
                .iter()
                .map(|f| {
                    json!({
                        "node-id": f.id,
                        "label": f.label,
                        "depth": f.depth,
                        "kind": f.kind.as_str(),
                    })
                })
                .collect(),
        )
    }

    fn palette_json(&self) -> Value {
        Value::Array(
            self.palette
                .iter()
                .map(|p| {
                    json!({
                        "item-id": p.id,
                        "label": p.label,
                        "icon": p.icon,
                        "category": p.category,
                    })
                })
                .collect(),
        )
    }
}

// ── docs ──────────────────────────────────────────────────────────

/// Documentation view + sidebar — both bindings render the *same*
/// `DocsTopic` (title / summary / body) and only differ in `mode`
/// (the sidebar embeds extra chrome). The shared shape is extracted
/// into `topic_props` honestly: rule-of-three threshold met at two
/// consumers with byte-identical JSON keys.
///
/// Two bindings consume this slot:
/// `shell.docs-view`, `shell.docs-sidebar`. The per-row block
/// `shell.docs-content` stays a stub — its props are routed through
/// the parent's container by the existing `lower_ui` (each docs
/// component already builds its own content child).
#[derive(Clone, Debug, Default)]
pub struct DocsSlot {
    pub topic: DocsTopic,
    pub sidebar_mode: String,
}

#[derive(Clone, Debug, Default)]
pub struct DocsTopic {
    pub title: String,
    pub summary: String,
    pub body: String,
}

impl DocsSlot {
    /// JSON for `shell.docs-view`. Full-page topic render — `mode`
    /// is fixed `"full"` because the view block hard-codes that
    /// dispatch in its `lower_ui`.
    pub fn docs_view_props(&self) -> Value {
        let mut props = self.topic_props();
        props["mode"] = json!("full");
        props
    }

    /// JSON for `shell.docs-sidebar`. Same topic shape with the
    /// sidebar-specific `mode` (e.g. `"summary"` / `"outline"`).
    /// Defaults to `"sidebar"` when empty.
    pub fn docs_sidebar_props(&self) -> Value {
        let mode = if self.sidebar_mode.is_empty() {
            "sidebar"
        } else {
            self.sidebar_mode.as_str()
        };
        let mut props = self.topic_props();
        props["mode"] = json!(mode);
        props
    }

    /// Shared shape — title / summary / body. Lives on the slot so
    /// neither binding closure invents its own keys (the §19
    /// no-duplication rule applied to a private helper).
    fn topic_props(&self) -> Value {
        json!({
            "title": self.topic.title,
            "summary": self.topic.summary,
            "body": self.topic.body,
        })
    }
}

// ── menus ─────────────────────────────────────────────────────────

/// Menu items for the open dropdown and the active context menu.
/// Both bindings consume the same `MenuItem` array shape, so the
/// JSON emitter is a single private helper (`items_json`) — exactly
/// the shared-shape extraction the rule-of-three justifies (two
/// consumers, identical keys, no plausible per-binding deviation).
///
/// Two bindings consume this slot:
/// `shell.menu-dropdown`, `shell.context-menu`. The per-row block
/// `shell.menu-item` stays a stub — items render inside the parent
/// menu's `items` array.
#[derive(Clone, Debug, Default)]
pub struct MenuSlot {
    pub dropdown: Vec<MenuItem>,
    pub context: Vec<MenuItem>,
}

#[derive(Clone, Debug)]
pub struct MenuItem {
    pub label: String,
    pub shortcut: Option<String>,
    pub command: Option<String>,
    pub separator: bool,
    pub enabled: bool,
}

impl MenuItem {
    pub fn separator() -> Self {
        Self {
            label: String::new(),
            shortcut: None,
            command: None,
            separator: true,
            enabled: false,
        }
    }
}

impl MenuSlot {
    pub fn menu_dropdown_props(&self) -> Value {
        json!({ "items": Self::items_json(&self.dropdown) })
    }

    pub fn context_menu_props(&self) -> Value {
        json!({ "items": Self::items_json(&self.context) })
    }

    /// Shared menu-item shape. Extracted on landing because both
    /// callers emit identical keys today *and* will continue to —
    /// the `MenuItem` struct is the sole vocabulary.
    fn items_json(items: &[MenuItem]) -> Value {
        Value::Array(
            items
                .iter()
                .map(|m| {
                    let mut o = json!({
                        "label": m.label,
                        "separator": m.separator,
                        "enabled": m.enabled,
                    });
                    if let Some(sc) = &m.shortcut {
                        o["shortcut"] = json!(sc);
                    }
                    if let Some(cmd) = &m.command {
                        o["command"] = json!(cmd);
                    }
                    o
                })
                .collect(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_chrome_emits_app_name_and_status() {
        let state = AppState::default();
        let props = state.chrome.app_window_props(&state.workspace);
        assert_eq!(props["app-name"], "Prism");
        assert_eq!(props["status"], "Ready");
        assert!(props["menus"].is_array());
        assert!(props["nav-buttons"].is_array());
    }

    #[test]
    fn status_bar_props_carries_status_only() {
        let mut state = AppState::default();
        state.chrome.status = "Saving…".into();
        let props = state.chrome.status_bar_props();
        assert_eq!(props["status"], "Saving…");
        assert!(props.get("app-name").is_none());
    }

    #[test]
    fn workflow_page_bar_marks_exactly_one_active() {
        let state = AppState::default();
        let props = state.workspace.workflow_page_bar_props();
        let pages = props["pages"].as_array().expect("pages array");
        assert_eq!(pages.len(), state.workspace.workspace.pages().len());
        let active_count = pages.iter().filter(|p| p["active"] == true).count();
        assert_eq!(active_count, 1);
        assert_eq!(pages[0]["active"], true);
    }

    #[test]
    fn switching_page_moves_active_flag() {
        let mut state = AppState::default();
        let target = state.workspace.workspace.pages()[2].id.clone();
        state.workspace.workspace.switch_page_by_id(&target);
        let props = state.workspace.workflow_page_bar_props();
        let pages = props["pages"].as_array().unwrap();
        assert_eq!(pages[2]["active"], true);
        assert_eq!(pages[0]["active"], false);
    }

    #[test]
    fn menu_bar_row_pulls_tabs_from_workspace() {
        let state = AppState::default();
        let props = state.chrome.menu_bar_row_props(&state.workspace);
        assert_eq!(props["app-name"], "Prism");
        let tabs = props["tabs"].as_array().expect("tabs array");
        assert_eq!(tabs.len(), state.workspace.workspace.pages().len());
        // First page is active in the default workspace.
        assert_eq!(tabs[0]["active"], true);
    }

    #[test]
    fn app_window_composes_chrome_with_workspace_tabs() {
        let mut state = AppState::default();
        let target = state.workspace.workspace.pages()[1].id.clone();
        state.workspace.workspace.switch_page_by_id(&target);
        let props = state.chrome.app_window_props(&state.workspace);
        let tabs = props["tabs"].as_array().unwrap();
        assert_eq!(tabs[1]["active"], true, "tabs reflect active page");
        assert_eq!(props["app-name"], "Prism", "chrome data still flows");
    }

    // ── overlay ───────────────────────────────────────────────────

    #[test]
    fn toast_stack_props_serialise_kind_as_string() {
        let mut overlay = OverlaySlot::default();
        overlay.toasts.push(Toast {
            title: "Saved".into(),
            body: "Project flushed".into(),
            kind: ToastKind::Success,
        });
        let props = overlay.toast_stack_props();
        let toasts = props["toasts"].as_array().unwrap();
        assert_eq!(toasts.len(), 1);
        assert_eq!(toasts[0]["kind"], "success");
        assert_eq!(toasts[0]["title"], "Saved");
    }

    #[test]
    fn command_palette_props_default_is_closed_with_empty_query() {
        let props = OverlaySlot::default().command_palette_props();
        assert_eq!(props["open"], false);
        assert_eq!(props["query"], "");
        assert_eq!(props["results"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn help_tooltip_props_collapses_to_invisible_when_none() {
        let props = OverlaySlot::default().help_tooltip_props();
        assert_eq!(props["visible"], false);
        assert_eq!(props["title"], "");
    }

    #[test]
    fn help_tooltip_props_emits_visible_when_present() {
        let overlay = OverlaySlot {
            help_tooltip: Some(HelpTooltip {
                title: "Save".into(),
                summary: "Persist project".into(),
            }),
            ..Default::default()
        };
        let props = overlay.help_tooltip_props();
        assert_eq!(props["visible"], true);
        assert_eq!(props["title"], "Save");
    }

    // ── builder ───────────────────────────────────────────────────

    #[test]
    fn properties_panel_props_round_trips_typed_rows() {
        let mut builder = BuilderSlot::default();
        builder.property_rows.push(PropertyRow {
            component: "shell.section-header".into(),
            props: json!({ "label": "Layout" }),
        });
        builder.property_rows.push(PropertyRow {
            component: "shell.field-editor".into(),
            props: json!({ "key": "x", "kind": "number", "value": 10 }),
        });
        let props = builder.properties_panel_props();
        let rows = props["rows"].as_array().unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0]["component"], "shell.section-header");
        assert_eq!(rows[1]["props"]["value"], 10);
    }

    #[test]
    fn signals_panel_props_emits_connection_array() {
        let mut builder = BuilderSlot::default();
        builder.signal_connections.push(SignalConnection {
            source_signal: "clicked".into(),
            action_kind: "SetProperty".into(),
            target_label: "x".into(),
            selected: false,
        });
        let props = builder.signals_panel_props();
        assert_eq!(props["title"], "Signals");
        let conns = props["connections"].as_array().unwrap();
        assert_eq!(conns[0]["source-signal"], "clicked");
        assert_eq!(conns[0]["action-kind"], "SetProperty");
    }

    #[test]
    fn schema_designer_props_carries_fields() {
        let builder = BuilderSlot {
            schema: SchemaDoc {
                title: "Posts".into(),
                schema_name: "post".into(),
                fields: vec![SchemaField {
                    name: "title".into(),
                    kind: "text".into(),
                    required: true,
                }],
            },
            ..Default::default()
        };
        let props = builder.schema_designer_props();
        assert_eq!(props["title"], "Posts");
        assert_eq!(props["schema-name"], "post");
        let fields = props["fields"].as_array().unwrap();
        assert_eq!(fields[0]["field-name"], "title");
        assert_eq!(fields[0]["required"], true);
    }

    #[test]
    fn inspector_tree_props_carries_typed_nodes() {
        let mut builder = BuilderSlot::default();
        builder.inspector.push(InspectorNode {
            id: "n1".into(),
            label: "Root".into(),
            depth: 0,
            selected: true,
        });
        let props = builder.inspector_tree_props();
        let nodes = props["nodes"].as_array().unwrap();
        assert_eq!(nodes[0]["selected"], true);
        assert_eq!(nodes[0]["depth"], 0);
    }

    // ── navigation ────────────────────────────────────────────────

    fn sample_nav() -> NavigationSlot {
        NavigationSlot {
            pages: vec![
                NavPage {
                    id: "home".into(),
                    title: "Home".into(),
                    route: "/".into(),
                    x: 0.0,
                    y: 0.0,
                    node_count: 4,
                    link_count: 1,
                    is_active: true,
                },
                NavPage {
                    id: "about".into(),
                    title: "About".into(),
                    route: "/about".into(),
                    x: 200.0,
                    y: 0.0,
                    node_count: 1,
                    link_count: 0,
                    is_active: false,
                },
            ],
            edges: vec![NavEdge {
                from: 0,
                to: 1,
                kind: NavEdgeKind::Href,
            }],
        }
    }

    #[test]
    fn nav_page_list_props_emits_list_only() {
        let nav = sample_nav();
        let props = nav.nav_page_list_props();
        let pages = props["pages"].as_array().unwrap();
        assert_eq!(pages.len(), 2);
        assert_eq!(pages[0]["page-title"], "Home");
        assert_eq!(pages[0]["node-count"], 4);
        assert!(props.get("edges").is_none(), "list does not leak edges");
    }

    #[test]
    fn nav_graph_props_emits_pages_with_positions_and_edges() {
        let nav = sample_nav();
        let props = nav.nav_graph_props();
        assert_eq!(props["title"], "Pages");
        let pages = props["pages"].as_array().unwrap();
        assert_eq!(pages[0]["label"], "Home");
        assert_eq!(pages[1]["x"], 200.0);
        let edges = props["edges"].as_array().unwrap();
        assert_eq!(edges[0]["kind"], "href");
        assert_eq!(edges[0]["from"], 0);
    }

    // ── catalog ───────────────────────────────────────────────────

    fn sample_catalog() -> CatalogSlot {
        CatalogSlot {
            launchpad_title: "Welcome".into(),
            apps: vec![AppCard {
                id: "lattice".into(),
                label: "Lattice".into(),
                icon: "icons/lattice.svg".into(),
                summary: "Visual web builder".into(),
            }],
            files: vec![
                FileNode {
                    id: "src".into(),
                    label: "src".into(),
                    depth: 0,
                    kind: FileKind::Directory,
                },
                FileNode {
                    id: "src/lib.rs".into(),
                    label: "lib.rs".into(),
                    depth: 1,
                    kind: FileKind::File,
                },
            ],
            palette: vec![PaletteItem {
                id: "heading".into(),
                label: "Heading".into(),
                icon: "icons/heading.svg".into(),
                category: "Text".into(),
            }],
            palette_selected: Some("heading".into()),
        }
    }

    #[test]
    fn launchpad_props_carry_title_and_apps() {
        let cat = sample_catalog();
        let props = cat.launchpad_props();
        assert_eq!(props["title"], "Welcome");
        let apps = props["apps"].as_array().unwrap();
        assert_eq!(apps[0]["app-id"], "lattice");
        assert_eq!(apps[0]["summary"], "Visual web builder");
    }

    #[test]
    fn explorer_props_emit_depth_and_kind() {
        let cat = sample_catalog();
        let props = cat.explorer_props();
        let nodes = props["nodes"].as_array().unwrap();
        assert_eq!(nodes[0]["kind"], "directory");
        assert_eq!(nodes[1]["kind"], "file");
        assert_eq!(nodes[1]["depth"], 1);
    }

    #[test]
    fn component_palette_props_omit_selected_when_none() {
        let mut cat = sample_catalog();
        cat.palette_selected = None;
        let props = cat.component_palette_props();
        assert!(props.get("selected-id").is_none());
    }

    #[test]
    fn component_palette_props_include_selected_when_some() {
        let cat = sample_catalog();
        let props = cat.component_palette_props();
        assert_eq!(props["selected-id"], "heading");
        assert_eq!(props["items"][0]["item-id"], "heading");
    }

    // ── docs ──────────────────────────────────────────────────────

    fn sample_docs() -> DocsSlot {
        DocsSlot {
            topic: DocsTopic {
                title: "Builder".into(),
                summary: "Edit visually.".into(),
                body: "Long form…".into(),
            },
            sidebar_mode: String::new(),
        }
    }

    #[test]
    fn docs_view_pins_mode_full() {
        let props = sample_docs().docs_view_props();
        assert_eq!(props["mode"], "full");
        assert_eq!(props["title"], "Builder");
    }

    #[test]
    fn docs_sidebar_defaults_to_sidebar_mode() {
        let props = sample_docs().docs_sidebar_props();
        assert_eq!(props["mode"], "sidebar");
    }

    #[test]
    fn docs_sidebar_respects_explicit_mode() {
        let mut docs = sample_docs();
        docs.sidebar_mode = "outline".into();
        let props = docs.docs_sidebar_props();
        assert_eq!(props["mode"], "outline");
    }

    #[test]
    fn docs_view_and_sidebar_share_topic_shape() {
        // Rule-of-three confirmation: both bindings emit byte-identical
        // title/summary/body keys, so the private helper is the sole
        // source of the shared shape. Drift would show up here first.
        let docs = sample_docs();
        let view = docs.docs_view_props();
        let sidebar = docs.docs_sidebar_props();
        assert_eq!(view["title"], sidebar["title"]);
        assert_eq!(view["summary"], sidebar["summary"]);
        assert_eq!(view["body"], sidebar["body"]);
    }

    // ── menus ─────────────────────────────────────────────────────

    fn sample_menus() -> MenuSlot {
        MenuSlot {
            dropdown: vec![
                MenuItem {
                    label: "Save".into(),
                    shortcut: Some("Ctrl+S".into()),
                    command: Some("file.save".into()),
                    separator: false,
                    enabled: true,
                },
                MenuItem::separator(),
                MenuItem {
                    label: "Quit".into(),
                    shortcut: None,
                    command: Some("app.quit".into()),
                    separator: false,
                    enabled: true,
                },
            ],
            context: vec![MenuItem {
                label: "Delete".into(),
                shortcut: Some("Del".into()),
                command: Some("edit.delete".into()),
                separator: false,
                enabled: true,
            }],
        }
    }

    #[test]
    fn menu_dropdown_emits_items_with_shortcut_and_command() {
        let props = sample_menus().menu_dropdown_props();
        let items = props["items"].as_array().unwrap();
        assert_eq!(items.len(), 3);
        assert_eq!(items[0]["label"], "Save");
        assert_eq!(items[0]["shortcut"], "Ctrl+S");
        assert_eq!(items[0]["command"], "file.save");
        assert_eq!(items[1]["separator"], true);
        // shortcut/command are omitted when None
        assert!(items[2].get("shortcut").is_none());
    }

    #[test]
    fn context_menu_uses_same_item_shape_as_dropdown() {
        // Rule-of-three confirmation: identical key set across both
        // emitters via the shared `items_json` helper. A drift would
        // require editing one site for both to keep parity, which is
        // exactly the duplication the helper prevents.
        let menus = sample_menus();
        let drop = menus.menu_dropdown_props();
        let ctx = menus.context_menu_props();
        let drop_keys: Vec<_> = drop["items"][0]
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect();
        let ctx_keys: Vec<_> = ctx["items"][0]
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect();
        assert_eq!(drop_keys, ctx_keys);
    }

    #[test]
    fn menu_separator_helper_marks_separator_true_and_disabled() {
        let sep = MenuItem::separator();
        assert!(sep.separator);
        assert!(!sep.enabled);
        assert!(sep.command.is_none());
    }

    #[test]
    fn nav_active_flag_propagates_through_both_emitters() {
        // §19 cross-binding parity: bumping `is_active` shows up in
        // both shapes — list and graph — without duplicate emitter
        // logic. The shared subset is the load-bearing check.
        let mut nav = sample_nav();
        nav.pages[0].is_active = false;
        nav.pages[1].is_active = true;
        let list = nav.nav_page_list_props();
        let graph = nav.nav_graph_props();
        assert_eq!(list["pages"][1]["is-active"], true);
        assert_eq!(graph["pages"][1]["is-active"], true);
    }
}
