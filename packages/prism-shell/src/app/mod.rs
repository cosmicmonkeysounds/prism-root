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
use prism_builder::{
    app::PrismApp, starter::register_builtins, BuilderDocument, ComponentRegistry, DispatchResult,
    LiveDocument, Node,
};
use prism_core::design_tokens::{DesignTokens, DEFAULT_TOKENS};
use prism_core::editor::EditorState;
#[cfg(feature = "native")]
use prism_core::foundation::persistence::CollectionStore;
use prism_core::foundation::vfs::VfsManager;
use prism_core::help::HelpRegistry;
use prism_core::shell_mode::{Permission, ShellMode, ShellModeContext};
use prism_core::{Action, Store, Subscription};
use serde::{Deserialize, Serialize};
use slint::{ComponentHandle, Model, ModelRc, Timer, TimerMode, VecModel};

use crate::command::CommandRegistry;
use crate::help::register_help_entries;
use crate::input::{update_panel_schemes, InputManager};
use crate::keybindings::UserKeybindings;
use crate::persistence::ProjectPersistence;
use crate::selection::SelectionModel;
use crate::telemetry::FirstPaint;
use crate::{
    AppCardItem, AppWindow, BreadcrumbItem, ButtonSpec, CommandItem, ComponentPaletteItem,
    DockDividerRect, DockPanelRect, EditorLine, ExplorerNodeItem, FieldRow, GridCellItem,
    GridEdgeHandle, GutterRect, InspectorNode, MenuDef, PreviewNode, SearchResultItem, TabItem,
    ToastItem, WorkflowPageItem,
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

mod callbacks;
mod commands;
mod mutations;
mod samples;
mod sync;

use commands::*;
use mutations::*;
use samples::sample_apps;
pub(crate) use sync::mime_from_extension;
use sync::*;

#[cfg(test)]
use samples::sample_document;

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
        prism_builder::register_core_widgets(&mut registry)
            .expect("core widget components must register");
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
}

// ── Tests ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use prism_builder::layout::{AbsoluteProps, Dimension, FlowDisplay, FlowProps, LayoutMode};
    use prism_builder::{FacetKind, FacetOutput, FacetTemplate, PrefabDef};
    use serde_json::json;

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
        let core_widget_count = prism_builder::collect_all_contributions().len();
        // 6 static categories + widget categories
        assert!(headers.len() >= 6);
        // 16 static components + core widgets
        assert_eq!(components.len(), 16 + core_widget_count);
        assert_eq!(items.len(), headers.len() + components.len());
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
    fn resolve_widget_data_populates_matching_nodes() {
        use prism_core::foundation::object_model::types::GraphObject;
        use prism_core::foundation::persistence::CollectionStore;

        let mut store = CollectionStore::new();
        let mut e1 = GraphObject::new("ev:1", "event", "Standup");
        e1.data
            .insert("date".into(), serde_json::json!("2026-05-01"));
        let mut e2 = GraphObject::new("ev:2", "event", "Retro");
        e2.data
            .insert("date".into(), serde_json::json!("2026-05-02"));
        store.put_object(&e1).unwrap();
        store.put_object(&e2).unwrap();

        let doc = BuilderDocument {
            root: Some(Node {
                id: "root".into(),
                component: "container".into(),
                children: vec![Node {
                    id: "cal-1".into(),
                    component: "calendar-agenda".into(),
                    ..Node::default()
                }],
                ..Node::default()
            }),
            ..BuilderDocument::default()
        };

        let data = resolve_widget_data(&doc, &store);
        assert!(data.contains_key("cal-1"));
        let val = &data["cal-1"];
        let events = val["events"].as_array().unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0]["name"], "Standup");
    }

    #[test]
    fn resolve_widget_data_skips_non_widget_nodes() {
        use prism_core::foundation::persistence::CollectionStore;

        let store = CollectionStore::new();
        let doc = BuilderDocument {
            root: Some(Node {
                id: "root".into(),
                component: "card".into(),
                ..Node::default()
            }),
            ..BuilderDocument::default()
        };

        let data = resolve_widget_data(&doc, &store);
        assert!(data.is_empty());
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

    #[test]
    fn apply_facet_edit_wrap_and_columns() {
        use prism_builder::{FacetDataSource, FacetDef, FacetLayout};
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

        assert!(!def.layout.wrap);
        assert_eq!(def.layout.columns, None);

        apply_facet_edit(&mut def, "wrap", "true");
        assert!(def.layout.wrap);

        apply_facet_edit(&mut def, "wrap", "false");
        assert!(!def.layout.wrap);

        apply_facet_edit(&mut def, "columns", "3");
        assert_eq!(def.layout.columns, Some(3));

        apply_facet_edit(&mut def, "columns", "0");
        assert_eq!(def.layout.columns, None);

        apply_facet_edit(&mut def, "columns", "");
        assert_eq!(def.layout.columns, None);
    }

    #[test]
    fn facet_promote_command_registered() {
        let reg = super::CommandRegistry::with_builtins();
        assert!(reg.get("facet.promote").is_some());
        assert_eq!(reg.get("facet.promote").unwrap().category, "Facet");
    }
}
