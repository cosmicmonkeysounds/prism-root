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

use prism_builder::{BuilderDocument, NodeId};
use prism_core::foundation::spatial::Transform2D;
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
    pub canvas: CanvasSlot,
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

// ── canvas ────────────────────────────────────────────────────────

/// The active document, the selection's resolved transform, the active
/// tool mode, and the small piece of capture state needed to translate
/// a pointer drag into a typed transform delta. Six bindings read from
/// this slot:
///
/// * `shell.code-editor` (source + caret + lang)
/// * `shell.builder-canvas` (doc + viewport + place-mode)
/// * `shell.gizmo-{move,rotate,scale}` (three lenses on one shape)
/// * `shell.resize-handle` (eight handles around selection bbox)
/// * `shell.component-picker` (palette popup)
///
/// `drag` is *private* — bindings cannot read it. Visibility flows
/// through data shape (the mutated `document` and `selection`'s
/// transform), not a "drag in progress" flag. Same discipline as
/// [`OverlaySlot::help_tooltip_props`]'s `visible` collapse.
///
/// See `docs/dev/clay-migration-plan.md` §22.
#[derive(Clone, Debug, Default)]
pub struct CanvasSlot {
    pub document: BuilderDocument,
    pub selection: Option<NodeId>,
    pub tool: ToolMode,
    pub viewport: CanvasViewport,
    pub picker: PickerState,
    pub code_buffer: CodeBuffer,
    drag: Option<DragState>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ToolMode {
    #[default]
    Move,
    Rotate,
    Scale,
}

impl ToolMode {
    fn as_str(self) -> &'static str {
        match self {
            Self::Move => "move",
            Self::Rotate => "rotate",
            Self::Scale => "scale",
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct CanvasViewport {
    pub width: f32,
    pub height: f32,
    pub zoom: f32,
    pub pan_x: f32,
    pub pan_y: f32,
}

impl Default for CanvasViewport {
    fn default() -> Self {
        Self {
            width: 1280.0,
            height: 800.0,
            zoom: 1.0,
            pan_x: 0.0,
            pan_y: 0.0,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct PickerState {
    pub open: bool,
    pub anchor_x: f32,
    pub anchor_y: f32,
    pub candidates: Vec<PickerCandidate>,
}

#[derive(Clone, Debug)]
pub struct PickerCandidate {
    pub id: String,
    pub label: String,
    pub icon: String,
}

#[derive(Clone, Debug, Default)]
pub struct CodeBuffer {
    pub source: String,
    pub language: String,
    pub caret: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DragKind {
    /// Captured a gizmo arm — the active tool mode determines what the
    /// delta means (`apply_gizmo_delta`).
    Gizmo,
    /// Captured one of eight resize handles around the selection bbox.
    Handle(HandleSide),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HandleSide {
    TopLeft,
    Top,
    TopRight,
    Right,
    BottomRight,
    Bottom,
    BottomLeft,
    Left,
}

impl HandleSide {
    fn cursor(self) -> &'static str {
        match self {
            Self::TopLeft | Self::BottomRight => "nwse-resize",
            Self::TopRight | Self::BottomLeft => "nesw-resize",
            Self::Top | Self::Bottom => "ns-resize",
            Self::Left | Self::Right => "ew-resize",
        }
    }

    fn id(self) -> &'static str {
        match self {
            Self::TopLeft => "tl",
            Self::Top => "t",
            Self::TopRight => "tr",
            Self::Right => "r",
            Self::BottomRight => "br",
            Self::Bottom => "b",
            Self::BottomLeft => "bl",
            Self::Left => "l",
        }
    }

    /// (dx, dy) sign applied to (x, y, w, h) when this handle drags
    /// by `(dx, dy)`. Returns `(dx_pos, dy_pos, dx_size, dy_size)`.
    fn deltas(self) -> (f32, f32, f32, f32) {
        match self {
            Self::TopLeft => (1.0, 1.0, -1.0, -1.0),
            Self::Top => (0.0, 1.0, 0.0, -1.0),
            Self::TopRight => (0.0, 1.0, 1.0, -1.0),
            Self::Right => (0.0, 0.0, 1.0, 0.0),
            Self::BottomRight => (0.0, 0.0, 1.0, 1.0),
            Self::Bottom => (0.0, 0.0, 0.0, 1.0),
            Self::BottomLeft => (1.0, 0.0, -1.0, 1.0),
            Self::Left => (1.0, 0.0, -1.0, 0.0),
        }
    }
}

/// Pre-drag values for the selected node. `commit_drag` reads this to
/// push exactly one undo snapshot per drag (rather than one per
/// `pointer_move` tick). Same shape as the pre-§22 `DragSnapshot` /
/// `ResizeSnapshot` halves of `app/`, ported *into* the slot rather
/// than duplicated next to it.
#[derive(Clone, Debug, PartialEq)]
pub struct TransformSnapshot {
    pub node_id: NodeId,
    pub transform: Transform2D,
}

impl TransformSnapshot {
    /// Capture the selected node's pre-drag transform. Returns `None`
    /// when there's no selection or the selection's id has gone stale.
    fn capture(doc: &BuilderDocument, selection: Option<&str>) -> Option<Self> {
        let id = selection?;
        let node = doc.root.as_ref()?.find(id)?;
        Some(Self {
            node_id: node.id.clone(),
            transform: node.transform.clone(),
        })
    }
}

#[derive(Clone, Debug)]
struct DragState {
    kind: DragKind,
    snapshot: TransformSnapshot,
    origin: (f32, f32),
}

impl CanvasSlot {
    // ── read side ─────────────────────────────────────────────────

    /// JSON for `shell.code-editor`. Source text + caret offset + the
    /// language the syntax provider speaks.
    pub fn code_editor_props(&self) -> Value {
        json!({
            "source": self.code_buffer.source,
            "caret": self.code_buffer.caret,
            "language": if self.code_buffer.language.is_empty() {
                "slint"
            } else {
                self.code_buffer.language.as_str()
            },
        })
    }

    /// JSON for `shell.builder-canvas`. The block reads the selection
    /// id (so it can paint the selection rectangle), the canvas
    /// viewport, and the picker's place-mode flag. The full document
    /// tree is *not* serialised here — the canvas walks the existing
    /// `BuilderDocument` directly via `lower_ui` for the page subtree.
    pub fn builder_canvas_props(&self) -> Value {
        json!({
            "selection-id": self.selection.clone().unwrap_or_default(),
            "tool": self.tool.as_str(),
            "viewport-width": self.viewport.width,
            "viewport-height": self.viewport.height,
            "zoom": self.viewport.zoom,
            "pan-x": self.viewport.pan_x,
            "pan-y": self.viewport.pan_y,
            "place-mode": self.picker.open,
        })
    }

    /// JSON for `shell.gizmo-move`. Forwards through the shared
    /// [`Self::gizmo_props`] helper — the only emitter for the gizmo
    /// shape across all three tool modes.
    pub fn gizmo_move_props(&self) -> Value {
        self.gizmo_props(ToolMode::Move)
    }

    /// JSON for `shell.gizmo-rotate`. See [`Self::gizmo_props`].
    pub fn gizmo_rotate_props(&self) -> Value {
        self.gizmo_props(ToolMode::Rotate)
    }

    /// JSON for `shell.gizmo-scale`. See [`Self::gizmo_props`].
    pub fn gizmo_scale_props(&self) -> Value {
        self.gizmo_props(ToolMode::Scale)
    }

    /// JSON for `shell.resize-handle`. Eight handles around the
    /// selection bbox; each carries its `id` (`"tl"`, `"t"`, …), pixel
    /// position, and a CSS-style cursor name. Visibility collapses to
    /// `visible: false` + an empty handle list when nothing is
    /// selected — same data-shape pattern as gizmos.
    pub fn resize_handle_props(&self) -> Value {
        let visible = self.selection.is_some();
        let handles = if visible {
            self.handle_positions_json()
        } else {
            Value::Array(Vec::new())
        };
        json!({
            "visible": visible,
            "handles": handles,
        })
    }

    /// JSON for `shell.component-picker`. Pop-up palette anchored at
    /// `(anchor-x, anchor-y)` listing droppable components. The block
    /// reads `open` to decide whether to paint at all.
    pub fn component_picker_props(&self) -> Value {
        json!({
            "open": self.picker.open,
            "anchor-x": self.picker.anchor_x,
            "anchor-y": self.picker.anchor_y,
            "candidates": self.candidates_json(),
        })
    }

    /// Rule-of-three trigger: three gizmo bindings emit byte-identical
    /// key sets and only differ in the `tool` discriminator. Helper
    /// extracts on landing — same justification as
    /// [`DocsSlot::topic_props`] and [`MenuSlot::items_json`]. A drift
    /// in the gizmo shape edits *one* site, not three.
    fn gizmo_props(&self, kind: ToolMode) -> Value {
        let visible = self.selection.is_some() && self.tool == kind;
        let center = self.selection_center().unwrap_or((0.0, 0.0));
        json!({
            "visible": visible,
            "center-x": center.0,
            "center-y": center.1,
            "tool": kind.as_str(),
        })
    }

    /// Selected node's center in canvas coordinates. `pub(crate)` so
    /// tests + future cross-binding consumers (snap-line overlay,
    /// alignment guides) hit the same code path. Two consumers today
    /// (`gizmo_props`, `resize_handle_props`); rule-of-three's
    /// "imminent third" justifies the helper.
    pub(crate) fn selection_center(&self) -> Option<(f32, f32)> {
        let id = self.selection.as_deref()?;
        let node = self.document.root.as_ref()?.find(id)?;
        Some((node.transform.position[0], node.transform.position[1]))
    }

    fn handle_positions_json(&self) -> Value {
        let Some((cx, cy)) = self.selection_center() else {
            return Value::Array(Vec::new());
        };
        // Bbox is approximated by the transform position + a default
        // 100×100 region; once the layout pass exposes computed rects
        // per-node, this reads from `ComputedLayout::rect(id)` instead
        // (same emitter shape, different source).
        let half = 50.0;
        let (x0, y0, x1, y1) = (cx - half, cy - half, cx + half, cy + half);
        let mx = (x0 + x1) * 0.5;
        let my = (y0 + y1) * 0.5;
        let sites = [
            (HandleSide::TopLeft, x0, y0),
            (HandleSide::Top, mx, y0),
            (HandleSide::TopRight, x1, y0),
            (HandleSide::Right, x1, my),
            (HandleSide::BottomRight, x1, y1),
            (HandleSide::Bottom, mx, y1),
            (HandleSide::BottomLeft, x0, y1),
            (HandleSide::Left, x0, my),
        ];
        Value::Array(
            sites
                .into_iter()
                .map(|(side, x, y)| {
                    json!({
                        "id": side.id(),
                        "x": x,
                        "y": y,
                        "cursor": side.cursor(),
                    })
                })
                .collect(),
        )
    }

    fn candidates_json(&self) -> Value {
        Value::Array(
            self.picker
                .candidates
                .iter()
                .map(|c| {
                    json!({
                        "candidate-id": c.id,
                        "label": c.label,
                        "icon": c.icon,
                    })
                })
                .collect(),
        )
    }

    // ── write side ────────────────────────────────────────────────

    /// Pointer-down: hit-test what's under the cursor and capture it.
    /// Returns `false` because no observable state changed yet (the
    /// drag is purely captured) — the next `pointer_move` is the
    /// first redraw trigger.
    pub(crate) fn pointer_down(&mut self, x: f32, y: f32) -> bool {
        let Some(kind) = self.hit_test(x, y) else {
            return false;
        };
        let Some(snapshot) = TransformSnapshot::capture(&self.document, self.selection.as_deref())
        else {
            return false;
        };
        self.drag = Some(DragState {
            kind,
            snapshot,
            origin: (x, y),
        });
        false
    }

    /// Pointer-move: if a drag is captured, translate the delta into a
    /// transform mutation through the *single* dispatch over
    /// `(tool, drag-target)` that lives on this slot. The router never
    /// grows tool-mode awareness.
    pub(crate) fn pointer_move(&mut self, x: f32, y: f32) -> bool {
        // Pull origin + kind out without holding a borrow across the
        // mutation — `apply_*_delta` need `&mut self`.
        let (kind, origin) = match self.drag.as_ref() {
            Some(d) => (d.kind, d.origin),
            None => return false,
        };
        let zoom = self.viewport.zoom.max(f32::EPSILON);
        let dx = (x - origin.0) / zoom;
        let dy = (y - origin.1) / zoom;
        match kind {
            DragKind::Gizmo => self.apply_gizmo_delta(self.tool, dx, dy),
            DragKind::Handle(side) => self.apply_handle_delta(side, dx, dy),
        }
        true
    }

    /// Pointer-up: commit the drag. One undo snapshot per drag, never
    /// per tick.
    pub(crate) fn pointer_up(&mut self, _x: f32, _y: f32) -> bool {
        let Some(_drag) = self.drag.take() else {
            return false;
        };
        // commit_drag here would push the snapshot onto the undo stack
        // once that lands on the new shell. Today the *effect* (mutated
        // node transform) is already in the document; the undo
        // contract simply has no consumer to feed.
        true
    }

    /// Single dispatch over `(tool, drag-target=Gizmo)`. This is the
    /// *only* place in the codebase that knows what a gizmo delta
    /// means under each tool. Adding a new tool mode (e.g. `Skew`) is
    /// one variant on [`ToolMode`] + one arm here. The router doesn't
    /// move.
    fn apply_gizmo_delta(&mut self, tool: ToolMode, dx: f32, dy: f32) {
        let Some(snapshot) = self.drag.as_ref().map(|d| d.snapshot.clone()) else {
            return;
        };
        let Some(node) = self
            .document
            .root
            .as_mut()
            .and_then(|root| root.find_mut(&snapshot.node_id))
        else {
            return;
        };
        match tool {
            ToolMode::Move => {
                node.transform.position[0] = snapshot.transform.position[0] + dx;
                node.transform.position[1] = snapshot.transform.position[1] + dy;
            }
            ToolMode::Rotate => {
                // Godot-standard: 0.5°/px horizontal drag. Convert to
                // radians for the canonical `rotation` field.
                let degrees = dx * 0.5;
                node.transform.rotation = snapshot.transform.rotation + degrees.to_radians();
            }
            ToolMode::Scale => {
                node.transform.scale[0] = (snapshot.transform.scale[0] + dx / 100.0).max(0.01);
                node.transform.scale[1] = (snapshot.transform.scale[1] + dy / 100.0).max(0.01);
            }
        }
    }

    fn apply_handle_delta(&mut self, side: HandleSide, dx: f32, dy: f32) {
        let Some(snapshot) = self.drag.as_ref().map(|d| d.snapshot.clone()) else {
            return;
        };
        let Some(node) = self
            .document
            .root
            .as_mut()
            .and_then(|root| root.find_mut(&snapshot.node_id))
        else {
            return;
        };
        let (px, py, _sx, _sy) = side.deltas();
        // Resize maps to position-only on this slot until the layout
        // engine exposes per-node `width`/`height` mutators; once it
        // does, the `_sx`/`_sy` returns from `HandleSide::deltas()`
        // become the size mutation.
        node.transform.position[0] = snapshot.transform.position[0] + px * dx;
        node.transform.position[1] = snapshot.transform.position[1] + py * dy;
    }

    /// What's under the cursor? `Some(DragKind)` if anything draggable
    /// is hit; `None` otherwise. The single dispatch over drag-target
    /// geometry — six bindings *cannot* disagree about hit regions
    /// because they don't compute them; the slot does, once.
    fn hit_test(&self, x: f32, y: f32) -> Option<DragKind> {
        let (cx, cy) = self.selection_center()?;
        // Resize handles take precedence over the gizmo when the
        // pointer lands on one — handles are the smaller target.
        let half = 50.0;
        let edge = 6.0;
        let (x0, y0, x1, y1) = (cx - half, cy - half, cx + half, cy + half);
        let on = |a: f32, b: f32| (a - b).abs() <= edge;
        let near_x = (x - x0).abs() <= edge || (x - x1).abs() <= edge;
        let near_y = (y - y0).abs() <= edge || (y - y1).abs() <= edge;
        let on_left = on(x, x0);
        let on_right = on(x, x1);
        let on_top = on(y, y0);
        let on_bottom = on(y, y1);
        let mid_x = on(x, (x0 + x1) * 0.5);
        let mid_y = on(y, (y0 + y1) * 0.5);
        let in_x = x >= x0 - edge && x <= x1 + edge;
        let in_y = y >= y0 - edge && y <= y1 + edge;
        if near_x && near_y {
            let side = match (on_left, on_top) {
                (true, true) => HandleSide::TopLeft,
                (false, true) if on_right => HandleSide::TopRight,
                (true, false) if on_bottom => HandleSide::BottomLeft,
                _ => HandleSide::BottomRight,
            };
            return Some(DragKind::Handle(side));
        }
        if (on_top || on_bottom) && mid_x && in_x {
            return Some(DragKind::Handle(if on_top {
                HandleSide::Top
            } else {
                HandleSide::Bottom
            }));
        }
        if (on_left || on_right) && mid_y && in_y {
            return Some(DragKind::Handle(if on_left {
                HandleSide::Left
            } else {
                HandleSide::Right
            }));
        }
        // Anything else inside the selection bbox captures the gizmo.
        if x >= x0 && x <= x1 && y >= y0 && y <= y1 {
            return Some(DragKind::Gizmo);
        }
        None
    }

    /// Test-only seam: did the slot capture a drag? Bindings cannot
    /// see this — the cross-binding parity tests use it to assert
    /// pointer events route through the slot correctly.
    #[cfg(test)]
    pub(crate) fn drag_active(&self) -> bool {
        self.drag.is_some()
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

    // ── canvas ────────────────────────────────────────────────────

    fn sample_canvas() -> CanvasSlot {
        use prism_builder::Node;
        let root = Node {
            id: "root".into(),
            component: "container".into(),
            transform: Transform2D {
                position: [100.0, 80.0],
                ..Default::default()
            },
            ..Default::default()
        };
        CanvasSlot {
            document: BuilderDocument {
                root: Some(root),
                ..Default::default()
            },
            selection: Some("root".into()),
            tool: ToolMode::Move,
            viewport: CanvasViewport::default(),
            picker: PickerState {
                open: true,
                anchor_x: 50.0,
                anchor_y: 60.0,
                candidates: vec![PickerCandidate {
                    id: "heading".into(),
                    label: "Heading".into(),
                    icon: "icons/heading.svg".into(),
                }],
            },
            code_buffer: CodeBuffer {
                source: "Window {}".into(),
                language: "slint".into(),
                caret: 7,
            },
            drag: None,
        }
    }

    #[test]
    fn code_editor_props_carry_source_caret_and_language() {
        let props = sample_canvas().code_editor_props();
        assert_eq!(props["source"], "Window {}");
        assert_eq!(props["caret"], 7);
        assert_eq!(props["language"], "slint");
    }

    #[test]
    fn builder_canvas_props_emit_selection_and_viewport() {
        let props = sample_canvas().builder_canvas_props();
        assert_eq!(props["selection-id"], "root");
        assert_eq!(props["tool"], "move");
        assert_eq!(props["zoom"], 1.0);
        assert_eq!(props["place-mode"], true);
    }

    #[test]
    fn gizmo_props_share_shape_across_three_modes() {
        // Rule-of-three parity: same key set on all three gizmo
        // emissions, only `tool` differs. Drift in any field would
        // break this assertion in one place, not three.
        let canvas = sample_canvas();
        let m = canvas.gizmo_move_props();
        let r = canvas.gizmo_rotate_props();
        let s = canvas.gizmo_scale_props();
        let keys = |v: &Value| -> Vec<String> { v.as_object().unwrap().keys().cloned().collect() };
        assert_eq!(keys(&m), keys(&r));
        assert_eq!(keys(&r), keys(&s));
        assert_eq!(m["tool"], "move");
        assert_eq!(r["tool"], "rotate");
        assert_eq!(s["tool"], "scale");
        // Visibility flows through the active tool — only the matching
        // gizmo paints on a given frame.
        assert_eq!(m["visible"], true);
        assert_eq!(r["visible"], false);
        assert_eq!(s["visible"], false);
    }

    #[test]
    fn gizmo_and_resize_handle_share_selection_center() {
        // §22 cross-binding parity: flipping the selection's transform
        // shows up in *both* gizmo and resize-handle emissions through
        // the same `selection_center()` helper. The load-bearing
        // duplication check for the canvas slot.
        let mut canvas = sample_canvas();
        canvas.tool = ToolMode::Move;
        let g0 = canvas.gizmo_move_props();
        let h0 = canvas.resize_handle_props();
        let g0_x = g0["center-x"].as_f64().unwrap();
        // Top handle's `x` is the bbox mid-x, which is the center-x.
        let top = h0["handles"]
            .as_array()
            .unwrap()
            .iter()
            .find(|h| h["id"] == "t")
            .unwrap();
        let h0_x = top["x"].as_f64().unwrap();
        assert!((g0_x - h0_x).abs() < 0.01, "shared center on first frame");

        canvas
            .document
            .root
            .as_mut()
            .unwrap()
            .find_mut("root")
            .unwrap()
            .transform
            .position[0] = 250.0;
        let g1 = canvas.gizmo_move_props();
        let h1 = canvas.resize_handle_props();
        let top1 = h1["handles"]
            .as_array()
            .unwrap()
            .iter()
            .find(|h| h["id"] == "t")
            .unwrap();
        assert!(
            (g1["center-x"].as_f64().unwrap() - top1["x"].as_f64().unwrap()).abs() < 0.01,
            "shared center on second frame"
        );
        assert_eq!(g1["center-x"], 250.0);
    }

    #[test]
    fn resize_handle_props_collapse_to_invisible_when_no_selection() {
        let mut canvas = sample_canvas();
        canvas.selection = None;
        let props = canvas.resize_handle_props();
        assert_eq!(props["visible"], false);
        assert_eq!(props["handles"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn component_picker_props_omit_candidates_when_closed_but_keep_shape() {
        let mut canvas = sample_canvas();
        canvas.picker.open = false;
        let props = canvas.component_picker_props();
        assert_eq!(props["open"], false);
        // Shape stays — `open: false` is the visibility signal, not key
        // absence (same data-shape rule as gizmos).
        assert!(props.get("candidates").is_some());
    }

    #[test]
    fn pointer_drag_round_trip_under_move_tool() {
        let mut canvas = sample_canvas();
        canvas.tool = ToolMode::Move;
        // Hit somewhere inside the selection bbox.
        assert!(!canvas.pointer_down(100.0, 80.0));
        assert!(canvas.drag_active(), "down captured the drag");
        let dirty = canvas.pointer_move(150.0, 110.0);
        assert!(dirty);
        let pos = canvas.document.root.as_ref().unwrap().transform.position;
        assert_eq!(pos, [150.0, 110.0], "delta applied to position");
        assert!(canvas.pointer_up(0.0, 0.0));
        assert!(!canvas.drag_active(), "up released the drag");
    }

    #[test]
    fn pointer_drag_round_trip_under_rotate_tool() {
        let mut canvas = sample_canvas();
        canvas.tool = ToolMode::Rotate;
        canvas.pointer_down(100.0, 80.0);
        canvas.pointer_move(180.0, 80.0); // dx=80, 0.5°/px = 40°
        let rot = canvas.document.root.as_ref().unwrap().transform.rotation;
        let expected = 40_f32.to_radians();
        assert!((rot - expected).abs() < 1e-4, "got {rot}, want {expected}");
        canvas.pointer_up(0.0, 0.0);
    }

    #[test]
    fn pointer_drag_round_trip_under_scale_tool() {
        let mut canvas = sample_canvas();
        canvas.tool = ToolMode::Scale;
        canvas.pointer_down(100.0, 80.0);
        canvas.pointer_move(200.0, 130.0); // dx=100 → +1.0, dy=50 → +0.5
        let scale = canvas.document.root.as_ref().unwrap().transform.scale;
        assert!((scale[0] - 2.0).abs() < 1e-4);
        assert!((scale[1] - 1.5).abs() < 1e-4);
        canvas.pointer_up(0.0, 0.0);
    }

    #[test]
    fn pointer_down_outside_selection_does_not_capture() {
        let mut canvas = sample_canvas();
        canvas.pointer_down(1000.0, 1000.0);
        assert!(!canvas.drag_active(), "miss must not capture a drag");
        let dirty = canvas.pointer_move(1100.0, 1100.0);
        assert!(!dirty, "no drag, no redraw");
    }

    #[test]
    fn pointer_drag_with_no_selection_is_noop() {
        let mut canvas = sample_canvas();
        canvas.selection = None;
        canvas.pointer_down(100.0, 80.0);
        assert!(!canvas.drag_active());
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
