//! Root application state + Slint binding layer.
//!
//! Everything reloadable lives behind a single [`AppState`] so §7's
//! hot-reload story is exactly one serde call. Mutation goes through
//! the [`Shell`] wrapper, which owns both a
//! `prism_core::Store<AppState>` and the root `AppWindow` Slint
//! handle. Shell state that callbacks need to mutate lives behind
//! `Rc<RefCell<ShellInner>>` so Slint closures can borrow it.

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::Arc;
use std::time::Instant;

use crate::signals::SignalRuntime;
use prism_builder::AssetSource;
use prism_builder::{
    app::{AppIcon, NavigationConfig, Page, PrismApp},
    compile_slint_preview, compute_layout, render_document_slint_preview_with_assets_and_data,
    layout::{
        AbsoluteProps, AlignOption, Dimension, FlexDirection, FlowDisplay, FlowProps,
        GridPlacement, JustifyOption, LayoutMode, PageSize, TrackSize,
    },
    path_from_string, preview_component_factory,
    starter::{builtin_prefab, card_prefab_def, materialize_prefab, register_builtins},
    AggregateOp, BuilderDocument, CellEdge, ComponentRegistry, DispatchResult, ExposedSlot,
    FacetBinding, FacetDataSource, FacetDef, FacetDirection, FacetKind, FacetLayout, FacetOutput,
    FacetTemplate, FacetVariantRule, FieldKind, FieldSpec, GridCell, LiveDocument, Node, NodeId,
    PrefabDef, ScriptLanguage, StyleProperties,
};
use prism_core::design_tokens::{DesignTokens, DEFAULT_TOKENS};
use prism_core::editor::EditorState;
#[cfg(feature = "native")]
use prism_core::foundation::persistence::{CollectionStore, EdgeFilter, ObjectFilter};
use prism_core::foundation::vfs::VfsManager;
use prism_core::help::HelpRegistry;
use prism_core::shell_mode::{Permission, ShellMode, ShellModeContext};
use prism_core::{Action, Store, Subscription};
use serde::{Deserialize, Serialize};
use serde_json::json;
use slint::{ComponentHandle, Model, ModelRc, SharedString, Timer, TimerMode, VecModel};

use crate::command::CommandRegistry;
use crate::help::register_help_entries;
use crate::input::{combo_from_slint, update_panel_schemes, FocusRegion, InputManager};
use crate::keybindings::UserKeybindings;
use crate::panels::{editor::CodeEditorPanel, properties::PropertiesPanel, Panel};
use crate::persistence::{PersistenceError, ProjectPersistence};
use crate::search::SearchIndex;
use crate::selection::SelectionModel;
use crate::telemetry::FirstPaint;
use crate::{
    AppCardItem, AppWindow, BreadcrumbItem, ButtonSpec, ColorPreset, CommandItem,
    ComponentPaletteItem, DockDividerRect, DockPanelRect, DockTabItem, DocsPanelData,
    EditorIndentGuide, EditorLine, EditorToken, ExplorerNodeItem, FieldRow, GridCellItem,
    GridEdgeHandle, GutterRect, HelpTooltipData, InspectorNode, MenuDef, MenuItem, PageLayoutData,
    PreviewNode, SearchResultItem, TabItem, ToastItem, WidgetToolbarItem, WorkflowPageItem,
};

// ── Persistent VecModels ───────────────────────────────────────────
//
// Slint's ChangeTracker (inside every Flickable / ScrollView) lazily
// evaluates child bindings each frame. If a model property was replaced
// wholesale (ModelRc::from(new_vec)), the for-repeater lazily rebuilds
// its items during that evaluation — destroying old VRc<ItemTree>
// instances mid-binding-eval, which triggers "Recursion detected" in
// PropertyHandle::remove_binding.
//
// Fix: keep ONE VecModel per model property for the entire lifetime
// of the window.  Set it on the AppWindow exactly once (in from_state),
// then update it in-place via set_row_data / push.  Items are NEVER
// removed; excess slots are hidden via a companion `*-count` property
// and `visible: i < root.*-count` in the Slint for-repeater.

/// Update a persistent VecModel in-place.  Returns the active count.
fn sync_model<T: Clone + 'static>(model: &VecModel<T>, new_data: &[T]) -> i32 {
    let old_len = model.row_count();
    let new_len = new_data.len();
    for (i, item) in new_data.iter().enumerate() {
        if i < old_len {
            model.set_row_data(i, item.clone());
        } else {
            model.push(item.clone());
        }
    }
    new_len as i32
}

macro_rules! persistent_models {
    ($($name:ident : $ty:ty),* $(,)?) => {
        struct PersistentModels {
            $( $name: Rc<VecModel<$ty>>, )*
        }
        impl PersistentModels {
            fn new() -> Self {
                Self {
                    $( $name: Rc::new(VecModel::default()), )*
                }
            }
        }
    };
}

persistent_models! {
    grid_cells: GridCellItem,
    grid_edge_handles: GridEdgeHandle,
    preview_nodes: PreviewNode,
    inspector_nodes: InspectorNode,
    property_rows: FieldRow,
    breadcrumbs: BreadcrumbItem,
    component_palette: ComponentPaletteItem,
    tabs: TabItem,
    command_results: CommandItem,
    notifications: ToastItem,
    search_results: SearchResultItem,
    column_gutters: GutterRect,
    row_gutters: GutterRect,
    app_cards: AppCardItem,
    explorer_nodes: ExplorerNodeItem,
    dock_panels: DockPanelRect,
    dock_dividers: DockDividerRect,
    workflow_pages: WorkflowPageItem,
    actions: ButtonSpec,
    menu_defs: MenuDef,
    editor_lines: EditorLine,
    signal_connections: crate::SignalConnectionItem,
    signal_list: crate::SignalItem,
    signal_target_nodes: crate::TargetNodeItem,
    nav_pages: crate::NavPageItem,
    nav_graph_nodes: crate::NavGraphNode,
    nav_graph_edges: crate::NavGraphEdge,
    schema_list: crate::SchemaListItem,
    widget_toolbar: crate::WidgetToolbarItem,
}

// ── Reloadable state ───────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppState {
    pub tokens: DesignTokens,
    pub context: ShellModeContext,
    pub shell_view: ShellView,
    pub workspace: prism_dock::DockWorkspace,
    pub apps: Vec<PrismApp>,
    pub builder_document: BuilderDocument,
    pub selection: SelectionModel,
    pub command_palette_open: bool,
    pub command_palette_query: String,
    pub search_query: String,
    pub editor_state: EditorState,
    pub toasts: Vec<ToastData>,
    pub show_grid_overlay: bool,
    pub show_activity_bar: bool,
    pub show_left_sidebar: bool,
    pub show_right_sidebar: bool,
    pub explorer_expanded: HashSet<String>,
    pub explorer_view_mode: crate::explorer::ExplorerViewMode,
    #[serde(default = "default_viewport_width")]
    pub viewport_width: f32,
    #[serde(default)]
    pub transform_tool: TransformTool,
    next_toast_id: u64,
    next_node_id: u64,
    next_app_id: u64,
    #[serde(default)]
    pub runtime_overrides: HashMap<String, serde_json::Map<String, serde_json::Value>>,
    #[serde(default)]
    pub selected_schema_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum TransformTool {
    #[default]
    Move,
    Rotate,
    Scale,
}

fn default_viewport_width() -> f32 {
    1280.0
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ShellView {
    Launchpad,
    App { app_id: String },
}

impl ShellView {
    pub fn is_launchpad(&self) -> bool {
        matches!(self, ShellView::Launchpad)
    }

    pub fn active_app_id(&self) -> Option<&str> {
        match self {
            ShellView::Launchpad => None,
            ShellView::App { app_id } => Some(app_id),
        }
    }
}

impl AppState {
    pub fn active_app(&self) -> Option<&PrismApp> {
        let id = self.shell_view.active_app_id()?;
        self.apps.iter().find(|a| a.id == id)
    }

    pub fn active_app_mut(&mut self) -> Option<&mut PrismApp> {
        let id = match &self.shell_view {
            ShellView::App { app_id } => app_id.clone(),
            _ => return None,
        };
        self.apps.iter_mut().find(|a| a.id == id)
    }

    fn sync_document_from_app(&mut self) {
        if let Some(app) = self.active_app() {
            if let Some(doc) = app.active_document() {
                self.builder_document = doc.clone();
            }
        }
    }

    fn sync_source_to_app(&mut self, source: &str) {
        if let Some(app) = self.active_app_mut() {
            if let Some(page_source) = app.active_source_mut() {
                *page_source = source.to_string();
            }
        }
    }

    fn sync_document_to_app(&mut self) {
        let doc = self.builder_document.clone();
        if let Some(app) = self.active_app_mut() {
            if let Some(page_doc) = app.active_document_mut() {
                *page_doc = doc;
            }
        }
    }

    pub fn sync_document_from_app_pub(&mut self) {
        self.sync_document_from_app();
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToastData {
    pub id: u64,
    pub title: String,
    pub body: String,
    pub kind: String,
    #[serde(skip)]
    pub created_at: Option<Instant>,
}

/// Derive the Slint `active_panel_id` from the dock workspace state.
/// Maps workflow pages to the legacy panel IDs that `app.slint` expects:
///   0 = Identity, 1 = Edit/Builder, 2 = CodeEditor, 3 = Explorer.
pub fn panel_id_for_slint(workspace: &prism_dock::DockWorkspace) -> i32 {
    match workspace.active_page().id.as_str() {
        "code" => 2,
        _ => 1,
    }
}

/// Map a legacy panel name to the corresponding workspace page id.
pub fn page_id_for_panel(panel: &str) -> &str {
    match panel {
        "identity" => "edit",
        "builder" | "edit" => "edit",
        "code-editor" | "code" => "code",
        "explorer" => "edit",
        "design" => "design",
        "fusion" => "fusion",
        "navigation" => "navigation",
        "preview" => "preview",
        _ => "edit",
    }
}

pub fn is_preview_mode(workspace: &prism_dock::DockWorkspace) -> bool {
    workspace.active_page().id == "preview"
}

impl Default for AppState {
    fn default() -> Self {
        let apps = sample_apps();
        Self {
            tokens: DEFAULT_TOKENS,
            context: ShellModeContext {
                shell_mode: ShellMode::Build,
                permission: Permission::Dev,
            },
            shell_view: ShellView::Launchpad,
            workspace: prism_dock::DockWorkspace::default(),
            apps,
            builder_document: BuilderDocument::page_shell(),
            selection: SelectionModel::default(),
            command_palette_open: false,
            command_palette_query: String::new(),
            search_query: String::new(),
            editor_state: {
                let mut es = EditorState::with_text(
                    "// Welcome to Prism Code Editor\n// Start typing to edit\n\nfn main() {\n    let greeting = \"Hello, Prism!\";\n    println!(\"{}\", greeting);\n}\n",
                );
                es.language = "rust".into();
                es
            },
            toasts: Vec::new(),
            show_grid_overlay: true,
            show_activity_bar: true,
            show_left_sidebar: true,
            show_right_sidebar: true,
            explorer_expanded: HashSet::new(),
            explorer_view_mode: crate::explorer::ExplorerViewMode::default(),
            viewport_width: 1280.0,
            transform_tool: TransformTool::Move,
            next_toast_id: 0,
            next_node_id: 100,
            next_app_id: 10,
            runtime_overrides: HashMap::new(),
            selected_schema_id: None,
        }
    }
}

fn sample_apps() -> Vec<PrismApp> {
    vec![
        PrismApp {
            id: "app-1".into(),
            name: "Lattice".into(),
            description: "Collaborative workspace with real-time CRDT sync.".into(),
            icon: AppIcon::Globe,
            pages: vec![
                Page {
                    id: "p1".into(),
                    title: "Home".into(),
                    route: "/".into(),
                    source: String::new(),
                    document: sample_document(),
                    style: StyleProperties::default(),
                },
                Page {
                    id: "p2".into(),
                    title: "Dashboard".into(),
                    route: "/dashboard".into(),
                    source: String::new(),
                    document: sample_dashboard_document(),
                    style: StyleProperties::default(),
                },
            ],
            active_page: 0,
            navigation: NavigationConfig::default(),
            style: StyleProperties::default(),
        },
        PrismApp {
            id: "app-2".into(),
            name: "Musica".into(),
            description: "Audio workstation with timeline and MIDI.".into(),
            icon: AppIcon::Music,
            pages: vec![Page {
                id: "p1".into(),
                title: "Studio".into(),
                route: "/".into(),
                source: String::new(),
                document: BuilderDocument::page_shell(),
                style: StyleProperties::default(),
            }],
            active_page: 0,
            navigation: NavigationConfig::default(),
            style: StyleProperties::default(),
        },
        PrismApp {
            id: "app-3".into(),
            name: "Flux".into(),
            description: "Visual dataflow editor for creative coding.".into(),
            icon: AppIcon::Zap,
            pages: vec![
                Page {
                    id: "p1".into(),
                    title: "Canvas".into(),
                    route: "/".into(),
                    source: String::new(),
                    document: BuilderDocument::page_shell(),
                    style: StyleProperties::default(),
                },
                Page {
                    id: "p2".into(),
                    title: "Settings".into(),
                    route: "/settings".into(),
                    source: String::new(),
                    document: BuilderDocument::page_shell(),
                    style: StyleProperties::default(),
                },
                Page {
                    id: "p3".into(),
                    title: "Preview".into(),
                    route: "/preview".into(),
                    source: String::new(),
                    document: BuilderDocument::page_shell(),
                    style: StyleProperties::default(),
                },
            ],
            active_page: 0,
            navigation: NavigationConfig::default(),
            style: StyleProperties::default(),
        },
    ]
}

fn sample_dashboard_document() -> BuilderDocument {
    BuilderDocument {
        root: Some(Node {
            id: "dash-root".into(),
            component: "container".into(),
            props: json!({ "spacing": 16 }),
            layout_mode: LayoutMode::Flow(FlowProps {
                display: FlowDisplay::Flex,
                flex_direction: FlexDirection::Column,
                gap: 16.0,
                ..Default::default()
            }),
            children: vec![
                Node {
                    id: "dash-title".into(),
                    component: "text".into(),
                    props: json!({ "body": "Dashboard", "level": "h2" }),
                    children: vec![],
                    ..Default::default()
                },
                Node {
                    id: "dash-text".into(),
                    component: "text".into(),
                    props: json!({ "body": "Overview of your workspace metrics and activity." }),
                    children: vec![],
                    ..Default::default()
                },
            ],
            ..Default::default()
        }),
        ..Default::default()
    }
}

fn sample_document() -> BuilderDocument {
    use prism_builder::layout::PageLayout;
    use prism_core::foundation::geometry::Edges;

    BuilderDocument {
        root: Some(Node {
            id: "root".into(),
            component: "container".into(),
            props: json!({ "spacing": 16 }),
            layout_mode: LayoutMode::Flow(FlowProps {
                display: FlowDisplay::Flex,
                flex_direction: FlexDirection::Column,
                gap: 16.0,
                ..Default::default()
            }),
            children: vec![
                Node {
                    id: "hero".into(),
                    component: "text".into(),
                    props: json!({ "body": "Welcome to Prism", "level": "h1" }),
                    children: vec![],
                    ..Default::default()
                },
                Node {
                    id: "intro".into(),
                    component: "text".into(),
                    props: json!({
                        "body": "The distributed visual operating system. Pick a panel to start editing."
                    }),
                    children: vec![],
                    ..Default::default()
                },
                Node {
                    id: "cols".into(),
                    component: "columns".into(),
                    props: json!({ "gap": 16 }),
                    layout_mode: LayoutMode::Flow(FlowProps {
                        display: FlowDisplay::Flex,
                        flex_direction: FlexDirection::Row,
                        gap: 16.0,
                        ..Default::default()
                    }),
                    children: vec![
                        Node {
                            id: "col1".into(),
                            component: "card".into(),
                            props: json!({ "title": "Build", "body": "Create pages visually with drag-and-drop components." }),
                            layout_mode: LayoutMode::Flow(FlowProps {
                                flex_grow: 1.0,
                                ..Default::default()
                            }),
                            children: vec![],
                            ..Default::default()
                        },
                        Node {
                            id: "col2".into(),
                            component: "card".into(),
                            props: json!({ "title": "Collaborate", "body": "Real-time CRDT sync across all connected peers." }),
                            layout_mode: LayoutMode::Flow(FlowProps {
                                flex_grow: 1.0,
                                ..Default::default()
                            }),
                            children: vec![],
                            ..Default::default()
                        },
                    ],
                    ..Default::default()
                },
            ],
            ..Default::default()
        }),
        page_layout: PageLayout {
            size: PageSize::Responsive,
            margins: Edges::new(32.0, 48.0, 32.0, 48.0),
            grid: Some(GridCell::split(
                prism_builder::layout::SplitDirection::Vertical,
                vec![
                    TrackSize::Fr { value: 1.0 },
                    TrackSize::Fr { value: 2.0 },
                    TrackSize::Fr { value: 1.0 },
                ],
                24.0,
                vec![GridCell::leaf(), GridCell::leaf(), GridCell::leaf()],
            )),
            column_gap: 24.0,
            ..Default::default()
        },
        ..Default::default()
    }
}

// ── Actions ────────────────────────────────────────────────────────

pub struct SelectPage(pub String);

impl Action<AppState> for SelectPage {
    fn apply(self, state: &mut AppState) {
        state.workspace.switch_page_by_id(&self.0);
    }
}

// ── Undo snapshots ─────────────────────────────────────────────────

#[derive(Debug, Clone)]
struct SourceSnapshot {
    description: String,
    source: String,
    selection: SelectionModel,
}

// ── Context menu ──────────────────────────────────────────────────

struct ContextMenuState {
    target_kind: String,
    target_id: String,
    x: f32,
    y: f32,
}

// ── Shell inner state (shared with callbacks) ──────────────────────

struct ShellInner {
    store: Store<AppState>,
    registry: Arc<ComponentRegistry>,
    live: Option<LiveDocument>,
    help: HelpRegistry,
    input: InputManager,
    commands: CommandRegistry,
    menus: crate::menu::MenuRegistry,
    models: PersistentModels,
    undo_past: Vec<SourceSnapshot>,
    undo_future: Vec<SourceSnapshot>,
    clipboard: Option<Node>,
    help_pending_id: String,
    help_active_id: String,
    dock_area_dims: (f32, f32),
    dock_dirty: Cell<bool>,
    syncing: Cell<bool>,
    sync_timer: Timer,
    dock_check_timer: Timer,
    drag_component_type: String,
    drag_initial_transform: Option<DragSnapshot>,
    resize_initial: Option<ResizeSnapshot>,
    gap_resize_snapshot: Option<GapResizeSnapshot>,
    pending_picker: Option<(String, f32, f32)>,
    pending_context_menu: Option<ContextMenuState>,
    vfs: VfsManager,
    toast_timer: Timer,
    user_color_swatches: Vec<String>,
    toggled_sections: std::collections::HashSet<String>,
    last_selected_node: Option<String>,
    nav_link_source: Option<usize>,
    persistence: ProjectPersistence,
    #[cfg(feature = "native")]
    collection: CollectionStore,
    #[cfg(feature = "native")]
    project: Option<crate::project::ProjectManager>,
}

impl ShellInner {
    fn push_undo(&mut self, description: &str) {
        let source = self
            .live
            .as_ref()
            .map(|l| l.source.clone())
            .unwrap_or_default();
        let selection = self.store.state().selection.clone();
        self.undo_past.push(SourceSnapshot {
            description: description.into(),
            source,
            selection,
        });
        self.undo_future.clear();
        if self.undo_past.len() > 100 {
            self.undo_past.remove(0);
        }
        self.persistence.mark_dirty();
    }

    fn perform_undo(&mut self) {
        let Some(snapshot) = self.undo_past.pop() else {
            return;
        };
        let current_source = self
            .live
            .as_ref()
            .map(|l| l.source.clone())
            .unwrap_or_default();
        let current_sel = self.store.state().selection.clone();
        self.undo_future.push(SourceSnapshot {
            description: snapshot.description.clone(),
            source: current_source,
            selection: current_sel,
        });
        if let Some(ref mut live) = self.live {
            let _ = live.set_source(snapshot.source);
        }
        let sel = snapshot.selection;
        self.store.mutate(|state| {
            state.selection = sel;
        });
        self.sync_builder_document();
    }

    fn perform_redo(&mut self) {
        let Some(snapshot) = self.undo_future.pop() else {
            return;
        };
        let current_source = self
            .live
            .as_ref()
            .map(|l| l.source.clone())
            .unwrap_or_default();
        let current_sel = self.store.state().selection.clone();
        self.undo_past.push(SourceSnapshot {
            description: snapshot.description.clone(),
            source: current_source,
            selection: current_sel,
        });
        if let Some(ref mut live) = self.live {
            let _ = live.set_source(snapshot.source);
        }
        let sel = snapshot.selection;
        self.store.mutate(|state| {
            state.selection = sel;
        });
        self.sync_builder_document();
    }

    fn fire_signal(
        &mut self,
        source_node: &str,
        signal: &str,
        payload: serde_json::Map<String, serde_json::Value>,
    ) -> bool {
        let is_preview = is_preview_mode(&self.store.state().workspace);
        eprintln!("[signal] fire_signal source={source_node} signal={signal} preview={is_preview}");
        if !is_preview {
            return false;
        }
        let connections = self
            .store
            .state()
            .active_app()
            .and_then(|a| a.active_document())
            .map(|d| d.connections.clone())
            .unwrap_or_default();
        eprintln!("[signal] connections count={}", connections.len());
        if connections.is_empty() {
            return false;
        }
        let results = SignalRuntime::fire(source_node, signal, payload, &connections);
        eprintln!("[signal] dispatch results={}", results.len());
        if results.is_empty() {
            return false;
        }
        let mut cascading: Vec<(String, String)> = Vec::new();
        let mut navigate_target: Option<String> = None;
        let mut custom_handlers: Vec<(String, serde_json::Map<String, serde_json::Value>)> =
            Vec::new();
        self.store.mutate(|state| {
            for result in &results {
                match result {
                    DispatchResult::SetProperty {
                        target_node,
                        key,
                        value,
                    } => {
                        state
                            .runtime_overrides
                            .entry(target_node.clone())
                            .or_default()
                            .insert(key.clone(), value.clone());
                    }
                    DispatchResult::ToggleVisibility { target_node } => {
                        let current = state
                            .runtime_overrides
                            .get(target_node.as_str())
                            .and_then(|m| m.get("visible"))
                            .and_then(|v| v.as_bool())
                            .or_else(|| {
                                state
                                    .builder_document
                                    .root
                                    .as_ref()
                                    .and_then(|r| r.find(target_node))
                                    .and_then(|n| n.props.get("visible"))
                                    .and_then(|v| v.as_bool())
                            })
                            .unwrap_or(true);
                        state
                            .runtime_overrides
                            .entry(target_node.clone())
                            .or_default()
                            .insert("visible".into(), serde_json::Value::from(!current));
                    }
                    DispatchResult::PlayAnimation {
                        target_node,
                        animation,
                    } => {
                        let entry = state
                            .runtime_overrides
                            .entry(target_node.clone())
                            .or_default();
                        entry.insert("animating".into(), serde_json::Value::from(true));
                        entry.insert(
                            "animation".into(),
                            serde_json::Value::from(animation.as_str()),
                        );
                    }
                    DispatchResult::EmitSignal {
                        target_node,
                        signal,
                    } => {
                        cascading.push((target_node.clone(), signal.clone()));
                    }
                    DispatchResult::NavigateTo { target } => {
                        navigate_target = Some(target.clone());
                    }
                    DispatchResult::Custom { handler, payload } => {
                        custom_handlers.push((handler.clone(), payload.clone()));
                    }
                }
            }
        });
        // If no explicit NavigateTo from connections, check for href prop on clicked nodes
        if navigate_target.is_none() && signal == "clicked" {
            let href = self
                .store
                .state()
                .builder_document
                .root
                .as_ref()
                .and_then(|r| r.find(source_node))
                .and_then(|n| n.props.get("href"))
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string());
            if let Some(h) = href {
                navigate_target = Some(h);
            }
        }
        if let Some(route) = navigate_target {
            self.store.mutate(|state| {
                state.runtime_overrides.clear();
                if let Some(app) = state.active_app_mut() {
                    // Match by route first, then by page ID
                    let idx = app
                        .find_page_by_route(&route)
                        .or_else(|| app.find_page_by_id(&route));
                    if let Some(idx) = idx {
                        app.active_page = idx;
                    }
                }
                state.sync_document_from_app();
            });
            self.load_active_page();
        }
        #[cfg(feature = "native")]
        if !custom_handlers.is_empty() {
            self.exec_custom_handlers(&custom_handlers, source_node, signal);
        }
        const MAX_CASCADE_DEPTH: usize = 8;
        for (i, (target, sig)) in cascading.into_iter().enumerate() {
            if i >= MAX_CASCADE_DEPTH {
                break;
            }
            self.fire_signal(&target, &sig, serde_json::Map::new());
        }
        self.store.mutate(|state| {
            Self::apply_runtime_overrides_to_doc(
                &mut state.builder_document,
                &state.runtime_overrides,
            );
        });
        true
    }

    fn apply_runtime_overrides_to_doc(
        doc: &mut BuilderDocument,
        overrides: &HashMap<String, serde_json::Map<String, serde_json::Value>>,
    ) {
        if overrides.is_empty() {
            return;
        }
        if let Some(ref mut root) = doc.root {
            Self::apply_overrides_to_tree(root, overrides);
        }
    }

    fn apply_overrides_to_tree(
        node: &mut Node,
        overrides: &HashMap<String, serde_json::Map<String, serde_json::Value>>,
    ) {
        if let Some(node_overrides) = overrides.get(&node.id) {
            if let Some(obj) = node.props.as_object_mut() {
                for (key, value) in node_overrides {
                    obj.insert(key.clone(), value.clone());
                }
            }
        }
        for child in &mut node.children {
            Self::apply_overrides_to_tree(child, overrides);
        }
    }

    #[cfg(feature = "native")]
    fn exec_custom_handlers(
        &mut self,
        handlers: &[(String, serde_json::Map<String, serde_json::Value>)],
        source_node: &str,
        signal: &str,
    ) {
        let page_source = self
            .store
            .state()
            .active_app()
            .and_then(|a| a.pages.get(a.active_page))
            .map(|p| p.source.clone())
            .unwrap_or_default();
        for (handler_name, payload) in handlers {
            let mut args = payload.clone();
            args.insert("_source_node".into(), serde_json::Value::from(source_node));
            args.insert("_signal".into(), serde_json::Value::from(signal));
            let script = build_handler_script(&page_source, handler_name);
            let mut call_args = serde_json::Map::new();
            call_args.insert("event".into(), serde_json::Value::Object(args));
            match prism_daemon::modules::luau_module::exec(&script, Some(&call_args)) {
                Ok(result) => {
                    self.apply_luau_result(&result);
                }
                Err(e) => {
                    self.add_toast(
                        &format!("Signal handler '{handler_name}' error"),
                        &e,
                        "error",
                    );
                }
            }
        }
    }

    #[cfg(feature = "native")]
    fn apply_luau_result(&mut self, result: &serde_json::Value) {
        use serde_json::Value;
        let Some(obj) = result.as_object() else {
            return;
        };
        let mut needs_sync = false;
        if let Some(Value::Array(actions)) = obj.get("_actions") {
            for action in actions {
                let Some(action_obj) = action.as_object() else {
                    continue;
                };
                match action_obj.get("type").and_then(|v| v.as_str()) {
                    Some("set_property") => {
                        let node_id = action_obj.get("node_id").and_then(|v| v.as_str());
                        let key = action_obj.get("key").and_then(|v| v.as_str());
                        let value = action_obj.get("value");
                        if let (Some(node_id), Some(key), Some(value)) = (node_id, key, value) {
                            if let Some(ref mut live) = self.live {
                                let formatted = match value {
                                    Value::String(s) => format!(
                                        "\"{}\"",
                                        prism_builder::slint_source::escape_slint_string(s)
                                    ),
                                    Value::Bool(b) => b.to_string(),
                                    Value::Number(n) => n.to_string(),
                                    _ => continue,
                                };
                                let _ = live.edit_prop_in_source(node_id, key, &formatted);
                                needs_sync = true;
                            }
                        }
                    }
                    Some("toggle_visibility") => {
                        let node_id = action_obj.get("node_id").and_then(|v| v.as_str());
                        if let Some(node_id) = node_id {
                            self.store.mutate(|state| {
                                if let Some(doc) =
                                    state.active_app_mut().and_then(|a| a.active_document_mut())
                                {
                                    let result = DispatchResult::ToggleVisibility {
                                        target_node: node_id.into(),
                                    };
                                    SignalRuntime::apply_result(&result, doc);
                                }
                            });
                            needs_sync = true;
                        }
                    }
                    Some("navigate") => {
                        let route = action_obj.get("route").and_then(|v| v.as_str());
                        if let Some(route) = route {
                            let route = route.to_string();
                            self.store.mutate(|state| {
                                if let Some(app) = state.active_app_mut() {
                                    let idx = app
                                        .find_page_by_route(&route)
                                        .or_else(|| app.find_page_by_id(&route));
                                    if let Some(idx) = idx {
                                        app.active_page = idx;
                                    }
                                }
                                state.sync_document_from_app();
                            });
                            self.load_active_page();
                            return;
                        }
                    }
                    Some("emit_signal") => {
                        let node_id = action_obj.get("node_id").and_then(|v| v.as_str());
                        let sig = action_obj.get("signal").and_then(|v| v.as_str());
                        if let (Some(node_id), Some(sig)) = (node_id, sig) {
                            self.fire_signal(node_id, sig, serde_json::Map::new());
                        }
                    }
                    _ => {}
                }
            }
        }
        if needs_sync {
            self.sync_builder_document();
        }
    }

    fn load_active_page(&mut self) {
        self.store.mutate(|state| {
            state.runtime_overrides.clear();
        });
        let state = self.store.state();
        if let Some(app) = state.active_app() {
            if let Some(page) = app.pages.get(app.active_page) {
                let registry = Arc::clone(&self.registry);
                let tokens = state.tokens;
                if page.source.is_empty() {
                    let live = LiveDocument::from_document(page.document.clone(), registry, tokens);
                    self.live = Some(live);
                } else {
                    let live = LiveDocument::from_source(page.source.clone(), registry, tokens);
                    self.live = Some(live);
                }
                self.sync_builder_document();
            }
        }
        self.undo_past.clear();
        self.undo_future.clear();
    }

    fn save_to_active_page(&mut self) {
        if let Some(ref mut live) = self.live {
            let source = live.source.clone();
            let mut doc = live.document().clone();
            self.store.mutate(|state| {
                state.sync_source_to_app(&source);
                doc.page_layout = state.builder_document.page_layout.clone();
                if let Some(app_doc) = state.active_app().and_then(|a| a.active_document()) {
                    doc.connections = app_doc.connections.clone();
                    doc.facets = app_doc.facets.clone();
                    doc.facet_schemas = app_doc.facet_schemas.clone();
                    doc.resources = app_doc.resources.clone();
                    doc.prefabs = app_doc.prefabs.clone();
                }
                Self::apply_runtime_overrides_to_doc(&mut doc, &state.runtime_overrides);
                state.builder_document = doc;
                state.sync_document_to_app();
            });
        }
    }

    fn sync_builder_document(&mut self) {
        if let Some(ref mut live) = self.live {
            let mut doc = live.document().clone();
            let source = live.source.clone();
            self.store.mutate(|state| {
                doc.page_layout = state.builder_document.page_layout.clone();
                if let Some(app_doc) = state.active_app().and_then(|a| a.active_document()) {
                    doc.connections = app_doc.connections.clone();
                    doc.facets = app_doc.facets.clone();
                    doc.facet_schemas = app_doc.facet_schemas.clone();
                    doc.resources = app_doc.resources.clone();
                    doc.prefabs = app_doc.prefabs.clone();
                }
                Self::apply_runtime_overrides_to_doc(&mut doc, &state.runtime_overrides);
                state.builder_document = doc;
                if state.editor_state.text() != source {
                    let cursor = state.editor_state.cursor.position;
                    state.editor_state.set_text(&source);
                    state.editor_state.language = "slint".into();
                    state
                        .editor_state
                        .set_cursor_position(cursor.line, cursor.col);
                }
            });
        }
    }

    fn switch_to_page(&mut self, page_index: usize) {
        self.save_to_active_page();
        self.store.mutate(|state| {
            if let Some(app) = state.active_app_mut() {
                app.active_page = page_index;
            }
            state.selection.clear();
            state.sync_document_from_app();
        });
        self.load_active_page();
        self.dock_dirty.set(true);
    }

    fn add_toast(&mut self, title: &str, body: &str, kind: &str) {
        self.store.mutate(|state| {
            let id = state.next_toast_id;
            state.next_toast_id += 1;
            state.toasts.push(ToastData {
                id,
                title: title.into(),
                body: body.into(),
                kind: kind.into(),
                created_at: Some(Instant::now()),
            });
            if state.toasts.len() > 5 {
                state.toasts.remove(0);
            }
        });
    }
}

// ── Shell ──────────────────────────────────────────────────────────

pub struct Shell {
    inner: Rc<RefCell<ShellInner>>,
    window: AppWindow,
    telemetry: FirstPaint,
}

impl Shell {
    pub fn new() -> Result<Self, slint::PlatformError> {
        Self::from_state(AppState::default())
    }

    pub fn from_state(state: AppState) -> Result<Self, slint::PlatformError> {
        let telemetry = FirstPaint::start();
        let window = AppWindow::new()?;
        let mut registry = ComponentRegistry::new();
        register_builtins(&mut registry).expect("starter components must register");
        let mut help = HelpRegistry::new();
        register_help_entries(&mut help, &registry);
        let mut input = InputManager::with_defaults();
        let user_kb = UserKeybindings::load(&UserKeybindings::default_path());
        user_kb.apply_to(&mut input);
        let models = PersistentModels::new();

        // Bind each persistent VecModel to the AppWindow ONCE.
        // From here on, push functions only call set_row_data / push.
        macro_rules! bind_model {
            ($prop:ident, $field:ident, $T:ty) => {
                window.$prop(ModelRc::from(
                    models.$field.clone() as Rc<dyn Model<Data = $T>>
                ));
            };
        }
        bind_model!(set_grid_cells, grid_cells, GridCellItem);
        bind_model!(set_grid_edge_handles, grid_edge_handles, GridEdgeHandle);
        bind_model!(set_preview_nodes, preview_nodes, PreviewNode);
        bind_model!(set_inspector_nodes, inspector_nodes, InspectorNode);
        bind_model!(set_property_rows, property_rows, FieldRow);
        bind_model!(set_breadcrumbs, breadcrumbs, BreadcrumbItem);
        bind_model!(
            set_component_palette,
            component_palette,
            ComponentPaletteItem
        );
        bind_model!(set_tabs, tabs, TabItem);
        bind_model!(set_command_results, command_results, CommandItem);
        bind_model!(set_notifications, notifications, ToastItem);
        bind_model!(set_search_results, search_results, SearchResultItem);
        bind_model!(set_column_gutters, column_gutters, GutterRect);
        bind_model!(set_row_gutters, row_gutters, GutterRect);
        bind_model!(set_app_cards, app_cards, AppCardItem);
        bind_model!(set_explorer_nodes, explorer_nodes, ExplorerNodeItem);
        bind_model!(set_dock_panels, dock_panels, DockPanelRect);
        bind_model!(set_dock_dividers, dock_dividers, DockDividerRect);
        bind_model!(set_workflow_pages, workflow_pages, WorkflowPageItem);
        bind_model!(set_actions, actions, ButtonSpec);
        bind_model!(set_menu_defs, menu_defs, MenuDef);
        bind_model!(set_editor_lines, editor_lines, EditorLine);
        bind_model!(
            set_signal_connections,
            signal_connections,
            crate::SignalConnectionItem
        );
        bind_model!(set_signal_list, signal_list, crate::SignalItem);
        bind_model!(
            set_signal_target_nodes,
            signal_target_nodes,
            crate::TargetNodeItem
        );
        bind_model!(set_nav_pages, nav_pages, crate::NavPageItem);
        bind_model!(set_nav_graph_nodes, nav_graph_nodes, crate::NavGraphNode);
        bind_model!(set_nav_graph_edges, nav_graph_edges, crate::NavGraphEdge);
        bind_model!(set_schema_list, schema_list, crate::SchemaListItem);
        bind_model!(
            set_widget_toolbar_items,
            widget_toolbar,
            crate::WidgetToolbarItem
        );

        let inner = Rc::new(RefCell::new(ShellInner {
            store: Store::new(state),
            registry: Arc::new(registry),
            live: None,
            help,
            input,
            commands: CommandRegistry::with_builtins(),
            menus: crate::menu::MenuRegistry::with_builtins(),
            models,
            undo_past: Vec::new(),
            undo_future: Vec::new(),
            clipboard: None,
            help_pending_id: String::new(),
            help_active_id: String::new(),
            dock_area_dims: (0.0, 0.0),
            dock_dirty: Cell::new(true),
            syncing: Cell::new(false),
            sync_timer: Timer::default(),
            dock_check_timer: Timer::default(),
            drag_component_type: String::new(),
            drag_initial_transform: None,
            resize_initial: None,
            gap_resize_snapshot: None,
            pending_picker: None,
            pending_context_menu: None,
            vfs: VfsManager::new(),
            toast_timer: Timer::default(),
            user_color_swatches: Vec::new(),
            toggled_sections: std::collections::HashSet::new(),
            last_selected_node: None,
            nav_link_source: None,
            persistence: ProjectPersistence::new(),
            #[cfg(feature = "native")]
            collection: CollectionStore::new(),
            #[cfg(feature = "native")]
            project: None,
        }));
        {
            let mut s = inner.borrow_mut();
            s.load_active_page();
            let state = s.store.state();
            let panel_id = panel_id_for_slint(&state.workspace);
            let has_sel = !state.selection.is_empty();
            update_panel_schemes(&mut s.input, panel_id);
            s.input.set_context("hasSelection", has_sel);
            s.input.set_context("hasClipboard", false);
        }
        let shell = Self {
            inner,
            window,
            telemetry,
        };
        sync_ui_from_shared(&shell.inner, &shell.window);
        shell.wire_callbacks();

        // Re-sync after first frame so dock-area dimensions are available
        let deferred_inner = Rc::clone(&shell.inner);
        let deferred_weak = shell.window.as_weak();
        let dock_init_timer = Timer::default();
        dock_init_timer.start(
            TimerMode::SingleShot,
            std::time::Duration::from_millis(0),
            move || {
                if let Some(w) = deferred_weak.upgrade() {
                    let new_w = w.get_dock_area_width();
                    let new_h = w.get_dock_area_height();
                    deferred_inner.borrow_mut().dock_area_dims = (new_w, new_h);
                    sync_ui_from_shared(&deferred_inner, &w);
                }
            },
        );
        std::mem::forget(dock_init_timer);

        // Toast auto-dismiss: every second, remove toasts older than 5s
        {
            let toast_inner = Rc::clone(&shell.inner);
            let toast_weak = shell.window.as_weak();
            shell.inner.borrow().toast_timer.start(
                TimerMode::Repeated,
                std::time::Duration::from_secs(1),
                move || {
                    let expired = {
                        let s = toast_inner.borrow();
                        let now = Instant::now();
                        s.store.state().toasts.iter().any(|t| {
                            t.created_at
                                .is_some_and(|c| now.duration_since(c).as_secs() >= 5)
                        })
                    };
                    if expired {
                        {
                            let now = Instant::now();
                            toast_inner.borrow_mut().store.mutate(|state| {
                                state.toasts.retain(|t| {
                                    t.created_at
                                        .is_none_or(|c| now.duration_since(c).as_secs() < 5)
                                });
                            });
                        }
                        if let Some(w) = toast_weak.upgrade() {
                            sync_ui_from_shared(&toast_inner, &w);
                        }
                    }
                },
            );
        }

        Ok(shell)
    }

    pub fn state(&self) -> AppState {
        self.inner.borrow().store.state().clone()
    }

    pub fn registry(&self) -> Arc<ComponentRegistry> {
        Arc::clone(&self.inner.borrow().registry)
    }

    pub fn window(&self) -> &AppWindow {
        &self.window
    }

    pub fn telemetry(&self) -> FirstPaint {
        self.telemetry.clone()
    }

    #[cfg(feature = "native")]
    pub fn with_collection<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&mut CollectionStore) -> R,
    {
        let mut inner = self.inner.borrow_mut();
        if let Some(ref mut proj) = inner.project {
            f(proj.collection())
        } else {
            f(&mut inner.collection)
        }
    }

    #[cfg(feature = "native")]
    pub fn open_project(
        &self,
        path: impl Into<std::path::PathBuf>,
    ) -> Result<(), crate::project::ProjectError> {
        let mut proj = crate::project::ProjectManager::open(path)?;
        let mut inner = self.inner.borrow_mut();
        let objects = proj.collection().list_objects(None);
        for obj in &objects {
            let _ = inner.collection.put_object(obj);
        }
        let edges = proj.collection().list_edges(None);
        for edge in &edges {
            let _ = inner.collection.put_edge(edge);
        }
        inner.project = Some(proj);
        Ok(())
    }

    #[cfg(feature = "native")]
    pub fn close_project(&self) {
        let mut inner = self.inner.borrow_mut();
        inner.project = None;
    }

    #[cfg(feature = "native")]
    pub fn save_project(&self) -> Result<Vec<String>, crate::project::ProjectError> {
        let mut inner = self.inner.borrow_mut();
        if let Some(ref mut proj) = inner.project {
            proj.save()
        } else {
            Ok(Vec::new())
        }
    }

    #[cfg(feature = "native")]
    pub fn has_project(&self) -> bool {
        self.inner.borrow().project.is_some()
    }

    pub fn select_page(&self, page_id: &str) {
        let pid = page_id.to_string();
        {
            let mut s = self.inner.borrow_mut();
            s.store.mutate(|state| {
                state.workspace.switch_page_by_id(&pid);
            });
            s.dock_dirty.set(true);
        }
        sync_ui_from_shared(&self.inner, &self.window);
    }

    pub fn subscribe<F>(&self, listener: F) -> Subscription
    where
        F: FnMut(&AppState) + 'static,
    {
        self.inner.borrow_mut().store.subscribe(listener)
    }

    pub fn unsubscribe(&self, subscription: Subscription) {
        self.inner.borrow_mut().store.unsubscribe(subscription);
    }

    pub fn snapshot(&self) -> Result<Vec<u8>, serde_json::Error> {
        self.inner.borrow().store.snapshot()
    }

    pub fn restore(&self, bytes: &[u8]) -> Result<(), serde_json::Error> {
        {
            let mut inner = self.inner.borrow_mut();
            inner.store.restore(bytes)?;
            inner.live = None;
        }
        sync_ui_from_shared(&self.inner, &self.window);
        Ok(())
    }

    pub fn load_project_file(&self, path: &std::path::Path) -> Result<(), String> {
        let mut s = self.inner.borrow_mut();
        match s.persistence.open_path(path) {
            Ok(apps) => {
                let name = s
                    .persistence
                    .project_name()
                    .unwrap_or_else(|| "project".into());
                s.store.mutate(|state| {
                    state.apps = apps;
                    state.shell_view = ShellView::Launchpad;
                    state.selection.clear();
                });
                s.live = None;
                s.undo_past.clear();
                s.undo_future.clear();
                s.add_toast("Opened", &format!("Loaded {name}"), "success");
                drop(s);
                sync_ui_from_shared(&self.inner, &self.window);
                Ok(())
            }
            Err(e) => Err(e.to_string()),
        }
    }

    pub fn add_notification(&self, title: &str, body: &str, kind: &str) {
        self.inner.borrow_mut().add_toast(title, body, kind);
        sync_ui_from_shared(&self.inner, &self.window);
    }

    pub fn run(self) -> Result<(), slint::PlatformError> {
        let telemetry = self.telemetry.clone();
        let already_logged = Rc::new(std::cell::Cell::new(false));
        if let Err(err) = self.window.window().set_rendering_notifier({
            let telemetry = telemetry.clone();
            move |state, _api| {
                if matches!(state, slint::RenderingState::AfterRendering)
                    && !telemetry.is_recorded()
                {
                    telemetry.record_first_paint();
                    if !already_logged.replace(true) {
                        if let Some(d) = telemetry.duration() {
                            eprintln!("prism-shell: first-paint {}ms", d.as_millis());
                        }
                    }
                }
            }
        }) {
            eprintln!("prism-shell: first-paint telemetry unavailable on this backend: {err}");
        }

        let close_inner = Rc::clone(&self.inner);
        self.window.window().on_close_requested(move || {
            let is_dirty = close_inner.borrow().persistence.is_dirty();
            if is_dirty && !crate::persistence::confirm_discard_changes() {
                slint::CloseRequestResponse::KeepWindowShown
            } else {
                slint::CloseRequestResponse::HideWindow
            }
        });

        self.window.run()
    }

    fn wire_callbacks(&self) {
        let weak = self.window.as_weak();
        let inner = Rc::clone(&self.inner);

        // Unified key dispatch — replaces hardcoded Slint FocusScope
        self.window.on_dispatch_key({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |text, ctrl, shift, alt, meta| -> bool {
                let cmd_id = {
                    let combo = combo_from_slint(&text, ctrl, shift, alt, meta);
                    match combo {
                        Some(ref c) => inner.borrow().input.dispatch(c).map(String::from),
                        None => None,
                    }
                };
                if let Some(cmd_id) = cmd_id {
                    execute_command(&inner, &weak, &cmd_id);
                    true
                } else {
                    false
                }
            }
        });

        // Menu bar command dispatch
        self.window.on_menu_command({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |cmd_id| {
                execute_command(&inner, &weak, &cmd_id);
            }
        });

        // Panel selection (sidebar + activity bar)
        self.window.on_select_panel({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |id| {
                {
                    let page_id = match id {
                        0 => "edit",
                        2 => "code",
                        3 => "edit",
                        _ => "edit",
                    };
                    let mut s = inner.borrow_mut();
                    let pid = page_id.to_string();
                    s.store.mutate(|state| {
                        state.workspace.switch_page_by_id(&pid);
                    });
                    s.dock_dirty.set(true);
                    let panel_id = panel_id_for_slint(&s.store.state().workspace);
                    update_panel_schemes(&mut s.input, panel_id);
                }
                if let Some(w) = weak.upgrade() {
                    sync_ui_from_shared(&inner, &w);
                }
            }
        });

        // Dock: workflow page clicked
        self.window.on_workflow_page_clicked({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |page_id| {
                {
                    let mut s = inner.borrow_mut();
                    let pid = page_id.to_string();
                    let was_preview = is_preview_mode(&s.store.state().workspace);
                    s.store.mutate(|state| {
                        state.workspace.switch_page_by_id(&pid);
                    });
                    if was_preview && !is_preview_mode(&s.store.state().workspace) {
                        s.store.mutate(|state| {
                            state.runtime_overrides.clear();
                        });
                        s.sync_builder_document();
                    }
                    s.dock_dirty.set(true);
                    let panel_id = panel_id_for_slint(&s.store.state().workspace);
                    update_panel_schemes(&mut s.input, panel_id);
                }
                if let Some(w) = weak.upgrade() {
                    sync_ui_from_shared(&inner, &w);
                }
            }
        });

        // Dock: tab clicked within a panel group
        self.window.on_dock_tab_clicked({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |addr_key, tab_index| {
                {
                    let addr = deserialize_addr(&addr_key);
                    let mut s = inner.borrow_mut();
                    s.store.mutate(|state| {
                        state
                            .workspace
                            .active_dock_mut()
                            .activate_tab(&addr, tab_index as usize);
                    });
                    s.dock_dirty.set(true);
                }
                if let Some(w) = weak.upgrade() {
                    sync_ui_from_shared(&inner, &w);
                }
            }
        });

        // Dock: divider dragged (ratio update)
        self.window.on_dock_divider_dragged({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |addr_key, new_ratio| {
                {
                    let addr = deserialize_addr(&addr_key);
                    let mut s = inner.borrow_mut();
                    s.store.mutate(|state| {
                        state
                            .workspace
                            .active_dock_mut()
                            .set_ratio(&addr, new_ratio);
                    });
                    s.dock_dirty.set(true);
                }
                if let Some(w) = weak.upgrade() {
                    sync_ui_from_shared(&inner, &w);
                }
            }
        });

        // Builder node clicked — select in edit mode, fire signal in preview mode
        self.window.on_builder_node_clicked({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |node_id| {
                eprintln!("[click] builder_node_clicked id={node_id}");
                {
                    let mut s = inner.borrow_mut();
                    let nid = node_id.to_string();
                    s.store.mutate(|state| {
                        state.selection.select(nid.clone());
                    });
                    if is_preview_mode(&s.store.state().workspace) {
                        s.fire_signal(&nid, "clicked", serde_json::Map::new());
                    } else if let Some(ref live) = s.live {
                        if let Some(sel) = live.select_node(&nid) {
                            s.store.mutate(|state| {
                                state
                                    .editor_state
                                    .set_cursor_position(sel.start_line, sel.start_col);
                                state
                                    .editor_state
                                    .extend_selection_to(sel.end_line, sel.end_col);
                            });
                        }
                    }
                }
                if let Some(w) = weak.upgrade() {
                    sync_ui_from_shared(&inner, &w);
                }
            }
        });

        // Builder node double-clicked — fire double-clicked signal in preview mode
        self.window.on_builder_node_double_clicked({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |node_id| {
                {
                    let nid = node_id.to_string();
                    let mut s = inner.borrow_mut();
                    if is_preview_mode(&s.store.state().workspace) {
                        s.fire_signal(&nid, "double-clicked", serde_json::Map::new());
                    }
                }
                if let Some(w) = weak.upgrade() {
                    sync_ui_from_shared(&inner, &w);
                }
            }
        });

        // Builder node hovered — fire hovered signal (preview mode only)
        self.window.on_builder_node_hovered({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |node_id, x, y| {
                let dispatched = {
                    let nid = node_id.to_string();
                    let mut s = inner.borrow_mut();
                    s.fire_signal(&nid, "hovered", {
                        let mut p = serde_json::Map::new();
                        p.insert("x".into(), serde_json::Value::from(x as f64));
                        p.insert("y".into(), serde_json::Value::from(y as f64));
                        p
                    })
                };
                if dispatched {
                    if let Some(w) = weak.upgrade() {
                        sync_ui_from_shared(&inner, &w);
                    }
                }
            }
        });

        self.window.on_builder_node_hover_ended({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |node_id| {
                let dispatched = {
                    let nid = node_id.to_string();
                    let mut s = inner.borrow_mut();
                    s.fire_signal(&nid, "hover-ended", serde_json::Map::new())
                };
                if dispatched {
                    if let Some(w) = weak.upgrade() {
                        sync_ui_from_shared(&inner, &w);
                    }
                }
            }
        });

        // Inline text editing in builder
        self.window.on_builder_text_edited({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |node_id, value| {
                if is_preview_mode(&inner.borrow().store.state().workspace) {
                    return;
                }
                let node_id = node_id.to_string();
                let value = value.to_string();
                {
                    let mut s = inner.borrow_mut();
                    s.push_undo("Edit text");
                    let key = {
                        let component = s.live.as_mut().and_then(|l| {
                            let doc = l.document();
                            doc.root
                                .as_ref()
                                .and_then(|r| r.find(&node_id).map(|n| n.component.clone()))
                        });
                        match component.as_deref() {
                            Some("text") => "body",
                            Some("card") | Some("accordion") => "title",
                            Some("code") => "code",
                            _ => "text",
                        }
                    };
                    let formatted = format!(
                        "\"{}\"",
                        prism_builder::slint_source::escape_slint_string(&value)
                    );
                    if let Some(ref mut live) = s.live {
                        let _ = live.edit_prop_in_source(&node_id, key, &formatted);
                    }
                    s.sync_builder_document();
                }
                if let Some(w) = weak.upgrade() {
                    sync_ui_from_shared(&inner, &w);
                }
            }
        });

        // Delete node from builder
        self.window.on_builder_delete_node({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |node_id| {
                if is_preview_mode(&inner.borrow().store.state().workspace) {
                    return;
                }
                {
                    let mut s = inner.borrow_mut();
                    let nid = if node_id.is_empty() {
                        match s.store.state().selection.primary().cloned() {
                            Some(id) => id,
                            None => return,
                        }
                    } else {
                        node_id.to_string()
                    };
                    s.fire_signal(&nid, "deleted", serde_json::Map::new());
                    s.push_undo("Delete node");
                    if let Some(ref mut live) = s.live {
                        let _ = live.remove_node_from_source(&nid);
                    }
                    s.store.mutate(|state| {
                        state.selection.clear();
                    });
                    s.sync_builder_document();
                }
                if let Some(w) = weak.upgrade() {
                    sync_ui_from_shared(&inner, &w);
                }
            }
        });

        // Add component from palette
        self.window.on_add_component({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |component_type| {
                if is_preview_mode(&inner.borrow().store.state().workspace) {
                    return;
                }
                let ct = component_type.to_string();
                {
                    let mut s = inner.borrow_mut();
                    s.push_undo(&format!("Add {ct}"));
                    let parent_id = s.store.state().selection.primary().cloned();

                    // Look up user-defined prefabs from the live document.
                    let user_prefab = s.store.state().builder_document.prefabs.get(&ct).cloned();
                    let prefab_def = builtin_prefab(&ct).or(user_prefab);

                    let mounted_id;
                    if let Some(prefab_def) = prefab_def {
                        let mut counter = s.store.state().next_node_id;
                        let tree = materialize_prefab(&prefab_def, &mut counter);
                        let root_id = tree.id.clone();
                        if let Some(ref mut live) = s.live {
                            let _ = live.insert_tree_in_source(parent_id.as_deref(), &tree, None);
                        }
                        mounted_id = root_id.clone();
                        s.store.mutate(|state| {
                            state.next_node_id = counter;
                            state.selection.select(root_id);
                        });
                    } else if ct == "facet" {
                        let counter = s.store.state().next_node_id;
                        let facet_id = format!("facet:n{counter}");
                        let node_id = format!("n{counter}");
                        let props = json!({ "facet_id": facet_id });
                        if let Some(ref mut live) = s.live {
                            let _ = live.insert_node_in_source(
                                parent_id.as_deref(),
                                &ct,
                                &node_id,
                                &props,
                                None,
                            );
                        }
                        mounted_id = node_id.clone();
                        let nid = node_id.clone();
                        s.store.mutate(|state| {
                            state.next_node_id += 1;
                            state.selection.select(nid);
                            if let Some(doc) =
                                state.active_app_mut().and_then(|a| a.active_document_mut())
                            {
                                // Ensure "card" prefab is available for facet rendering.
                                doc.prefabs
                                    .entry("card".into())
                                    .or_insert_with(card_prefab_def);
                                doc.facets.insert(
                                    facet_id.clone(),
                                    FacetDef {
                                        id: facet_id.clone(),
                                        label: "New Facet".into(),
                                        description: String::new(),
                                        kind: FacetKind::List,
                                        schema_id: None,
                                        template: FacetTemplate::default(),
                                        output: FacetOutput::default(),
                                        data: FacetDataSource::Static {
                                            items: vec![],
                                            records: vec![],
                                        },
                                        bindings: vec![],
                                        variant_rules: vec![],
                                        layout: FacetLayout::default(),
                                        resolved_data: None,
                                    },
                                );
                            }
                        });
                    } else {
                        let node_id = format!("n{}", s.store.state().next_node_id);
                        let props = default_props_for_component(&ct);
                        if let Some(ref mut live) = s.live {
                            let _ = live.insert_node_in_source(
                                parent_id.as_deref(),
                                &ct,
                                &node_id,
                                &props,
                                None,
                            );
                        }
                        mounted_id = node_id.clone();
                        let nid = node_id.clone();
                        s.store.mutate(|state| {
                            state.next_node_id += 1;
                            state.selection.select(nid);
                        });
                    }
                    s.sync_builder_document();
                    s.fire_signal(&mounted_id, "mounted", serde_json::Map::new());
                }
                if let Some(w) = weak.upgrade() {
                    sync_ui_from_shared(&inner, &w);
                }
            }
        });

        // File browse button — opens native file dialog, imports into VFS
        self.window.on_file_browse_requested({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |key| {
                let key = key.to_string();
                let picked = {
                    #[cfg(feature = "native")]
                    {
                        rfd::FileDialog::new()
                            .add_filter(
                                "Images",
                                &["png", "jpg", "jpeg", "gif", "webp", "svg", "bmp", "ico"],
                            )
                            .add_filter("All files", &["*"])
                            .pick_file()
                    }
                    #[cfg(not(feature = "native"))]
                    {
                        None::<std::path::PathBuf>
                    }
                };
                if let Some(path) = picked {
                    let bytes = match std::fs::read(&path) {
                        Ok(b) => b,
                        Err(e) => {
                            let mut s = inner.borrow_mut();
                            s.add_toast("Import failed", &format!("{e}"), "error");
                            if let Some(w) = weak.upgrade() {
                                sync_ui_from_shared(&inner, &w);
                            }
                            return;
                        }
                    };
                    let filename = path
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_string();
                    let mime = mime_from_extension(
                        path.extension().and_then(|e| e.to_str()).unwrap_or(""),
                    );
                    let bref = {
                        let s = inner.borrow();
                        s.vfs.import_file(&bytes, &filename, mime)
                    };
                    let asset = AssetSource::Vfs {
                        hash: bref.hash,
                        filename: bref.filename,
                        mime_type: bref.mime_type,
                        size: bref.size,
                    };
                    let json_str = serde_json::to_string(&asset.to_prop()).unwrap_or_default();
                    let formatted = format!(
                        "\"{}\"",
                        prism_builder::slint_source::escape_slint_string(&json_str)
                    );
                    {
                        let mut s = inner.borrow_mut();
                        let selected_id = s.store.state().selection.primary().cloned();
                        if let Some(ref target_id) = selected_id {
                            s.push_undo(&format!("Set {key}"));
                            if let Some(ref mut live) = s.live {
                                let _ = live.edit_prop_in_source(target_id, &key, &formatted);
                            }
                            s.sync_builder_document();
                        }
                        s.add_toast("Imported", &filename, "success");
                    }
                }
                if let Some(w) = weak.upgrade() {
                    sync_ui_from_shared(&inner, &w);
                }
            }
        });

        // Unified property field editing (routes by key prefix)
        self.window.on_property_field_edited({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |key, value| {
                if is_preview_mode(&inner.borrow().store.state().workspace) {
                    return;
                }
                eprintln!(
                    "[property-edit] key={key:?} value={value:?} syncing={}",
                    inner.borrow().syncing.get()
                );
                if inner.borrow().syncing.get() {
                    return;
                }
                let key = key.to_string();
                let value = value.to_string();
                {
                    let mut s = inner.borrow_mut();
                    let selected_id = s.store.state().selection.primary().cloned();
                    if key.starts_with("layout.") {
                        if let Some(ref target_id) = selected_id {
                            s.push_undo(&format!("Edit {key}"));
                            let tid = target_id.clone();
                            if let Some(ref mut live) = s.live {
                                let _ = live.mutate_document(|doc| {
                                    if let Some(ref mut root) = doc.root {
                                        apply_node_layout_edit(root, &tid, &key, &value);
                                    }
                                });
                            }
                            s.sync_builder_document();
                        }
                    } else if key.starts_with("transform.") {
                        if let Some(ref target_id) = selected_id {
                            s.push_undo(&format!("Edit {key}"));
                            let tid = target_id.clone();
                            if let Some(ref mut live) = s.live {
                                let _ = live.mutate_document(|doc| {
                                    if let Some(ref mut root) = doc.root {
                                        apply_node_transform_edit(root, &tid, &key, &value);
                                    }
                                });
                            }
                            s.sync_builder_document();
                        }
                    } else if key.starts_with("style.") || key.starts_with("inherited.style.") {
                        let style_key = key
                            .strip_prefix("inherited.style.")
                            .or_else(|| key.strip_prefix("style."))
                            .unwrap_or(&key);
                        let sk = style_key.to_string();
                        s.push_undo(&format!("Edit style {key}"));
                        if let Some(ref target_id) = selected_id {
                            let tid = target_id.clone();
                            if let Some(ref mut live) = s.live {
                                let _ = live.mutate_document(|doc| {
                                    if let Some(ref mut root) = doc.root {
                                        if let Some(node) = root.find_mut(&tid) {
                                            apply_style_edit(&mut node.style, &sk, &value);
                                        }
                                    }
                                });
                            }
                            s.sync_builder_document();
                        } else {
                            s.store.mutate(|state| {
                                if let Some(app) = state.active_app_mut() {
                                    if let Some(page) = app.pages.get_mut(app.active_page) {
                                        apply_style_edit(&mut page.style, &sk, &value);
                                    }
                                }
                            });
                        }
                    } else if key.starts_with("facet.") {
                        if let Some(ref target_id) = selected_id {
                            let facet_id = s
                                .store
                                .state()
                                .builder_document
                                .root
                                .as_ref()
                                .and_then(|r| r.find(target_id))
                                .and_then(|n| n.props.get("facet_id"))
                                .and_then(|v| v.as_str())
                                .map(|s| s.to_string());
                            if let Some(fid) = facet_id {
                                s.push_undo(&format!("Edit {key}"));
                                let fkey = key.strip_prefix("facet.").unwrap_or(&key).to_string();
                                let val = value.clone();
                                s.store.mutate(|state| {
                                    if let Some(doc) =
                                        state.active_app_mut().and_then(|a| a.active_document_mut())
                                    {
                                        if let Some(def) = doc.facets.get_mut(&fid) {
                                            apply_facet_edit(def, &fkey, &val);
                                        }
                                    }
                                });
                                s.sync_builder_document();
                            }
                        }
                    } else if key.starts_with("schema.") {
                        let skey = key.strip_prefix("schema.").unwrap_or(&key).to_string();
                        let val = value.clone();
                        s.push_undo(&format!("Edit {key}"));
                        s.store.mutate(|state| {
                            let sid = resolve_schema_id(state);
                            if let Some(sid) = sid {
                                if let Some(doc) =
                                    state.active_app_mut().and_then(|a| a.active_document_mut())
                                {
                                    if let Some(schema) = doc.facet_schemas.get_mut(&sid) {
                                        crate::panels::schema::apply_schema_edit(
                                            schema, &skey, &val,
                                        );
                                    }
                                }
                            }
                        });
                        s.sync_builder_document();
                    } else if let Some(ref target_id) = selected_id {
                        let kind = field_kind_for_key(&s, &key);
                        let (source_key, formatted) =
                            slint_source_key_for_edit(&s, &key, &value, kind.as_deref());
                        s.push_undo(&format!("Edit {key}"));
                        if let Some(ref mut live) = s.live {
                            let _ = live.edit_prop_in_source(target_id, &source_key, &formatted);
                        }
                        s.sync_builder_document();
                        s.fire_signal(target_id, "changed", {
                            let mut p = serde_json::Map::new();
                            p.insert("key".into(), serde_json::Value::from(key.as_str()));
                            p.insert("value".into(), serde_json::Value::from(value.as_str()));
                            p
                        });
                    }
                }
                if let Some(w) = weak.upgrade() {
                    sync_ui_from_shared(&inner, &w);
                }
            }
        });

        // Unified property numeric editing (routes by key prefix)
        self.window.on_property_field_edited_number({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |key, val| {
                if is_preview_mode(&inner.borrow().store.state().workspace) {
                    return;
                }
                eprintln!(
                    "[property-edit-number] key={key:?} val={val} syncing={}",
                    inner.borrow().syncing.get()
                );
                if inner.borrow().syncing.get() {
                    return;
                }
                let key = key.to_string();
                let value = format_slider_value(val);
                eprintln!("[property-edit-number] formatted value={value:?} for key={key:?}");
                {
                    let mut s = inner.borrow_mut();
                    let selected_id = s.store.state().selection.primary().cloned();
                    eprintln!("[property-edit-number] selected_id={selected_id:?}");
                    if key.starts_with("layout.") {
                        if let Some(ref target_id) = selected_id {
                            s.push_undo(&format!("Edit {key}"));
                            let tid = target_id.clone();
                            if let Some(ref mut live) = s.live {
                                let _ = live.mutate_document(|doc| {
                                    if let Some(ref mut root) = doc.root {
                                        apply_node_layout_edit(root, &tid, &key, &value);
                                    }
                                });
                            }
                            s.sync_builder_document();
                        }
                    } else if key.starts_with("transform.") {
                        if let Some(ref target_id) = selected_id {
                            s.push_undo(&format!("Edit {key}"));
                            let tid = target_id.clone();
                            if let Some(ref mut live) = s.live {
                                let res = live.mutate_document(|doc| {
                                    if let Some(ref mut root) = doc.root {
                                        apply_node_transform_edit(root, &tid, &key, &value);
                                    }
                                });
                                eprintln!("[transform-edit-num] mutate result={res:?}");
                            }
                            s.sync_builder_document();
                            if let Some(node) = s
                                .store
                                .state()
                                .builder_document
                                .root
                                .as_ref()
                                .and_then(|r| r.find(&tid))
                            {
                                eprintln!(
                                    "[transform-edit-num] post-sync pos={:?} rot={:.3} scale={:?}",
                                    node.transform.position,
                                    node.transform.rotation,
                                    node.transform.scale
                                );
                            } else {
                                eprintln!("[transform-edit-num] node {tid} NOT FOUND after sync");
                            }
                        }
                    } else if key.starts_with("style.") || key.starts_with("inherited.style.") {
                        let style_key = key
                            .strip_prefix("inherited.style.")
                            .or_else(|| key.strip_prefix("style."))
                            .unwrap_or(&key);
                        let sk = style_key.to_string();
                        s.push_undo(&format!("Edit style {key}"));
                        if let Some(ref target_id) = selected_id {
                            let tid = target_id.clone();
                            if let Some(ref mut live) = s.live {
                                let _ = live.mutate_document(|doc| {
                                    if let Some(ref mut root) = doc.root {
                                        if let Some(node) = root.find_mut(&tid) {
                                            apply_style_edit(&mut node.style, &sk, &value);
                                        }
                                    }
                                });
                            }
                            s.sync_builder_document();
                        } else {
                            s.store.mutate(|state| {
                                if let Some(app) = state.active_app_mut() {
                                    if let Some(page) = app.pages.get_mut(app.active_page) {
                                        apply_style_edit(&mut page.style, &sk, &value);
                                    }
                                }
                            });
                        }
                    } else if key.starts_with("facet.") {
                        if let Some(ref target_id) = selected_id {
                            let facet_id = s
                                .store
                                .state()
                                .builder_document
                                .root
                                .as_ref()
                                .and_then(|r| r.find(target_id))
                                .and_then(|n| n.props.get("facet_id"))
                                .and_then(|v| v.as_str())
                                .map(|s| s.to_string());
                            if let Some(fid) = facet_id {
                                s.push_undo(&format!("Edit {key}"));
                                let fkey = key.strip_prefix("facet.").unwrap_or(&key).to_string();
                                s.store.mutate(|state| {
                                    if let Some(doc) =
                                        state.active_app_mut().and_then(|a| a.active_document_mut())
                                    {
                                        if let Some(def) = doc.facets.get_mut(&fid) {
                                            apply_facet_edit(def, &fkey, &format!("{val}"));
                                        }
                                    }
                                });
                                s.sync_builder_document();
                            }
                        }
                    } else if key.starts_with("schema.") {
                        let skey = key.strip_prefix("schema.").unwrap_or(&key).to_string();
                        s.push_undo(&format!("Edit {key}"));
                        s.store.mutate(|state| {
                            let sid = resolve_schema_id(state);
                            if let Some(sid) = sid {
                                if let Some(doc) =
                                    state.active_app_mut().and_then(|a| a.active_document_mut())
                                {
                                    if let Some(schema) = doc.facet_schemas.get_mut(&sid) {
                                        crate::panels::schema::apply_schema_edit(
                                            schema,
                                            &skey,
                                            &format_slider_value(val),
                                        );
                                    }
                                }
                            }
                        });
                        s.sync_builder_document();
                    } else {
                        if let Some(ref target_id) = selected_id {
                            let kind = field_kind_for_key(&s, &key);
                            let formatted = match kind.as_deref() {
                                Some("integer") => format!("{}", val as i64),
                                Some("number") => format!("{}px", format_slider_value(val)),
                                _ => format_slider_value(val),
                            };
                            s.push_undo(&format!("Edit {key}"));
                            if let Some(ref mut live) = s.live {
                                let _ = live.edit_prop_in_source(target_id, &key, &formatted);
                            }
                            s.sync_builder_document();
                        }
                    }
                }
                if let Some(w) = weak.upgrade() {
                    sync_ui_from_shared(&inner, &w);
                }
            }
        });

        // Section toggle (manual override of auto-collapse)
        self.window.on_property_section_toggled({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |section_id| {
                let section_id = section_id.to_string();
                {
                    let mut s = inner.borrow_mut();
                    if s.toggled_sections.contains(&section_id) {
                        s.toggled_sections.remove(&section_id);
                    } else {
                        s.toggled_sections.insert(section_id);
                    }
                }
                if let Some(w) = weak.upgrade() {
                    sync_ui_from_shared(&inner, &w);
                }
            }
        });

        // Inspector node selection
        self.window.on_node_clicked({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |node_id| {
                inner.borrow_mut().store.mutate(|state| {
                    state.selection.select(node_id.to_string());
                });
                if let Some(w) = weak.upgrade() {
                    sync_ui_from_shared(&inner, &w);
                }
            }
        });

        // Node reordering
        fn handle_node_move(
            inner: &Rc<RefCell<ShellInner>>,
            weak: &slint::Weak<AppWindow>,
            node_id: &slint::SharedString,
            direction: i32,
            label: &str,
        ) {
            if is_preview_mode(&inner.borrow().store.state().workspace) {
                return;
            }
            {
                let mut s = inner.borrow_mut();
                let nid = if node_id.is_empty() {
                    match s.store.state().selection.primary().cloned() {
                        Some(id) => id,
                        None => return,
                    }
                } else {
                    node_id.to_string()
                };
                s.push_undo(label);
                if let Some(ref mut live) = s.live {
                    let _ = live.move_node_in_source(&nid, direction);
                }
                s.sync_builder_document();
            }
            if let Some(w) = weak.upgrade() {
                sync_ui_from_shared(inner, &w);
            }
        }
        self.window.on_node_move_up({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |node_id| handle_node_move(&inner, &weak, &node_id, -1, "Move node up")
        });
        self.window.on_node_move_down({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |node_id| handle_node_move(&inner, &weak, &node_id, 1, "Move node down")
        });

        // App navigation: open app from launchpad
        self.window.on_open_app({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |app_id| {
                let app_id = app_id.to_string();
                {
                    let mut s = inner.borrow_mut();
                    s.store.mutate(|state| {
                        state.shell_view = ShellView::App {
                            app_id: app_id.clone(),
                        };
                        state.workspace.switch_page_by_id("edit");
                        state.selection.clear();
                        state.sync_document_from_app();
                    });
                    s.load_active_page();
                    s.dock_dirty.set(true);
                }
                if let Some(w) = weak.upgrade() {
                    sync_ui_from_shared(&inner, &w);
                }
            }
        });

        // App navigation: go back to launchpad
        self.window.on_go_home({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move || {
                {
                    let mut s = inner.borrow_mut();
                    s.save_to_active_page();
                    s.store.mutate(|state| {
                        state.shell_view = ShellView::Launchpad;
                        state.selection.clear();
                    });
                    s.live = None;
                    s.dock_dirty.set(true);
                }
                if let Some(w) = weak.upgrade() {
                    sync_ui_from_shared(&inner, &w);
                }
            }
        });

        // Explorer: node clicked — navigate to app/page
        self.window.on_explorer_node_clicked({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |node_id| {
                let nid = node_id.to_string();
                {
                    let mut s = inner.borrow_mut();
                    if let Some(app_id) = nid.strip_prefix("app:") {
                        s.save_to_active_page();
                        s.store.mutate(|state| {
                            state.shell_view = ShellView::App {
                                app_id: app_id.into(),
                            };
                            state.workspace.switch_page_by_id("edit");
                            state.selection.clear();
                            state.sync_document_from_app();
                        });
                        s.load_active_page();
                        s.dock_dirty.set(true);
                    } else if let Some(rest) = nid.strip_prefix("page:") {
                        let parts: Vec<&str> = rest.splitn(2, ':').collect();
                        if parts.len() == 2 {
                            let aid = parts[0].to_string();
                            let pid = parts[1].to_string();
                            s.save_to_active_page();
                            s.store.mutate(|state| {
                                state.shell_view = ShellView::App { app_id: aid };
                                if let Some(app) = state.active_app_mut() {
                                    if let Some(idx) = app.pages.iter().position(|p| p.id == pid) {
                                        app.active_page = idx;
                                    }
                                }
                                state.selection.clear();
                                state.sync_document_from_app();
                            });
                            s.load_active_page();
                            s.dock_dirty.set(true);
                        }
                    }
                }
                if let Some(w) = weak.upgrade() {
                    sync_ui_from_shared(&inner, &w);
                }
            }
        });

        // Explorer: toggle expand/collapse
        self.window.on_explorer_toggle_expand({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |node_id| {
                let nid = node_id.to_string();
                inner.borrow_mut().store.mutate(|state| {
                    if state.explorer_expanded.contains(&nid) {
                        state.explorer_expanded.remove(&nid);
                    } else {
                        state.explorer_expanded.insert(nid);
                    }
                });
                if let Some(w) = weak.upgrade() {
                    sync_ui_from_shared(&inner, &w);
                }
            }
        });

        // App navigation: create new app
        self.window.on_create_app({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move || {
                let new_app_id;
                {
                    let mut s = inner.borrow_mut();
                    s.save_to_active_page();
                    let id_num = s.store.state().next_app_id;
                    new_app_id = format!("app-{id_num}");
                    let naid = new_app_id.clone();
                    s.store.mutate(|state| {
                        state.next_app_id += 1;
                        state.apps.push(PrismApp {
                            id: naid.clone(),
                            name: format!("App {id_num}"),
                            description: "A new Prism app.".into(),
                            icon: AppIcon::Cube,
                            pages: vec![Page {
                                id: "page-1".into(),
                                title: "Home".into(),
                                route: "/".into(),
                                source: String::new(),
                                document: BuilderDocument::page_shell(),
                                style: StyleProperties::default(),
                            }],
                            active_page: 0,
                            navigation: NavigationConfig::default(),
                            style: StyleProperties::default(),
                        });
                        state.shell_view = ShellView::App { app_id: naid };
                        state.workspace.switch_page_by_id("edit");
                        state.selection.clear();
                        state.sync_document_from_app();
                    });
                    s.load_active_page();
                    s.dock_dirty.set(true);
                    s.add_toast(
                        "App created",
                        &format!("App {id_num} is ready to edit"),
                        "success",
                    );
                }
                if let Some(w) = weak.upgrade() {
                    sync_ui_from_shared(&inner, &w);
                }
            }
        });

        // Page navigation: switch page within an app
        self.window.on_switch_page({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |page_index| {
                inner.borrow_mut().switch_to_page(page_index as usize);
                if let Some(w) = weak.upgrade() {
                    sync_ui_from_shared(&inner, &w);
                }
            }
        });

        // Page navigation: add new page to active app
        self.window.on_add_page({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move || {
                execute_command(&inner, &weak, "add_page");
            }
        });

        // Command palette
        self.window.on_toggle_command_palette({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move || {
                execute_command(&inner, &weak, "command_palette.toggle");
            }
        });
        self.window.on_command_palette_input({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |query| {
                inner.borrow_mut().store.mutate(|state| {
                    state.command_palette_query = query.to_string();
                });
                if let Some(w) = weak.upgrade() {
                    sync_ui_from_shared(&inner, &w);
                }
            }
        });
        self.window.on_command_selected({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |index| {
                let cmd_id = {
                    let s = inner.borrow();
                    let query = &s.store.state().command_palette_query;
                    let results = s.commands.filter(query);
                    results.get(index as usize).map(|c| c.id.clone())
                };
                if let Some(cmd_id) = cmd_id {
                    execute_command(&inner, &weak, &cmd_id);
                }
            }
        });

        // Notifications
        self.window.on_dismiss_notification({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |toast_id| {
                inner.borrow_mut().store.mutate(|state| {
                    state.toasts.retain(|t| t.id != toast_id as u64);
                });
                if let Some(w) = weak.upgrade() {
                    sync_ui_from_shared(&inner, &w);
                }
            }
        });

        // Undo / redo
        self.window.on_undo_clicked({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move || {
                if is_preview_mode(&inner.borrow().store.state().workspace) {
                    return;
                }
                inner.borrow_mut().perform_undo();
                if let Some(w) = weak.upgrade() {
                    sync_ui_from_shared(&inner, &w);
                }
            }
        });
        self.window.on_redo_clicked({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move || {
                if is_preview_mode(&inner.borrow().store.state().workspace) {
                    return;
                }
                inner.borrow_mut().perform_redo();
                if let Some(w) = weak.upgrade() {
                    sync_ui_from_shared(&inner, &w);
                }
            }
        });

        // Escape (from editor FocusScope and other direct callers)
        self.window.on_escape_pressed({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move || {
                let cmd = if inner.borrow().store.state().command_palette_open {
                    "command_palette.close"
                } else {
                    "navigate.escape"
                };
                execute_command(&inner, &weak, cmd);
            }
        });

        // Search
        self.window.on_search_input({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |query| {
                inner.borrow_mut().store.mutate(|state| {
                    state.search_query = query.to_string();
                });
                if let Some(w) = weak.upgrade() {
                    sync_ui_from_shared(&inner, &w);
                }
            }
        });
        self.window.on_search_result_clicked({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |node_id| {
                inner.borrow_mut().store.mutate(|state| {
                    state.selection.select(node_id.to_string());
                });
                if let Some(w) = weak.upgrade() {
                    sync_ui_from_shared(&inner, &w);
                }
            }
        });

        self.window.on_search_focus({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move || {
                execute_command(&inner, &weak, "search.focus");
            }
        });

        // Help tooltip hover
        let show_timer = Rc::new(Timer::default());
        let hide_timer = Rc::new(Timer::default());

        self.window.on_help_hover({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            let show_timer = Rc::clone(&show_timer);
            let hide_timer = Rc::clone(&hide_timer);
            move |help_id, x, y| {
                let help_id = help_id.to_string();
                hide_timer.stop();

                if help_id == "__tooltip__" {
                    return;
                }

                let active_id = inner.borrow().help_active_id.clone();
                if help_id == active_id {
                    return;
                }

                inner.borrow_mut().help_pending_id = help_id.clone();

                let inner_show = Rc::clone(&inner);
                let weak_show = weak.clone();
                let hide_timer_show = Rc::clone(&hide_timer);
                let x_val = x;
                let y_val = y;
                show_timer.start(
                    TimerMode::SingleShot,
                    std::time::Duration::from_millis(380),
                    move || {
                        let pending = inner_show.borrow().help_pending_id.clone();
                        if pending.is_empty() {
                            return;
                        }
                        let (title, summary, has_docs) = {
                            let s = inner_show.borrow();
                            match s.help.get(&pending) {
                                Some(entry) => (
                                    entry.title.clone(),
                                    entry.summary.clone(),
                                    entry.doc_path.is_some(),
                                ),
                                None => return,
                            }
                        };
                        inner_show.borrow_mut().help_active_id = pending;
                        if let Some(w) = weak_show.upgrade() {
                            w.set_help_tooltip(HelpTooltipData {
                                visible: true,
                                title: SharedString::from(title),
                                summary: SharedString::from(summary),
                                has_docs,
                                tip_x: x_val,
                                tip_y: y_val,
                            });
                        }

                        // Auto-hide after 8 seconds of inactivity
                        let inner_autohide = Rc::clone(&inner_show);
                        let weak_autohide = weak_show.clone();
                        hide_timer_show.start(
                            TimerMode::SingleShot,
                            std::time::Duration::from_secs(8),
                            move || {
                                inner_autohide.borrow_mut().help_active_id.clear();
                                inner_autohide.borrow_mut().help_pending_id.clear();
                                if let Some(w) = weak_autohide.upgrade() {
                                    w.set_help_tooltip(HelpTooltipData {
                                        visible: false,
                                        title: SharedString::new(),
                                        summary: SharedString::new(),
                                        has_docs: false,
                                        tip_x: 0.0,
                                        tip_y: 0.0,
                                    });
                                }
                            },
                        );
                    },
                );
            }
        });

        self.window.on_help_leave({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            let show_timer = Rc::clone(&show_timer);
            let hide_timer = Rc::clone(&hide_timer);
            move || {
                show_timer.stop();
                inner.borrow_mut().help_pending_id.clear();

                let inner_hide = Rc::clone(&inner);
                let weak_hide = weak.clone();
                hide_timer.start(
                    TimerMode::SingleShot,
                    std::time::Duration::from_millis(120),
                    move || {
                        inner_hide.borrow_mut().help_active_id.clear();
                        if let Some(w) = weak_hide.upgrade() {
                            w.set_help_tooltip(HelpTooltipData {
                                visible: false,
                                title: SharedString::new(),
                                summary: SharedString::new(),
                                has_docs: false,
                                tip_x: 0.0,
                                tip_y: 0.0,
                            });
                        }
                    },
                );
            }
        });

        self.window.on_help_docs_clicked({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move || {
                open_docs_panel(&inner, &weak);
            }
        });

        self.window.on_help_entry_clicked({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move || {
                open_docs_panel(&inner, &weak);
            }
        });

        self.window.on_docs_open_full({
            let weak = weak.clone();
            move || {
                if let Some(w) = weak.upgrade() {
                    let sidebar = w.get_docs_panel();
                    w.set_docs_view(DocsPanelData {
                        visible: true,
                        help_id: sidebar.help_id.clone(),
                        title: sidebar.title.clone(),
                        summary: sidebar.summary.clone(),
                        body: sidebar.body.clone(),
                    });
                    w.set_docs_panel(DocsPanelData {
                        visible: false,
                        help_id: SharedString::new(),
                        title: SharedString::new(),
                        summary: SharedString::new(),
                        body: SharedString::new(),
                    });
                }
            }
        });

        self.window.on_docs_panel_close({
            let weak = weak.clone();
            move || {
                if let Some(w) = weak.upgrade() {
                    w.set_docs_panel(DocsPanelData {
                        visible: false,
                        help_id: SharedString::new(),
                        title: SharedString::new(),
                        summary: SharedString::new(),
                        body: SharedString::new(),
                    });
                }
            }
        });

        self.window.on_docs_view_close({
            let weak = weak.clone();
            move || {
                if let Some(w) = weak.upgrade() {
                    w.set_docs_view(DocsPanelData {
                        visible: false,
                        help_id: SharedString::new(),
                        title: SharedString::new(),
                        summary: SharedString::new(),
                        body: SharedString::new(),
                    });
                }
            }
        });

        // Breadcrumb navigation
        self.window.on_breadcrumb_clicked({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |node_id| {
                inner.borrow_mut().store.mutate(|state| {
                    state.selection.select(node_id.to_string());
                });
                if let Some(w) = weak.upgrade() {
                    sync_ui_from_shared(&inner, &w);
                }
            }
        });

        // Clipboard: copy
        self.window.on_copy_clicked({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move || {
                execute_command(&inner, &weak, "edit.copy");
            }
        });

        // Clipboard: paste
        self.window.on_paste_clicked({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move || {
                execute_command(&inner, &weak, "edit.paste");
            }
        });

        // Clipboard: cut
        self.window.on_cut_clicked({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move || {
                execute_command(&inner, &weak, "edit.cut");
            }
        });

        // Clipboard: duplicate
        self.window.on_duplicate_clicked({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move || {
                execute_command(&inner, &weak, "edit.duplicate");
            }
        });

        // Code editor action (special keys)
        self.window.on_editor_action({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |action| {
                let action = action.to_string();
                {
                    let mut s = inner.borrow_mut();
                    s.store.mutate(|state| {
                        state.editor_state.handle_action(&action);
                    });
                    let text = s.store.state().editor_state.text();
                    if let Some(ref mut live) = s.live {
                        let _ = live.set_source(text);
                    }
                    s.sync_builder_document();
                }
                if let Some(w) = weak.upgrade() {
                    sync_ui_from_shared(&inner, &w);
                }
            }
        });

        // Code editor character typed
        self.window.on_editor_char_typed({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |ch| {
                let ch = ch.to_string();
                if let Some(c) = ch.chars().next() {
                    if !c.is_control() {
                        {
                            let mut s = inner.borrow_mut();
                            s.store.mutate(|state| {
                                state.editor_state.insert_char(c);
                            });
                            let text = s.store.state().editor_state.text();
                            if let Some(ref mut live) = s.live {
                                let _ = live.set_source(text);
                            }
                            s.sync_builder_document();
                        }
                        if let Some(w) = weak.upgrade() {
                            sync_ui_from_shared(&inner, &w);
                        }
                    }
                }
            }
        });

        // Code editor mouse click (display row -> buffer line + select builder node)
        self.window.on_editor_click({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |display_row, col| {
                {
                    let mut s = inner.borrow_mut();
                    let buf_line = {
                        let state = s.store.state();
                        display_row_to_buffer_line(&state.editor_state, display_row as usize)
                    };
                    let col = col as usize;
                    s.store.mutate(|state| {
                        state.editor_state.set_cursor_position(buf_line, col);
                    });
                    if let Some(ref mut live) = s.live {
                        live.editor.set_cursor_position(buf_line, col);
                        if let Some(node_id) = live.node_at_cursor() {
                            let nid = node_id.to_string();
                            s.store.mutate(|state| {
                                state.selection.select(nid);
                            });
                        }
                    }
                }
                if let Some(w) = weak.upgrade() {
                    sync_ui_from_shared(&inner, &w);
                }
            }
        });

        // Code editor mouse drag (selection, display row -> buffer line)
        self.window.on_editor_drag({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |display_row, col| {
                inner.borrow_mut().store.mutate(|state| {
                    let buf_line =
                        display_row_to_buffer_line(&state.editor_state, display_row as usize);
                    state
                        .editor_state
                        .extend_selection_to(buf_line, col as usize);
                });
                if let Some(w) = weak.upgrade() {
                    sync_ui_from_shared(&inner, &w);
                }
            }
        });

        // Code editor fold toggle (gutter click)
        self.window.on_editor_fold_toggle({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |display_row| {
                inner.borrow_mut().store.mutate(|state| {
                    let buf_line =
                        display_row_to_buffer_line(&state.editor_state, display_row as usize);
                    state.editor_state.toggle_fold_at_line(buf_line);
                });
                if let Some(w) = weak.upgrade() {
                    sync_ui_from_shared(&inner, &w);
                }
            }
        });

        // Grid overlay toggle
        self.window.on_toggle_grid_overlay({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move || {
                if inner.borrow().syncing.get() {
                    return;
                }
                inner.borrow_mut().store.mutate(|state| {
                    state.show_grid_overlay = !state.show_grid_overlay;
                });
                if let Some(w) = weak.upgrade() {
                    sync_ui_from_shared(&inner, &w);
                }
            }
        });

        // Page layout editing
        self.window.on_page_layout_edited({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |key, value| {
                if inner.borrow().syncing.get() {
                    return;
                }
                let key = key.to_string();
                let value = value.to_string();
                {
                    let mut s = inner.borrow_mut();
                    s.push_undo(&format!("Edit page {key}"));
                    s.store.mutate(|state| {
                        apply_page_layout_edit(
                            &mut state.builder_document.page_layout,
                            &key,
                            &value,
                        );
                    });
                }
                if let Some(w) = weak.upgrade() {
                    sync_ui_from_shared(&inner, &w);
                }
            }
        });

        // Node drag — snapshot initial transform on press
        self.window.on_node_drag_started({
            let inner = Rc::clone(&inner);
            move |node_id| {
                if is_preview_mode(&inner.borrow().store.state().workspace) {
                    return;
                }
                let nid = node_id.to_string();
                let mut s = inner.borrow_mut();
                if let Some(ref mut live) = s.live {
                    let pre_drag_source = live.source.clone();
                    let doc = live.document();
                    if let Some(ref root) = doc.root {
                        if let Some(t) = find_node_transform(root, &nid) {
                            s.drag_initial_transform = Some(DragSnapshot {
                                node_id: nid.clone(),
                                position: t.position,
                                rotation: t.rotation,
                                scale: t.scale,
                                pre_drag_source,
                            });
                        }
                    }
                }
                s.fire_signal(&nid, "drag-started", serde_json::Map::new());
            }
        });

        // Node drag — live update while dragging (delta from press point)
        self.window.on_node_drag_moved({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |node_id, tool, dx, dy, shift| {
                if is_preview_mode(&inner.borrow().store.state().workspace) {
                    return;
                }
                if inner.borrow().syncing.get() {
                    return;
                }
                let nid = node_id.to_string();
                let tool = tool.to_string();
                {
                    let mut s = inner.borrow_mut();
                    let snap = s.drag_initial_transform.clone();
                    if let (Some(ref snap), Some(ref mut live)) = (&snap, &mut s.live) {
                        if snap.node_id == nid {
                            let _ = live.mutate_document(|doc| {
                                if let Some(ref mut root) = doc.root {
                                    apply_drag_to_node(root, &tool, dx, dy, shift, snap);
                                }
                            });
                        }
                    }
                    s.sync_builder_document();
                }
                if let Some(w) = weak.upgrade() {
                    sync_ui_from_shared(&inner, &w);
                }
            }
        });

        // Node drag — commit final transform with undo
        self.window.on_node_drag_finished({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |node_id, _tool, _dx, _dy, _shift| {
                if is_preview_mode(&inner.borrow().store.state().workspace) {
                    return;
                }
                if inner.borrow().syncing.get() {
                    return;
                }
                let nid = node_id.to_string();
                {
                    let mut s = inner.borrow_mut();
                    let snap = s.drag_initial_transform.take();
                    if let Some(snap) = snap {
                        if snap.node_id == nid {
                            let desc = match _tool.to_string().as_str() {
                                "rotate" => "Rotate node",
                                "scale" => "Scale node",
                                _ => "Move node",
                            };
                            let selection = s.store.state().selection.clone();
                            s.undo_past.push(SourceSnapshot {
                                description: desc.into(),
                                source: snap.pre_drag_source,
                                selection,
                            });
                            s.undo_future.clear();
                            if s.undo_past.len() > 100 {
                                s.undo_past.remove(0);
                            }
                        }
                    }
                    s.sync_builder_document();
                    s.fire_signal(&nid, "drag-ended", serde_json::Map::new());
                }
                if let Some(w) = weak.upgrade() {
                    sync_ui_from_shared(&inner, &w);
                }
            }
        });

        // Node resize — snapshot initial position + dimensions on press
        self.window.on_node_resize_started({
            let inner = Rc::clone(&inner);
            move |node_id, _handle| {
                if is_preview_mode(&inner.borrow().store.state().workspace) {
                    return;
                }
                use prism_core::foundation::geometry::Size2;
                let nid = node_id.to_string();
                let mut s = inner.borrow_mut();
                let state = s.store.state();
                let doc = &state.builder_document;
                if let Some(ref root) = doc.root {
                    let vp = Size2::new(state.viewport_width, 800.0);
                    let layout = compute_layout(doc, vp);
                    if let Some(t) = find_node_transform(root, &nid) {
                        let (w, h) =
                            find_node_layout_size(root, &nid, &layout).unwrap_or((100.0, 100.0));
                        s.resize_initial = Some(ResizeSnapshot {
                            node_id: nid,
                            position: t.position,
                            width: w,
                            height: h,
                        });
                    }
                }
            }
        });

        // Node resize — live update while dragging
        self.window.on_node_resize_moved({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |node_id, handle, dx, dy, shift| {
                if is_preview_mode(&inner.borrow().store.state().workspace) {
                    return;
                }
                if inner.borrow().syncing.get() {
                    return;
                }
                let nid = node_id.to_string();
                let handle = handle.to_string();
                {
                    let mut s = inner.borrow_mut();
                    let snap = s.resize_initial.clone();
                    if let (Some(ref snap), Some(ref mut live)) = (&snap, &mut s.live) {
                        if snap.node_id == nid {
                            let _ = live.mutate_document(|doc| {
                                if let Some(ref mut root) = doc.root {
                                    apply_resize_to_node(root, &handle, dx, dy, shift, snap);
                                }
                            });
                        }
                    }
                    s.sync_builder_document();
                }
                if let Some(w) = weak.upgrade() {
                    sync_ui_from_shared(&inner, &w);
                }
            }
        });

        // Node resize — commit final size with undo
        self.window.on_node_resize_finished({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |node_id, handle, dx, dy, shift| {
                if is_preview_mode(&inner.borrow().store.state().workspace) {
                    return;
                }
                if inner.borrow().syncing.get() {
                    return;
                }
                let nid = node_id.to_string();
                let handle = handle.to_string();
                {
                    let mut s = inner.borrow_mut();
                    let snap = s.resize_initial.take();
                    if let Some(snap) = snap {
                        if snap.node_id == nid {
                            s.push_undo("Resize node");
                            if let Some(ref mut live) = s.live {
                                let _ = live.mutate_document(|doc| {
                                    if let Some(ref mut root) = doc.root {
                                        apply_resize_to_node(root, &handle, dx, dy, shift, &snap);
                                    }
                                });
                            }
                        }
                    }
                    s.sync_builder_document();
                }
                if let Some(w) = weak.upgrade() {
                    sync_ui_from_shared(&inner, &w);
                }
            }
        });

        // Grid cell clicked (select node or open palette for empty cell)
        self.window.on_grid_cell_clicked({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |path| {
                if is_preview_mode(&inner.borrow().store.state().workspace) {
                    return;
                }
                {
                    let mut s = inner.borrow_mut();
                    let occupant_id = {
                        let doc = &s.store.state().builder_document;
                        let cell_path = path_from_string(path.as_str());
                        doc.page_layout
                            .grid
                            .as_ref()
                            .and_then(|g| g.at(&cell_path))
                            .and_then(|c| c.node_id().map(String::from))
                    };
                    if let Some(id) = occupant_id {
                        s.store.mutate(|state| {
                            state.selection.select(id);
                        });
                    }
                }
                if let Some(w) = weak.upgrade() {
                    sync_ui_from_shared(&inner, &w);
                }
            }
        });

        // Grid add at edge (recursive subdivision)
        self.window.on_grid_add_at_edge({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |cell_path, edge| {
                if is_preview_mode(&inner.borrow().store.state().workspace) {
                    return;
                }
                {
                    let mut s = inner.borrow_mut();
                    s.push_undo("Add cell");
                    let path = path_from_string(cell_path.as_str());
                    let cell_edge = match edge.as_str() {
                        "top" => CellEdge::Top,
                        "bottom" => CellEdge::Bottom,
                        "left" => CellEdge::Left,
                        "right" => CellEdge::Right,
                        _ => return,
                    };
                    s.store.mutate(|state| {
                        let _ = state
                            .builder_document
                            .page_layout
                            .insert_at_edge(&path, cell_edge);
                    });
                }
                if let Some(w) = weak.upgrade() {
                    sync_ui_from_shared(&inner, &w);
                }
            }
        });

        // Gap resize — snapshot track sizes on press
        self.window.on_grid_gap_resize_started({
            let inner = Rc::clone(&inner);
            move |parent_path, gap_index| {
                if is_preview_mode(&inner.borrow().store.state().workspace) {
                    return;
                }
                let mut s = inner.borrow_mut();
                s.push_undo("Resize gap");
                let pp = path_from_string(parent_path.as_str());
                let gi = gap_index as usize;
                let doc = &s.store.state().builder_document;
                let resolved = doc.page_layout.resolved_size();
                let vw = s.store.state().viewport_width;
                let (pw, ph) = resolved
                    .map(|sz| (sz.width, sz.height))
                    .unwrap_or((vw, vw * 0.625));
                let content_w = pw - doc.page_layout.margins.left - doc.page_layout.margins.right;
                let content_h = ph - doc.page_layout.margins.top - doc.page_layout.margins.bottom;

                if let Some(grid) = &doc.page_layout.grid {
                    let parent = if pp.is_empty() {
                        Some(grid)
                    } else {
                        grid.at(&pp)
                    };
                    if let Some(prism_builder::GridCell::Split {
                        direction, tracks, ..
                    }) = parent
                    {
                        if gi + 1 < tracks.len() {
                            let available = match direction {
                                prism_builder::SplitDirection::Horizontal => content_w,
                                prism_builder::SplitDirection::Vertical => content_h,
                            };
                            s.gap_resize_snapshot = Some(GapResizeSnapshot {
                                parent_path: pp,
                                gap_index: gi,
                                track_a: tracks[gi],
                                track_b: tracks[gi + 1],
                                available,
                            });
                        }
                    }
                }
            }
        });

        // Gap resize — live update while dragging (cumulative delta from press)
        self.window.on_grid_gap_resize_moved({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |_parent_path, _gap_index, delta| {
                if is_preview_mode(&inner.borrow().store.state().workspace) {
                    return;
                }
                if inner.borrow().syncing.get() {
                    return;
                }
                {
                    let mut s = inner.borrow_mut();
                    let snap = s.gap_resize_snapshot.clone();
                    if let Some(ref snap) = snap {
                        s.store.mutate(|state| {
                            let layout = &mut state.builder_document.page_layout;
                            let grid = match layout.grid.as_mut() {
                                Some(g) => g,
                                None => return,
                            };
                            let parent = if snap.parent_path.is_empty() {
                                grid
                            } else {
                                match grid.at_mut(&snap.parent_path) {
                                    Some(p) => p,
                                    None => return,
                                }
                            };
                            if let prism_builder::GridCell::Split { tracks, .. } = parent {
                                if snap.gap_index + 1 < tracks.len() {
                                    tracks[snap.gap_index] = snap.track_a;
                                    tracks[snap.gap_index + 1] = snap.track_b;
                                }
                            }
                            let _ = layout.resize_gap(
                                &snap.parent_path,
                                snap.gap_index,
                                delta,
                                snap.available,
                            );
                        });
                    }
                }
                if let Some(w) = weak.upgrade() {
                    sync_ui_from_shared(&inner, &w);
                }
            }
        });

        // Gap resize — finished
        self.window.on_grid_gap_resize_finished({
            let inner = Rc::clone(&inner);
            move || {
                inner.borrow_mut().gap_resize_snapshot = None;
            }
        });

        // Inspector delete-track (parses "cell:<path>" id format)
        self.window.on_inspector_delete_track({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |id| {
                if is_preview_mode(&inner.borrow().store.state().workspace) {
                    return;
                }
                let id_str = id.as_str();
                if let Some(path_str) = id_str.strip_prefix("cell:") {
                    let path = path_from_string(path_str);
                    {
                        let mut s = inner.borrow_mut();
                        s.push_undo("Remove cell");
                        s.store.mutate(|state| {
                            let _ = state.builder_document.page_layout.remove_cell(&path);
                        });
                    }
                    if let Some(w) = weak.upgrade() {
                        sync_ui_from_shared(&inner, &w);
                    }
                }
            }
        });

        // Grid cell add component
        self.window.on_grid_cell_add_component({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |component_type, path| {
                if is_preview_mode(&inner.borrow().store.state().workspace) {
                    return;
                }
                let ct = component_type.to_string();
                let path_str = path.to_string();
                {
                    let mut s = inner.borrow_mut();
                    s.push_undo(&format!("Add {ct} at {path_str}"));

                    let user_prefab_grid =
                        s.store.state().builder_document.prefabs.get(&ct).cloned();
                    if let Some(prefab_def) = builtin_prefab(&ct).or(user_prefab_grid) {
                        let mut counter = s.store.state().next_node_id;
                        let tree = materialize_prefab(&prefab_def, &mut counter);
                        let root_id = tree.id.clone();
                        if let Some(ref mut live) = s.live {
                            let _ = live.insert_tree_in_source(Some("root"), &tree, None);
                        }
                        let cell_path = path_from_string(&path_str);
                        s.store.mutate(|state| {
                            state.next_node_id = counter;
                            state.selection.select(root_id.clone());
                            state
                                .builder_document
                                .page_layout
                                .place_node_at(&cell_path, root_id)
                                .ok();
                        });
                    } else {
                        let node_id = format!("n{}", s.store.state().next_node_id);
                        let props = default_props_for_component(&ct);
                        if let Some(ref mut live) = s.live {
                            let _ = live.insert_node_in_source(
                                Some("root"),
                                &ct,
                                &node_id,
                                &props,
                                None,
                            );
                        }
                        let nid = node_id.clone();
                        let cell_path = path_from_string(&path_str);
                        s.store.mutate(|state| {
                            state.next_node_id += 1;
                            state.selection.select(nid.clone());
                            state
                                .builder_document
                                .page_layout
                                .place_node_at(&cell_path, nid)
                                .ok();
                        });
                    }
                    s.sync_builder_document();
                    s.drag_component_type.clear();
                    s.pending_picker = None;
                }
                if let Some(w) = weak.upgrade() {
                    sync_ui_from_shared(&inner, &w);
                }
            }
        });

        // Picker show — sets position + visibility via deferred sync
        self.window.on_picker_show({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |path, x, y| {
                if is_preview_mode(&inner.borrow().store.state().workspace) {
                    return;
                }
                {
                    let mut s = inner.borrow_mut();
                    s.pending_picker = Some((path.to_string(), x, y));
                }
                if let Some(w) = weak.upgrade() {
                    sync_ui_from_shared(&inner, &w);
                }
            }
        });

        // Picker dismiss
        self.window.on_picker_dismiss({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move || {
                {
                    inner.borrow_mut().pending_picker = None;
                }
                if let Some(w) = weak.upgrade() {
                    sync_ui_from_shared(&inner, &w);
                }
            }
        });

        // Palette drag toggle (place mode)
        self.window.on_palette_drag_toggle({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |component_type| {
                if is_preview_mode(&inner.borrow().store.state().workspace) {
                    return;
                }
                {
                    let mut s = inner.borrow_mut();
                    if s.drag_component_type == component_type.as_str() {
                        s.drag_component_type.clear();
                    } else {
                        s.drag_component_type = component_type.to_string();
                    }
                }
                if let Some(w) = weak.upgrade() {
                    sync_ui_from_shared(&inner, &w);
                }
            }
        });

        // Context menu show — select target and build context-sensitive items
        self.window.on_context_menu_show({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |target_kind, target_id, x, y| {
                {
                    let mut s = inner.borrow_mut();
                    let kind = target_kind.to_string();
                    let id = target_id.to_string();
                    // Select the right-clicked item so commands operate on it
                    if matches!(
                        kind.as_str(),
                        "inspector-node" | "grid-cell" | "builder-node"
                    ) {
                        let id_clone = id.clone();
                        if !s.store.state().selection.contains(&id_clone) {
                            s.store.mutate(|state| {
                                state.selection.select(id_clone);
                            });
                        }
                    }
                    s.pending_context_menu = Some(ContextMenuState {
                        target_kind: kind,
                        target_id: id,
                        x,
                        y,
                    });
                }
                if let Some(w) = weak.upgrade() {
                    sync_ui_from_shared(&inner, &w);
                }
            }
        });

        // Context menu dismiss
        self.window.on_context_menu_dismiss({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move || {
                inner.borrow_mut().pending_context_menu = None;
                if let Some(w) = weak.upgrade() {
                    sync_ui_from_shared(&inner, &w);
                }
            }
        });

        // Context menu command — dismiss menu and execute the command
        self.window.on_context_menu_command({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |cmd_id| {
                {
                    inner.borrow_mut().pending_context_menu = None;
                }
                if let Some(w) = weak.upgrade() {
                    sync_ui_from_shared(&inner, &w);
                }
                execute_command(&inner, &weak, &cmd_id);
            }
        });

        // Save a user color swatch
        self.window.on_save_color_swatch({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |hex| {
                let hex = hex.to_string();
                if hex.is_empty() {
                    return;
                }
                {
                    let mut s = inner.borrow_mut();
                    if !s.user_color_swatches.contains(&hex) {
                        s.user_color_swatches.push(hex);
                    }
                }
                if let Some(w) = weak.upgrade() {
                    push_user_swatches(&inner, &w);
                }
            }
        });

        // Remove a user color swatch by index
        self.window.on_remove_color_swatch({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |index| {
                {
                    let mut s = inner.borrow_mut();
                    let idx = index as usize;
                    if idx < s.user_color_swatches.len() {
                        s.user_color_swatches.remove(idx);
                    }
                }
                if let Some(w) = weak.upgrade() {
                    push_user_swatches(&inner, &w);
                }
            }
        });

        // Remove a signal connection by id
        self.window.on_signal_connection_remove({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |connection_id| {
                {
                    let mut s = inner.borrow_mut();
                    let cid = connection_id.to_string();
                    s.push_undo("Remove signal connection");
                    s.store.mutate(|state| {
                        if let Some(doc) =
                            state.active_app_mut().and_then(|a| a.active_document_mut())
                        {
                            crate::panels::signals::SignalsPanel::remove_connection(doc, &cid);
                        }
                    });
                    s.sync_builder_document();
                }
                if let Some(w) = weak.upgrade() {
                    sync_ui_from_shared(&inner, &w);
                }
            }
        });

        // Add a signal connection from the signals panel quick-add
        self.window.on_signal_connection_add({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |signal_name, target_idx, action_idx| {
                {
                    let mut s = inner.borrow_mut();
                    let source = match s.store.state().selection.primary().cloned() {
                        Some(id) => id,
                        None => return,
                    };
                    let sig = signal_name.to_string();
                    let target = if target_idx <= 0 {
                        source.clone()
                    } else {
                        let doc = &s.store.state().builder_document;
                        let targets = crate::panels::signals::SignalsPanel::available_targets(doc);
                        let ti = (target_idx - 1) as usize;
                        targets
                            .get(ti)
                            .map(|t| t.node_id.clone())
                            .unwrap_or_else(|| source.clone())
                    };
                    let has_duplicate = s
                        .store
                        .state()
                        .active_app()
                        .and_then(|a| a.active_document())
                        .map(|doc| {
                            crate::panels::signals::SignalsPanel::has_duplicate(
                                doc, &source, &sig, &target,
                            )
                        })
                        .unwrap_or(false);
                    if has_duplicate {
                        s.add_toast(
                            "Duplicate connection",
                            "A connection with the same source signal and target already exists.",
                            "warning",
                        );
                        return;
                    }
                    let action = crate::panels::signals::action_kind_from_index(action_idx, &sig);
                    let conn_id = format!("conn-{}", s.store.state().next_node_id);
                    s.push_undo("Add signal connection");
                    s.store.mutate(|state| {
                        state.next_node_id += 1;
                        if let Some(doc) =
                            state.active_app_mut().and_then(|a| a.active_document_mut())
                        {
                            let conn = crate::panels::signals::SignalsPanel::create_connection(
                                &conn_id, &source, &sig, &target, action,
                            );
                            doc.connections.push(conn);
                        }
                    });
                    s.sync_builder_document();
                }
                if let Some(w) = weak.upgrade() {
                    sync_ui_from_shared(&inner, &w);
                }
            }
        });

        // ── Navigation panel callbacks ──────────────────────────────
        self.window.on_nav_add_page({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move || {
                execute_command(&inner, &weak, "add_page");
            }
        });
        self.window.on_nav_delete_page({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |page_index| {
                {
                    let mut s = inner.borrow_mut();
                    s.save_to_active_page();
                    let deleted = s.store.state().active_app().is_some_and(|app| {
                        app.pages.len() > 1 && (page_index as usize) < app.pages.len()
                    });
                    if deleted {
                        s.push_undo("Delete page");
                        s.store.mutate(|state| {
                            if let Some(app) = state.active_app_mut() {
                                crate::panels::navigation::NavigationPanel::delete_page(
                                    app,
                                    page_index as usize,
                                );
                            }
                            state.selection.clear();
                            state.sync_document_from_app();
                        });
                        s.load_active_page();
                    }
                }
                if let Some(w) = weak.upgrade() {
                    sync_ui_from_shared(&inner, &w);
                }
            }
        });
        self.window.on_nav_rename_page({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |page_index, new_title| {
                {
                    let mut s = inner.borrow_mut();
                    s.push_undo("Rename page");
                    s.store.mutate(|state| {
                        if let Some(app) = state.active_app_mut() {
                            crate::panels::navigation::NavigationPanel::rename_page(
                                app,
                                page_index as usize,
                                &new_title,
                            );
                        }
                    });
                }
                if let Some(w) = weak.upgrade() {
                    sync_ui_from_shared(&inner, &w);
                }
            }
        });
        self.window.on_nav_set_route({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |page_index, new_route| {
                {
                    let mut s = inner.borrow_mut();
                    s.push_undo("Set page route");
                    s.store.mutate(|state| {
                        if let Some(app) = state.active_app_mut() {
                            crate::panels::navigation::NavigationPanel::set_page_route(
                                app,
                                page_index as usize,
                                &new_route,
                            );
                        }
                    });
                }
                if let Some(w) = weak.upgrade() {
                    sync_ui_from_shared(&inner, &w);
                }
            }
        });
        self.window.on_nav_move_page_up({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |page_index| {
                {
                    let mut s = inner.borrow_mut();
                    s.save_to_active_page();
                    s.push_undo("Move page up");
                    s.store.mutate(|state| {
                        if let Some(app) = state.active_app_mut() {
                            crate::panels::navigation::NavigationPanel::move_page_up(
                                app,
                                page_index as usize,
                            );
                        }
                    });
                }
                if let Some(w) = weak.upgrade() {
                    sync_ui_from_shared(&inner, &w);
                }
            }
        });
        self.window.on_nav_move_page_down({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |page_index| {
                {
                    let mut s = inner.borrow_mut();
                    s.save_to_active_page();
                    s.push_undo("Move page down");
                    s.store.mutate(|state| {
                        if let Some(app) = state.active_app_mut() {
                            crate::panels::navigation::NavigationPanel::move_page_down(
                                app,
                                page_index as usize,
                            );
                        }
                    });
                }
                if let Some(w) = weak.upgrade() {
                    sync_ui_from_shared(&inner, &w);
                }
            }
        });
        self.window.on_nav_select_page({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |page_index| {
                inner.borrow_mut().switch_to_page(page_index as usize);
                if let Some(w) = weak.upgrade() {
                    sync_ui_from_shared(&inner, &w);
                }
            }
        });
        self.window.on_nav_cycle_style({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move || {
                {
                    let mut s = inner.borrow_mut();
                    s.push_undo("Change navigation style");
                    s.store.mutate(|state| {
                        if let Some(app) = state.active_app_mut() {
                            use prism_builder::app::NavigationStyle;
                            let next = match app.navigation.style {
                                NavigationStyle::Tabs => NavigationStyle::Sidebar,
                                NavigationStyle::Sidebar => NavigationStyle::BottomBar,
                                NavigationStyle::BottomBar => NavigationStyle::None,
                                NavigationStyle::None => NavigationStyle::Tabs,
                            };
                            crate::panels::navigation::NavigationPanel::set_navigation_style(
                                app, next,
                            );
                        }
                    });
                }
                if let Some(w) = weak.upgrade() {
                    sync_ui_from_shared(&inner, &w);
                }
            }
        });

        // Navigation graph: click node = select page
        self.window.on_nav_graph_node_clicked({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |page_index| {
                inner.borrow_mut().switch_to_page(page_index as usize);
                if let Some(w) = weak.upgrade() {
                    sync_ui_from_shared(&inner, &w);
                }
            }
        });

        // Navigation graph: begin link drag
        self.window.on_nav_graph_link_start({
            let inner = Rc::clone(&inner);
            move |source_idx| {
                inner.borrow_mut().nav_link_source = Some(source_idx as usize);
            }
        });

        // Navigation graph: complete link (add NavigateTo connection)
        self.window.on_nav_graph_link_end({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |target_idx| {
                let source_idx;
                {
                    let mut s = inner.borrow_mut();
                    source_idx = s.nav_link_source.take();
                }
                let Some(src) = source_idx else { return };
                let tgt = target_idx as usize;
                if src == tgt {
                    return;
                }
                {
                    let mut s = inner.borrow_mut();
                    let target_route = s
                        .store
                        .state()
                        .active_app()
                        .and_then(|app| app.pages.get(tgt))
                        .map(|p| p.route.clone());
                    if let Some(route) = target_route {
                        use prism_builder::signal::{ActionKind, Connection};
                        let conn_id = format!("nav-{src}-{tgt}");
                        s.push_undo("Add navigation link");
                        s.store.mutate(|state| {
                            if let Some(app) = state.active_app_mut() {
                                if let Some(page) = app.pages.get_mut(src) {
                                    let already = page.document.connections.iter().any(|c| {
                                        matches!(&c.action, ActionKind::NavigateTo { target } if target == &route)
                                    });
                                    if !already {
                                        let source_node = page
                                            .document
                                            .root
                                            .as_ref()
                                            .map(|r| r.id.clone())
                                            .unwrap_or_default();
                                        page.document.connections.push(Connection {
                                            id: conn_id,
                                            source_node,
                                            signal: "clicked".into(),
                                            target_node: String::new(),
                                            action: ActionKind::NavigateTo {
                                                target: route,
                                            },
                                            params: serde_json::Value::Null,
                                        });
                                    }
                                }
                            }
                        });
                    }
                }
                if let Some(w) = weak.upgrade() {
                    sync_ui_from_shared(&inner, &w);
                }
            }
        });

        // Navigation graph: remove a link edge
        self.window.on_nav_graph_link_remove({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |edge_id| {
                let edge_id = edge_id.to_string();
                {
                    let mut s = inner.borrow_mut();
                    s.push_undo("Remove navigation link");
                    if edge_id.starts_with("href:") {
                        // href:<page_idx>:<node_id> — clear the href prop
                        let parts: Vec<&str> = edge_id.splitn(3, ':').collect();
                        if parts.len() == 3 {
                            if let Ok(page_idx) = parts[1].parse::<usize>() {
                                let node_id = parts[2].to_string();
                                s.store.mutate(|state| {
                                    if let Some(app) = state.active_app_mut() {
                                        if let Some(page) = app.pages.get_mut(page_idx) {
                                            if let Some(root) = &mut page.document.root {
                                                clear_href_on_node(root, &node_id);
                                            }
                                        }
                                    }
                                });
                            }
                        }
                    } else {
                        // Signal connection — remove by connection ID
                        s.store.mutate(|state| {
                            if let Some(app) = state.active_app_mut() {
                                for page in &mut app.pages {
                                    page.document.connections.retain(|c| c.id != edge_id);
                                }
                            }
                        });
                    }
                }
                if let Some(w) = weak.upgrade() {
                    sync_ui_from_shared(&inner, &w);
                }
            }
        });

        // Schema designer: select a schema
        self.window.on_schema_select({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |schema_id| {
                let sid = schema_id.to_string();
                inner.borrow_mut().store.mutate(|state| {
                    state.selected_schema_id = if sid.is_empty() { None } else { Some(sid) };
                });
                if let Some(w) = weak.upgrade() {
                    sync_ui_from_shared(&inner, &w);
                }
            }
        });

        // Widget toolbar action
        self.window.on_widget_toolbar_action({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |action_id| {
                let action_id = action_id.to_string();
                {
                    let mut s = inner.borrow_mut();
                    let sel = s.store.state().selection.clone();
                    let Some(node_id) = sel.primary() else {
                        return;
                    };
                    let node_id = node_id.clone();
                    let comp = s
                        .store
                        .state()
                        .builder_document
                        .root
                        .as_ref()
                        .and_then(|n| n.find(&node_id))
                        .and_then(|node| s.registry.get(&node.component));
                    let Some(comp) = comp else {
                        return;
                    };
                    let actions = comp.toolbar_actions();
                    let Some(action) = actions.iter().find(|a| a.id == action_id) else {
                        return;
                    };
                    use prism_core::widget::ToolbarActionKind;
                    match &action.kind {
                        ToolbarActionKind::Signal { signal } => {
                            s.fire_signal(&node_id, signal, serde_json::Map::new());
                        }
                        ToolbarActionKind::SetConfig { key, value } => {
                            s.push_undo("Set widget config");
                            let key = key.clone();
                            let value = value.clone();
                            s.store.mutate(|state| {
                                if let Some(node) = state
                                    .builder_document
                                    .root
                                    .as_mut()
                                    .and_then(|n| n.find_mut(&node_id))
                                {
                                    if let Some(obj) = node.props.as_object_mut() {
                                        obj.insert(key, value);
                                    }
                                }
                            });
                        }
                        ToolbarActionKind::ToggleConfig { key } => {
                            s.push_undo("Toggle widget config");
                            let key = key.clone();
                            s.store.mutate(|state| {
                                if let Some(node) = state
                                    .builder_document
                                    .root
                                    .as_mut()
                                    .and_then(|n| n.find_mut(&node_id))
                                {
                                    if let Some(obj) = node.props.as_object_mut() {
                                        let current = obj
                                            .get(&key)
                                            .and_then(|v| v.as_bool())
                                            .unwrap_or(false);
                                        obj.insert(key, serde_json::Value::Bool(!current));
                                    }
                                }
                            });
                        }
                        ToolbarActionKind::Custom { action_type } => {
                            s.fire_signal(
                                &node_id,
                                &format!("custom:{action_type}"),
                                serde_json::Map::new(),
                            );
                        }
                    }
                }
                if let Some(w) = weak.upgrade() {
                    sync_ui_from_shared(&inner, &w);
                }
            }
        });

        // Viewport preset changed
        self.window.on_viewport_preset_changed({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |preset| {
                let width = match preset.as_str() {
                    "Tablet" => 768.0,
                    "Mobile" => 375.0,
                    _ => 1280.0,
                };
                {
                    let mut s = inner.borrow_mut();
                    s.store.mutate(|state| {
                        state.viewport_width = width;
                    });
                }
                if let Some(w) = weak.upgrade() {
                    sync_ui_from_shared(&inner, &w);
                }
            }
        });
    }
}

fn push_user_swatches(shared: &Rc<RefCell<ShellInner>>, window: &AppWindow) {
    let inner = shared.borrow();
    let items: Vec<ColorPreset> = inner
        .user_color_swatches
        .iter()
        .map(|hex| {
            let c = parse_hex_color(hex);
            ColorPreset {
                c,
                hex: hex.clone().into(),
            }
        })
        .collect();
    let model = std::rc::Rc::new(slint::VecModel::from(items));
    window.set_user_color_swatches(slint::ModelRc::from(model));
}

// ── Sync UI ────────────────────────────────────────────────────────

fn sync_ui_from_shared(shared: &Rc<RefCell<ShellInner>>, window: &AppWindow) {
    if shared.borrow().syncing.get() {
        return;
    }
    {
        let mut inner = shared.borrow_mut();
        inner.save_to_active_page();
        let has_sel = !inner.store.state().selection.is_empty();
        let has_clip = inner.clipboard.is_some();
        let palette_open = inner.store.state().command_palette_open;
        inner.input.set_context("hasSelection", has_sel);
        inner.input.set_context("hasClipboard", has_clip);
        inner.input.set_context("commandPaletteOpen", palette_open);
        let current_selected = inner.store.state().selection.as_option();
        if current_selected != inner.last_selected_node {
            let prev = inner.last_selected_node.clone();
            let curr = current_selected.clone();
            if let Some(prev) = prev {
                inner.fire_signal(&prev, "blurred", serde_json::Map::new());
            }
            if let Some(curr) = curr {
                inner.fire_signal(&curr, "focused", serde_json::Map::new());
            }
            inner.toggled_sections.clear();
            inner.last_selected_node = current_selected;
        }
    }
    // Defer Slint property writes to the next event loop tick to avoid
    // recursion when a callback sets properties that the triggering
    // element depends on (e.g. grid-cells set from within GridCanvas click).
    let shared_clone = Rc::clone(shared);
    let weak = window.as_weak();
    shared.borrow().sync_timer.start(
        TimerMode::SingleShot,
        std::time::Duration::from_millis(0),
        move || {
            if let Some(w) = weak.upgrade() {
                {
                    let inner = shared_clone.borrow();
                    inner.syncing.set(true);
                    sync_ui_impl(&inner, &w);
                }
                // Keep syncing=true through the render frame. Slint evaluates
                // dirty bindings between timer ticks, so Slider `changed` /
                // ComboBox `selected` callbacks that fire during the render
                // phase will see syncing=true and skip. The dock_check_timer
                // (next event-loop tick) clears the flag.
                let sc2 = Rc::clone(&shared_clone);
                let weak2 = w.as_weak();
                shared_clone.borrow().dock_check_timer.start(
                    TimerMode::SingleShot,
                    std::time::Duration::from_millis(0),
                    move || {
                        sc2.borrow().syncing.set(false);
                        if let Some(w2) = weak2.upgrade() {
                            let new_w = w2.get_dock_area_width();
                            let new_h = w2.get_dock_area_height();
                            if new_w > 0.0 && new_h > 0.0 {
                                let needs_relayout = {
                                    let mut s = sc2.borrow_mut();
                                    let dirty = s.dock_dirty.get();
                                    let (old_w, old_h) = s.dock_area_dims;
                                    let dims_changed =
                                        (old_w - new_w).abs() > 0.5 || (old_h - new_h).abs() > 0.5;
                                    s.dock_area_dims = (new_w, new_h);
                                    s.dock_dirty.set(false);
                                    dirty || dims_changed
                                };
                                if needs_relayout {
                                    let inner = sc2.borrow();
                                    push_dock_layout(
                                        &inner.models,
                                        &w2,
                                        &inner.store.state().workspace,
                                        (new_w, new_h),
                                    );
                                }
                            }
                        }
                    },
                );
            }
        },
    );
}

fn sync_ui_impl(inner: &ShellInner, window: &AppWindow) {
    let state = inner.store.state();

    // Launchpad vs App view
    let is_launchpad = state.shell_view.is_launchpad();
    window.set_is_launchpad(is_launchpad);
    window.set_preview_mode(is_preview_mode(&state.workspace));

    if is_launchpad {
        push_app_cards(&inner.models, window, &state.apps);
        window.set_active_app_name(SharedString::new());
        window.set_active_page_name(SharedString::new());
    } else {
        let app = state.active_app();
        window.set_active_app_name(SharedString::from(
            app.map(|a| a.name.as_str()).unwrap_or(""),
        ));
        window.set_active_page_name(SharedString::from(
            app.and_then(|a| a.pages.get(a.active_page))
                .map(|p| p.title.as_str())
                .unwrap_or(""),
        ));
    }

    // Project name and dirty state
    let mut proj_name = inner.persistence.project_name().unwrap_or_default();
    #[cfg(feature = "native")]
    if proj_name.is_empty() {
        if let Some(ref proj) = inner.project {
            proj_name = proj
                .root()
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
        }
    }
    window.set_project_name(SharedString::from(proj_name));
    let mut is_dirty = inner.persistence.is_dirty();
    #[cfg(feature = "native")]
    if let Some(ref proj) = inner.project {
        is_dirty = is_dirty || proj.is_dirty();
    }
    window.set_project_dirty(is_dirty);

    // Shell chrome visibility
    window.set_show_activity_bar(state.show_activity_bar);
    window.set_show_left_sidebar(state.show_left_sidebar);
    window.set_show_right_sidebar(state.show_right_sidebar);

    // Viewport
    window.set_viewport_width(state.viewport_width);
    let preset = match state.viewport_width as u32 {
        768 => "Tablet",
        375 => "Mobile",
        _ => "Desktop",
    };
    window.set_viewport_preset(SharedString::from(preset));

    // Menu bar
    push_menu_defs(&inner.models, window, &inner.menus, &inner.commands);

    // Activity bar panel selection — derived from dock workspace
    let slint_panel_id = panel_id_for_slint(&state.workspace);
    window.set_active_panel_id(slint_panel_id);

    // Drag/place mode
    window.set_drag_component_type(SharedString::from(inner.drag_component_type.as_str()));

    // Transform tool
    window.set_transform_tool(SharedString::from(match state.transform_tool {
        TransformTool::Move => "move",
        TransformTool::Rotate => "rotate",
        TransformTool::Scale => "scale",
    }));

    // Component picker overlay
    if let Some((ref path, x, y)) = inner.pending_picker {
        window.set_pending_add_path(SharedString::from(path.as_str()));
        window.set_pending_add_x(x);
        window.set_pending_add_y(y);
        window.set_show_component_picker(true);
    } else {
        window.set_show_component_picker(false);
    }

    // Context menu overlay
    if let Some(ref ctx) = inner.pending_context_menu {
        let items = build_context_menu_items(inner, &ctx.target_kind, &ctx.target_id);
        let slint_items: Vec<MenuItem> = items
            .iter()
            .map(|item| MenuItem {
                label: SharedString::from(item.label.as_str()),
                shortcut: SharedString::from(item.shortcut.as_str()),
                command_id: SharedString::from(item.command_id.as_str()),
                enabled: item.enabled,
                is_separator: item.is_separator,
            })
            .collect();
        let model = Rc::new(slint::VecModel::from(slint_items));
        window.set_context_menu_items(model.into());
        window.set_context_menu_items_count(items.len() as i32);
        window.set_context_menu_x(ctx.x);
        window.set_context_menu_y(ctx.y);
        window.set_show_context_menu(true);
    } else {
        window.set_show_context_menu(false);
    }

    // Toolbar state
    window.set_has_selection(!state.selection.is_empty());
    window.set_has_clipboard(inner.clipboard.is_some());

    // Panel title + hint
    let (title, hint) = panel_metadata_from_workspace(&state.workspace);
    window.set_panel_title(SharedString::from(title));
    window.set_panel_hint(SharedString::from(hint));

    // Dock layout is pushed on a SEPARATE event-loop tick (dock_check_timer)
    // to avoid Slint property-evaluation recursion. Replacing the dock-panels
    // model in the same tick as content property updates causes Slint to
    // tear down and recreate all panel views mid-evaluation.

    // Fill all panel data (code editor + builder can coexist on any page).
    // NOTE: each push function replaces its model outright — no need to
    // clear first.  The old clear_panel_slots() call blanked every model
    // to empty before the pushes refilled them, which caused every
    // `if model.length > 0` conditional in Slint to toggle false→true on
    // every sync, destroying and recreating subtrees mid-evaluation and
    // triggering Slint's "Recursion detected" panic.
    if is_launchpad {
        clear_panel_slots(&inner.models, window);
    }
    if !is_launchpad {
        push_editor_data(&inner.models, window, &state.editor_state);
        push_builder_preview(&inner.models, window, &state.builder_document);

        // Resolve facet data (Script/ObjectQuery/Lookup kinds) before the
        // render walker, which reads `resolved_data` on each FacetDef.
        let has_dynamic_facets = state.builder_document.facets.values().any(|f| {
            matches!(
                &f.kind,
                FacetKind::Script { .. } | FacetKind::ObjectQuery { .. } | FacetKind::Lookup { .. }
            )
        });
        let has_scalar_facets = state
            .builder_document
            .facets
            .values()
            .any(|f| f.is_scalar());
        #[cfg(feature = "native")]
        let widget_data = resolve_widget_data(&state.builder_document, &inner.collection);
        #[cfg(not(feature = "native"))]
        let widget_data = HashMap::new();
        let needs_clone = has_dynamic_facets || has_scalar_facets;
        let resolved_doc = if needs_clone {
            let mut doc = state.builder_document.clone();
            #[cfg(feature = "native")]
            if has_dynamic_facets {
                resolve_facet_data(&mut doc, &inner.collection);
            }
            prism_builder::apply_scalar_bindings(&mut doc);
            Some(doc)
        } else {
            None
        };
        let preview_doc = resolved_doc.as_ref().unwrap_or(&state.builder_document);

        push_wysiwyg_preview(
            window,
            preview_doc,
            &inner.registry,
            &state.tokens,
            &inner.vfs,
            &widget_data,
        );
        push_live_preview(
            &inner.models,
            window,
            &state.builder_document,
            &state.selection,
            state.viewport_width,
        );
        push_inspector_nodes(
            &inner.models,
            window,
            &state.builder_document,
            &state.selection,
        );
        push_property_sections(
            &inner.models,
            window,
            &state.builder_document,
            &inner.registry,
            &state.selection,
            state.active_app(),
            &inner.toggled_sections,
            Some(&state.workspace),
            state.selected_schema_id.as_deref(),
        );
        push_widget_toolbar(
            &inner.models,
            window,
            &state.builder_document,
            &inner.registry,
            &state.selection,
        );
        push_signal_panel_data(
            &inner.models,
            window,
            &state.builder_document,
            &inner.registry,
            &state.selection,
        );
        push_navigation_panel_data(&inner.models, window, state.active_app());
        push_schema_list(
            &inner.models,
            window,
            &state.builder_document,
            state.selected_schema_id.as_deref(),
        );
        push_breadcrumbs(
            &inner.models,
            window,
            &state.builder_document,
            &state.selection,
        );
        let vw = state.viewport_width;
        push_page_layout_data(
            &inner.models,
            window,
            &state.builder_document,
            state.show_grid_overlay,
            vw,
        );
        push_composition_counts(
            window,
            &state.builder_document,
            &inner.registry,
            &state.selection,
        );
        push_grid_cells(
            &inner.models,
            window,
            &state.builder_document,
            &state.selection,
            vw,
        );
        push_grid_edge_handles(&inner.models, window, &state.builder_document, vw);
        #[cfg(feature = "native")]
        let project_files = {
            use prism_core::foundation::persistence::ObjectFilter;
            let file_objects = inner.collection.list_objects(Some(&ObjectFilter {
                types: Some(vec!["file".into()]),
                exclude_deleted: true,
                ..Default::default()
            }));
            file_objects
                .into_iter()
                .map(|o| crate::explorer::ProjectFileEntry {
                    id: o.id.as_str().to_string(),
                    extension: o
                        .data
                        .get("extension")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    name: o.name,
                })
                .collect::<Vec<_>>()
        };
        #[cfg(not(feature = "native"))]
        let project_files = Vec::<crate::explorer::ProjectFileEntry>::new();
        push_explorer_nodes(
            &inner.models,
            window,
            &state.apps,
            &state.shell_view,
            &state.explorer_expanded,
            &project_files,
        );
    }

    // Tabs — derived from the active app's pages
    let tab_items: Vec<TabItem> = if let Some(app) = state.active_app() {
        app.pages
            .iter()
            .enumerate()
            .map(|(i, page)| TabItem {
                id: i as i32,
                title: SharedString::from(&page.title),
                active: i == app.active_page,
            })
            .collect()
    } else {
        Vec::new()
    };
    let count = sync_model(&inner.models.tabs, &tab_items);
    window.set_tabs_count(count);

    // Command palette
    window.set_command_palette_visible(state.command_palette_open);
    let filtered = inner.commands.filter(&state.command_palette_query);
    let cmd_items: Vec<CommandItem> = filtered
        .iter()
        .map(|c| CommandItem {
            label: SharedString::from(&c.label),
            shortcut: SharedString::from(c.shortcut.as_deref().unwrap_or("")),
            category: SharedString::from(&c.category),
        })
        .collect();
    let count = sync_model(&inner.models.command_results, &cmd_items);
    window.set_command_results_count(count);

    // Notifications
    let toast_items: Vec<ToastItem> = state
        .toasts
        .iter()
        .map(|t| ToastItem {
            id: t.id as i32,
            title: SharedString::from(&t.title),
            body: SharedString::from(&t.body),
            kind: SharedString::from(&t.kind),
        })
        .collect();
    let count = sync_model(&inner.models.notifications, &toast_items);
    window.set_notifications_count(count);

    // Undo/redo state
    window.set_can_undo(!inner.undo_past.is_empty());
    window.set_can_redo(!inner.undo_future.is_empty());
    window.set_undo_label(SharedString::from(
        inner
            .undo_past
            .last()
            .map(|s| s.description.as_str())
            .unwrap_or(""),
    ));
    window.set_redo_label(SharedString::from(
        inner
            .undo_future
            .last()
            .map(|s| s.description.as_str())
            .unwrap_or(""),
    ));

    // Search results
    let search_items: Vec<SearchResultItem> = if state.search_query.is_empty() {
        Vec::new()
    } else {
        let idx = SearchIndex::build(&state.builder_document);
        idx.query(&state.search_query)
            .into_iter()
            .take(10)
            .map(|r| SearchResultItem {
                node_id: SharedString::from(&r.node_id),
                component_type: SharedString::from(&r.component),
                field: SharedString::from(&r.field),
                snippet: SharedString::from(&r.snippet),
            })
            .collect()
    };
    let count = sync_model(&inner.models.search_results, &search_items);
    window.set_search_results_count(count);
}

fn clear_panel_slots(models: &PersistentModels, window: &AppWindow) {
    sync_model(&models.actions, &[]);
    window.set_actions_count(0);
    window.set_builder_node_count(0);
    window.set_builder_source(SharedString::new());
    window.set_inspector_tree(SharedString::new());
    sync_model(&models.inspector_nodes, &[]);
    window.set_inspector_nodes_count(0);
    sync_model(&models.grid_edge_handles, &[]);
    window.set_grid_edge_handles_count(0);
    window.set_selected_component(SharedString::new());
    sync_model(&models.component_palette, &[]);
    window.set_component_palette_count(0);
    sync_model(&models.widget_toolbar, &[]);
    window.set_widget_toolbar_count(0);
}

fn push_explorer_nodes(
    models: &PersistentModels,
    window: &AppWindow,
    apps: &[PrismApp],
    shell_view: &ShellView,
    expanded: &HashSet<String>,
    project_files: &[crate::explorer::ProjectFileEntry],
) {
    let mut tree = crate::explorer::build_explorer_tree(apps, shell_view, expanded);
    let file_nodes = crate::explorer::build_project_file_nodes(project_files, expanded);
    tree.extend(file_nodes);
    let items: Vec<ExplorerNodeItem> = tree
        .into_iter()
        .map(|n| ExplorerNodeItem {
            id: SharedString::from(&n.id),
            label: SharedString::from(&n.label),
            kind: SharedString::from(n.kind.as_str()),
            depth: n.depth,
            expanded: n.expanded,
            is_active: n.is_active,
        })
        .collect();
    let count = sync_model(&models.explorer_nodes, &items);
    window.set_explorer_nodes_count(count);
}

fn push_menu_defs(
    models: &PersistentModels,
    window: &AppWindow,
    menus: &crate::menu::MenuRegistry,
    commands: &CommandRegistry,
) {
    let defs: Vec<MenuDef> = menus
        .menu_names()
        .iter()
        .map(|name| {
            let resolved = menus.items_for_menu(name, commands);
            let items: Vec<MenuItem> = resolved
                .into_iter()
                .map(|r| MenuItem {
                    label: SharedString::from(&r.label),
                    shortcut: SharedString::from(&r.shortcut),
                    command_id: SharedString::from(&r.command_id),
                    enabled: true,
                    is_separator: r.is_separator,
                })
                .collect();
            let items_model = Rc::new(VecModel::from(items));
            MenuDef {
                label: SharedString::from(name.as_str()),
                items: ModelRc::from(items_model as Rc<dyn Model<Data = MenuItem>>),
            }
        })
        .collect();
    let count = sync_model(&models.menu_defs, &defs);
    window.set_menu_defs_count(count);
}

fn push_app_cards(models: &PersistentModels, window: &AppWindow, apps: &[PrismApp]) {
    let items: Vec<AppCardItem> = apps
        .iter()
        .map(|app| AppCardItem {
            id: SharedString::from(&app.id),
            name: SharedString::from(&app.name),
            description: SharedString::from(&app.description),
            icon: SharedString::from(app.icon.label()),
            accent_color: parse_hex_color(app.icon.accent_color()),
            page_count: app.pages.len() as i32,
        })
        .collect();
    let count = sync_model(&models.app_cards, &items);
    window.set_app_cards_count(count);
}

fn push_builder_preview(models: &PersistentModels, window: &AppWindow, doc: &BuilderDocument) {
    let node_count = count_nodes(doc.root.as_ref());
    window.set_builder_node_count(node_count);
    let palette = component_palette_items(doc);
    let count = sync_model(&models.component_palette, &palette);
    window.set_component_palette_count(count);
}

fn count_nodes(root: Option<&Node>) -> i32 {
    match root {
        None => 0,
        Some(node) => {
            1 + node
                .children
                .iter()
                .map(|c| count_nodes(Some(c)))
                .sum::<i32>()
        }
    }
}

fn push_live_preview(
    models: &PersistentModels,
    window: &AppWindow,
    doc: &BuilderDocument,
    selection: &SelectionModel,
    viewport_width: f32,
) {
    use prism_core::foundation::geometry::Size2;

    let root = match &doc.root {
        Some(r) => r,
        None => {
            sync_model(&models.preview_nodes, &[]);
            window.set_preview_nodes_count(0);
            return;
        }
    };

    let vp = Size2::new(viewport_width, 800.0);
    let layout = compute_layout(doc, vp);
    let mut items: Vec<PreviewNode> = Vec::new();

    fn walk_preview(
        node: &Node,
        layout: &prism_builder::ComputedLayout,
        selection: &SelectionModel,
        items: &mut Vec<PreviewNode>,
        parent_global_x: f32,
        parent_global_y: f32,
    ) {
        let (global_x, global_y) = if let Some(nl) = layout.nodes.get(&node.id) {
            let r = &nl.rect;
            let gx = r.origin.x + parent_global_x;
            let gy = r.origin.y + parent_global_y;
            let ct = node.component.as_str();
            let props = &node.props;
            let fg = slint::Color::from_argb_u8(255, 216, 222, 233); // #d8dee9
            let transparent = slint::Color::from_argb_u8(0, 0, 0, 0);

            let (text, label, alt, placeholder) = extract_preview_text(ct, props);
            let font_size = match ct {
                "text" => {
                    let level = props
                        .get("level")
                        .and_then(|v| v.as_str())
                        .unwrap_or("paragraph");
                    match level {
                        "h1" => 32.0,
                        "h2" => 26.0,
                        "h3" => 22.0,
                        "h4" => 18.0,
                        "h5" => 16.0,
                        "h6" => 14.0,
                        _ => 14.0,
                    }
                }
                "button" => 14.0,
                "code" => 13.0,
                _ => 14.0,
            };
            let border_radius = match ct {
                "card" => 8.0,
                "button" | "code" | "image" => 6.0,
                "table" | "accordion" => 4.0,
                _ => 0.0,
            };
            let bg = match ct {
                "card" => slint::Color::from_argb_u8(255, 46, 52, 64),
                "code" => slint::Color::from_argb_u8(255, 26, 30, 40),
                "image" => slint::Color::from_argb_u8(255, 42, 49, 64),
                "input" => transparent,
                "button" => transparent,
                _ => transparent,
            };

            let is_layout_only = matches!(ct, "container" | "columns" | "form" | "list" | "spacer");
            let positioned = node.layout_mode.is_positioned();
            let layout_mode_str = match &node.layout_mode {
                prism_builder::LayoutMode::Flow(_) => "flow",
                prism_builder::LayoutMode::Free => "free",
                prism_builder::LayoutMode::Absolute(_) => "absolute",
                prism_builder::LayoutMode::Relative(_) => "relative",
            };
            if !is_layout_only {
                items.push(PreviewNode {
                    id: SharedString::from(&node.id),
                    component_type: SharedString::from(ct),
                    selected: selection.contains(&node.id),
                    x: gx,
                    y: gy,
                    w: r.size.width,
                    h: r.size.height,
                    text: SharedString::from(&text),
                    label: SharedString::from(&label),
                    alt: SharedString::from(&alt),
                    placeholder: SharedString::from(&placeholder),
                    font_size: font_size as f32,
                    border_radius: border_radius as f32,
                    fg,
                    bg,
                    positioned,
                    layout_mode: SharedString::from(layout_mode_str),
                    rotation_deg: node.transform.rotation.to_degrees(),
                });
            }
            (gx, gy)
        } else {
            (parent_global_x, parent_global_y)
        };
        for child in &node.children {
            walk_preview(child, layout, selection, items, global_x, global_y);
        }
    }

    walk_preview(root, &layout, selection, &mut items, 0.0, 0.0);
    let count = sync_model(&models.preview_nodes, &items);
    window.set_preview_nodes_count(count);
}

fn push_wysiwyg_preview(
    window: &AppWindow,
    doc: &BuilderDocument,
    registry: &ComponentRegistry,
    tokens: &DesignTokens,
    vfs: &VfsManager,
    widget_data: &HashMap<String, serde_json::Value>,
) {
    if doc.root.is_none() {
        window.set_preview_factory_ready(false);
        window.set_preview_factory(Default::default());
        return;
    }
    let asset_paths = materialize_vfs_assets(doc, vfs);
    eprintln!(
        "[preview] grid cells={} children={}",
        doc.page_layout.leaf_count(),
        doc.root.as_ref().map(|r| r.children.len()).unwrap_or(0),
    );
    match render_document_slint_preview_with_assets_and_data(
        doc,
        registry,
        tokens,
        asset_paths,
        widget_data.clone(),
    ) {
        Ok(source) => {
            eprintln!("[preview] source:\n{source}");
            match compile_slint_preview(&source) {
                Ok(definition) => {
                    window.set_preview_factory(preview_component_factory(definition));
                    window.set_preview_factory_ready(true);
                }
                Err(e) => {
                    eprintln!("[preview] compile error: {e}");
                    window.set_preview_factory(Default::default());
                    window.set_preview_factory_ready(false);
                }
            }
        }
        Err(e) => {
            eprintln!("[preview] render error: {e}");
            window.set_preview_factory(Default::default());
            window.set_preview_factory_ready(false);
        }
    }
}

fn materialize_vfs_assets(
    doc: &BuilderDocument,
    vfs: &VfsManager,
) -> std::collections::HashMap<String, std::path::PathBuf> {
    use prism_builder::asset::collect_vfs_hashes;

    let mut paths = std::collections::HashMap::new();
    let root = match &doc.root {
        Some(r) => r,
        None => return paths,
    };
    let hashes = collect_vfs_hashes(root);
    if hashes.is_empty() {
        return paths;
    }
    let dir = std::env::temp_dir().join("prism-preview-assets");
    let _ = std::fs::create_dir_all(&dir);
    for hash in hashes {
        if let Some(data) = vfs.adapter().read(&hash) {
            let mime = vfs.stat(&hash).map(|s| s.mime_type.clone());
            let ext = mime.as_deref().map(mime_to_extension).unwrap_or("");
            let filename = if ext.is_empty() {
                hash.clone()
            } else {
                format!("{hash}.{ext}")
            };
            let path = dir.join(&filename);
            if !path.exists() {
                let _ = std::fs::write(&path, &data);
            }
            paths.insert(hash, path);
        }
    }
    paths
}

fn mime_to_extension(mime: &str) -> &'static str {
    match mime {
        "image/png" => "png",
        "image/jpeg" | "image/jpg" => "jpg",
        "image/gif" => "gif",
        "image/webp" => "webp",
        "image/svg+xml" => "svg",
        "image/bmp" => "bmp",
        "image/tiff" => "tiff",
        _ => "",
    }
}

fn extract_preview_text(
    component_type: &str,
    props: &serde_json::Value,
) -> (String, String, String, String) {
    let s = |key: &str| {
        props
            .get(key)
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string()
    };
    match component_type {
        "text" => (s("body"), String::new(), String::new(), String::new()),
        "button" => {
            let t = s("text");
            (
                if t.is_empty() { "Submit".into() } else { t },
                String::new(),
                String::new(),
                String::new(),
            )
        }
        "card" => (s("title"), s("body"), String::new(), String::new()),
        "image" => (String::new(), String::new(), s("alt"), String::new()),
        "input" => (String::new(), s("label"), String::new(), s("placeholder")),
        "code" => (s("code"), String::new(), String::new(), String::new()),
        "table" => (s("headers"), s("caption"), String::new(), String::new()),
        "accordion" => (s("title"), String::new(), String::new(), String::new()),
        "tabs" => (s("labels"), String::new(), String::new(), String::new()),
        _ => (String::new(), String::new(), String::new(), String::new()),
    }
}

fn push_inspector_nodes(
    models: &PersistentModels,
    window: &AppWindow,
    doc: &BuilderDocument,
    selection: &SelectionModel,
) {
    let items = if doc.page_layout.has_grid() {
        flatten_inspector_grid(doc, selection)
    } else {
        flatten_inspector_nodes(doc.root.as_ref(), selection)
    };
    let count = sync_model(&models.inspector_nodes, &items);
    window.set_inspector_nodes_count(count);
}

fn field_row_data_to_slint(r: &crate::panels::properties::FieldRowData) -> FieldRow {
    use slint::Color;
    let swatch = if r.kind == "color" {
        parse_hex_color(&r.value)
    } else {
        Color::from_argb_u8(0, 0, 0, 0)
    };
    let opts: Vec<SharedString> = r
        .options
        .iter()
        .map(|o| SharedString::from(o.as_str()))
        .collect();
    FieldRow {
        key: SharedString::from(r.key.as_str()),
        label: SharedString::from(r.label.as_str()),
        kind: SharedString::from(r.kind.as_str()),
        value: SharedString::from(r.value.as_str()),
        required: r.required,
        min: r.min,
        max: r.max,
        has_bounds: r.has_bounds,
        options: ModelRc::from(Rc::new(VecModel::from(opts)) as Rc<dyn Model<Data = SharedString>>),
        swatch,
        section_id: SharedString::default(),
    }
}

fn parse_hex_color(hex: &str) -> slint::Color {
    let hex = hex.trim_start_matches('#');
    let r = u8::from_str_radix(hex.get(0..2).unwrap_or("00"), 16).unwrap_or(0);
    let g = u8::from_str_radix(hex.get(2..4).unwrap_or("00"), 16).unwrap_or(0);
    let b = u8::from_str_radix(hex.get(4..6).unwrap_or("00"), 16).unwrap_or(0);
    let a = u8::from_str_radix(hex.get(6..8).unwrap_or("ff"), 16).unwrap_or(255);
    slint::Color::from_argb_u8(a, r, g, b)
}

#[allow(clippy::too_many_arguments)]
fn push_property_sections(
    models: &PersistentModels,
    window: &AppWindow,
    doc: &BuilderDocument,
    registry: &ComponentRegistry,
    selection: &SelectionModel,
    app: Option<&PrismApp>,
    toggled_sections: &std::collections::HashSet<String>,
    workspace: Option<&prism_dock::DockWorkspace>,
    selected_schema_id: Option<&str>,
) {
    let selected = selection.as_option();
    let component_id = PropertiesPanel::selected_component(doc, &selected);
    window.set_selected_component(SharedString::from(component_id));

    if let Some(ws) = workspace {
        if ws.active_page().id == "data" {
            let schema = selected_schema_id
                .and_then(|id| doc.facet_schemas.get(id))
                .or_else(|| doc.facet_schemas.values().next());
            if let Some(schema) = schema {
                let schema_rows = crate::panels::schema::SchemaDesignerPanel::field_rows(schema);
                let rows: Vec<FieldRow> = schema_rows
                    .into_iter()
                    .map(|r| field_row_data_to_slint(&r))
                    .collect();
                let count = sync_model(&models.property_rows, &rows);
                window.set_property_rows_count(count);
                return;
            }
        }
    }

    let mut sections = PropertiesPanel::sections(doc, registry, &selected, app);

    for section in &mut sections {
        if toggled_sections.contains(&section.id) {
            section.collapsed = !section.collapsed;
        }
    }

    let flat = PropertiesPanel::flatten_sections(&sections);
    let rows: Vec<FieldRow> = flat
        .into_iter()
        .map(|r| field_row_data_to_slint(&r))
        .collect();
    let count = sync_model(&models.property_rows, &rows);
    window.set_property_rows_count(count);

    if let Some(selected_id) = &selected {
        if let Some(node) = doc.root.as_ref().and_then(|n| n.find(selected_id)) {
            let t = &node.transform;
            window.set_transform_pos_x(t.position[0]);
            window.set_transform_pos_y(t.position[1]);
            window.set_transform_rotation_deg(t.rotation.to_degrees());
            window.set_node_scale_x(t.scale[0]);
            window.set_node_scale_y(t.scale[1]);
            window.set_transform_anchor_value(SharedString::from(
                crate::panels::properties::format_anchor(t.anchor),
            ));
        }
    }
}

// ── Context menu items ─────────────────────────────────────────────

struct ContextMenuItemDef {
    label: String,
    shortcut: String,
    command_id: String,
    enabled: bool,
    is_separator: bool,
}

impl ContextMenuItemDef {
    fn action(label: &str, command_id: &str, shortcut: &str, enabled: bool) -> Self {
        Self {
            label: label.into(),
            shortcut: shortcut.into(),
            command_id: command_id.into(),
            enabled,
            is_separator: false,
        }
    }

    fn separator() -> Self {
        Self {
            label: String::new(),
            shortcut: String::new(),
            command_id: String::new(),
            enabled: false,
            is_separator: true,
        }
    }
}

fn build_context_menu_items(
    inner: &ShellInner,
    target_kind: &str,
    target_id: &str,
) -> Vec<ContextMenuItemDef> {
    let has_selection = !inner.store.state().selection.is_empty();
    let has_clipboard = inner.clipboard.is_some();

    let selected_component = if !target_id.is_empty() {
        inner
            .store
            .state()
            .builder_document
            .root
            .as_ref()
            .and_then(|r| r.find(target_id))
            .map(|n| n.component.clone())
    } else {
        None
    };
    let is_facet = selected_component.as_deref() == Some("facet");

    match target_kind {
        "inspector-node" | "grid-cell" | "builder-node" => {
            let mut items = vec![
                ContextMenuItemDef::action("Cut", "edit.cut", "Ctrl+X", has_selection),
                ContextMenuItemDef::action("Copy", "edit.copy", "Ctrl+C", has_selection),
                ContextMenuItemDef::action("Paste", "edit.paste", "Ctrl+V", has_clipboard),
                ContextMenuItemDef::action("Duplicate", "edit.duplicate", "Ctrl+D", has_selection),
                ContextMenuItemDef::separator(),
                ContextMenuItemDef::action("Move Up", "navigate.inspector_prev", "", has_selection),
                ContextMenuItemDef::action(
                    "Move Down",
                    "navigate.inspector_next",
                    "",
                    has_selection,
                ),
                ContextMenuItemDef::separator(),
                ContextMenuItemDef::action("Delete", "selection.delete", "", has_selection),
            ];
            if is_facet {
                items.push(ContextMenuItemDef::separator());
                items.push(ContextMenuItemDef::action(
                    "Add Item",
                    "facet.add_item",
                    "",
                    true,
                ));
                items.push(ContextMenuItemDef::action(
                    "Clear Items",
                    "facet.clear_items",
                    "",
                    true,
                ));
                items.push(ContextMenuItemDef::action(
                    "Refresh Data",
                    "facet.refresh",
                    "",
                    true,
                ));
            } else if has_selection {
                items.push(ContextMenuItemDef::separator());
                items.push(ContextMenuItemDef::action(
                    "Save as Prefab",
                    "prefab.save_from_selection",
                    "",
                    true,
                ));
            }
            items
        }
        "inspector-row" => {
            vec![ContextMenuItemDef::action(
                "Delete Track",
                "selection.delete",
                "",
                true,
            )]
        }
        "grid-cell-empty" => {
            vec![
                ContextMenuItemDef::action("Paste", "edit.paste", "Ctrl+V", has_clipboard),
                ContextMenuItemDef::separator(),
                ContextMenuItemDef::action("Select All", "selection.all", "Ctrl+A", true),
            ]
        }
        "explorer-app" | "explorer-page" | "explorer-node" => {
            let mut items = vec![ContextMenuItemDef::action("Open", "panel.edit", "", true)];
            if target_kind == "explorer-page" {
                items.push(ContextMenuItemDef::separator());
                items.push(ContextMenuItemDef::action("Add Page", "add_page", "", true));
            }
            items
        }
        _ => vec![],
    }
}

// ── Docs panel ────────────────────────────────────────────────────

fn open_docs_panel(shared: &Rc<RefCell<ShellInner>>, weak: &slint::Weak<AppWindow>) {
    let active_id = shared.borrow().help_active_id.clone();
    if active_id.is_empty() {
        return;
    }
    let (title, summary, body) = {
        let s = shared.borrow();
        match s.help.get(&active_id) {
            Some(entry) => (
                entry.title.clone(),
                entry.summary.clone(),
                entry.body.clone().unwrap_or_default(),
            ),
            None => return,
        }
    };
    {
        let mut s = shared.borrow_mut();
        s.help_active_id.clear();
        s.help_pending_id.clear();
    }
    if let Some(w) = weak.upgrade() {
        w.set_help_tooltip(HelpTooltipData {
            visible: false,
            title: SharedString::new(),
            summary: SharedString::new(),
            has_docs: false,
            tip_x: 0.0,
            tip_y: 0.0,
        });
        w.set_docs_panel(DocsPanelData {
            visible: true,
            help_id: SharedString::from(&active_id),
            title: SharedString::from(title),
            summary: SharedString::from(summary),
            body: SharedString::from(body),
        });
    }
}

// ── Command dispatch ───────────────────────────────────────────────

fn execute_command(
    shared: &Rc<RefCell<ShellInner>>,
    weak: &slint::Weak<AppWindow>,
    command_id: &str,
) {
    match command_id {
        "edit.undo" => {
            shared.borrow_mut().perform_undo();
        }
        "edit.redo" => {
            shared.borrow_mut().perform_redo();
        }
        "command_palette.toggle" => {
            let mut s = shared.borrow_mut();
            let open = s.store.state().command_palette_open;
            s.store.mutate(|state| {
                state.command_palette_open = !open;
                if open {
                    state.command_palette_query.clear();
                }
            });
            s.input.set_context("commandPaletteOpen", !open);
        }
        "command_palette.close" => {
            let mut s = shared.borrow_mut();
            s.store.mutate(|state| {
                state.command_palette_open = false;
                state.command_palette_query.clear();
            });
            s.input.set_context("commandPaletteOpen", false);
        }
        "navigate.escape" => {
            let was_tooltip;
            {
                let mut s = shared.borrow_mut();
                was_tooltip = !s.help_active_id.is_empty();
                if was_tooltip {
                    s.help_active_id.clear();
                    s.help_pending_id.clear();
                } else {
                    s.store.mutate(|state| {
                        state.selection.clear();
                    });
                }
            }
            if let Some(w) = weak.upgrade() {
                if was_tooltip {
                    w.set_help_tooltip(HelpTooltipData {
                        visible: false,
                        title: SharedString::new(),
                        summary: SharedString::new(),
                        has_docs: false,
                        tip_x: 0.0,
                        tip_y: 0.0,
                    });
                }
                let empty_docs = DocsPanelData {
                    visible: false,
                    help_id: SharedString::new(),
                    title: SharedString::new(),
                    summary: SharedString::new(),
                    body: SharedString::new(),
                };
                if w.get_docs_view().visible {
                    w.set_docs_view(empty_docs);
                } else if w.get_docs_panel().visible {
                    w.set_docs_panel(empty_docs);
                }
            }
        }
        "panel.identity" | "panel.edit" | "panel.builder" | "panel.inspector"
        | "panel.properties" | "panel.explorer" | "panel.navigation" | "view.file_explorer" => {
            let page_id = page_id_for_panel(
                command_id
                    .strip_prefix("panel.")
                    .or_else(|| command_id.strip_prefix("view.file_"))
                    .unwrap_or("edit"),
            );
            let mut s = shared.borrow_mut();
            let pid = page_id.to_string();
            s.store.mutate(|state| {
                state.workspace.switch_page_by_id(&pid);
            });
            s.dock_dirty.set(true);
            let panel_id = panel_id_for_slint(&s.store.state().workspace);
            update_panel_schemes(&mut s.input, panel_id);
        }
        "panel.code_editor" => {
            let mut s = shared.borrow_mut();
            s.store.mutate(|state| {
                state.workspace.switch_page_by_id("code");
            });
            s.dock_dirty.set(true);
            let panel_id = panel_id_for_slint(&s.store.state().workspace);
            update_panel_schemes(&mut s.input, panel_id);
        }
        "add_page" => {
            let mut s = shared.borrow_mut();
            s.save_to_active_page();
            s.push_undo("Add page");
            s.store.mutate(|state| {
                if let Some(app) = state.active_app_mut() {
                    crate::panels::navigation::NavigationPanel::create_page(app);
                }
                state.selection.clear();
                state.sync_document_from_app();
            });
            s.load_active_page();
            s.dock_dirty.set(true);
        }
        "selection.delete" => {
            let mut s = shared.borrow_mut();
            let selected_id = s.store.state().selection.primary().cloned();
            if let Some(ref target_id) = selected_id {
                s.fire_signal(target_id, "deleted", serde_json::Map::new());
                s.push_undo("Delete node");
                if let Some(ref mut live) = s.live {
                    let _ = live.remove_node_from_source(target_id);
                }
                s.store.mutate(|state| {
                    state.selection.clear();
                });
                s.sync_builder_document();
            }
        }
        "selection.all" => {
            let mut s = shared.borrow_mut();
            let all_ids = collect_node_ids(s.store.state().builder_document.root.as_ref());
            s.store.mutate(|state| {
                state.selection.clear();
                for id in all_ids {
                    state.selection.extend(id);
                }
            });
        }
        "edit.copy" => {
            let mut s = shared.borrow_mut();
            let target_id = s.store.state().selection.primary().cloned();
            let node = target_id.and_then(|tid| {
                s.live.as_mut().and_then(|l| {
                    let doc = l.document();
                    doc.root.as_ref().and_then(|n| n.find(&tid)).cloned()
                })
            });
            if let Some(node) = node {
                let comp = node.component.clone();
                s.clipboard = Some(node);
                s.add_toast("Copied", &format!("{comp} copied"), "info");
            }
        }
        "edit.paste" => {
            let mut s = shared.borrow_mut();
            let clip = s.clipboard.clone();
            if let Some(ref clip) = clip {
                s.push_undo("Paste");
                let mut next_id = s.store.state().next_node_id;
                let new_node = clone_node_with_new_ids(clip, &mut next_id);
                let new_id = new_node.id.clone();
                let parent_id = s.store.state().selection.primary().cloned();
                if let Some(ref mut live) = s.live {
                    let _ = live.insert_tree_in_source(parent_id.as_deref(), &new_node, None);
                }
                s.store.mutate(|state| {
                    state.next_node_id = next_id;
                    state.selection.select(new_id);
                });
                s.sync_builder_document();
            }
        }
        "edit.cut" => {
            let mut s = shared.borrow_mut();
            let target_id = s.store.state().selection.primary().cloned();
            let target_and_node = target_id.and_then(|tid| {
                s.live.as_mut().and_then(|l| {
                    let doc = l.document();
                    let node = doc.root.as_ref().and_then(|n| n.find(&tid))?.clone();
                    Some((tid, node))
                })
            });
            if let Some((target_id, node)) = target_and_node {
                s.clipboard = Some(node);
                s.push_undo("Cut");
                if let Some(ref mut live) = s.live {
                    let _ = live.remove_node_from_source(&target_id);
                }
                s.store.mutate(|state| {
                    state.selection.clear();
                });
                s.sync_builder_document();
            }
        }
        "edit.duplicate" => {
            let mut s = shared.borrow_mut();
            let sel_id = s.store.state().selection.primary().cloned();
            let next_id_start = s.store.state().next_node_id;
            let target_and_node = sel_id.and_then(|tid| {
                s.live.as_mut().and_then(|l| {
                    let doc = l.document();
                    let node = doc.root.as_ref().and_then(|n| n.find(&tid))?.clone();
                    Some((tid, node, next_id_start))
                })
            });
            if let Some((target_id, node, mut next_id)) = target_and_node {
                s.push_undo("Duplicate");
                let new_node = clone_node_with_new_ids(&node, &mut next_id);
                let new_id = new_node.id.clone();
                if let Some(ref mut live) = s.live {
                    let _ = live.insert_tree_in_source(Some(&target_id), &new_node, None);
                }
                s.store.mutate(|state| {
                    state.next_node_id = next_id;
                    state.selection.select(new_id);
                });
                s.sync_builder_document();
            }
        }
        "notification.dismiss_all" => {
            shared.borrow_mut().store.mutate(|state| {
                state.toasts.clear();
            });
        }
        // ── Navigation ────────────────────────────────────────────
        "navigate.next_tab" => {
            let mut s = shared.borrow_mut();
            let page_count = s
                .store
                .state()
                .active_app()
                .map(|a| a.pages.len())
                .unwrap_or(0);
            if page_count > 1 {
                s.save_to_active_page();
                s.store.mutate(|state| {
                    if let Some(app) = state.active_app_mut() {
                        app.active_page = (app.active_page + 1) % app.pages.len();
                    }
                    state.selection.clear();
                    state.sync_document_from_app();
                });
                s.load_active_page();
            }
        }
        "navigate.prev_tab" => {
            let mut s = shared.borrow_mut();
            let page_count = s
                .store
                .state()
                .active_app()
                .map(|a| a.pages.len())
                .unwrap_or(0);
            if page_count > 1 {
                s.save_to_active_page();
                s.store.mutate(|state| {
                    if let Some(app) = state.active_app_mut() {
                        let len = app.pages.len();
                        app.active_page = if app.active_page == 0 {
                            len - 1
                        } else {
                            app.active_page - 1
                        };
                    }
                    state.selection.clear();
                    state.sync_document_from_app();
                });
                s.load_active_page();
            }
        }
        "navigate.inspector_prev" => {
            let mut s = shared.borrow_mut();
            let ids = collect_node_ids(s.store.state().builder_document.root.as_ref());
            if let Some(current) = s.store.state().selection.primary().cloned() {
                if let Some(idx) = ids.iter().position(|id| *id == current) {
                    if idx > 0 {
                        let new_id = ids[idx - 1].clone();
                        s.store.mutate(|state| state.selection.select(new_id));
                    }
                }
            } else if !ids.is_empty() {
                let first = ids[0].clone();
                s.store.mutate(|state| state.selection.select(first));
            }
        }
        "navigate.inspector_next" => {
            let mut s = shared.borrow_mut();
            let ids = collect_node_ids(s.store.state().builder_document.root.as_ref());
            if let Some(current) = s.store.state().selection.primary().cloned() {
                if let Some(idx) = ids.iter().position(|id| *id == current) {
                    if idx + 1 < ids.len() {
                        let new_id = ids[idx + 1].clone();
                        s.store.mutate(|state| state.selection.select(new_id));
                    }
                }
            } else if !ids.is_empty() {
                let first = ids[0].clone();
                s.store.mutate(|state| state.selection.select(first));
            }
        }
        "search.focus" => {
            let mut s = shared.borrow_mut();
            if s.store.state().workspace.active_page().id != "edit" {
                s.store.mutate(|state| {
                    state.workspace.switch_page_by_id("edit");
                });
                s.dock_dirty.set(true);
                let panel_id = panel_id_for_slint(&s.store.state().workspace);
                update_panel_schemes(&mut s.input, panel_id);
            }
            s.input.set_focus(FocusRegion::Search);
        }
        "view.toggle_left_sidebar" | "navigate.sidebar_toggle" => {
            shared.borrow_mut().store.mutate(|state| {
                state.show_left_sidebar = !state.show_left_sidebar;
            });
        }
        "view.toggle_right_sidebar" => {
            shared.borrow_mut().store.mutate(|state| {
                state.show_right_sidebar = !state.show_right_sidebar;
            });
        }
        "view.toggle_activity_bar" => {
            shared.borrow_mut().store.mutate(|state| {
                state.show_activity_bar = !state.show_activity_bar;
            });
        }
        "view.toggle_grid" => {
            shared.borrow_mut().store.mutate(|state| {
                state.show_grid_overlay = !state.show_grid_overlay;
            });
        }
        "view.zoom_in" => {
            if let Some(w) = weak.upgrade() {
                let cur = w.get_canvas_zoom();
                w.set_canvas_zoom((cur + 0.1).min(3.0));
            }
        }
        "view.zoom_out" => {
            if let Some(w) = weak.upgrade() {
                let cur = w.get_canvas_zoom();
                w.set_canvas_zoom((cur - 0.1).max(0.25));
            }
        }
        "view.zoom_reset" => {
            if let Some(w) = weak.upgrade() {
                w.set_canvas_zoom(1.0);
            }
        }
        "view.zoom_to_fit" => {
            if let Some(w) = weak.upgrade() {
                let pl = w.get_page_layout();
                let pw = pl.page_width;
                let ph = pl.page_height;
                if pw > 0.0 && ph > 0.0 {
                    let s = shared.borrow();
                    let (dock_w, dock_h) = s.dock_area_dims;
                    let canvas_w = (dock_w * 0.5).max(200.0);
                    let canvas_h = (dock_h * 0.85).max(200.0);
                    let fit = (canvas_w / pw).min(canvas_h / ph).clamp(0.25, 3.0);
                    drop(s);
                    w.set_canvas_zoom(fit);
                }
            }
        }
        "tool.move" => {
            shared
                .borrow_mut()
                .store
                .mutate(|s| s.transform_tool = TransformTool::Move);
        }
        "tool.rotate" => {
            shared
                .borrow_mut()
                .store
                .mutate(|s| s.transform_tool = TransformTool::Rotate);
        }
        "tool.scale" => {
            shared
                .borrow_mut()
                .store
                .mutate(|s| s.transform_tool = TransformTool::Scale);
        }
        "file.save" => {
            let mut s = shared.borrow_mut();
            s.save_to_active_page();
            #[cfg(feature = "native")]
            if let Some(ref mut proj) = s.project {
                match proj.save() {
                    Ok(_) => {}
                    Err(e) => s.add_toast("Vault save failed", &e.to_string(), "error"),
                }
            }
            let apps = s.store.state().apps.clone();
            let reg = Arc::clone(&s.registry);
            let tokens = s.store.state().tokens;
            let result = s.persistence.save(&apps, &reg, &tokens);
            match result {
                Ok(path) => {
                    let name = path
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_else(|| path.display().to_string());
                    s.add_toast("Saved", &format!("Project saved to {name}"), "success");
                }
                Err(PersistenceError::NoPath) => {
                    drop(s);
                    execute_command(shared, weak, "file.save_as");
                    return;
                }
                Err(PersistenceError::Cancelled) => {}
                Err(e) => s.add_toast("Save failed", &e.to_string(), "error"),
            }
        }
        "file.save_as" => {
            let mut s = shared.borrow_mut();
            s.save_to_active_page();
            let apps = s.store.state().apps.clone();
            let reg = Arc::clone(&s.registry);
            let tokens = s.store.state().tokens;
            match s.persistence.save_as(&apps, &reg, &tokens) {
                Ok(path) => {
                    let name = path
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_else(|| path.display().to_string());
                    s.add_toast("Saved", &format!("Project saved to {name}"), "success");
                }
                Err(PersistenceError::Cancelled) => {}
                Err(e) => s.add_toast("Save failed", &e.to_string(), "error"),
            }
        }
        "file.open" => {
            {
                let s = shared.borrow();
                if s.persistence.is_dirty() && !crate::persistence::confirm_discard_changes() {
                    return;
                }
            }
            let mut s = shared.borrow_mut();
            s.save_to_active_page();
            match s.persistence.open() {
                Ok(apps) => {
                    let name = s
                        .persistence
                        .project_name()
                        .unwrap_or_else(|| "project".into());
                    s.store.mutate(|state| {
                        state.apps = apps;
                        state.shell_view = ShellView::Launchpad;
                        state.selection.clear();
                    });
                    s.live = None;
                    s.undo_past.clear();
                    s.undo_future.clear();
                    s.add_toast("Opened", &format!("Loaded {name}"), "success");
                }
                Err(PersistenceError::Cancelled) => {}
                Err(e) => s.add_toast("Open failed", &e.to_string(), "error"),
            }
        }
        "file.new" => {
            {
                let s = shared.borrow();
                if s.persistence.is_dirty() && !crate::persistence::confirm_discard_changes() {
                    return;
                }
            }
            let mut s = shared.borrow_mut();
            s.persistence.clear_path();
            s.store.mutate(|state| {
                state.apps = sample_apps();
                state.shell_view = ShellView::Launchpad;
                state.selection.clear();
            });
            s.live = None;
            s.undo_past.clear();
            s.undo_future.clear();
            s.add_toast("New Project", "Started a new project", "info");
        }
        "file.revert" => {
            let mut s = shared.borrow_mut();
            let path = s.persistence.current_path().cloned();
            match path {
                Some(p) => {
                    if !s.persistence.is_dirty() {
                        s.add_toast("Revert", "No changes to revert", "info");
                        return;
                    }
                    if !crate::persistence::confirm_discard_changes() {
                        return;
                    }
                    match s.persistence.open_path(&p) {
                        Ok(apps) => {
                            let name = s
                                .persistence
                                .project_name()
                                .unwrap_or_else(|| "project".into());
                            s.store.mutate(|state| {
                                state.apps = apps;
                                state.shell_view = ShellView::Launchpad;
                                state.selection.clear();
                            });
                            s.live = None;
                            s.undo_past.clear();
                            s.undo_future.clear();
                            s.add_toast("Reverted", &format!("Reloaded {name}"), "success");
                        }
                        Err(e) => s.add_toast("Revert failed", &e.to_string(), "error"),
                    }
                }
                None => {
                    s.add_toast("Revert", "No saved file to revert to", "info");
                }
            }
        }
        #[cfg(feature = "native")]
        "project.open_folder" => {
            let folder = rfd::FileDialog::new()
                .set_title("Open Project Folder")
                .pick_folder();
            if let Some(path) = folder {
                let mut s = shared.borrow_mut();
                match crate::project::ProjectManager::open(&path) {
                    Ok(mut proj) => {
                        let objects = proj.collection().list_objects(None);
                        for obj in &objects {
                            let _ = s.collection.put_object(obj);
                        }
                        let edges = proj.collection().list_edges(None);
                        for edge in &edges {
                            let _ = s.collection.put_edge(edge);
                        }
                        let name = path
                            .file_name()
                            .map(|n| n.to_string_lossy().into_owned())
                            .unwrap_or_else(|| path.display().to_string());
                        s.project = Some(proj);
                        s.add_toast(
                            "Folder opened",
                            &format!("{name} — {} files", objects.len()),
                            "success",
                        );
                    }
                    Err(e) => {
                        s.add_toast("Open folder failed", &e.to_string(), "error");
                    }
                }
            }
        }
        #[cfg(feature = "native")]
        "project.close" => {
            let mut s = shared.borrow_mut();
            if s.project.is_some() {
                s.project = None;
                s.collection = CollectionStore::new();
                s.add_toast("Folder closed", "Project folder closed", "info");
            }
        }
        "facet.add_item" => {
            let mut s = shared.borrow_mut();
            let selected = s.store.state().selection.primary().cloned();
            if let Some(ref node_id) = selected {
                let facet_id = s
                    .store
                    .state()
                    .builder_document
                    .root
                    .as_ref()
                    .and_then(|r| r.find(node_id))
                    .and_then(|n| n.props.get("facet_id"))
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                if let Some(fid) = facet_id {
                    s.push_undo("Add facet item");
                    s.store.mutate(|state| {
                        if let Some(doc) =
                            state.active_app_mut().and_then(|a| a.active_document_mut())
                        {
                            if let Some(def) = doc.facets.get_mut(&fid) {
                                if let FacetDataSource::Static {
                                    ref mut items,
                                    ref mut records,
                                } = def.data
                                {
                                    if let Some(schema_id) = &def.schema_id {
                                        if let Some(schema) = doc.facet_schemas.get(schema_id) {
                                            let rec_id = format!("rec:{}", records.len() + 1);
                                            records.push(schema.default_record(rec_id));
                                        } else {
                                            items.push(serde_json::json!({}));
                                        }
                                    } else {
                                        items.push(serde_json::json!({}));
                                    }
                                }
                            }
                        }
                    });
                    s.sync_builder_document();
                }
            }
        }
        "facet.clear_items" => {
            let mut s = shared.borrow_mut();
            let selected = s.store.state().selection.primary().cloned();
            if let Some(ref node_id) = selected {
                let facet_id = s
                    .store
                    .state()
                    .builder_document
                    .root
                    .as_ref()
                    .and_then(|r| r.find(node_id))
                    .and_then(|n| n.props.get("facet_id"))
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                if let Some(fid) = facet_id {
                    s.push_undo("Clear facet items");
                    s.store.mutate(|state| {
                        if let Some(doc) =
                            state.active_app_mut().and_then(|a| a.active_document_mut())
                        {
                            if let Some(def) = doc.facets.get_mut(&fid) {
                                if let FacetDataSource::Static {
                                    ref mut items,
                                    ref mut records,
                                } = def.data
                                {
                                    items.clear();
                                    records.clear();
                                }
                            }
                        }
                    });
                    s.sync_builder_document();
                }
            }
        }
        "facet.refresh" => {
            let mut s = shared.borrow_mut();
            s.sync_builder_document();
            s.add_toast("Facet", "Data refreshed", "success");
        }
        "schema.create" => {
            let mut s = shared.borrow_mut();
            let counter = s.store.state().next_node_id;
            let schema_id = format!("schema:n{counter}");
            s.push_undo("Create schema");
            let sid = schema_id.clone();
            s.store.mutate(|state| {
                state.next_node_id += 1;
                if let Some(doc) = state.active_app_mut().and_then(|a| a.active_document_mut()) {
                    doc.facet_schemas.insert(
                        sid.clone(),
                        prism_builder::FacetSchema {
                            id: sid.clone(),
                            label: "New Schema".into(),
                            description: String::new(),
                            fields: vec![
                                prism_core::widget::FieldSpec::text("title", "Title").required()
                            ],
                        },
                    );
                }
                state.selected_schema_id = Some(sid);
            });
            s.sync_builder_document();
            s.add_toast("Schema created", &schema_id, "success");
        }
        "schema.delete" => {
            let mut s = shared.borrow_mut();
            let target_id = s.store.state().selected_schema_id.clone().or_else(|| {
                s.store
                    .state()
                    .active_app()
                    .and_then(|a| a.active_document())
                    .and_then(|doc| doc.facet_schemas.keys().next().cloned())
            });
            if let Some(sid) = target_id {
                let label = sid.clone();
                s.push_undo("Delete schema");
                s.store.mutate(|state| {
                    if let Some(doc) = state.active_app_mut().and_then(|a| a.active_document_mut())
                    {
                        doc.facet_schemas.shift_remove(&sid);
                        for facet in doc.facets.values_mut() {
                            if facet.schema_id.as_deref() == Some(sid.as_str()) {
                                facet.schema_id = None;
                            }
                        }
                    }
                    state.selected_schema_id = state
                        .active_app()
                        .and_then(|a| a.active_document())
                        .and_then(|doc| doc.facet_schemas.keys().next().cloned());
                });
                s.sync_builder_document();
                s.add_toast("Schema deleted", &label, "success");
            }
        }
        "schema.add_field" => {
            let mut s = shared.borrow_mut();
            let sid = s.store.state().selected_schema_id.clone().or_else(|| {
                s.store
                    .state()
                    .active_app()
                    .and_then(|a| a.active_document())
                    .and_then(|doc| doc.facet_schemas.keys().next().cloned())
            });
            if let Some(sid) = sid {
                s.push_undo("Add schema field");
                s.store.mutate(|state| {
                    if let Some(doc) = state.active_app_mut().and_then(|a| a.active_document_mut())
                    {
                        if let Some(schema) = doc.facet_schemas.get_mut(&sid) {
                            let n = schema.fields.len() + 1;
                            schema.fields.push(prism_core::widget::FieldSpec::text(
                                format!("field_{n}"),
                                format!("Field {n}"),
                            ));
                        }
                    }
                });
                s.sync_builder_document();
            }
        }
        "schema.delete_field" => {
            let mut s = shared.borrow_mut();
            let sid = s.store.state().selected_schema_id.clone().or_else(|| {
                s.store
                    .state()
                    .active_app()
                    .and_then(|a| a.active_document())
                    .and_then(|doc| doc.facet_schemas.keys().next().cloned())
            });
            if let Some(sid) = sid {
                s.push_undo("Delete schema field");
                s.store.mutate(|state| {
                    if let Some(doc) = state.active_app_mut().and_then(|a| a.active_document_mut())
                    {
                        if let Some(schema) = doc.facet_schemas.get_mut(&sid) {
                            if schema.fields.len() > 1 {
                                schema.fields.pop();
                            }
                        }
                    }
                });
                s.sync_builder_document();
            }
        }
        "panel.schema_designer" => {
            let mut s = shared.borrow_mut();
            s.store.mutate(|state| {
                state.workspace.switch_page_by_id("data");
            });
        }
        "prefab.save_from_selection" => {
            let mut s = shared.borrow_mut();
            let selected = s.store.state().selection.primary().cloned();
            if let Some(ref node_id) = selected {
                let node_snapshot = s
                    .store
                    .state()
                    .builder_document
                    .root
                    .as_ref()
                    .and_then(|r| r.find(node_id))
                    .cloned();
                if let Some(node) = node_snapshot {
                    let counter = s.store.state().next_node_id;
                    let prefab_id = format!("prefab:n{counter}");
                    let label = {
                        let c = &node.component;
                        let mut chars = c.chars();
                        match chars.next() {
                            Some(first) => {
                                first.to_uppercase().collect::<String>() + chars.as_str()
                            }
                            None => c.clone(),
                        }
                    };
                    let label = format!("{label} Prefab");
                    let exposed = auto_expose_slots(&node);
                    let def = PrefabDef {
                        id: prefab_id.clone(),
                        label: label.clone(),
                        description: String::new(),
                        root: node,
                        exposed,
                        variants: vec![],
                        thumbnail: None,
                    };
                    s.push_undo("Save as prefab");
                    s.store.mutate(|state| {
                        state.next_node_id += 1;
                        if let Some(doc) =
                            state.active_app_mut().and_then(|a| a.active_document_mut())
                        {
                            doc.prefabs.insert(prefab_id, def);
                        }
                    });
                    s.sync_builder_document();
                    s.add_toast(
                        "Prefab saved",
                        &format!("'{label}' saved to Prefabs"),
                        "success",
                    );
                }
            }
        }
        other => {
            if let Some(n) = other
                .strip_prefix("navigate.tab.")
                .and_then(|s| s.parse::<usize>().ok())
            {
                let mut s = shared.borrow_mut();
                let page_count = s
                    .store
                    .state()
                    .active_app()
                    .map(|a| a.pages.len())
                    .unwrap_or(0);
                if n >= 1 && n <= page_count {
                    s.save_to_active_page();
                    s.store.mutate(|state| {
                        if let Some(app) = state.active_app_mut() {
                            app.active_page = n - 1;
                        }
                        state.selection.clear();
                        state.sync_document_from_app();
                    });
                    s.load_active_page();
                }
            } else {
                eprintln!("prism-shell: unknown command {other}");
            }
        }
    }
    // Close palette after non-palette command execution
    if !matches!(
        command_id,
        "command_palette.toggle" | "command_palette.close"
    ) {
        let mut s = shared.borrow_mut();
        if s.store.state().command_palette_open {
            s.store.mutate(|state| {
                state.command_palette_open = false;
                state.command_palette_query.clear();
            });
            s.input.set_context("commandPaletteOpen", false);
        }
    }
    if let Some(w) = weak.upgrade() {
        sync_ui_from_shared(shared, &w);
    }
}

// ── Helpers ────────────────────────────────────────────────────────

fn panel_metadata_from_workspace(
    workspace: &prism_dock::DockWorkspace,
) -> (&'static str, &'static str) {
    match workspace.active_page().id.as_str() {
        "code" => {
            let p = CodeEditorPanel::new();
            (p.title(), p.hint())
        }
        "design" => ("Design", "Design components and layouts."),
        "fusion" => ("Fusion", "Node-based creative composition."),
        "preview" => ("Preview", "Interactive preview with live signals."),
        _ => ("Editor", "Build your page visually."),
    }
}

fn serialize_addr(addr: &prism_dock::NodeAddress) -> String {
    addr.0
        .iter()
        .map(|&b| if b { "1" } else { "0" })
        .collect::<Vec<_>>()
        .join(".")
}

fn deserialize_addr(s: &str) -> prism_dock::NodeAddress {
    if s.is_empty() {
        return prism_dock::NodeAddress::root();
    }
    prism_dock::NodeAddress(
        s.split('.')
            .filter(|p| !p.is_empty())
            .map(|p| p == "1")
            .collect(),
    )
}

fn collect_split_bounds(
    node: &prism_dock::DockNode,
    bounds: &prism_dock::Rect,
    addr: &prism_dock::NodeAddress,
    out: &mut std::collections::HashMap<String, (f32, f32)>,
) {
    if let prism_dock::DockNode::Split {
        axis,
        ratio,
        first,
        second,
        ..
    } = node
    {
        let (origin, size) = match axis {
            prism_dock::Axis::Horizontal => (bounds.x, bounds.width),
            prism_dock::Axis::Vertical => (bounds.y, bounds.height),
        };
        out.insert(serialize_addr(addr), (origin, size));

        let half_div = prism_dock::layout::DIVIDER_THICKNESS / 2.0;
        match axis {
            prism_dock::Axis::Horizontal => {
                let split_x = bounds.x + ratio * bounds.width;
                let fb = prism_dock::Rect::new(
                    bounds.x,
                    bounds.y,
                    (split_x - half_div - bounds.x).max(0.0),
                    bounds.height,
                );
                let sb = prism_dock::Rect::new(
                    split_x + half_div,
                    bounds.y,
                    (bounds.x + bounds.width - split_x - half_div).max(0.0),
                    bounds.height,
                );
                collect_split_bounds(first, &fb, &addr.first(), out);
                collect_split_bounds(second, &sb, &addr.second(), out);
            }
            prism_dock::Axis::Vertical => {
                let split_y = bounds.y + ratio * bounds.height;
                let fb = prism_dock::Rect::new(
                    bounds.x,
                    bounds.y,
                    bounds.width,
                    (split_y - half_div - bounds.y).max(0.0),
                );
                let sb = prism_dock::Rect::new(
                    bounds.x,
                    split_y + half_div,
                    bounds.width,
                    (bounds.y + bounds.height - split_y - half_div).max(0.0),
                );
                collect_split_bounds(first, &fb, &addr.first(), out);
                collect_split_bounds(second, &sb, &addr.second(), out);
            }
        }
    }
}

fn push_signal_panel_data(
    models: &PersistentModels,
    window: &AppWindow,
    doc: &BuilderDocument,
    registry: &ComponentRegistry,
    selection: &SelectionModel,
) {
    use crate::panels::signals::SignalsPanel;

    let selected = selection.as_option();
    eprintln!(
        "[signals-panel] selected={:?} doc_connections={}",
        selected,
        doc.connections.len()
    );
    let conn_rows = match &selected {
        Some(node_id) => SignalsPanel::connections_for_node(doc, node_id),
        None => SignalsPanel::connection_rows(doc),
    };
    let conn_items: Vec<crate::SignalConnectionItem> = conn_rows
        .iter()
        .map(|r| crate::SignalConnectionItem {
            id: SharedString::from(r.id.as_str()),
            source_label: SharedString::from(r.source_label.as_str()),
            signal: SharedString::from(r.signal.as_str()),
            target_label: SharedString::from(r.target_label.as_str()),
            action_kind: SharedString::from(r.action_kind.as_str()),
            action_summary: SharedString::from(r.action_summary.as_str()),
        })
        .collect();
    let count = sync_model(&models.signal_connections, &conn_items);
    window.set_signal_connections_count(count);

    let sig_items: Vec<crate::SignalItem> = if let Some(node_id) = &selected {
        let available = SignalsPanel::available_signals(node_id, doc, registry);
        available
            .iter()
            .map(|s| crate::SignalItem {
                name: SharedString::from(s.signal_name.as_str()),
                description: SharedString::from(s.description.as_str()),
                payload_summary: SharedString::from(s.payload_summary.as_str()),
            })
            .collect()
    } else {
        vec![]
    };
    let count = sync_model(&models.signal_list, &sig_items);
    window.set_signal_list_count(count);

    let targets = SignalsPanel::available_targets(doc);
    let target_items: Vec<crate::TargetNodeItem> = targets
        .iter()
        .map(|t| crate::TargetNodeItem {
            node_id: SharedString::from(t.node_id.as_str()),
            label: SharedString::from(t.label.as_str()),
            component: SharedString::from(t.component.as_str()),
        })
        .collect();
    let count = sync_model(&models.signal_target_nodes, &target_items);
    window.set_signal_target_nodes_count(count);

    let mut target_labels: Vec<SharedString> = Vec::with_capacity(targets.len() + 1);
    target_labels.push(SharedString::from("(self)"));
    for t in &targets {
        target_labels.push(SharedString::from(format!("{} [{}]", t.label, t.component)));
    }
    let target_label_model = std::rc::Rc::new(slint::VecModel::from(target_labels));
    window.set_signal_target_labels(slint::ModelRc::from(target_label_model));
}

fn push_widget_toolbar(
    models: &PersistentModels,
    window: &AppWindow,
    doc: &BuilderDocument,
    registry: &ComponentRegistry,
    selection: &SelectionModel,
) {
    let actions = selection
        .primary()
        .and_then(|sel_id| doc.root.as_ref().and_then(|n| n.find(sel_id)))
        .and_then(|node| registry.get(&node.component))
        .map(|comp| comp.toolbar_actions())
        .unwrap_or_default();
    let items: Vec<WidgetToolbarItem> = actions
        .iter()
        .map(|a| WidgetToolbarItem {
            action_id: SharedString::from(&a.id),
            label: SharedString::from(&a.label),
            group: SharedString::from(a.group.as_deref().unwrap_or("")),
        })
        .collect();
    let count = sync_model(&models.widget_toolbar, &items);
    window.set_widget_toolbar_count(count);
}

fn push_schema_list(
    models: &PersistentModels,
    window: &AppWindow,
    doc: &BuilderDocument,
    selected_id: Option<&str>,
) {
    let items: Vec<crate::SchemaListItem> = doc
        .facet_schemas
        .values()
        .map(|s| crate::SchemaListItem {
            id: SharedString::from(&s.id),
            label: SharedString::from(&s.label),
            field_count: s.fields.len() as i32,
            selected: selected_id == Some(s.id.as_str()),
        })
        .collect();
    let count = sync_model(&models.schema_list, &items);
    window.set_schema_list_count(count);
    let label = selected_id
        .and_then(|id| doc.facet_schemas.get(id))
        .map(|s| s.label.as_str())
        .unwrap_or("");
    window.set_selected_schema_label(SharedString::from(label));
}

fn push_navigation_panel_data(
    models: &PersistentModels,
    window: &AppWindow,
    app: Option<&PrismApp>,
) {
    let nav_items: Vec<crate::NavPageItem> = app
        .map(|app| {
            crate::panels::navigation::NavigationPanel::page_rows(app)
                .into_iter()
                .map(|row| crate::NavPageItem {
                    index: row.index as i32,
                    id: SharedString::from(&row.id),
                    title: SharedString::from(&row.title),
                    route: SharedString::from(&row.route),
                    is_active: row.is_active,
                    node_count: row.node_count as i32,
                    link_count: row.link_count as i32,
                })
                .collect()
        })
        .unwrap_or_default();
    let count = sync_model(&models.nav_pages, &nav_items);
    window.set_nav_pages_count(count);

    let nav_style_label = app
        .map(|app| match app.navigation.style {
            prism_builder::app::NavigationStyle::Tabs => "Tabs",
            prism_builder::app::NavigationStyle::Sidebar => "Sidebar",
            prism_builder::app::NavigationStyle::BottomBar => "Bottom Bar",
            prism_builder::app::NavigationStyle::None => "None",
        })
        .unwrap_or("Tabs");
    window.set_nav_style_label(SharedString::from(nav_style_label));

    // Graph nodes
    use crate::panels::navigation::NavigationPanel;
    let graph_nodes: Vec<crate::NavGraphNode> = app
        .map(|app| {
            NavigationPanel::graph_nodes(app)
                .into_iter()
                .map(|n| crate::NavGraphNode {
                    page_index: n.page_index as i32,
                    id: SharedString::from(&n.id),
                    title: SharedString::from(&n.title),
                    route: SharedString::from(&n.route),
                    is_active: n.is_active,
                    node_count: n.node_count as i32,
                    link_count: n.link_count as i32,
                    x: n.x,
                    y: n.y,
                    w: n.width,
                    h: n.height,
                })
                .collect()
        })
        .unwrap_or_default();
    let gn_count = sync_model(&models.nav_graph_nodes, &graph_nodes);
    window.set_nav_graph_nodes_count(gn_count);

    // Graph edges — pre-compute line endpoints from node centers
    let graph_edges: Vec<crate::NavGraphEdge> = app
        .map(|app| {
            NavigationPanel::graph_edges(app)
                .into_iter()
                .map(|e| {
                    let (x1, y1) = graph_nodes
                        .get(e.source_page_index)
                        .map(|n| (n.x + n.w / 2.0, n.y + n.h / 2.0))
                        .unwrap_or((0.0, 0.0));
                    let (x2, y2) = graph_nodes
                        .get(e.target_page_index)
                        .map(|n| (n.x + n.w / 2.0, n.y + n.h / 2.0))
                        .unwrap_or((0.0, 0.0));
                    crate::NavGraphEdge {
                        id: SharedString::from(&e.id),
                        source_page_index: e.source_page_index as i32,
                        target_page_index: e.target_page_index as i32,
                        label: SharedString::from(&e.label),
                        kind: SharedString::from(&e.kind),
                        x1,
                        y1,
                        x2,
                        y2,
                    }
                })
                .collect()
        })
        .unwrap_or_default();
    let ge_count = sync_model(&models.nav_graph_edges, &graph_edges);
    window.set_nav_graph_edges_count(ge_count);
}

fn clear_href_on_node(node: &mut prism_builder::document::Node, target_id: &str) {
    if node.id == target_id {
        if let Some(m) = node.props.as_object_mut() {
            m.remove("href");
        }
        return;
    }
    for child in &mut node.children {
        clear_href_on_node(child, target_id);
    }
}

/// Resolve facet data for kinds that need external execution (Script,
/// ObjectQuery, Lookup). Called before the render walker so
/// `FacetDef::resolve_items` can read `resolved_data`.
#[cfg(feature = "native")]
fn resolve_facet_data(doc: &mut BuilderDocument, collection: &CollectionStore) {
    for facet in doc.facets.values_mut() {
        match &facet.kind {
            FacetKind::Script {
                ref source,
                ref language,
                ref graph,
            } => {
                let effective_source = match language {
                    ScriptLanguage::VisualGraph => {
                        if let Some(g) = graph {
                            use prism_core::language::luau::LuauVisualLanguage;
                            use prism_core::language::visual::VisualLanguage;
                            match LuauVisualLanguage::new().compile(g) {
                                Ok(compiled) => compiled,
                                Err(e) => {
                                    eprintln!("[facet] graph compile error: {}", e.message);
                                    facet.resolved_data = None;
                                    continue;
                                }
                            }
                        } else {
                            facet.resolved_data = None;
                            continue;
                        }
                    }
                    ScriptLanguage::Luau => source.clone(),
                };
                if effective_source.is_empty() {
                    facet.resolved_data = None;
                    continue;
                }
                match prism_daemon::modules::luau_module::exec(&effective_source, None) {
                    Ok(result) => {
                        if let Some(arr) = result.as_array() {
                            facet.resolved_data = Some(arr.clone());
                        } else {
                            facet.resolved_data = Some(vec![result]);
                        }
                    }
                    Err(e) => {
                        eprintln!("[facet] script error: {e}");
                        facet.resolved_data = None;
                    }
                }
            }
            FacetKind::ObjectQuery { query } => {
                let entity_type = match &query.object_type {
                    Some(t) if !t.is_empty() => t,
                    _ => {
                        facet.resolved_data = None;
                        continue;
                    }
                };
                let objects = collection.list_objects(Some(&ObjectFilter {
                    types: Some(vec![entity_type.clone()]),
                    exclude_deleted: true,
                    ..Default::default()
                }));
                let mut items: Vec<serde_json::Value> = objects
                    .iter()
                    .filter_map(|obj| serde_json::to_value(obj).ok())
                    .collect();

                query.apply(&mut items);
                facet.resolved_data = if items.is_empty() { None } else { Some(items) };
            }
            FacetKind::Lookup {
                source_entity,
                edge_type,
                target_entity,
            } => {
                if source_entity.is_empty() || edge_type.is_empty() || target_entity.is_empty() {
                    facet.resolved_data = None;
                    continue;
                }
                let sources = collection.list_objects(Some(&ObjectFilter {
                    types: Some(vec![source_entity.clone()]),
                    exclude_deleted: true,
                    ..Default::default()
                }));
                let mut seen = std::collections::HashSet::new();
                let mut targets = Vec::new();
                for src in &sources {
                    let edges = collection.list_edges(Some(&EdgeFilter {
                        source_id: Some(src.id.clone()),
                        relation: Some(edge_type.clone()),
                        ..Default::default()
                    }));
                    for edge in &edges {
                        if !seen.insert(edge.target_id.as_str().to_string()) {
                            continue;
                        }
                        if let Some(obj) = collection.get_object(&edge.target_id) {
                            if obj.type_name == *target_entity && obj.deleted_at.is_none() {
                                if let Ok(val) = serde_json::to_value(&obj) {
                                    targets.push(val);
                                }
                            }
                        }
                    }
                }
                facet.resolved_data = if targets.is_empty() {
                    None
                } else {
                    Some(targets)
                };
            }
            _ => {}
        }
    }
}

/// Resolve `data_query` for core widget nodes. Walks the document tree,
/// finds nodes whose component has a declared `data_query` + `data_key`,
/// queries the `CollectionStore`, and returns a map of node_id → resolved
/// data `Value` (an object with the `data_key` mapped to the result array).
///
/// Follows the same pre-resolution pattern as [`resolve_facet_data`]:
/// the shell resolves data before the render walker runs, and the
/// render context merges it into node props via `widget_data`.
#[cfg(feature = "native")]
fn resolve_widget_data(
    doc: &BuilderDocument,
    collection: &CollectionStore,
) -> HashMap<String, serde_json::Value> {
    use prism_builder::core_widget::collect_all_contributions;

    let contributions = collect_all_contributions();
    let contrib_map: HashMap<&str, &prism_core::widget::WidgetContribution> = contributions
        .iter()
        .filter(|c| c.data_query.is_some() && c.data_key.is_some())
        .map(|c| (c.id.as_str(), c))
        .collect();

    if contrib_map.is_empty() {
        return HashMap::new();
    }

    let mut result = HashMap::new();
    fn walk_nodes(
        node: &Node,
        contrib_map: &HashMap<&str, &prism_core::widget::WidgetContribution>,
        collection: &CollectionStore,
        result: &mut HashMap<String, serde_json::Value>,
    ) {
        if let Some(contrib) = contrib_map.get(node.component.as_str()) {
            let query = contrib.data_query.as_ref().unwrap();
            let data_key = contrib.data_key.as_ref().unwrap();

            let mut items: Vec<serde_json::Value> = if let Some(obj_type) = &query.object_type {
                if obj_type.is_empty() {
                    Vec::new()
                } else {
                    collection
                        .list_objects(Some(&ObjectFilter {
                            types: Some(vec![obj_type.clone()]),
                            exclude_deleted: true,
                            ..Default::default()
                        }))
                        .iter()
                        .filter_map(|obj| serde_json::to_value(obj).ok())
                        .collect()
                }
            } else {
                collection
                    .list_objects(Some(&ObjectFilter {
                        exclude_deleted: true,
                        ..Default::default()
                    }))
                    .iter()
                    .filter_map(|obj| serde_json::to_value(obj).ok())
                    .collect()
            };

            query.apply(&mut items);
            result.insert(
                node.id.clone(),
                serde_json::json!({ data_key: items }),
            );
        }
        for child in &node.children {
            walk_nodes(child, &contrib_map, collection, result);
        }
    }

    if let Some(root) = &doc.root {
        walk_nodes(root, &contrib_map, collection, &mut result);
    }
    result
}

/// Build a Luau script that includes the signal handler stdlib, the
/// page source (which defines the handler functions), and a call to
/// the target handler. The stdlib collects actions into `_actions`
/// which the shell reads back after execution.
#[cfg(feature = "native")]
fn build_handler_script(page_source: &str, handler_name: &str) -> String {
    format!(
        r#"local _actions = {{}}

function set_property(node_id, key, value)
    table.insert(_actions, {{ type = "set_property", node_id = node_id, key = key, value = value }})
end

function toggle_visibility(node_id)
    table.insert(_actions, {{ type = "toggle_visibility", node_id = node_id }})
end

function navigate(route)
    table.insert(_actions, {{ type = "navigate", route = route }})
end

function emit_signal(node_id, signal)
    table.insert(_actions, {{ type = "emit_signal", node_id = node_id, signal = signal }})
end

{page_source}

local _result = {handler_name}(event)
if type(_result) == "table" then
    _result._actions = _actions
    return _result
end
return {{ _actions = _actions }}
"#
    )
}

fn push_dock_layout(
    models: &PersistentModels,
    window: &AppWindow,
    workspace: &prism_dock::DockWorkspace,
    dims: (f32, f32),
) {
    let (w, h) = dims;
    if w <= 0.0 || h <= 0.0 {
        return;
    }

    let dock = workspace.active_dock();
    let bounds = prism_dock::Rect::new(0.0, 0.0, w, h);
    let layout = prism_dock::compute_layout(&dock.root, bounds.clone());

    let mut split_bounds = std::collections::HashMap::new();
    collect_split_bounds(
        &dock.root,
        &bounds,
        &prism_dock::NodeAddress::root(),
        &mut split_bounds,
    );

    let mut panels: Vec<DockPanelRect> = Vec::new();
    let mut dividers: Vec<DockDividerRect> = Vec::new();

    for lr in &layout {
        let addr_str = serialize_addr(&lr.addr);
        let addr_key = SharedString::from(addr_str.as_str());
        match &lr.kind {
            prism_dock::LayoutNodeKind::TabGroup { tabs, active } => {
                let active_id = tabs.get(*active).cloned().unwrap_or_default();
                let label = prism_dock::PanelKind::from_id(&active_id)
                    .map(|k| k.meta().label)
                    .unwrap_or("Panel");
                let tab_items: Vec<DockTabItem> = tabs
                    .iter()
                    .enumerate()
                    .map(|(i, pid)| {
                        let tab_label = prism_dock::PanelKind::from_id(pid)
                            .map(|k| k.meta().label)
                            .unwrap_or("Panel");
                        DockTabItem {
                            panel_id: SharedString::from(pid.as_str()),
                            label: SharedString::from(tab_label),
                            active: i == *active,
                        }
                    })
                    .collect();
                let tab_model = Rc::new(VecModel::from(tab_items));
                panels.push(DockPanelRect {
                    addr_key,
                    x: lr.rect.x,
                    y: lr.rect.y,
                    width: lr.rect.width,
                    height: lr.rect.height,
                    panel_id: SharedString::from(active_id.as_str()),
                    panel_label: SharedString::from(label),
                    tabs: ModelRc::from(tab_model as Rc<dyn Model<Data = DockTabItem>>),
                });
            }
            prism_dock::LayoutNodeKind::SplitDivider { axis, .. } => {
                let (parent_origin, parent_size) =
                    split_bounds.get(&addr_str).copied().unwrap_or((
                        0.0,
                        if *axis == prism_dock::Axis::Horizontal {
                            w
                        } else {
                            h
                        },
                    ));
                dividers.push(DockDividerRect {
                    addr_key,
                    x: lr.rect.x,
                    y: lr.rect.y,
                    width: lr.rect.width,
                    height: lr.rect.height,
                    is_horizontal: *axis == prism_dock::Axis::Horizontal,
                    parent_origin,
                    parent_size,
                });
            }
        }
    }

    let count = sync_model(&models.dock_panels, &panels);
    window.set_dock_panels_count(count);
    let count = sync_model(&models.dock_dividers, &dividers);
    window.set_dock_dividers_count(count);

    // Workflow pages
    let active_page_id = workspace.active_page().id.as_str();
    let page_items: Vec<WorkflowPageItem> = workspace
        .pages()
        .iter()
        .map(|p| WorkflowPageItem {
            id: SharedString::from(p.id.as_str()),
            label: SharedString::from(p.label.as_str()),
            active: p.id == active_page_id,
        })
        .collect();
    let count = sync_model(&models.workflow_pages, &page_items);
    window.set_workflow_pages_count(count);
}

fn flatten_inspector_nodes(root: Option<&Node>, selection: &SelectionModel) -> Vec<InspectorNode> {
    let mut items = Vec::new();
    if let Some(node) = root {
        flatten_walk(node, 0, selection, &mut items);
    }
    items
}

fn flatten_walk(node: &Node, depth: i32, selection: &SelectionModel, out: &mut Vec<InspectorNode>) {
    out.push(InspectorNode {
        id: SharedString::from(&node.id),
        component_id: SharedString::from(&node.component),
        depth,
        selected: selection.contains(&node.id),
        kind: SharedString::from("node"),
    });
    for child in &node.children {
        flatten_walk(child, depth + 1, selection, out);
    }
}

fn flatten_inspector_grid(doc: &BuilderDocument, selection: &SelectionModel) -> Vec<InspectorNode> {
    let grid = match &doc.page_layout.grid {
        Some(g) => g,
        None => return Vec::new(),
    };

    let child_map: std::collections::HashMap<&str, &str> = doc
        .root
        .as_ref()
        .map(|r| {
            r.children
                .iter()
                .map(|c| (c.id.as_str(), c.component.as_str()))
                .collect()
        })
        .unwrap_or_default();

    let mut items = Vec::new();
    inspect_grid_cell(grid, &mut Vec::new(), 0, &child_map, selection, &mut items);
    items
}

fn inspect_grid_cell(
    cell: &GridCell,
    path: &mut Vec<usize>,
    depth: i32,
    child_map: &std::collections::HashMap<&str, &str>,
    selection: &SelectionModel,
    out: &mut Vec<InspectorNode>,
) {
    match cell {
        GridCell::Leaf { node_id } => {
            let path_str = prism_builder::path_to_string(path);
            match node_id {
                Some(id) => {
                    let comp = child_map.get(id.as_str()).copied().unwrap_or("?");
                    out.push(InspectorNode {
                        id: SharedString::from(id.as_str()),
                        component_id: SharedString::from(comp),
                        depth,
                        selected: selection.contains(id),
                        kind: SharedString::from("node"),
                    });
                }
                None => {
                    out.push(InspectorNode {
                        id: SharedString::from(format!("cell:{path_str}")),
                        component_id: SharedString::from("(empty)"),
                        depth,
                        selected: false,
                        kind: SharedString::from("empty"),
                    });
                }
            }
        }
        GridCell::Split {
            direction,
            children,
            ..
        } => {
            let label = match direction {
                prism_builder::layout::SplitDirection::Horizontal => "Rows",
                prism_builder::layout::SplitDirection::Vertical => "Columns",
            };
            let path_str = prism_builder::path_to_string(path);
            out.push(InspectorNode {
                id: SharedString::from(format!("cell:{path_str}")),
                component_id: SharedString::from(format!("{label} ({})", children.len())),
                depth,
                selected: false,
                kind: SharedString::from("row"),
            });
            for (i, child) in children.iter().enumerate() {
                path.push(i);
                inspect_grid_cell(child, path, depth + 1, child_map, selection, out);
                path.pop();
            }
        }
    }
}

fn field_kind_for_key(inner: &ShellInner, key: &str) -> Option<String> {
    let state = inner.store.state();
    let selected = state.selection.primary()?;
    let node = state
        .builder_document
        .root
        .as_ref()
        .and_then(|n| n.find(selected))?;
    let component = inner.registry.get(&node.component)?;
    Some(
        component
            .schema()
            .into_iter()
            .find(|s| s.key == key)
            .map(|s| match s.kind {
                FieldKind::Number(_) => "number",
                FieldKind::Integer(_) => "integer",
                FieldKind::Boolean => "boolean",
                FieldKind::Color => "color",
                FieldKind::File(_) => "file",
                FieldKind::Select(_) => "select",
                _ => "text",
            })
            .unwrap_or("text")
            .to_string(),
    )
}

pub(crate) fn mime_from_extension(ext: &str) -> &'static str {
    match ext.to_ascii_lowercase().as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "bmp" => "image/bmp",
        "ico" => "image/x-icon",
        "pdf" => "application/pdf",
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        "ogg" => "audio/ogg",
        _ => "application/octet-stream",
    }
}

fn resolve_schema_id(state: &AppState) -> Option<String> {
    state
        .selected_schema_id
        .clone()
        .filter(|id| {
            state
                .active_app()
                .and_then(|a| a.active_document())
                .map(|doc| doc.facet_schemas.contains_key(id))
                .unwrap_or(false)
        })
        .or_else(|| {
            state
                .active_app()
                .and_then(|a| a.active_document())
                .and_then(|doc| doc.facet_schemas.keys().next().cloned())
        })
}

fn format_slider_value(val: f32) -> String {
    if val.fract() == 0.0 && val.is_finite() {
        format!("{}", val as i64)
    } else {
        format!("{:.2}", val)
    }
}

fn format_value_for_source(value: &str, kind: Option<&str>) -> String {
    match kind {
        Some("number") => format!("{value}px"),
        Some("integer") => value.to_string(),
        Some("boolean") => value.to_string(),
        Some("color") => value.to_string(),
        _ => format!(
            "\"{}\"",
            prism_builder::slint_source::escape_slint_string(value)
        ),
    }
}

/// Translate a schema property key to its Slint source equivalent and
/// format the value appropriately. Returns `(slint_key, formatted_value)`.
fn slint_source_key_for_edit(
    inner: &ShellInner,
    schema_key: &str,
    value: &str,
    kind: Option<&str>,
) -> (String, String) {
    let component_type = {
        let state = inner.store.state();
        state.selection.primary().and_then(|sel| {
            state
                .builder_document
                .root
                .as_ref()
                .and_then(|r| r.find(sel))
                .map(|n| n.component.clone())
        })
    };
    let slint_key = match (component_type.as_deref(), schema_key) {
        (Some("image"), "fit") => Some("image-fit"),
        _ => None,
    };
    if let Some(slint_key) = slint_key {
        if let Some(ref live) = inner.live {
            let selected = inner.store.state().selection.primary().cloned();
            if let Some(ref id) = selected {
                if let Some(span) = live.source_map.span_for_node(id) {
                    if span.props.iter().any(|p| p.key == slint_key) {
                        return (slint_key.to_string(), value.to_string());
                    }
                }
            }
        }
    }
    (schema_key.to_string(), format_value_for_source(value, kind))
}

fn default_props_for_component(component: &str) -> serde_json::Value {
    match component {
        "text" => json!({ "body": "New paragraph", "level": "paragraph" }),
        "image" => json!({ "src": "", "alt": "Image", "fit": "cover" }),
        "container" => json!({ "spacing": 12 }),
        "form" => json!({ "method": "post" }),
        "input" => json!({ "name": "field", "type": "text", "placeholder": "Enter value" }),
        "button" => json!({ "text": "Button" }),
        "card" => json!({ "title": "Card", "body": "" }),
        "code" => json!({ "code": "// code here", "language": "" }),
        "divider" => json!({}),
        "spacer" => json!({ "height": 24 }),
        "columns" => json!({ "gap": 16 }),
        "list" => json!({ "ordered": false }),
        "table" => json!({ "headers": "Column 1, Column 2" }),
        "tabs" => json!({ "labels": "Tab 1, Tab 2" }),
        "accordion" => json!({ "title": "Section", "open": true }),
        "facet" => json!({ "facet_id": "" }),
        _ => json!({}),
    }
}

fn component_palette_items(doc: &BuilderDocument) -> Vec<ComponentPaletteItem> {
    let mut items = Vec::new();

    let make_header = |cat: &str| ComponentPaletteItem {
        component_type: SharedString::default(),
        label: SharedString::from(cat),
        description: SharedString::default(),
        category: SharedString::from(cat),
        is_header: true,
    };
    let make_item = |ty: &str, label: &str, desc: &str, cat: &str| ComponentPaletteItem {
        component_type: SharedString::from(ty),
        label: SharedString::from(label),
        description: SharedString::from(desc),
        category: SharedString::from(cat),
        is_header: false,
    };

    #[allow(clippy::type_complexity)]
    let static_categories: &[(&str, &[(&str, &str, &str)])] = &[
        (
            "CONTENT",
            &[
                ("text", "Text", "Paragraph, heading, or link"),
                ("image", "Image", "Image placeholder"),
                ("code", "Code", "Preformatted code block"),
            ],
        ),
        (
            "LAYOUT",
            &[
                ("container", "Container", "Layout wrapper for children"),
                ("columns", "Columns", "Side-by-side horizontal layout"),
                ("list", "List", "Ordered or unordered list"),
                ("table", "Table", "Data table with column headers"),
                ("tabs", "Tabs", "Tabbed content panels"),
                ("accordion", "Accordion", "Collapsible content section"),
            ],
        ),
        (
            "FORM",
            &[
                ("button", "Button", "Submit / action button"),
                ("input", "Input", "Text / email / password field"),
                ("form", "Form", "HTML form wrapper"),
            ],
        ),
        (
            "DECORATION",
            &[
                ("divider", "Divider", "Horizontal separator line"),
                ("spacer", "Spacer", "Vertical spacing element"),
            ],
        ),
    ];

    for (category, components) in static_categories {
        items.push(make_header(category));
        for (ty, label, desc) in *components {
            items.push(make_item(ty, label, desc, category));
        }
    }

    // PREFABS section: builtin "card" + user-defined prefabs
    items.push(make_header("PREFABS"));
    items.push(make_item(
        "card",
        "Card",
        "Bordered card with title and body",
        "PREFABS",
    ));
    for (id, def) in &doc.prefabs {
        if id != "card" {
            let desc = if def.description.is_empty() {
                format!("User prefab: {}", def.label)
            } else {
                def.description.clone()
            };
            items.push(make_item(id.as_str(), &def.label, &desc, "PREFABS"));
        }
    }

    // PROGRAMMATIC section
    items.push(make_header("PROGRAMMATIC"));
    items.push(make_item(
        "facet",
        "Facet",
        "Repeat a prefab template over a data source",
        "PROGRAMMATIC",
    ));

    items
}

fn clone_node_with_new_ids(node: &Node, counter: &mut u64) -> Node {
    let new_id = format!("n{}", *counter);
    *counter += 1;
    Node {
        id: new_id,
        component: node.component.clone(),
        props: node.props.clone(),
        children: node
            .children
            .iter()
            .map(|c| clone_node_with_new_ids(c, counter))
            .collect(),
        layout_mode: node.layout_mode.clone(),
        transform: node.transform.clone(),
        modifiers: node.modifiers.clone(),
        style: node.style.clone(),
    }
}

fn push_breadcrumbs(
    models: &PersistentModels,
    window: &AppWindow,
    doc: &BuilderDocument,
    selection: &SelectionModel,
) {
    let selected = selection.primary();
    let items: Vec<BreadcrumbItem> = if let (Some(root), Some(target)) = (&doc.root, selected) {
        let mut path = Vec::new();
        find_path_to_node(root, target, &mut path);
        path.iter()
            .enumerate()
            .map(|(i, (id, component))| BreadcrumbItem {
                id: SharedString::from(id.as_str()),
                label: SharedString::from(component.as_str()),
                has_separator: i > 0,
            })
            .collect()
    } else {
        Vec::new()
    };
    let count = sync_model(&models.breadcrumbs, &items);
    window.set_breadcrumbs_count(count);
}

fn find_path_to_node(node: &Node, target: &str, path: &mut Vec<(String, String)>) -> bool {
    path.push((node.id.clone(), node.component.clone()));
    if node.id == target {
        return true;
    }
    for child in &node.children {
        if find_path_to_node(child, target, path) {
            return true;
        }
    }
    path.pop();
    false
}

fn collect_node_ids(root: Option<&Node>) -> Vec<NodeId> {
    let mut ids = Vec::new();
    if let Some(node) = root {
        collect_ids_walk(node, &mut ids);
    }
    ids
}

fn collect_ids_walk(node: &Node, ids: &mut Vec<NodeId>) {
    ids.push(node.id.clone());
    for child in &node.children {
        collect_ids_walk(child, ids);
    }
}

// ── Page layout data ──────────────────────────────────────────────

fn push_page_layout_data(
    models: &PersistentModels,
    window: &AppWindow,
    doc: &BuilderDocument,
    show_grid: bool,
    viewport_width: f32,
) {
    let pl = &doc.page_layout;
    let resolved = pl.resolved_size();
    let is_responsive = resolved.is_none();
    let (pw, ph) = resolved
        .map(|s| (s.width, s.height))
        .unwrap_or((viewport_width, viewport_width * 0.625));

    let size_label = match pl.size {
        PageSize::Responsive => "Responsive",
        PageSize::A4 => "A4",
        PageSize::A3 => "A3",
        PageSize::A5 => "A5",
        PageSize::Letter => "Letter",
        PageSize::Legal => "Legal",
        PageSize::Tabloid => "Tabloid",
        PageSize::Custom { .. } => "Custom",
    };

    window.set_page_layout(PageLayoutData {
        page_width: pw,
        page_height: ph,
        margin_top: pl.margins.top,
        margin_right: pl.margins.right,
        margin_bottom: pl.margins.bottom,
        margin_left: pl.margins.left,
        column_gap: pl.column_gap,
        row_gap: pl.row_gap,
        cell_count: pl.leaf_count() as i32,
        show_grid,
        is_responsive,
        page_size_label: SharedString::from(size_label),
    });

    // Gutters are no longer needed with the recursive grid model —
    // each split has its own gap handled by flatten_cells.
    sync_model(&models.column_gutters, &[]);
    window.set_column_gutters_count(0);
    sync_model(&models.row_gutters, &[]);
    window.set_row_gutters_count(0);
}

fn apply_facet_edit(def: &mut FacetDef, key: &str, value: &str) {
    match key {
        "kind" => {
            def.kind = FacetKind::from_tag(value);
        }
        "component_id" => def.set_component_id(value),
        "label" => def.label = value.to_string(),
        "direction" => {
            def.layout.direction = match value {
                "row" | "Row" => FacetDirection::Row,
                _ => FacetDirection::Column,
            };
        }
        "gap" => {
            if let Ok(v) = value.parse::<f32>() {
                def.layout.gap = v;
            }
        }
        "source_kind" => {
            def.data = match value {
                "resource" => {
                    let id = match &def.data {
                        FacetDataSource::Resource { id } => id.clone(),
                        FacetDataSource::Query { source, .. } => source.clone(),
                        _ => String::new(),
                    };
                    FacetDataSource::Resource { id }
                }
                "query" => {
                    let source = match &def.data {
                        FacetDataSource::Resource { id } => id.clone(),
                        FacetDataSource::Query { source, .. } => source.clone(),
                        _ => String::new(),
                    };
                    FacetDataSource::Query {
                        source,
                        query: prism_core::widget::DataQuery::default(),
                    }
                }
                _ => FacetDataSource::Static {
                    items: vec![],
                    records: vec![],
                },
            };
        }
        "source_id" => match &mut def.data {
            FacetDataSource::Resource { id } => *id = value.to_string(),
            FacetDataSource::Query { source, .. } => *source = value.to_string(),
            _ => {}
        },
        "filter" => {
            if let FacetDataSource::Query { query, .. } = &mut def.data {
                if value.is_empty() {
                    query.filters.clear();
                } else if let Some(qf) = prism_builder::parse_filter_expr(value) {
                    query.filters = vec![qf];
                }
            }
        }
        "sort_by" => {
            if let FacetDataSource::Query { query, .. } = &mut def.data {
                if value.is_empty() {
                    query.sort.clear();
                } else {
                    let (descending, path) = if let Some(stripped) = value.strip_prefix('-') {
                        (true, stripped)
                    } else {
                        (false, value)
                    };
                    query.sort = vec![prism_core::widget::QuerySort {
                        field: path.to_string(),
                        descending,
                    }];
                }
            }
        }
        "schema_id" => {
            def.schema_id = if value.is_empty() || value == "(none)" {
                None
            } else {
                Some(value.to_string())
            };
        }
        // ObjectQuery fields — all modify the embedded DataQuery
        "entity_type" => {
            if let FacetKind::ObjectQuery { ref mut query } = def.kind {
                query.object_type = if value.is_empty() {
                    None
                } else {
                    Some(value.to_string())
                };
            }
        }
        "oq_filter" => {
            if let FacetKind::ObjectQuery { ref mut query } = def.kind {
                if value.is_empty() {
                    query.filters.clear();
                } else if let Some(qf) = prism_builder::parse_filter_expr(value) {
                    query.filters = vec![qf];
                }
            }
        }
        "oq_sort_by" => {
            if let FacetKind::ObjectQuery { ref mut query } = def.kind {
                if value.is_empty() {
                    query.sort.clear();
                } else {
                    let (descending, path) = if let Some(stripped) = value.strip_prefix('-') {
                        (true, stripped)
                    } else {
                        (false, value)
                    };
                    query.sort = vec![prism_core::widget::QuerySort {
                        field: path.to_string(),
                        descending,
                    }];
                }
            }
        }
        "oq_limit" => {
            if let FacetKind::ObjectQuery { ref mut query } = def.kind {
                query.limit = value.parse::<usize>().ok().filter(|&n| n > 0);
            }
        }
        // Script fields
        "script_source" => {
            if let FacetKind::Script { ref mut source, .. } = def.kind {
                *source = value.to_string();
            }
        }
        "script_language" => {
            if let FacetKind::Script {
                ref mut language, ..
            } = def.kind
            {
                *language = match value {
                    "visual-graph" => ScriptLanguage::VisualGraph,
                    _ => ScriptLanguage::Luau,
                };
            }
            sync_script_language(def);
        }
        // Aggregate fields
        "agg_operation" => {
            if let FacetKind::Aggregate {
                ref mut operation, ..
            } = def.kind
            {
                *operation = AggregateOp::from_tag(value);
            }
        }
        "agg_field" => {
            if let FacetKind::Aggregate { ref mut field, .. } = def.kind {
                *field = if value.is_empty() {
                    None
                } else {
                    Some(value.to_string())
                };
            }
        }
        "agg_separator" => {
            if let FacetKind::Aggregate {
                operation: AggregateOp::Join { ref mut separator },
                ..
            } = def.kind
            {
                *separator = value.to_string();
            }
        }
        // Lookup fields
        "lookup_source" => {
            if let FacetKind::Lookup {
                ref mut source_entity,
                ..
            } = def.kind
            {
                *source_entity = value.to_string();
            }
        }
        "lookup_edge" => {
            if let FacetKind::Lookup {
                ref mut edge_type, ..
            } = def.kind
            {
                *edge_type = value.to_string();
            }
        }
        "lookup_target" => {
            if let FacetKind::Lookup {
                ref mut target_entity,
                ..
            } = def.kind
            {
                *target_entity = value.to_string();
            }
        }
        key if key.starts_with("binding.") => {
            let slot_key = &key["binding.".len()..];
            if !slot_key.is_empty() {
                if let Some(existing) = def.bindings.iter_mut().find(|b| b.slot_key == slot_key) {
                    existing.item_field = value.to_string();
                } else if !value.is_empty() {
                    def.bindings.push(FacetBinding {
                        slot_key: slot_key.to_string(),
                        item_field: value.to_string(),
                    });
                }
                def.bindings.retain(|b| !b.item_field.is_empty());
            }
        }
        key if key.starts_with("record.") => {
            let rest = &key["record.".len()..];
            if let Some((idx_str, field_key)) = rest.split_once('.') {
                if let Ok(idx) = idx_str.parse::<usize>() {
                    if let FacetDataSource::Static {
                        ref mut records, ..
                    } = def.data
                    {
                        if let Some(rec) = records.get_mut(idx) {
                            let parsed: serde_json::Value = serde_json::from_str(value)
                                .unwrap_or_else(|_| {
                                    if value.is_empty() {
                                        serde_json::Value::Null
                                    } else if value == "true" {
                                        serde_json::Value::Bool(true)
                                    } else if value == "false" {
                                        serde_json::Value::Bool(false)
                                    } else if let Ok(n) = value.parse::<f64>() {
                                        serde_json::json!(n)
                                    } else {
                                        serde_json::Value::String(value.to_string())
                                    }
                                });
                            rec.fields.insert(field_key.to_string(), parsed);
                        }
                    }
                }
            }
        }
        "add_variant_rule" => {
            def.variant_rules.push(FacetVariantRule {
                field: String::new(),
                value: String::new(),
                axis_key: String::new(),
                axis_value: String::new(),
            });
        }
        key if key.starts_with("remove_variant_rule.") => {
            if let Ok(idx) = key["remove_variant_rule.".len()..].parse::<usize>() {
                if idx < def.variant_rules.len() {
                    def.variant_rules.remove(idx);
                }
            }
        }
        key if key.starts_with("variant_rule.") => {
            let rest = &key["variant_rule.".len()..];
            if let Some((idx_str, field_name)) = rest.split_once('.') {
                if let Ok(idx) = idx_str.parse::<usize>() {
                    if let Some(rule) = def.variant_rules.get_mut(idx) {
                        match field_name {
                            "field" => rule.field = value.to_string(),
                            "value" => rule.value = value.to_string(),
                            "axis_key" => rule.axis_key = value.to_string(),
                            "axis_value" => rule.axis_value = value.to_string(),
                            _ => {}
                        }
                    }
                }
            }
        }
        "template_type" => match value {
            "inline" => {
                if !def.is_inline() {
                    def.template = FacetTemplate::Inline {
                        root: Box::new(Node {
                            id: "inline-root".into(),
                            component: "container".into(),
                            ..Default::default()
                        }),
                    };
                }
            }
            _ => {
                if def.is_inline() {
                    def.template = FacetTemplate::ComponentRef {
                        component_id: "card".into(),
                    };
                }
            }
        },
        "output_type" => match value {
            "scalar" => {
                if !def.is_scalar() {
                    def.output = FacetOutput::Scalar {
                        target_node: String::new(),
                        target_prop: String::new(),
                    };
                }
            }
            _ => {
                def.output = FacetOutput::Repeated;
            }
        },
        "scalar_target_node" => {
            if let FacetOutput::Scalar {
                ref mut target_node,
                ..
            } = def.output
            {
                *target_node = value.to_string();
            }
        }
        "scalar_target_prop" => {
            if let FacetOutput::Scalar {
                ref mut target_prop,
                ..
            } = def.output
            {
                *target_prop = value.to_string();
            }
        }
        _ => {}
    }
}

fn sync_script_language(def: &mut FacetDef) {
    if let FacetKind::Script {
        ref mut source,
        ref language,
        ref mut graph,
    } = def.kind
    {
        match language {
            ScriptLanguage::VisualGraph => {
                if !source.is_empty() && graph.is_none() {
                    use prism_core::language::luau::LuauVisualLanguage;
                    use prism_core::language::visual::VisualLanguage;
                    if let Ok(g) = LuauVisualLanguage::new().decompile(source) {
                        *graph = Some(g);
                    }
                }
            }
            ScriptLanguage::Luau => {
                if let Some(g) = graph.take() {
                    use prism_core::language::luau::LuauVisualLanguage;
                    use prism_core::language::visual::VisualLanguage;
                    if let Ok(s) = LuauVisualLanguage::new().compile(&g) {
                        *source = s;
                    }
                }
            }
        }
    }
}

/// Build exposed slots from all string props on the root node.
/// Provides a starting-point binding surface when saving as a prefab.
fn auto_expose_slots(node: &Node) -> Vec<ExposedSlot> {
    let mut slots = Vec::new();
    if let Some(obj) = node.props.as_object() {
        for (key, val) in obj {
            if val.is_string() && !key.starts_with('_') {
                let label = {
                    let mut chars = key.chars();
                    match chars.next() {
                        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                        None => key.clone(),
                    }
                };
                slots.push(ExposedSlot {
                    key: key.clone(),
                    target_node: node.id.clone(),
                    target_prop: key.clone(),
                    spec: FieldSpec::text(key.as_str(), label.as_str()),
                });
            }
        }
    }
    slots
}

fn apply_style_edit(style: &mut StyleProperties, key: &str, value: &str) {
    let parse_f32 = |s: &str| s.parse::<f32>().ok();
    let parse_u16 = |s: &str| s.parse::<u16>().ok();
    let opt_string = |s: &str| {
        if s.is_empty() {
            None
        } else {
            Some(s.to_string())
        }
    };

    match key {
        "font_family" => style.font_family = opt_string(value),
        "font_size" => style.font_size = parse_f32(value),
        "font_weight" => style.font_weight = parse_u16(value),
        "line_height" => style.line_height = parse_f32(value),
        "letter_spacing" => style.letter_spacing = parse_f32(value),
        "color" => style.color = opt_string(value),
        "background" => style.background = opt_string(value),
        "accent" => style.accent = opt_string(value),
        "base_spacing" => style.base_spacing = parse_f32(value),
        "border_radius" => style.border_radius = parse_f32(value),
        _ => {}
    }
}

fn apply_page_layout_edit(pl: &mut prism_builder::layout::PageLayout, key: &str, value: &str) {
    let parse_f32 = |s: &str| s.parse::<f32>().unwrap_or(0.0);

    match key {
        "page_size" => {
            pl.size = match value {
                "Responsive" => PageSize::Responsive,
                "A4" => PageSize::A4,
                "A3" => PageSize::A3,
                "A5" => PageSize::A5,
                "Letter" => PageSize::Letter,
                "Legal" => PageSize::Legal,
                "Tabloid" => PageSize::Tabloid,
                "Custom" => PageSize::Custom {
                    width: 1280.0,
                    height: 800.0,
                },
                _ => pl.size,
            };
        }
        "column_gap" => {
            pl.column_gap = parse_f32(value);
            pl.row_gap = parse_f32(value);
        }
        "margin_top" => pl.margins.top = parse_f32(value),
        "margin_right" => pl.margins.right = parse_f32(value),
        "margin_bottom" => pl.margins.bottom = parse_f32(value),
        "margin_left" => pl.margins.left = parse_f32(value),
        _ => {}
    }
}

fn apply_node_layout_edit(root: &mut Node, target: &str, key: &str, value: &str) -> bool {
    if root.id == target {
        apply_layout_to_node(root, key, value);
        return true;
    }
    for child in &mut root.children {
        if apply_node_layout_edit(child, target, key, value) {
            return true;
        }
    }
    false
}

fn apply_node_transform_edit(root: &mut Node, target: &str, key: &str, value: &str) -> bool {
    if root.id == target {
        apply_transform_to_node(root, key, value);
        return true;
    }
    for child in &mut root.children {
        if apply_node_transform_edit(child, target, key, value) {
            return true;
        }
    }
    false
}

fn apply_transform_to_node(node: &mut Node, key: &str, value: &str) {
    use prism_core::foundation::spatial::Anchor;
    let parse_f32 = |s: &str| s.parse::<f32>().unwrap_or(0.0);
    let t = &mut node.transform;
    match key {
        "transform.x" => t.position[0] = parse_f32(value),
        "transform.y" => t.position[1] = parse_f32(value),
        "transform.rotation" => t.rotation = parse_f32(value).to_radians(),
        "transform.scale_x" => t.scale[0] = parse_f32(value),
        "transform.scale_y" => t.scale[1] = parse_f32(value),
        "transform.anchor" => {
            t.anchor = match value {
                "top-left" => Anchor::TopLeft,
                "top-center" => Anchor::TopCenter,
                "top-right" => Anchor::TopRight,
                "center-left" => Anchor::CenterLeft,
                "center" => Anchor::Center,
                "center-right" => Anchor::CenterRight,
                "bottom-left" => Anchor::BottomLeft,
                "bottom-center" => Anchor::BottomCenter,
                "bottom-right" => Anchor::BottomRight,
                "stretch" => Anchor::Stretch,
                _ => t.anchor,
            };
        }
        _ => {}
    }
}

#[derive(Clone)]
struct DragSnapshot {
    node_id: String,
    position: [f32; 2],
    rotation: f32,
    scale: [f32; 2],
    pre_drag_source: String,
}

fn find_node_transform(
    root: &Node,
    target: &str,
) -> Option<prism_core::foundation::spatial::Transform2D> {
    if root.id == target {
        return Some(root.transform.clone());
    }
    for child in &root.children {
        if let Some(t) = find_node_transform(child, target) {
            return Some(t);
        }
    }
    None
}

fn apply_drag_to_node(
    root: &mut Node,
    tool: &str,
    dx: f32,
    dy: f32,
    shift: bool,
    snap: &DragSnapshot,
) {
    let node = if root.id == snap.node_id {
        root
    } else {
        fn find_mut<'a>(node: &'a mut Node, id: &str) -> Option<&'a mut Node> {
            for child in &mut node.children {
                if child.id == id {
                    return Some(child);
                }
                if let Some(n) = find_mut(child, id) {
                    return Some(n);
                }
            }
            None
        }
        match find_mut(root, &snap.node_id) {
            Some(n) => n,
            None => return,
        }
    };
    let t = &mut node.transform;
    match tool {
        "move" => {
            if shift {
                // Shift: constrain to major axis
                if dx.abs() > dy.abs() {
                    t.position[0] = snap.position[0] + dx;
                    t.position[1] = snap.position[1];
                } else {
                    t.position[0] = snap.position[0];
                    t.position[1] = snap.position[1] + dy;
                }
            } else {
                t.position[0] = snap.position[0] + dx;
                t.position[1] = snap.position[1] + dy;
            }
        }
        "rotate" => {
            let raw = snap.rotation + (dx * 0.5_f32).to_radians();
            if shift {
                // Shift: snap to 15-degree increments
                let deg = raw.to_degrees();
                let snapped = (deg / 15.0).round() * 15.0;
                t.rotation = snapped.to_radians();
            } else {
                t.rotation = raw;
            }
        }
        "scale" => {
            if shift {
                // Shift: uniform scale (use dx for both axes)
                let factor = (1.0 + dx / 100.0).max(0.01);
                t.scale[0] = snap.scale[0] * factor;
                t.scale[1] = snap.scale[1] * factor;
            } else {
                t.scale[0] = (snap.scale[0] * (1.0 + dx / 100.0)).max(0.01);
                t.scale[1] = (snap.scale[1] * (1.0 - dy / 100.0)).max(0.01);
            }
        }
        _ => {}
    }
}

#[derive(Clone)]
struct ResizeSnapshot {
    node_id: String,
    position: [f32; 2],
    width: f32,
    height: f32,
}

#[derive(Clone)]
struct GapResizeSnapshot {
    parent_path: Vec<usize>,
    gap_index: usize,
    track_a: prism_builder::TrackSize,
    track_b: prism_builder::TrackSize,
    available: f32,
}

fn find_node_layout_size(
    root: &Node,
    target: &str,
    layout: &prism_builder::ComputedLayout,
) -> Option<(f32, f32)> {
    if root.id == target {
        return layout
            .nodes
            .get(target)
            .map(|nl| (nl.rect.size.width, nl.rect.size.height));
    }
    for child in &root.children {
        if let Some(s) = find_node_layout_size(child, target, layout) {
            return Some(s);
        }
    }
    None
}

fn apply_resize_to_node(
    root: &mut Node,
    handle: &str,
    dx: f32,
    dy: f32,
    shift: bool,
    snap: &ResizeSnapshot,
) {
    fn find_mut<'a>(node: &'a mut Node, id: &str) -> Option<&'a mut Node> {
        for child in &mut node.children {
            if child.id == id {
                return Some(child);
            }
            if let Some(n) = find_mut(child, id) {
                return Some(n);
            }
        }
        None
    }
    let node = if root.id == snap.node_id {
        root
    } else {
        match find_mut(root, &snap.node_id) {
            Some(n) => n,
            None => return,
        }
    };

    let (mut dx, mut dy) = (dx, dy);
    if shift {
        // Uniform: constrain aspect ratio
        let aspect = if snap.height > 0.0 {
            snap.width / snap.height
        } else {
            1.0
        };
        match handle {
            "tl" | "br" => {
                let d = if dx.abs() > dy.abs() { dx } else { dy * aspect };
                dx = d;
                dy = d / aspect;
            }
            "tr" | "bl" => {
                let d = if dx.abs() > dy.abs() {
                    dx
                } else {
                    -dy * aspect
                };
                dx = d;
                dy = -d / aspect;
            }
            _ => {}
        }
    }

    // Compute new position and size based on which handle is being dragged.
    // "tl" moves origin and shrinks; "br" only grows; edges move one axis.
    let (mut new_x, mut new_y, mut new_w, mut new_h) =
        (snap.position[0], snap.position[1], snap.width, snap.height);

    match handle {
        "tl" => {
            new_x += dx;
            new_y += dy;
            new_w -= dx;
            new_h -= dy;
        }
        "t" => {
            new_y += dy;
            new_h -= dy;
        }
        "tr" => {
            new_w += dx;
            new_y += dy;
            new_h -= dy;
        }
        "r" => {
            new_w += dx;
        }
        "br" => {
            new_w += dx;
            new_h += dy;
        }
        "b" => {
            new_h += dy;
        }
        "bl" => {
            new_x += dx;
            new_w -= dx;
            new_h += dy;
        }
        "l" => {
            new_x += dx;
            new_w -= dx;
        }
        _ => {}
    }

    let min_size = 4.0;
    new_w = new_w.max(min_size);
    new_h = new_h.max(min_size);

    node.transform.position = [new_x, new_y];
    match &mut node.layout_mode {
        LayoutMode::Absolute(abs) => {
            abs.width = Dimension::Px { value: new_w };
            abs.height = Dimension::Px { value: new_h };
        }
        LayoutMode::Free => {
            node.layout_mode = LayoutMode::Absolute(AbsoluteProps::fixed(new_w, new_h));
        }
        LayoutMode::Relative(f) | LayoutMode::Flow(f) => {
            f.width = Dimension::Px { value: new_w };
            f.height = Dimension::Px { value: new_h };
        }
    }
}

fn apply_layout_to_node(node: &mut Node, key: &str, value: &str) {
    let parse_f32 = |s: &str| s.parse::<f32>().unwrap_or(0.0);

    // Handle Absolute mode width/height edits directly.
    if let LayoutMode::Absolute(abs) = &mut node.layout_mode {
        match key {
            "layout.display" => match value {
                "absolute" => return,
                "free" => {
                    node.layout_mode = LayoutMode::Free;
                    return;
                }
                "relative" => {
                    node.layout_mode = LayoutMode::Relative(FlowProps::default());
                    return;
                }
                _ => {
                    node.layout_mode = LayoutMode::Flow(FlowProps::default());
                }
            },
            "layout.width_unit" => {
                let cur = match abs.width {
                    Dimension::Px { value } | Dimension::Percent { value } => value,
                    Dimension::Auto => 0.0,
                };
                abs.width = match value {
                    "auto" => Dimension::Auto,
                    "px" => Dimension::Px { value: cur },
                    "%" => Dimension::Percent {
                        value: cur.min(100.0),
                    },
                    _ => abs.width,
                };
                return;
            }
            "layout.width_value" => {
                let v = value.parse::<f32>().unwrap_or(0.0);
                abs.width = match abs.width {
                    Dimension::Px { .. } => Dimension::Px { value: v },
                    Dimension::Percent { .. } => Dimension::Percent { value: v },
                    Dimension::Auto => Dimension::Px { value: v },
                };
                return;
            }
            "layout.height_unit" => {
                let cur = match abs.height {
                    Dimension::Px { value } | Dimension::Percent { value } => value,
                    Dimension::Auto => 0.0,
                };
                abs.height = match value {
                    "auto" => Dimension::Auto,
                    "px" => Dimension::Px { value: cur },
                    "%" => Dimension::Percent {
                        value: cur.min(100.0),
                    },
                    _ => abs.height,
                };
                return;
            }
            "layout.height_value" => {
                let v = value.parse::<f32>().unwrap_or(0.0);
                abs.height = match abs.height {
                    Dimension::Px { .. } => Dimension::Px { value: v },
                    Dimension::Percent { .. } => Dimension::Percent { value: v },
                    Dimension::Auto => Dimension::Px { value: v },
                };
                return;
            }
            _ => return,
        }
    }

    let flow = match &mut node.layout_mode {
        LayoutMode::Flow(f) | LayoutMode::Relative(f) => f,
        LayoutMode::Free => {
            if key == "layout.display" && value != "free" {
                match value {
                    "absolute" => {
                        node.layout_mode = LayoutMode::Absolute(AbsoluteProps::default());
                        return;
                    }
                    "relative" => {
                        node.layout_mode = LayoutMode::Relative(FlowProps::default());
                        return;
                    }
                    _ => {
                        node.layout_mode = LayoutMode::Flow(FlowProps::default());
                    }
                }
                match &mut node.layout_mode {
                    LayoutMode::Flow(f) => f,
                    _ => unreachable!(),
                }
            } else {
                return;
            }
        }
        LayoutMode::Absolute(_) => unreachable!(),
    };

    match key {
        "layout.display" => match value {
            "block" => flow.display = FlowDisplay::Block,
            "flex" => flow.display = FlowDisplay::Flex,
            "grid" => flow.display = FlowDisplay::Grid,
            "none" => flow.display = FlowDisplay::None,
            "free" => {
                node.layout_mode = LayoutMode::Free;
            }
            "absolute" => {
                node.layout_mode = LayoutMode::Absolute(AbsoluteProps::default());
            }
            "relative" => {
                node.layout_mode = LayoutMode::Relative(flow.clone());
            }
            _ => {}
        },
        "layout.width" => flow.width = parse_dimension(value),
        "layout.height" => flow.height = parse_dimension(value),
        "layout.gap" => flow.gap = parse_f32(value),
        "layout.flex_direction" => {
            flow.flex_direction = match value {
                "row" => FlexDirection::Row,
                "column" => FlexDirection::Column,
                "row-reverse" => FlexDirection::RowReverse,
                "column-reverse" => FlexDirection::ColumnReverse,
                _ => flow.flex_direction,
            };
        }
        "layout.flex_grow" => flow.flex_grow = parse_f32(value),
        "layout.flex_shrink" => flow.flex_shrink = parse_f32(value),
        "layout.align_items" => {
            flow.align_items = match value {
                "auto" => AlignOption::Auto,
                "start" => AlignOption::Start,
                "end" => AlignOption::End,
                "center" => AlignOption::Center,
                "stretch" => AlignOption::Stretch,
                "baseline" => AlignOption::Baseline,
                _ => flow.align_items,
            };
        }
        "layout.justify_content" => {
            flow.justify_content = match value {
                "start" => JustifyOption::Start,
                "end" => JustifyOption::End,
                "center" => JustifyOption::Center,
                "space-between" => JustifyOption::SpaceBetween,
                "space-around" => JustifyOption::SpaceAround,
                "space-evenly" => JustifyOption::SpaceEvenly,
                "stretch" => JustifyOption::Stretch,
                _ => flow.justify_content,
            };
        }
        "layout.grid_column" => flow.grid_column = parse_grid_placement(value),
        "layout.grid_row" => flow.grid_row = parse_grid_placement(value),
        "layout.padding" => {
            let vals = parse_edge_values(value);
            flow.padding = vals;
        }
        "layout.padding_top" => flow.padding.top = parse_f32(value),
        "layout.padding_right" => flow.padding.right = parse_f32(value),
        "layout.padding_bottom" => flow.padding.bottom = parse_f32(value),
        "layout.padding_left" => flow.padding.left = parse_f32(value),
        "layout.margin" => {
            let vals = parse_edge_values(value);
            flow.margin = vals;
        }
        "layout.margin_top" => flow.margin.top = parse_f32(value),
        "layout.margin_right" => flow.margin.right = parse_f32(value),
        "layout.margin_bottom" => flow.margin.bottom = parse_f32(value),
        "layout.margin_left" => flow.margin.left = parse_f32(value),
        "layout.width_unit" => {
            let current_value = match flow.width {
                Dimension::Px { value } => value,
                Dimension::Percent { value } => value,
                Dimension::Auto => 0.0,
            };
            flow.width = match value {
                "auto" => Dimension::Auto,
                "px" => Dimension::Px {
                    value: current_value,
                },
                "%" => Dimension::Percent {
                    value: current_value.min(100.0),
                },
                _ => flow.width,
            };
        }
        "layout.width_value" => {
            let v = parse_f32(value);
            flow.width = match flow.width {
                Dimension::Px { .. } => Dimension::Px { value: v },
                Dimension::Percent { .. } => Dimension::Percent { value: v },
                Dimension::Auto => Dimension::Px { value: v },
            };
        }
        "layout.height_unit" => {
            let current_value = match flow.height {
                Dimension::Px { value } => value,
                Dimension::Percent { value } => value,
                Dimension::Auto => 0.0,
            };
            flow.height = match value {
                "auto" => Dimension::Auto,
                "px" => Dimension::Px {
                    value: current_value,
                },
                "%" => Dimension::Percent {
                    value: current_value.min(100.0),
                },
                _ => flow.height,
            };
        }
        "layout.height_value" => {
            let v = parse_f32(value);
            flow.height = match flow.height {
                Dimension::Px { .. } => Dimension::Px { value: v },
                Dimension::Percent { .. } => Dimension::Percent { value: v },
                Dimension::Auto => Dimension::Px { value: v },
            };
        }
        "layout.grid_column_type" => {
            flow.grid_column = match value {
                "auto" => GridPlacement::Auto,
                "line" => GridPlacement::Line {
                    index: match flow.grid_column {
                        GridPlacement::Line { index } => index,
                        GridPlacement::Span { count } => count as i16,
                        GridPlacement::Auto => 1,
                    },
                },
                "span" => GridPlacement::Span {
                    count: match flow.grid_column {
                        GridPlacement::Span { count } => count,
                        GridPlacement::Line { index } => index.max(1) as u16,
                        GridPlacement::Auto => 1,
                    },
                },
                _ => flow.grid_column,
            };
        }
        "layout.grid_column_value" => {
            let v = parse_f32(value);
            flow.grid_column = match flow.grid_column {
                GridPlacement::Line { .. } => GridPlacement::Line { index: v as i16 },
                GridPlacement::Span { .. } => GridPlacement::Span {
                    count: (v as u16).max(1),
                },
                GridPlacement::Auto => GridPlacement::Line { index: v as i16 },
            };
        }
        "layout.grid_row_type" => {
            flow.grid_row = match value {
                "auto" => GridPlacement::Auto,
                "line" => GridPlacement::Line {
                    index: match flow.grid_row {
                        GridPlacement::Line { index } => index,
                        GridPlacement::Span { count } => count as i16,
                        GridPlacement::Auto => 1,
                    },
                },
                "span" => GridPlacement::Span {
                    count: match flow.grid_row {
                        GridPlacement::Span { count } => count,
                        GridPlacement::Line { index } => index.max(1) as u16,
                        GridPlacement::Auto => 1,
                    },
                },
                _ => flow.grid_row,
            };
        }
        "layout.grid_row_value" => {
            let v = parse_f32(value);
            flow.grid_row = match flow.grid_row {
                GridPlacement::Line { .. } => GridPlacement::Line { index: v as i16 },
                GridPlacement::Span { .. } => GridPlacement::Span {
                    count: (v as u16).max(1),
                },
                GridPlacement::Auto => GridPlacement::Line { index: v as i16 },
            };
        }
        _ => {}
    }
}

fn parse_dimension(s: &str) -> Dimension {
    let s = s.trim();
    if s == "auto" {
        return Dimension::Auto;
    }
    if let Some(px) = s.strip_suffix("px") {
        if let Ok(v) = px.trim().parse::<f32>() {
            return Dimension::Px { value: v };
        }
    }
    if let Some(pct) = s.strip_suffix('%') {
        if let Ok(v) = pct.trim().parse::<f32>() {
            return Dimension::Percent { value: v };
        }
    }
    if let Ok(v) = s.parse::<f32>() {
        return Dimension::Px { value: v };
    }
    Dimension::Auto
}

fn parse_grid_placement(s: &str) -> GridPlacement {
    let s = s.trim();
    if s == "auto" {
        return GridPlacement::Auto;
    }
    if let Some(rest) = s.strip_prefix("span ") {
        if let Ok(v) = rest.trim().parse::<u16>() {
            return GridPlacement::Span { count: v };
        }
    }
    if let Some(rest) = s.strip_prefix("line ") {
        if let Ok(v) = rest.trim().parse::<i16>() {
            return GridPlacement::Line { index: v };
        }
    }
    if let Ok(v) = s.parse::<i16>() {
        return GridPlacement::Line { index: v };
    }
    GridPlacement::Auto
}

fn parse_edge_values(s: &str) -> prism_core::foundation::geometry::Edges<f32> {
    let parts: Vec<f32> = s
        .split_whitespace()
        .filter_map(|p| p.parse::<f32>().ok())
        .collect();
    match parts.len() {
        1 => prism_core::foundation::geometry::Edges::all(parts[0]),
        2 => prism_core::foundation::geometry::Edges::symmetric(parts[0], parts[1]),
        4 => prism_core::foundation::geometry::Edges::new(parts[0], parts[1], parts[2], parts[3]),
        _ => prism_core::foundation::geometry::Edges::ZERO,
    }
}

fn push_composition_counts(
    window: &AppWindow,
    doc: &BuilderDocument,
    _registry: &prism_builder::ComponentRegistry,
    selection: &SelectionModel,
) {
    let selected = selection.as_option();
    let conn_count = doc
        .connections
        .iter()
        .filter(|c| {
            selected
                .as_ref()
                .is_some_and(|id| c.source_node == *id || c.target_node == *id)
        })
        .count();
    window.set_connection_count(conn_count as i32);
    window.set_resource_count(doc.resources.len() as i32);
    window.set_prefab_count(doc.prefabs.len() as i32);
}

fn push_grid_cells(
    models: &PersistentModels,
    window: &AppWindow,
    doc: &BuilderDocument,
    selection: &SelectionModel,
    viewport_width: f32,
) {
    if !doc.page_layout.has_grid() {
        sync_model(&models.grid_cells, &[]);
        window.set_grid_cells_count(0);
        return;
    }

    let resolved = doc.page_layout.resolved_size();
    let (pw, ph) = resolved
        .map(|s| (s.width, s.height))
        .unwrap_or((viewport_width, viewport_width * 0.625));
    let content_w = pw - doc.page_layout.margins.left - doc.page_layout.margins.right;
    let content_h = ph - doc.page_layout.margins.top - doc.page_layout.margins.bottom;

    let flat = doc.page_layout.flatten_cells(content_w, content_h);

    let child_map: std::collections::HashMap<&str, &Node> = doc
        .root
        .as_ref()
        .map(|r| r.children.iter().map(|c| (c.id.as_str(), c)).collect())
        .unwrap_or_default();

    let cells: Vec<GridCellItem> = flat
        .iter()
        .map(|fc| {
            let path_str = prism_builder::path_to_string(&fc.path);
            let (is_empty, node_id, component_type, selected, preview_text) = match &fc.node_id {
                Some(id) => {
                    let node = child_map.get(id.as_str());
                    let ct = node.map(|n| n.component.as_str()).unwrap_or("");
                    let preview = node
                        .and_then(|n| {
                            n.props
                                .get("text")
                                .or_else(|| n.props.get("body"))
                                .or_else(|| n.props.get("title"))
                                .and_then(|v| v.as_str())
                        })
                        .unwrap_or("");
                    (false, id.as_str(), ct, selection.contains(id), preview)
                }
                None => (true, "", "", false, ""),
            };
            GridCellItem {
                path: SharedString::from(path_str),
                x: fc.x,
                y: fc.y,
                width: fc.width,
                height: fc.height,
                is_empty,
                node_id: SharedString::from(node_id),
                component_type: SharedString::from(component_type),
                selected,
                preview_text: SharedString::from(preview_text),
            }
        })
        .collect();

    let count = sync_model(&models.grid_cells, &cells);
    window.set_grid_cells_count(count);
}

fn push_grid_edge_handles(
    models: &PersistentModels,
    window: &AppWindow,
    doc: &BuilderDocument,
    viewport_width: f32,
) {
    if !doc.page_layout.has_grid() {
        sync_model(&models.grid_edge_handles, &[]);
        window.set_grid_edge_handles_count(0);
        return;
    }

    let resolved = doc.page_layout.resolved_size();
    let (pw, ph) = resolved
        .map(|s| (s.width, s.height))
        .unwrap_or((viewport_width, viewport_width * 0.625));
    let content_w = pw - doc.page_layout.margins.left - doc.page_layout.margins.right;
    let content_h = ph - doc.page_layout.margins.top - doc.page_layout.margins.bottom;

    let edge_handles = doc.page_layout.flatten_edge_handles(content_w, content_h);

    let handles: Vec<GridEdgeHandle> = edge_handles
        .iter()
        .map(|eh| {
            let edge_str = match eh.edge {
                CellEdge::Top => "top",
                CellEdge::Bottom => "bottom",
                CellEdge::Left => "left",
                CellEdge::Right => "right",
            };
            let orientation_str = match eh.orientation {
                prism_builder::SplitDirection::Horizontal => "horizontal",
                prism_builder::SplitDirection::Vertical => "vertical",
            };
            GridEdgeHandle {
                cell_path: SharedString::from(prism_builder::path_to_string(&eh.cell_path)),
                edge: SharedString::from(edge_str),
                parent_path: SharedString::from(prism_builder::path_to_string(&eh.parent_path)),
                gap_index: eh.gap_index as i32,
                is_gap: eh.is_gap,
                orientation: SharedString::from(orientation_str),
                x: eh.x,
                y: eh.y,
                width: eh.width,
                height: eh.height,
            }
        })
        .collect();

    let count = sync_model(&models.grid_edge_handles, &handles);
    window.set_grid_edge_handles_count(count);
}

fn push_editor_data(models: &PersistentModels, window: &AppWindow, es: &EditorState) {
    use prism_core::editor::{
        active_indent_depth, compute_line_indent_guides, highlight_line, is_foldable, TokenKind,
    };

    let line_count = es.buffer.line_count();
    let cursor_line = es.cursor.position.line;
    let cursor_col = es.cursor.position.col;
    let active_depth = active_indent_depth(&es.buffer, cursor_line, es.tab_width);

    let (sel_start, sel_end) = es
        .selection
        .as_ref()
        .map(|s| s.ordered_positions(&es.buffer))
        .unzip();

    let mut lines: Vec<EditorLine> = Vec::with_capacity(line_count);

    let mut i = 0;
    while i < line_count {
        if es.fold_state.is_hidden(i) {
            i += 1;
            continue;
        }

        let raw = es.buffer.line(i).unwrap_or_default();
        let trimmed = raw.trim_end_matches('\n');
        let tokens_raw = highlight_line(trimmed, &es.language);

        let tokens: Vec<EditorToken> = tokens_raw
            .into_iter()
            .map(|t| {
                let c = match t.kind {
                    TokenKind::Keyword => slint::Color::from_rgb_u8(0xc6, 0x78, 0xdd),
                    TokenKind::String => slint::Color::from_rgb_u8(0x98, 0xc3, 0x79),
                    TokenKind::Comment => slint::Color::from_rgb_u8(0x5c, 0x63, 0x70),
                    TokenKind::Number => slint::Color::from_rgb_u8(0xd1, 0x9a, 0x66),
                    TokenKind::Operator => slint::Color::from_rgb_u8(0x56, 0xb6, 0xc2),
                    TokenKind::Punctuation => slint::Color::from_rgb_u8(0xab, 0xb2, 0xbf),
                    TokenKind::Identifier => slint::Color::from_rgb_u8(0xe0, 0x6c, 0x75),
                    TokenKind::Whitespace => slint::Color::from_argb_u8(0, 0, 0, 0),
                    TokenKind::Plain => slint::Color::from_rgb_u8(0xab, 0xb2, 0xbf),
                };
                EditorToken {
                    text: SharedString::from(t.text),
                    token_color: c,
                    col_offset: 0,
                }
            })
            .collect();
        let token_model = Rc::new(VecModel::from(tokens));

        let is_current = i == cursor_line;
        let (sf, st) = compute_line_selection(i, trimmed.len(), &sel_start, &sel_end);

        let guides_raw = compute_line_indent_guides(&es.buffer, i, es.tab_width, active_depth);
        let guides: Vec<EditorIndentGuide> = guides_raw
            .into_iter()
            .map(|g| EditorIndentGuide {
                depth: g.depth as i32,
                active: g.active,
            })
            .collect();
        let guide_model = Rc::new(VecModel::from(guides));

        let folded = es.fold_state.is_fold_start(i);
        let foldable = folded || is_foldable(&es.buffer, i, es.tab_width);
        let fold_preview = if folded {
            es.fold_state
                .get_fold(i)
                .map(|f| SharedString::from(&f.preview))
                .unwrap_or_default()
        } else {
            SharedString::default()
        };

        lines.push(EditorLine {
            number: (i + 1) as i32,
            buffer_line: i as i32,
            tokens: ModelRc::from(token_model as Rc<dyn Model<Data = EditorToken>>),
            indent_guides: ModelRc::from(guide_model as Rc<dyn Model<Data = EditorIndentGuide>>),
            is_current,
            sel_from: sf,
            sel_to: st,
            is_foldable: foldable,
            is_folded: folded,
            fold_preview,
        });

        i += 1;
    }

    let count = sync_model(&models.editor_lines, &lines);
    window.set_editor_lines_count(count);
    window.set_editor_cursor_line(cursor_line as i32);
    window.set_editor_cursor_col(cursor_col as i32);
    window.set_editor_cursor_visible(true);

    let cursor_prefix: String = es
        .buffer
        .line(cursor_line)
        .unwrap_or_default()
        .trim_end_matches('\n')
        .chars()
        .take(cursor_col)
        .collect();
    window.set_editor_cursor_prefix(SharedString::from(cursor_prefix));
    window.set_editor_language(SharedString::from(&es.language));
    window.set_editor_line_count(line_count as i32);
    window.set_editor_char_count(es.buffer.len_chars() as i32);
}

fn display_row_to_buffer_line(es: &EditorState, display_row: usize) -> usize {
    let mut display = 0;
    for i in 0..es.buffer.line_count() {
        if es.fold_state.is_hidden(i) {
            continue;
        }
        if display == display_row {
            return i;
        }
        display += 1;
    }
    es.buffer.line_count().saturating_sub(1)
}

fn compute_line_selection(
    line: usize,
    line_len: usize,
    sel_start: &Option<prism_core::editor::Position>,
    sel_end: &Option<prism_core::editor::Position>,
) -> (i32, i32) {
    let (start, end) = match (sel_start, sel_end) {
        (Some(s), Some(e)) => (s, e),
        _ => return (-1, -1),
    };
    if line < start.line || line > end.line {
        return (-1, -1);
    }
    let from = if line == start.line { start.col } else { 0 };
    let to = if line == end.line {
        end.col
    } else {
        line_len + 1
    };
    if from == to {
        return (-1, -1);
    }
    (from as i32, to as i32)
}

// ── Tests ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_app_state_starts_on_launchpad() {
        let state = AppState::default();
        assert!(state.shell_view.is_launchpad());
        assert_eq!(state.workspace.active_page().id, "edit");
    }

    #[test]
    fn default_state_has_apps_with_documents() {
        let state = AppState::default();
        assert!(!state.apps.is_empty());
        let first_app = &state.apps[0];
        assert!(first_app.active_document().unwrap().root.is_some());
    }

    #[test]
    fn store_snapshot_restore_round_trips_app_state() {
        let mut store: Store<AppState> = Store::new(AppState::default());
        store.mutate(|s| {
            s.workspace.switch_page_by_id("code");
        });
        let bytes = store.snapshot().expect("snapshot");
        let mut fresh: Store<AppState> = Store::new(AppState::default());
        fresh.restore(&bytes).expect("restore");
        assert_eq!(fresh.state().workspace.active_page().id, "code");
        assert!(!fresh.state().apps.is_empty());
    }

    #[test]
    fn workspace_pages_available() {
        let state = AppState::default();
        assert_eq!(state.workspace.pages().len(), 7);
        let ids: Vec<&str> = state
            .workspace
            .pages()
            .iter()
            .map(|p| p.id.as_str())
            .collect();
        assert_eq!(
            ids,
            &[
                "edit",
                "design",
                "code",
                "fusion",
                "navigation",
                "data",
                "preview"
            ]
        );
    }

    #[test]
    fn panel_id_for_slint_maps_pages() {
        let mut state = AppState::default();
        assert_eq!(panel_id_for_slint(&state.workspace), 1);
        state.workspace.switch_page_by_id("code");
        assert_eq!(panel_id_for_slint(&state.workspace), 2);
        state.workspace.switch_page_by_id("fusion");
        assert_eq!(panel_id_for_slint(&state.workspace), 1);
        state.workspace.switch_page_by_id("preview");
        assert_eq!(panel_id_for_slint(&state.workspace), 1);
    }

    #[test]
    fn preview_mode_only_on_preview_page() {
        let mut state = AppState::default();
        assert!(!is_preview_mode(&state.workspace));
        state.workspace.switch_page_by_id("preview");
        assert!(is_preview_mode(&state.workspace));
        state.workspace.switch_page_by_id("edit");
        assert!(!is_preview_mode(&state.workspace));
    }

    #[test]
    fn select_page_action_mutates_state() {
        let mut store: Store<AppState> = Store::new(AppState::default());
        store.dispatch(SelectPage("code".into()));
        assert_eq!(store.state().workspace.active_page().id, "code");
    }

    #[test]
    fn selection_model_starts_empty_on_launchpad() {
        let state = AppState::default();
        assert!(state.selection.is_empty());
    }

    #[test]
    fn default_state_starts_on_launchpad_with_apps() {
        let state = AppState::default();
        assert!(state.shell_view.is_launchpad());
        assert!(state.apps.len() >= 2);
        assert!(!state.apps[0].pages.is_empty());
    }

    #[test]
    fn shell_view_app_exposes_app_id() {
        let view = ShellView::App {
            app_id: "test".into(),
        };
        assert_eq!(view.active_app_id(), Some("test"));
        assert!(!view.is_launchpad());
    }

    #[test]
    fn flatten_inspector_nodes_produces_correct_depths() {
        let doc = sample_document();
        let sel = SelectionModel::single("hero".into());
        let items = flatten_inspector_nodes(doc.root.as_ref(), &sel);
        assert_eq!(items.len(), 6);
        assert_eq!(items[0].depth, 0);
        assert_eq!(items[0].id, "root");
        assert!(!items[0].selected);
        assert_eq!(items[1].depth, 1);
        assert_eq!(items[1].id, "hero");
        assert!(items[1].selected);
        assert_eq!(items[2].depth, 1);
        assert_eq!(items[2].id, "intro");
        assert_eq!(items[3].depth, 1);
        assert_eq!(items[3].id, "cols");
        assert_eq!(items[4].depth, 2);
        assert_eq!(items[4].id, "col1");
        assert_eq!(items[5].depth, 2);
        assert_eq!(items[5].id, "col2");
    }

    #[test]
    fn collect_node_ids_walks_full_tree() {
        let doc = sample_document();
        let ids = collect_node_ids(doc.root.as_ref());
        assert_eq!(ids, vec!["root", "hero", "intro", "cols", "col1", "col2"]);
    }

    #[test]
    fn default_props_are_non_empty() {
        for ct in [
            "text",
            "image",
            "container",
            "form",
            "input",
            "button",
            "card",
            "code",
            "spacer",
            "columns",
            "table",
            "tabs",
            "accordion",
        ] {
            let props = default_props_for_component(ct);
            assert!(
                !props.as_object().unwrap().is_empty(),
                "{ct} should have default props"
            );
        }
    }

    #[test]
    fn component_palette_has_categorized_items() {
        let doc = BuilderDocument::default();
        let items = component_palette_items(&doc);
        let headers: Vec<_> = items.iter().filter(|i| i.is_header).collect();
        let components: Vec<_> = items.iter().filter(|i| !i.is_header).collect();
        assert_eq!(headers.len(), 6);
        assert_eq!(components.len(), 16);
        assert_eq!(items.len(), 22);
        assert_eq!(headers[0].label.as_str(), "CONTENT");
        assert_eq!(headers[1].label.as_str(), "LAYOUT");
        assert_eq!(headers[2].label.as_str(), "FORM");
        assert_eq!(headers[3].label.as_str(), "DECORATION");
        assert_eq!(headers[4].label.as_str(), "PREFABS");
        assert_eq!(headers[5].label.as_str(), "PROGRAMMATIC");
    }

    #[test]
    fn user_prefabs_appear_in_palette() {
        let mut doc = BuilderDocument::default();
        doc.prefabs.insert(
            "prefab:hero".into(),
            PrefabDef {
                id: "prefab:hero".into(),
                label: "Hero".into(),
                description: "Hero section".into(),
                root: Node {
                    id: "hero".into(),
                    component: "container".into(),
                    ..Default::default()
                },
                exposed: vec![],
                variants: vec![],
                thumbnail: None,
            },
        );
        let items = component_palette_items(&doc);
        let prefab_items: Vec<_> = items
            .iter()
            .filter(|i| !i.is_header && i.category.as_str() == "PREFABS")
            .collect();
        assert_eq!(prefab_items.len(), 2);
        assert!(prefab_items
            .iter()
            .any(|i| i.component_type.as_str() == "prefab:hero"));
    }

    #[test]
    fn apply_facet_edit_binding_creates_and_updates() {
        use prism_builder::{FacetDataSource, FacetDef, FacetKind, FacetLayout};
        let mut def = FacetDef {
            id: "facet:test".into(),
            label: "Test".into(),
            description: String::new(),
            kind: FacetKind::List,
            schema_id: None,
            data: FacetDataSource::Static {
                items: vec![],
                records: vec![],
            },
            bindings: vec![],
            variant_rules: vec![],
            layout: FacetLayout::default(),
            template: FacetTemplate::default(),
            output: FacetOutput::default(),
            resolved_data: None,
        };
        apply_facet_edit(&mut def, "binding.title", "name");
        assert_eq!(def.bindings.len(), 1);
        assert_eq!(def.bindings[0].slot_key, "title");
        assert_eq!(def.bindings[0].item_field, "name");
        apply_facet_edit(&mut def, "binding.title", "full_name");
        assert_eq!(def.bindings.len(), 1);
        assert_eq!(def.bindings[0].item_field, "full_name");
        apply_facet_edit(&mut def, "binding.title", "");
        assert_eq!(def.bindings.len(), 0);
    }

    #[test]
    fn auto_expose_slots_extracts_string_props() {
        let node = Node {
            id: "n1".into(),
            component: "text".into(),
            props: serde_json::json!({ "body": "Hello", "level": "h1", "count": 5 }),
            ..Default::default()
        };
        let slots = auto_expose_slots(&node);
        assert_eq!(slots.len(), 2);
        assert!(slots.iter().any(|s| s.key == "body"));
        assert!(slots.iter().any(|s| s.key == "level"));
        assert!(slots.iter().all(|s| s.target_node == "n1"));
    }

    #[test]
    fn toast_data_serializes() {
        let toast = ToastData {
            id: 1,
            title: "Saved".into(),
            body: "Document saved.".into(),
            kind: "success".into(),
            created_at: None,
        };
        let json = serde_json::to_string(&toast).unwrap();
        let restored: ToastData = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.title, "Saved");
    }

    #[test]
    fn clone_node_with_new_ids_generates_unique_ids() {
        let node = Node {
            id: "original".into(),
            component: "text".into(),
            props: json!({ "body": "Hello", "level": "h1" }),
            children: vec![Node {
                id: "child".into(),
                component: "text".into(),
                props: json!({ "body": "World" }),
                children: vec![],
                ..Default::default()
            }],
            ..Default::default()
        };
        let mut counter = 50u64;
        let cloned = clone_node_with_new_ids(&node, &mut counter);
        assert_eq!(cloned.id, "n50");
        assert_eq!(cloned.children[0].id, "n51");
        assert_eq!(counter, 52);
        assert_eq!(cloned.component, "text");
        assert_eq!(cloned.props["body"], "Hello");
    }

    #[test]
    fn find_path_to_node_returns_path() {
        let doc = sample_document();
        let root = doc.root.as_ref().unwrap();
        let mut path = Vec::new();
        assert!(find_path_to_node(root, "hero", &mut path));
        assert_eq!(path.len(), 2);
        assert_eq!(path[0].0, "root");
        assert_eq!(path[1].0, "hero");
    }

    #[test]
    fn find_path_to_node_returns_false_for_missing() {
        let doc = sample_document();
        let root = doc.root.as_ref().unwrap();
        let mut path = Vec::new();
        assert!(!find_path_to_node(root, "nonexistent", &mut path));
        assert!(path.is_empty());
    }

    // ── Transform edit tests ──────────────────────────────────────

    fn transform_node() -> Node {
        Node {
            id: "n1".into(),
            component: "text".into(),
            props: json!({}),
            layout_mode: LayoutMode::Absolute(AbsoluteProps::default()),
            ..Default::default()
        }
    }

    #[test]
    fn transform_edit_position_x() {
        let mut node = transform_node();
        apply_transform_to_node(&mut node, "transform.x", "42.5");
        assert!((node.transform.position[0] - 42.5).abs() < f32::EPSILON);
        assert!((node.transform.position[1] - 0.0).abs() < f32::EPSILON);
    }

    #[test]
    fn transform_edit_position_y() {
        let mut node = transform_node();
        apply_transform_to_node(&mut node, "transform.y", "-100");
        assert!((node.transform.position[1] - (-100.0)).abs() < f32::EPSILON);
    }

    #[test]
    fn transform_edit_rotation_degrees_to_radians() {
        let mut node = transform_node();
        apply_transform_to_node(&mut node, "transform.rotation", "90");
        let expected = 90.0_f32.to_radians();
        assert!((node.transform.rotation - expected).abs() < 1e-5);
    }

    #[test]
    fn transform_edit_rotation_negative() {
        let mut node = transform_node();
        apply_transform_to_node(&mut node, "transform.rotation", "-45");
        let expected = (-45.0_f32).to_radians();
        assert!((node.transform.rotation - expected).abs() < 1e-5);
    }

    #[test]
    fn transform_edit_scale_x() {
        let mut node = transform_node();
        apply_transform_to_node(&mut node, "transform.scale_x", "2.5");
        assert!((node.transform.scale[0] - 2.5).abs() < f32::EPSILON);
        assert!((node.transform.scale[1] - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn transform_edit_scale_y() {
        let mut node = transform_node();
        apply_transform_to_node(&mut node, "transform.scale_y", "0.5");
        assert!((node.transform.scale[1] - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn transform_edit_anchor_all_variants() {
        use prism_core::foundation::spatial::Anchor;
        let cases = [
            ("top-left", Anchor::TopLeft),
            ("top-center", Anchor::TopCenter),
            ("top-right", Anchor::TopRight),
            ("center-left", Anchor::CenterLeft),
            ("center", Anchor::Center),
            ("center-right", Anchor::CenterRight),
            ("bottom-left", Anchor::BottomLeft),
            ("bottom-center", Anchor::BottomCenter),
            ("bottom-right", Anchor::BottomRight),
            ("stretch", Anchor::Stretch),
        ];
        for (label, expected) in cases {
            let mut node = transform_node();
            apply_transform_to_node(&mut node, "transform.anchor", label);
            assert_eq!(node.transform.anchor, expected, "anchor {label}");
        }
    }

    #[test]
    fn transform_edit_anchor_unknown_preserves_current() {
        use prism_core::foundation::spatial::Anchor;
        let mut node = transform_node();
        node.transform.anchor = Anchor::Center;
        apply_transform_to_node(&mut node, "transform.anchor", "nonsense");
        assert_eq!(node.transform.anchor, Anchor::Center);
    }

    #[test]
    fn transform_edit_unknown_key_is_noop() {
        let mut node = transform_node();
        let before = node.transform.clone();
        apply_transform_to_node(&mut node, "transform.z", "999");
        assert_eq!(node.transform, before);
    }

    #[test]
    fn transform_edit_invalid_number_defaults_to_zero() {
        let mut node = transform_node();
        node.transform.position[0] = 50.0;
        apply_transform_to_node(&mut node, "transform.x", "abc");
        assert!((node.transform.position[0] - 0.0).abs() < f32::EPSILON);
    }

    #[test]
    fn node_transform_edit_recursive_finds_child() {
        let mut root = Node {
            id: "root".into(),
            component: "container".into(),
            props: json!({}),
            children: vec![
                Node {
                    id: "a".into(),
                    component: "text".into(),
                    props: json!({}),
                    children: vec![Node {
                        id: "deep".into(),
                        component: "text".into(),
                        props: json!({}),
                        ..Default::default()
                    }],
                    ..Default::default()
                },
                transform_node(),
            ],
            ..Default::default()
        };
        assert!(apply_node_transform_edit(
            &mut root,
            "deep",
            "transform.x",
            "77"
        ));
        let deep = root.children[0].children[0].clone();
        assert!((deep.transform.position[0] - 77.0).abs() < f32::EPSILON);
    }

    #[test]
    fn node_transform_edit_returns_false_for_missing() {
        let mut root = transform_node();
        assert!(!apply_node_transform_edit(
            &mut root,
            "nonexistent",
            "transform.x",
            "10"
        ));
    }

    // ── Layout mode switching tests ───────────────────────────────

    #[test]
    fn layout_switch_flow_to_absolute() {
        let mut node = Node {
            id: "n".into(),
            component: "text".into(),
            props: json!({}),
            layout_mode: LayoutMode::Flow(FlowProps::default()),
            ..Default::default()
        };
        apply_layout_to_node(&mut node, "layout.display", "absolute");
        assert!(matches!(node.layout_mode, LayoutMode::Absolute(_)));
    }

    #[test]
    fn layout_switch_flow_to_relative() {
        let mut node = Node {
            id: "n".into(),
            component: "text".into(),
            props: json!({}),
            layout_mode: LayoutMode::Flow(FlowProps {
                gap: 8.0,
                ..Default::default()
            }),
            ..Default::default()
        };
        apply_layout_to_node(&mut node, "layout.display", "relative");
        match &node.layout_mode {
            LayoutMode::Relative(f) => assert!((f.gap - 8.0).abs() < f32::EPSILON),
            other => panic!("expected Relative, got {:?}", other),
        }
    }

    #[test]
    fn layout_switch_flow_to_free() {
        let mut node = Node {
            id: "n".into(),
            component: "text".into(),
            props: json!({}),
            layout_mode: LayoutMode::Flow(FlowProps::default()),
            ..Default::default()
        };
        apply_layout_to_node(&mut node, "layout.display", "free");
        assert!(matches!(node.layout_mode, LayoutMode::Free));
    }

    #[test]
    fn layout_switch_absolute_to_flow() {
        let mut node = Node {
            id: "n".into(),
            component: "text".into(),
            props: json!({}),
            layout_mode: LayoutMode::Absolute(AbsoluteProps::default()),
            ..Default::default()
        };
        apply_layout_to_node(&mut node, "layout.display", "flex");
        match &node.layout_mode {
            LayoutMode::Flow(f) => assert_eq!(f.display, FlowDisplay::Flex),
            other => panic!("expected Flow, got {:?}", other),
        }
    }

    #[test]
    fn layout_switch_absolute_to_free() {
        let mut node = Node {
            id: "n".into(),
            component: "text".into(),
            props: json!({}),
            layout_mode: LayoutMode::Absolute(AbsoluteProps::default()),
            ..Default::default()
        };
        apply_layout_to_node(&mut node, "layout.display", "free");
        assert!(matches!(node.layout_mode, LayoutMode::Free));
    }

    #[test]
    fn layout_switch_absolute_to_relative() {
        let mut node = Node {
            id: "n".into(),
            component: "text".into(),
            props: json!({}),
            layout_mode: LayoutMode::Absolute(AbsoluteProps::default()),
            ..Default::default()
        };
        apply_layout_to_node(&mut node, "layout.display", "relative");
        assert!(matches!(node.layout_mode, LayoutMode::Relative(_)));
    }

    #[test]
    fn layout_switch_free_to_absolute() {
        let mut node = Node {
            id: "n".into(),
            component: "text".into(),
            props: json!({}),
            layout_mode: LayoutMode::Free,
            ..Default::default()
        };
        apply_layout_to_node(&mut node, "layout.display", "absolute");
        assert!(matches!(node.layout_mode, LayoutMode::Absolute(_)));
    }

    #[test]
    fn layout_switch_free_to_flow() {
        let mut node = Node {
            id: "n".into(),
            component: "text".into(),
            props: json!({}),
            layout_mode: LayoutMode::Free,
            ..Default::default()
        };
        apply_layout_to_node(&mut node, "layout.display", "block");
        assert!(matches!(node.layout_mode, LayoutMode::Flow(_)));
    }

    #[test]
    fn layout_relative_display_change_stays_relative() {
        let mut node = Node {
            id: "n".into(),
            component: "text".into(),
            props: json!({}),
            layout_mode: LayoutMode::Relative(FlowProps::default()),
            ..Default::default()
        };
        apply_layout_to_node(&mut node, "layout.display", "flex");
        match &node.layout_mode {
            LayoutMode::Relative(f) => assert_eq!(f.display, FlowDisplay::Flex),
            other => panic!("expected Relative, got {:?}", other),
        }
    }

    #[test]
    fn layout_switch_relative_to_free() {
        let mut node = Node {
            id: "n".into(),
            component: "text".into(),
            props: json!({}),
            layout_mode: LayoutMode::Relative(FlowProps::default()),
            ..Default::default()
        };
        apply_layout_to_node(&mut node, "layout.display", "free");
        assert!(matches!(node.layout_mode, LayoutMode::Free));
    }

    #[test]
    fn layout_switch_relative_to_absolute() {
        let mut node = Node {
            id: "n".into(),
            component: "text".into(),
            props: json!({}),
            layout_mode: LayoutMode::Relative(FlowProps::default()),
            ..Default::default()
        };
        apply_layout_to_node(&mut node, "layout.display", "absolute");
        assert!(matches!(node.layout_mode, LayoutMode::Absolute(_)));
    }

    #[test]
    fn layout_absolute_width_edit() {
        let mut node = Node {
            id: "n".into(),
            component: "text".into(),
            props: json!({}),
            layout_mode: LayoutMode::Absolute(AbsoluteProps::default()),
            ..Default::default()
        };
        apply_layout_to_node(&mut node, "layout.width_unit", "px");
        apply_layout_to_node(&mut node, "layout.width_value", "200");
        match &node.layout_mode {
            LayoutMode::Absolute(abs) => {
                assert!(
                    matches!(abs.width, Dimension::Px { value } if (value - 200.0).abs() < f32::EPSILON)
                );
            }
            other => panic!("expected Absolute, got {:?}", other),
        }
    }

    #[test]
    fn layout_absolute_height_percent() {
        let mut node = Node {
            id: "n".into(),
            component: "text".into(),
            props: json!({}),
            layout_mode: LayoutMode::Absolute(AbsoluteProps::default()),
            ..Default::default()
        };
        apply_layout_to_node(&mut node, "layout.height_unit", "%");
        apply_layout_to_node(&mut node, "layout.height_value", "50");
        match &node.layout_mode {
            LayoutMode::Absolute(abs) => {
                assert!(
                    matches!(abs.height, Dimension::Percent { value } if (value - 50.0).abs() < f32::EPSILON)
                );
            }
            other => panic!("expected Absolute, got {:?}", other),
        }
    }

    #[test]
    fn layout_absolute_noop_for_same_mode() {
        let mut node = Node {
            id: "n".into(),
            component: "text".into(),
            props: json!({}),
            layout_mode: LayoutMode::Absolute(AbsoluteProps::default()),
            ..Default::default()
        };
        apply_layout_to_node(&mut node, "layout.display", "absolute");
        assert!(matches!(node.layout_mode, LayoutMode::Absolute(_)));
    }

    #[test]
    fn layout_flow_gap_edit() {
        let mut node = Node {
            id: "n".into(),
            component: "text".into(),
            props: json!({}),
            layout_mode: LayoutMode::Flow(FlowProps::default()),
            ..Default::default()
        };
        apply_layout_to_node(&mut node, "layout.gap", "16");
        match &node.layout_mode {
            LayoutMode::Flow(f) => assert!((f.gap - 16.0).abs() < f32::EPSILON),
            other => panic!("expected Flow, got {:?}", other),
        }
    }

    #[test]
    fn layout_recursive_edit_finds_target() {
        let mut root = Node {
            id: "root".into(),
            component: "container".into(),
            props: json!({}),
            children: vec![Node {
                id: "child".into(),
                component: "text".into(),
                props: json!({}),
                layout_mode: LayoutMode::Flow(FlowProps::default()),
                ..Default::default()
            }],
            ..Default::default()
        };
        assert!(apply_node_layout_edit(
            &mut root,
            "child",
            "layout.display",
            "absolute"
        ));
        assert!(matches!(
            root.children[0].layout_mode,
            LayoutMode::Absolute(_)
        ));
    }

    fn make_absolute_node(id: &str, x: f32, y: f32, w: f32, h: f32) -> Node {
        Node {
            id: id.to_string(),
            component: "card".to_string(),
            layout_mode: LayoutMode::Absolute(AbsoluteProps::fixed(w, h)),
            transform: prism_core::foundation::spatial::Transform2D {
                position: [x, y],
                ..Default::default()
            },
            ..Node::default()
        }
    }

    #[test]
    fn resize_br_grows_size() {
        let mut root = make_absolute_node("n1", 10.0, 20.0, 100.0, 80.0);
        let snap = ResizeSnapshot {
            node_id: "n1".into(),
            position: [10.0, 20.0],
            width: 100.0,
            height: 80.0,
        };
        apply_resize_to_node(&mut root, "br", 30.0, 15.0, false, &snap);
        assert_eq!(root.transform.position, [10.0, 20.0]);
        match &root.layout_mode {
            LayoutMode::Absolute(a) => {
                assert_eq!(a.width, Dimension::Px { value: 130.0 });
                assert_eq!(a.height, Dimension::Px { value: 95.0 });
            }
            _ => panic!("expected Absolute"),
        }
    }

    #[test]
    fn resize_tl_moves_origin_and_shrinks() {
        let mut root = make_absolute_node("n1", 50.0, 50.0, 200.0, 150.0);
        let snap = ResizeSnapshot {
            node_id: "n1".into(),
            position: [50.0, 50.0],
            width: 200.0,
            height: 150.0,
        };
        apply_resize_to_node(&mut root, "tl", 20.0, 10.0, false, &snap);
        assert_eq!(root.transform.position, [70.0, 60.0]);
        match &root.layout_mode {
            LayoutMode::Absolute(a) => {
                assert_eq!(a.width, Dimension::Px { value: 180.0 });
                assert_eq!(a.height, Dimension::Px { value: 140.0 });
            }
            _ => panic!("expected Absolute"),
        }
    }

    #[test]
    fn resize_r_only_changes_width() {
        let mut root = make_absolute_node("n1", 0.0, 0.0, 100.0, 100.0);
        let snap = ResizeSnapshot {
            node_id: "n1".into(),
            position: [0.0, 0.0],
            width: 100.0,
            height: 100.0,
        };
        apply_resize_to_node(&mut root, "r", 50.0, 25.0, false, &snap);
        assert_eq!(root.transform.position, [0.0, 0.0]);
        match &root.layout_mode {
            LayoutMode::Absolute(a) => {
                assert_eq!(a.width, Dimension::Px { value: 150.0 });
                assert_eq!(a.height, Dimension::Px { value: 100.0 });
            }
            _ => panic!("expected Absolute"),
        }
    }

    #[test]
    fn resize_enforces_minimum_size() {
        let mut root = make_absolute_node("n1", 0.0, 0.0, 50.0, 50.0);
        let snap = ResizeSnapshot {
            node_id: "n1".into(),
            position: [0.0, 0.0],
            width: 50.0,
            height: 50.0,
        };
        apply_resize_to_node(&mut root, "tl", 200.0, 200.0, false, &snap);
        match &root.layout_mode {
            LayoutMode::Absolute(a) => {
                assert_eq!(a.width, Dimension::Px { value: 4.0 });
                assert_eq!(a.height, Dimension::Px { value: 4.0 });
            }
            _ => panic!("expected Absolute"),
        }
    }

    #[test]
    fn resize_shift_constrains_aspect_ratio_br() {
        let mut root = make_absolute_node("n1", 0.0, 0.0, 200.0, 100.0);
        let snap = ResizeSnapshot {
            node_id: "n1".into(),
            position: [0.0, 0.0],
            width: 200.0,
            height: 100.0,
        };
        apply_resize_to_node(&mut root, "br", 40.0, 5.0, true, &snap);
        match &root.layout_mode {
            LayoutMode::Absolute(a) => {
                if let Dimension::Px { value: w } = a.width {
                    if let Dimension::Px { value: h } = a.height {
                        let ratio = w / h;
                        assert!(
                            (ratio - 2.0).abs() < 0.01,
                            "aspect ratio should be 2:1, got {ratio}"
                        );
                    }
                }
            }
            _ => panic!("expected Absolute"),
        }
    }

    #[test]
    fn resize_free_node_promotes_to_absolute() {
        let mut root = Node {
            id: "n1".to_string(),
            component: "card".to_string(),
            layout_mode: LayoutMode::Free,
            transform: prism_core::foundation::spatial::Transform2D {
                position: [10.0, 10.0],
                ..Default::default()
            },
            ..Node::default()
        };
        let snap = ResizeSnapshot {
            node_id: "n1".into(),
            position: [10.0, 10.0],
            width: 80.0,
            height: 60.0,
        };
        apply_resize_to_node(&mut root, "br", 20.0, 10.0, false, &snap);
        match &root.layout_mode {
            LayoutMode::Absolute(a) => {
                assert_eq!(a.width, Dimension::Px { value: 100.0 });
                assert_eq!(a.height, Dimension::Px { value: 70.0 });
            }
            _ => panic!("expected Absolute after resize of Free node"),
        }
    }

    #[test]
    fn resolve_facet_data_object_query() {
        use prism_builder::{FacetDataSource, FacetDef, FacetLayout};
        use prism_core::foundation::object_model::types::GraphObject;
        use prism_core::foundation::persistence::CollectionStore;

        let mut store = CollectionStore::new();
        let mut a = GraphObject::new("obj:1", "Task", "Alpha");
        a.data.insert("priority".into(), serde_json::json!(1));
        let mut b = GraphObject::new("obj:2", "Task", "Beta");
        b.data.insert("priority".into(), serde_json::json!(3));
        let mut c = GraphObject::new("obj:3", "Note", "Gamma");
        c.data.insert("priority".into(), serde_json::json!(2));
        store.put_object(&a).unwrap();
        store.put_object(&b).unwrap();
        store.put_object(&c).unwrap();

        let mut doc = BuilderDocument::default();
        doc.facets.insert(
            "facet:q".into(),
            FacetDef {
                id: "facet:q".into(),
                label: "Tasks".into(),
                description: String::new(),
                kind: FacetKind::ObjectQuery {
                    query: prism_core::widget::DataQuery {
                        object_type: Some("Task".into()),
                        sort: vec![prism_core::widget::QuerySort {
                            field: "data.priority".into(),
                            descending: true,
                        }],
                        ..Default::default()
                    },
                },
                schema_id: None,
                data: FacetDataSource::default(),
                bindings: vec![],
                variant_rules: vec![],
                layout: FacetLayout::default(),
                template: FacetTemplate::default(),
                output: FacetOutput::default(),
                resolved_data: None,
            },
        );

        resolve_facet_data(&mut doc, &store);
        let resolved = doc
            .facets
            .get("facet:q")
            .unwrap()
            .resolved_data
            .as_ref()
            .unwrap();
        assert_eq!(resolved.len(), 2);
        assert_eq!(resolved[0]["name"], "Beta");
        assert_eq!(resolved[1]["name"], "Alpha");
    }

    #[test]
    fn resolve_facet_data_object_query_with_filter_and_limit() {
        use prism_builder::{FacetDataSource, FacetDef, FacetLayout};
        use prism_core::foundation::object_model::types::GraphObject;
        use prism_core::foundation::persistence::CollectionStore;

        let mut store = CollectionStore::new();
        for i in 0..5 {
            let mut obj = GraphObject::new(format!("obj:{i}"), "Item", format!("Item {i}"));
            obj.data
                .insert("active".into(), serde_json::json!(i % 2 == 0));
            store.put_object(&obj).unwrap();
        }

        let mut doc = BuilderDocument::default();
        doc.facets.insert(
            "facet:f".into(),
            FacetDef {
                id: "facet:f".into(),
                label: "Active Items".into(),
                description: String::new(),
                kind: FacetKind::ObjectQuery {
                    query: prism_core::widget::DataQuery {
                        object_type: Some("Item".into()),
                        filters: vec![prism_core::widget::QueryFilter::new(
                            "data.active",
                            prism_core::widget::FilterOp::Eq,
                            serde_json::json!(true),
                        )],
                        limit: Some(2),
                        ..Default::default()
                    },
                },
                schema_id: None,
                data: FacetDataSource::default(),
                bindings: vec![],
                variant_rules: vec![],
                layout: FacetLayout::default(),
                template: FacetTemplate::default(),
                output: FacetOutput::default(),
                resolved_data: None,
            },
        );

        resolve_facet_data(&mut doc, &store);
        let resolved = doc
            .facets
            .get("facet:f")
            .unwrap()
            .resolved_data
            .as_ref()
            .unwrap();
        assert_eq!(resolved.len(), 2);
    }

    #[test]
    fn resolve_facet_data_lookup() {
        use prism_builder::{FacetDataSource, FacetDef, FacetLayout};
        use prism_core::foundation::object_model::types::{GraphObject, ObjectEdge};
        use prism_core::foundation::persistence::CollectionStore;

        let mut store = CollectionStore::new();
        let proj = GraphObject::new("proj:1", "Project", "Prism");
        let user_a = GraphObject::new("user:1", "User", "Alice");
        let user_b = GraphObject::new("user:2", "User", "Bob");
        let note = GraphObject::new("note:1", "Note", "Irrelevant");
        store.put_object(&proj).unwrap();
        store.put_object(&user_a).unwrap();
        store.put_object(&user_b).unwrap();
        store.put_object(&note).unwrap();

        let edge1: ObjectEdge = serde_json::from_value(serde_json::json!({
            "id": "e1", "sourceId": "proj:1", "targetId": "user:1",
            "relation": "has_member", "createdAt": "2026-01-01T00:00:00Z", "data": {}
        }))
        .unwrap();
        let edge2: ObjectEdge = serde_json::from_value(serde_json::json!({
            "id": "e2", "sourceId": "proj:1", "targetId": "user:2",
            "relation": "has_member", "createdAt": "2026-01-01T00:00:00Z", "data": {}
        }))
        .unwrap();
        let edge3: ObjectEdge = serde_json::from_value(serde_json::json!({
            "id": "e3", "sourceId": "proj:1", "targetId": "note:1",
            "relation": "has_note", "createdAt": "2026-01-01T00:00:00Z", "data": {}
        }))
        .unwrap();
        store.put_edge(&edge1).unwrap();
        store.put_edge(&edge2).unwrap();
        store.put_edge(&edge3).unwrap();

        let mut doc = BuilderDocument::default();
        doc.facets.insert(
            "facet:l".into(),
            FacetDef {
                id: "facet:l".into(),
                label: "Members".into(),
                description: String::new(),
                kind: FacetKind::Lookup {
                    source_entity: "Project".into(),
                    edge_type: "has_member".into(),
                    target_entity: "User".into(),
                },
                schema_id: None,
                data: FacetDataSource::default(),
                bindings: vec![],
                variant_rules: vec![],
                layout: FacetLayout::default(),
                template: FacetTemplate::default(),
                output: FacetOutput::default(),
                resolved_data: None,
            },
        );

        resolve_facet_data(&mut doc, &store);
        let resolved = doc
            .facets
            .get("facet:l")
            .unwrap()
            .resolved_data
            .as_ref()
            .unwrap();
        assert_eq!(resolved.len(), 2);
        let names: Vec<&str> = resolved.iter().filter_map(|v| v["name"].as_str()).collect();
        assert!(names.contains(&"Alice"));
        assert!(names.contains(&"Bob"));
    }

    #[test]
    fn resolve_facet_data_empty_entity_type_skips() {
        use prism_builder::{FacetDataSource, FacetDef, FacetLayout};
        use prism_core::foundation::persistence::CollectionStore;

        let store = CollectionStore::new();
        let mut doc = BuilderDocument::default();
        doc.facets.insert(
            "facet:e".into(),
            FacetDef {
                id: "facet:e".into(),
                label: "Empty".into(),
                description: String::new(),
                kind: FacetKind::ObjectQuery {
                    query: prism_core::widget::DataQuery::default(),
                },
                schema_id: None,
                data: FacetDataSource::default(),
                bindings: vec![],
                variant_rules: vec![],
                layout: FacetLayout::default(),
                template: FacetTemplate::default(),
                output: FacetOutput::default(),
                resolved_data: None,
            },
        );

        resolve_facet_data(&mut doc, &store);
        assert!(doc.facets.get("facet:e").unwrap().resolved_data.is_none());
    }

    #[test]
    fn resolve_facet_data_script_returns_array() {
        use prism_builder::{FacetDataSource, FacetDef, FacetLayout, ScriptLanguage};
        use prism_core::foundation::persistence::CollectionStore;

        let store = CollectionStore::new();
        let mut doc = BuilderDocument::default();
        doc.facets.insert(
            "facet:s".into(),
            FacetDef {
                id: "facet:s".into(),
                label: "Scripted".into(),
                description: String::new(),
                kind: FacetKind::Script {
                    source: r#"return {
                        { name = "X", value = 1 },
                        { name = "Y", value = 2 },
                    }"#
                    .into(),
                    language: ScriptLanguage::default(),
                    graph: None,
                },
                schema_id: None,
                data: FacetDataSource::default(),
                bindings: vec![],
                variant_rules: vec![],
                layout: FacetLayout::default(),
                template: FacetTemplate::default(),
                output: FacetOutput::default(),
                resolved_data: None,
            },
        );

        resolve_facet_data(&mut doc, &store);
        let resolved = doc
            .facets
            .get("facet:s")
            .unwrap()
            .resolved_data
            .as_ref()
            .unwrap();
        assert_eq!(resolved.len(), 2);
        assert_eq!(resolved[0]["name"], "X");
        assert_eq!(resolved[1]["name"], "Y");
    }

    #[test]
    fn apply_facet_edit_variant_rule_crud() {
        use prism_builder::{FacetDataSource, FacetDef, FacetKind, FacetLayout};
        let mut def = FacetDef {
            id: "facet:test".into(),
            label: "Test".into(),
            description: String::new(),
            kind: FacetKind::List,
            schema_id: None,
            data: FacetDataSource::default(),
            bindings: vec![],
            variant_rules: vec![],
            layout: FacetLayout::default(),
            template: FacetTemplate::default(),
            output: FacetOutput::default(),
            resolved_data: None,
        };
        assert!(def.variant_rules.is_empty());

        apply_facet_edit(&mut def, "add_variant_rule", "");
        assert_eq!(def.variant_rules.len(), 1);
        assert!(def.variant_rules[0].field.is_empty());

        apply_facet_edit(&mut def, "variant_rule.0.field", "status");
        apply_facet_edit(&mut def, "variant_rule.0.value", "featured");
        apply_facet_edit(&mut def, "variant_rule.0.axis_key", "variant");
        apply_facet_edit(&mut def, "variant_rule.0.axis_value", "highlight");
        assert_eq!(def.variant_rules[0].field, "status");
        assert_eq!(def.variant_rules[0].value, "featured");
        assert_eq!(def.variant_rules[0].axis_key, "variant");
        assert_eq!(def.variant_rules[0].axis_value, "highlight");

        apply_facet_edit(&mut def, "add_variant_rule", "");
        assert_eq!(def.variant_rules.len(), 2);

        apply_facet_edit(&mut def, "remove_variant_rule.0", "");
        assert_eq!(def.variant_rules.len(), 1);
    }

    #[test]
    fn apply_facet_edit_script_language_switch() {
        use prism_builder::{FacetDataSource, FacetDef, FacetKind, FacetLayout, ScriptLanguage};
        let mut def = FacetDef {
            id: "facet:test".into(),
            label: "Test".into(),
            description: String::new(),
            kind: FacetKind::Script {
                source: "return {}".into(),
                language: ScriptLanguage::Luau,
                graph: None,
            },
            schema_id: None,
            data: FacetDataSource::default(),
            bindings: vec![],
            variant_rules: vec![],
            layout: FacetLayout::default(),
            template: FacetTemplate::default(),
            output: FacetOutput::default(),
            resolved_data: None,
        };

        apply_facet_edit(&mut def, "script_language", "visual-graph");
        match &def.kind {
            FacetKind::Script { language, .. } => {
                assert_eq!(*language, ScriptLanguage::VisualGraph);
            }
            _ => panic!("expected Script kind"),
        }

        apply_facet_edit(&mut def, "script_language", "luau");
        match &def.kind {
            FacetKind::Script { language, .. } => {
                assert_eq!(*language, ScriptLanguage::Luau);
            }
            _ => panic!("expected Script kind"),
        }
    }

    #[test]
    fn apply_facet_edit_object_query_fields() {
        use prism_builder::{FacetDataSource, FacetDef, FacetKind, FacetLayout};
        let mut def = FacetDef {
            id: "facet:test".into(),
            label: "Test".into(),
            description: String::new(),
            kind: FacetKind::ObjectQuery {
                query: prism_core::widget::DataQuery::default(),
            },
            schema_id: None,
            data: FacetDataSource::default(),
            bindings: vec![],
            variant_rules: vec![],
            layout: FacetLayout::default(),
            template: FacetTemplate::default(),
            output: FacetOutput::default(),
            resolved_data: None,
        };

        apply_facet_edit(&mut def, "entity_type", "BlogPost");
        apply_facet_edit(&mut def, "oq_filter", "status == published");
        apply_facet_edit(&mut def, "oq_sort_by", "-created_at");
        apply_facet_edit(&mut def, "oq_limit", "10");

        match &def.kind {
            FacetKind::ObjectQuery { query } => {
                assert_eq!(query.object_type.as_deref(), Some("BlogPost"));
                assert_eq!(query.filters.len(), 1);
                assert_eq!(query.filters[0].field, "status");
                assert_eq!(query.sort.len(), 1);
                assert_eq!(query.sort[0].field, "created_at");
                assert!(query.sort[0].descending);
                assert_eq!(query.limit, Some(10));
            }
            _ => panic!("expected ObjectQuery kind"),
        }

        apply_facet_edit(&mut def, "oq_filter", "");
        match &def.kind {
            FacetKind::ObjectQuery { query } => assert!(query.filters.is_empty()),
            _ => panic!("expected ObjectQuery kind"),
        }
    }

    #[test]
    fn apply_facet_edit_aggregate_fields() {
        use prism_builder::{AggregateOp, FacetDataSource, FacetDef, FacetKind, FacetLayout};
        let mut def = FacetDef {
            id: "facet:test".into(),
            label: "Test".into(),
            description: String::new(),
            kind: FacetKind::Aggregate {
                operation: AggregateOp::Count,
                field: None,
            },
            schema_id: None,
            data: FacetDataSource::default(),
            bindings: vec![],
            variant_rules: vec![],
            layout: FacetLayout::default(),
            template: FacetTemplate::default(),
            output: FacetOutput::default(),
            resolved_data: None,
        };

        apply_facet_edit(&mut def, "agg_operation", "sum");
        apply_facet_edit(&mut def, "agg_field", "price");

        match &def.kind {
            FacetKind::Aggregate { operation, field } => {
                assert!(matches!(operation, AggregateOp::Sum));
                assert_eq!(field.as_deref(), Some("price"));
            }
            _ => panic!("expected Aggregate kind"),
        }

        apply_facet_edit(&mut def, "agg_operation", "join");
        apply_facet_edit(&mut def, "agg_separator", " | ");
        match &def.kind {
            FacetKind::Aggregate { operation, .. } => match operation {
                AggregateOp::Join { separator } => assert_eq!(separator, " | "),
                _ => panic!("expected Join"),
            },
            _ => panic!("expected Aggregate kind"),
        }
    }

    #[test]
    fn apply_facet_edit_lookup_fields() {
        use prism_builder::{FacetDataSource, FacetDef, FacetKind, FacetLayout};
        let mut def = FacetDef {
            id: "facet:test".into(),
            label: "Test".into(),
            description: String::new(),
            kind: FacetKind::Lookup {
                source_entity: String::new(),
                edge_type: String::new(),
                target_entity: String::new(),
            },
            schema_id: None,
            data: FacetDataSource::default(),
            bindings: vec![],
            variant_rules: vec![],
            layout: FacetLayout::default(),
            template: FacetTemplate::default(),
            output: FacetOutput::default(),
            resolved_data: None,
        };

        apply_facet_edit(&mut def, "lookup_source", "Project");
        apply_facet_edit(&mut def, "lookup_edge", "has_member");
        apply_facet_edit(&mut def, "lookup_target", "User");

        match &def.kind {
            FacetKind::Lookup {
                source_entity,
                edge_type,
                target_entity,
            } => {
                assert_eq!(source_entity, "Project");
                assert_eq!(edge_type, "has_member");
                assert_eq!(target_entity, "User");
            }
            _ => panic!("expected Lookup kind"),
        }
    }

    #[test]
    fn apply_facet_edit_kind_switch() {
        use prism_builder::{FacetDataSource, FacetDef, FacetKind, FacetLayout};
        let mut def = FacetDef {
            id: "facet:test".into(),
            label: "Test".into(),
            description: String::new(),
            kind: FacetKind::List,
            schema_id: None,
            data: FacetDataSource::default(),
            bindings: vec![],
            variant_rules: vec![],
            layout: FacetLayout::default(),
            template: FacetTemplate::default(),
            output: FacetOutput::default(),
            resolved_data: None,
        };

        apply_facet_edit(&mut def, "kind", "object-query");
        assert!(matches!(def.kind, FacetKind::ObjectQuery { .. }));

        apply_facet_edit(&mut def, "kind", "script");
        assert!(matches!(def.kind, FacetKind::Script { .. }));

        apply_facet_edit(&mut def, "kind", "aggregate");
        assert!(matches!(def.kind, FacetKind::Aggregate { .. }));

        apply_facet_edit(&mut def, "kind", "lookup");
        assert!(matches!(def.kind, FacetKind::Lookup { .. }));

        apply_facet_edit(&mut def, "kind", "list");
        assert!(matches!(def.kind, FacetKind::List));
    }

    #[test]
    fn sync_script_language_decompiles_luau_to_graph() {
        use prism_builder::{FacetDataSource, FacetDef, FacetKind, FacetLayout, ScriptLanguage};
        let mut def = FacetDef {
            id: "facet:test".into(),
            label: "Test".into(),
            description: String::new(),
            kind: FacetKind::Script {
                source: "return {}".into(),
                language: ScriptLanguage::Luau,
                graph: None,
            },
            schema_id: None,
            data: FacetDataSource::default(),
            bindings: vec![],
            variant_rules: vec![],
            layout: FacetLayout::default(),
            template: FacetTemplate::default(),
            output: FacetOutput::default(),
            resolved_data: None,
        };

        apply_facet_edit(&mut def, "script_language", "visual-graph");
        match &def.kind {
            FacetKind::Script {
                language, graph, ..
            } => {
                assert_eq!(*language, ScriptLanguage::VisualGraph);
                assert!(
                    graph.is_some(),
                    "switching to visual-graph should decompile existing source"
                );
            }
            _ => panic!("expected Script kind"),
        }
    }
}
