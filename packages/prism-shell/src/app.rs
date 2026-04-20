//! Root application state + Slint binding layer.
//!
//! Everything reloadable lives behind a single [`AppState`] so §7's
//! hot-reload story is exactly one serde call. Mutation goes through
//! the [`Shell`] wrapper, which owns both a
//! `prism_core::Store<AppState>` and the root `AppWindow` Slint
//! handle. Shell state that callbacks need to mutate lives behind
//! `Rc<RefCell<ShellInner>>` so Slint closures can borrow it.

use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;
use std::sync::Arc;

use prism_builder::{
    app::{AppIcon, NavigationConfig, Page, PrismApp},
    layout::{
        AlignOption, Dimension, FlexDirection, FlowDisplay, FlowProps, GridPlacement,
        JustifyOption, LayoutMode, PageSize, TrackSize,
    },
    starter::register_builtins,
    BuilderDocument, ComponentRegistry, FieldKind, LiveDocument, Node, NodeId, StyleProperties,
};
use prism_core::design_tokens::{DesignTokens, DEFAULT_TOKENS};
use prism_core::editor::EditorState;
use prism_core::help::HelpRegistry;
use prism_core::shell_mode::{Permission, ShellMode, ShellModeContext};
use prism_core::{Action, Store, Subscription};
use serde::{Deserialize, Serialize};
use serde_json::json;
use slint::{ComponentHandle, Model, ModelRc, SharedString, Timer, TimerMode, VecModel};

use crate::command::CommandRegistry;
use crate::help::register_help_entries;
use crate::input::{combo_from_slint, update_panel_schemes, FocusRegion, InputManager};
use crate::panels::{editor::CodeEditorPanel, properties::PropertiesPanel, Panel};
use crate::search::SearchIndex;
use crate::selection::SelectionModel;
use crate::telemetry::FirstPaint;
use crate::{
    AppCardItem, AppWindow, BreadcrumbItem, BuilderNode, ButtonSpec, CommandItem,
    ComponentPaletteItem, DockDividerRect, DockPanelRect, DockTabItem, DocsPanelData,
    EditorIndentGuide, EditorLine, EditorToken, ExplorerNodeItem, FieldRow, GridCellItem,
    GutterRect, HelpTooltipData, InspectorNode, MenuDef, MenuItem, ModifierItem, PageLayoutData,
    SearchResultItem, SignalItem, TabItem, ToastItem, VariantItem, WorkflowPageItem,
};

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
    next_toast_id: u64,
    next_node_id: u64,
    next_app_id: u64,
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
        _ => "edit",
    }
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
            next_toast_id: 0,
            next_node_id: 100,
            next_app_id: 10,
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
                    component: "heading".into(),
                    props: json!({ "text": "Dashboard", "level": 2 }),
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
                    component: "heading".into(),
                    props: json!({ "text": "Welcome to Prism", "level": 1 }),
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
            columns: vec![
                TrackSize::Fr { value: 1.0 },
                TrackSize::Fr { value: 2.0 },
                TrackSize::Fr { value: 1.0 },
            ],
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

// ── Shell inner state (shared with callbacks) ──────────────────────

struct ShellInner {
    store: Store<AppState>,
    registry: Arc<ComponentRegistry>,
    live: Option<LiveDocument>,
    help: HelpRegistry,
    input: InputManager,
    commands: CommandRegistry,
    menus: crate::menu::MenuRegistry,
    undo_past: Vec<SourceSnapshot>,
    undo_future: Vec<SourceSnapshot>,
    clipboard: Option<Node>,
    help_pending_id: String,
    help_active_id: String,
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

    fn load_active_page(&mut self) {
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
                state.builder_document = doc;
                state.sync_document_to_app();
            });
        }
    }

    fn sync_builder_document(&mut self) {
        if let Some(ref mut live) = self.live {
            let mut doc = live.document().clone();
            self.store.mutate(|state| {
                doc.page_layout = state.builder_document.page_layout.clone();
                state.builder_document = doc;
            });
        }
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
        let inner = Rc::new(RefCell::new(ShellInner {
            store: Store::new(state),
            registry: Arc::new(registry),
            live: None,
            help,
            input: InputManager::with_defaults(),
            commands: CommandRegistry::with_builtins(),
            menus: crate::menu::MenuRegistry::with_builtins(),
            undo_past: Vec::new(),
            undo_future: Vec::new(),
            clipboard: None,
            help_pending_id: String::new(),
            help_active_id: String::new(),
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
                    sync_ui_from_shared(&deferred_inner, &w);
                }
            },
        );
        std::mem::forget(dock_init_timer);

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

    pub fn select_page(&self, page_id: &str) {
        let pid = page_id.to_string();
        self.inner.borrow_mut().store.mutate(|state| {
            state.workspace.switch_page_by_id(&pid);
        });
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
        self.inner.borrow_mut().store.restore(bytes)?;
        sync_ui_from_shared(&self.inner, &self.window);
        Ok(())
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
                    s.store.mutate(|state| {
                        state.workspace.switch_page_by_id(&pid);
                    });
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
                }
                if let Some(w) = weak.upgrade() {
                    sync_ui_from_shared(&inner, &w);
                }
            }
        });

        // Builder node selection
        self.window.on_builder_node_clicked({
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

        // Inline text editing in builder
        self.window.on_builder_text_edited({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |node_id, value| {
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
                let ct = component_type.to_string();
                {
                    let mut s = inner.borrow_mut();
                    s.push_undo(&format!("Add {ct}"));
                    let node_id = {
                        let id = s.store.state().next_node_id;
                        format!("n{id}")
                    };
                    let props = default_props_for_component(&ct);
                    let parent_id = s.store.state().selection.primary().cloned();
                    if let Some(ref mut live) = s.live {
                        let _ =
                            live.insert_node_in_source(parent_id.as_deref(), &ct, &node_id, &props);
                    }
                    let nid = node_id.clone();
                    s.store.mutate(|state| {
                        state.next_node_id += 1;
                        state.selection.select(nid);
                    });
                    s.sync_builder_document();
                }
                if let Some(w) = weak.upgrade() {
                    sync_ui_from_shared(&inner, &w);
                }
            }
        });

        // Property field editing
        self.window.on_field_edited({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |key, value| {
                let key = key.to_string();
                let value = value.to_string();
                {
                    let mut s = inner.borrow_mut();
                    let selected_id = s.store.state().selection.primary().cloned();
                    if let Some(ref target_id) = selected_id {
                        let kind = field_kind_for_key(&s, &key);
                        let formatted = format_value_for_source(&value, kind.as_deref());
                        s.push_undo(&format!("Edit {key}"));
                        if let Some(ref mut live) = s.live {
                            let _ = live.edit_prop_in_source(target_id, &key, &formatted);
                        }
                        s.sync_builder_document();
                    }
                }
                if let Some(w) = weak.upgrade() {
                    sync_ui_from_shared(&inner, &w);
                }
            }
        });

        // Numeric field editing (from sliders)
        self.window.on_field_edited_number({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |key, val| {
                let key = key.to_string();
                {
                    let mut s = inner.borrow_mut();
                    let selected_id = s.store.state().selection.primary().cloned();
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
                {
                    let mut s = inner.borrow_mut();
                    s.save_to_active_page();
                    s.store.mutate(|state| {
                        if let Some(app) = state.active_app_mut() {
                            app.active_page = page_index as usize;
                        }
                        state.selection.clear();
                        state.sync_document_from_app();
                    });
                    s.load_active_page();
                }
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
                {
                    let mut s = inner.borrow_mut();
                    s.save_to_active_page();
                    s.store.mutate(|state| {
                        if let Some(app) = state.active_app_mut() {
                            let page_num = app.pages.len() + 1;
                            app.pages.push(Page {
                                id: format!("page-{page_num}"),
                                title: format!("Page {page_num}"),
                                route: format!("/page-{page_num}"),
                                source: String::new(),
                                document: BuilderDocument::page_shell(),
                                style: StyleProperties::default(),
                            });
                            app.active_page = app.pages.len() - 1;
                        }
                        state.selection.clear();
                        state.sync_document_from_app();
                    });
                    s.load_active_page();
                }
                if let Some(w) = weak.upgrade() {
                    sync_ui_from_shared(&inner, &w);
                }
            }
        });

        // Tab management (close a page tab)
        self.window.on_tab_activated({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |_tab_id| {
                if let Some(w) = weak.upgrade() {
                    sync_ui_from_shared(&inner, &w);
                }
            }
        });
        self.window.on_tab_closed({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |_tab_id| {
                if let Some(w) = weak.upgrade() {
                    sync_ui_from_shared(&inner, &w);
                }
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
                inner.borrow_mut().store.mutate(|state| {
                    state.editor_state.handle_action(&action);
                });
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
                        inner.borrow_mut().store.mutate(|state| {
                            state.editor_state.insert_char(c);
                        });
                        if let Some(w) = weak.upgrade() {
                            sync_ui_from_shared(&inner, &w);
                        }
                    }
                }
            }
        });

        // Code editor mouse click (display row -> buffer line)
        self.window.on_editor_click({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |display_row, col| {
                inner.borrow_mut().store.mutate(|state| {
                    let buf_line =
                        display_row_to_buffer_line(&state.editor_state, display_row as usize);
                    state
                        .editor_state
                        .set_cursor_position(buf_line, col as usize);
                });
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

        // Layout field editing (per-node)
        self.window.on_layout_field_edited({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |key, value| {
                let key = key.to_string();
                let value = value.to_string();
                {
                    let mut s = inner.borrow_mut();
                    let selected_id = s.store.state().selection.primary().cloned();
                    if let Some(ref target_id) = selected_id {
                        s.push_undo(&format!("Edit layout {key}"));
                        let tid = target_id.clone();
                        s.store.mutate(|state| {
                            if let Some(ref mut root) = state.builder_document.root {
                                apply_node_layout_edit(root, &tid, &key, &value);
                            }
                        });
                    }
                }
                if let Some(w) = weak.upgrade() {
                    sync_ui_from_shared(&inner, &w);
                }
            }
        });

        // Layout numeric field editing (from sliders)
        self.window.on_layout_field_edited_number({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |key, val| {
                let key = key.to_string();
                let value = format_slider_value(val);
                {
                    let mut s = inner.borrow_mut();
                    let selected_id = s.store.state().selection.primary().cloned();
                    if let Some(ref target_id) = selected_id {
                        s.push_undo(&format!("Edit layout {key}"));
                        let tid = target_id.clone();
                        s.store.mutate(|state| {
                            if let Some(ref mut root) = state.builder_document.root {
                                apply_node_layout_edit(root, &tid, &key, &value);
                            }
                        });
                    }
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
            move |col, row| {
                {
                    let mut s = inner.borrow_mut();
                    let occupant_id = {
                        let doc = &s.store.state().builder_document;
                        let children_placements: Vec<(String, Option<usize>, Option<usize>)> = doc
                            .root
                            .as_ref()
                            .map(|r| {
                                r.children
                                    .iter()
                                    .map(|c| {
                                        let (gc, gr) = match &c.layout_mode {
                                            LayoutMode::Flow(f) => (
                                                f.grid_column.resolved_index(),
                                                f.grid_row.resolved_index(),
                                            ),
                                            _ => (None, None),
                                        };
                                        (c.id.clone(), gc, gr)
                                    })
                                    .collect()
                            })
                            .unwrap_or_default();
                        children_placements
                            .iter()
                            .find(|(_, gc, gr)| {
                                gc.unwrap_or(0) == col as usize && gr.unwrap_or(0) == row as usize
                            })
                            .map(|(id, _, _)| id.clone())
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

        // Grid add column
        self.window.on_grid_add_column({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |index| {
                {
                    let mut s = inner.borrow_mut();
                    s.push_undo("Add column");
                    let idx = index as usize;
                    s.store.mutate(|state| {
                        state
                            .builder_document
                            .page_layout
                            .insert_column(idx, TrackSize::Fr { value: 1.0 });
                    });
                }
                if let Some(w) = weak.upgrade() {
                    sync_ui_from_shared(&inner, &w);
                }
            }
        });

        // Grid add row
        self.window.on_grid_add_row({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |index| {
                {
                    let mut s = inner.borrow_mut();
                    s.push_undo("Add row");
                    let idx = index as usize;
                    s.store.mutate(|state| {
                        state
                            .builder_document
                            .page_layout
                            .insert_row(idx, TrackSize::Auto);
                    });
                }
                if let Some(w) = weak.upgrade() {
                    sync_ui_from_shared(&inner, &w);
                }
            }
        });

        // Grid remove column
        self.window.on_grid_remove_column({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |index| {
                {
                    let mut s = inner.borrow_mut();
                    s.push_undo("Remove column");
                    let idx = index as usize;
                    s.store.mutate(|state| {
                        let _ = state.builder_document.page_layout.remove_column(idx);
                    });
                }
                if let Some(w) = weak.upgrade() {
                    sync_ui_from_shared(&inner, &w);
                }
            }
        });

        // Grid remove row
        self.window.on_grid_remove_row({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |index| {
                {
                    let mut s = inner.borrow_mut();
                    s.push_undo("Remove row");
                    let idx = index as usize;
                    s.store.mutate(|state| {
                        let _ = state.builder_document.page_layout.remove_row(idx);
                    });
                }
                if let Some(w) = weak.upgrade() {
                    sync_ui_from_shared(&inner, &w);
                }
            }
        });

        // Grid track resize
        self.window.on_grid_track_resize({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |direction, index, new_size| {
                {
                    let mut s = inner.borrow_mut();
                    s.push_undo("Resize track");
                    let idx = index as usize;
                    let track = TrackSize::Fixed { value: new_size };
                    s.store.mutate(|state| {
                        let pl = &mut state.builder_document.page_layout;
                        if direction == "col" {
                            let _ = pl.resize_column(idx, track);
                        } else {
                            let _ = pl.resize_row(idx, track);
                        }
                    });
                }
                if let Some(w) = weak.upgrade() {
                    sync_ui_from_shared(&inner, &w);
                }
            }
        });

        // Grid cell add component
        self.window.on_grid_cell_add_component({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |component_type, col, row| {
                let ct = component_type.to_string();
                {
                    let mut s = inner.borrow_mut();
                    s.push_undo(&format!("Add {ct} at ({col},{row})"));
                    let node_id = {
                        let id = s.store.state().next_node_id;
                        format!("n{id}")
                    };
                    let props = default_props_for_component(&ct);
                    if let Some(ref mut live) = s.live {
                        let _ = live.insert_node_in_source(Some("root"), &ct, &node_id, &props);
                    }
                    let nid = node_id.clone();
                    s.store.mutate(|state| {
                        state.next_node_id += 1;
                        state.selection.select(nid);
                    });
                    s.sync_builder_document();
                }
                if let Some(w) = weak.upgrade() {
                    sync_ui_from_shared(&inner, &w);
                }
            }
        });

        // Style field edits (from properties panel)
        self.window.on_style_field_edited({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |key, value| {
                let key = key.to_string();
                let value = value.to_string();
                {
                    let mut s = inner.borrow_mut();
                    s.push_undo(&format!("Edit style {key}"));
                    let selected_id = s.store.state().selection.primary().cloned();
                    let style_key = key.strip_prefix("style.").unwrap_or(&key);
                    let sk = style_key.to_string();
                    if let Some(ref target_id) = selected_id {
                        let tid = target_id.clone();
                        s.store.mutate(|state| {
                            if let Some(ref mut root) = state.builder_document.root {
                                if let Some(node) = root.find_mut(&tid) {
                                    apply_style_edit(&mut node.style, &sk, &value);
                                }
                            }
                        });
                    } else {
                        s.store.mutate(|state| {
                            if let Some(app) = state.active_app_mut() {
                                if let Some(page) = app.pages.get_mut(app.active_page) {
                                    apply_style_edit(&mut page.style, &sk, &value);
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

        // Style numeric field edits
        self.window.on_style_field_edited_number({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |key, val| {
                let key = key.to_string();
                let value = format_slider_value(val);
                {
                    let mut s = inner.borrow_mut();
                    s.push_undo(&format!("Edit style {key}"));
                    let selected_id = s.store.state().selection.primary().cloned();
                    let style_key = key.strip_prefix("style.").unwrap_or(&key);
                    let sk = style_key.to_string();
                    if let Some(ref target_id) = selected_id {
                        let tid = target_id.clone();
                        s.store.mutate(|state| {
                            if let Some(ref mut root) = state.builder_document.root {
                                if let Some(node) = root.find_mut(&tid) {
                                    apply_style_edit(&mut node.style, &sk, &value);
                                }
                            }
                        });
                    } else {
                        s.store.mutate(|state| {
                            if let Some(app) = state.active_app_mut() {
                                if let Some(page) = app.pages.get_mut(app.active_page) {
                                    apply_style_edit(&mut page.style, &sk, &value);
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
                    w.set_viewport_width(width);
                    w.set_viewport_preset(SharedString::from(preset.as_str()));
                    sync_ui_from_shared(&inner, &w);
                }
            }
        });
    }
}

// ── Sync UI ────────────────────────────────────────────────────────

fn sync_ui_from_shared(shared: &Rc<RefCell<ShellInner>>, window: &AppWindow) {
    let mut inner = shared.borrow_mut();
    inner.save_to_active_page();
    let has_sel = !inner.store.state().selection.is_empty();
    let has_clip = inner.clipboard.is_some();
    let palette_open = inner.store.state().command_palette_open;
    inner.input.set_context("hasSelection", has_sel);
    inner.input.set_context("hasClipboard", has_clip);
    inner.input.set_context("commandPaletteOpen", palette_open);
    sync_ui_impl(&inner, window);
}

fn sync_ui_impl(inner: &ShellInner, window: &AppWindow) {
    let state = inner.store.state();

    // Launchpad vs App view
    let is_launchpad = state.shell_view.is_launchpad();
    window.set_is_launchpad(is_launchpad);

    if is_launchpad {
        push_app_cards(window, &state.apps);
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

    // Shell chrome visibility
    window.set_show_activity_bar(state.show_activity_bar);
    window.set_show_left_sidebar(state.show_left_sidebar);
    window.set_show_right_sidebar(state.show_right_sidebar);

    // Viewport
    window.set_viewport_width(state.viewport_width);

    // Menu bar
    push_menu_defs(window, &inner.menus, &inner.commands);

    // Activity bar panel selection — derived from dock workspace
    let slint_panel_id = panel_id_for_slint(&state.workspace);
    window.set_active_panel_id(slint_panel_id);

    // Toolbar state
    window.set_has_selection(!state.selection.is_empty());
    window.set_has_clipboard(inner.clipboard.is_some());

    // Panel title + hint
    let (title, hint) = panel_metadata_from_workspace(&state.workspace);
    window.set_panel_title(SharedString::from(title));
    window.set_panel_hint(SharedString::from(hint));

    // Dock layout — push panel rects, dividers, and workflow pages
    push_dock_layout(window, &state.workspace);

    // Clear all panel slots
    clear_panel_slots(window);

    // Fill active panel (only when inside an app)
    if !is_launchpad {
        match slint_panel_id {
            2 => {
                push_editor_data(window, &state.editor_state);
            }
            _ => {
                push_builder_preview(window, &state.builder_document, &state.selection);
                push_inspector_nodes(window, &state.builder_document, &state.selection);
                push_property_rows(
                    window,
                    &state.builder_document,
                    &inner.registry,
                    &state.selection,
                );
                push_breadcrumbs(window, &state.builder_document, &state.selection);
                let vw = state.viewport_width;
                push_page_layout_data(window, &state.builder_document, state.show_grid_overlay, vw);
                push_layout_rows(window, &state.builder_document, &state.selection);
                push_composition_data(
                    window,
                    &state.builder_document,
                    &inner.registry,
                    &state.selection,
                );
                push_grid_cells(window, &state.builder_document, &state.selection, vw);
                push_style_rows(
                    window,
                    state.active_app(),
                    &state.selection,
                    &state.builder_document,
                );
                push_explorer_nodes(
                    window,
                    &state.apps,
                    &state.shell_view,
                    &state.explorer_expanded,
                );
            }
        }
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
    let tab_rc = Rc::new(VecModel::from(tab_items));
    window.set_tabs(ModelRc::from(tab_rc as Rc<dyn Model<Data = TabItem>>));

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
    let cmd_rc = Rc::new(VecModel::from(cmd_items));
    window.set_command_results(ModelRc::from(cmd_rc as Rc<dyn Model<Data = CommandItem>>));

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
    let toast_rc = Rc::new(VecModel::from(toast_items));
    window.set_notifications(ModelRc::from(toast_rc as Rc<dyn Model<Data = ToastItem>>));

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
    let search_rc = Rc::new(VecModel::from(search_items));
    window.set_search_results(ModelRc::from(
        search_rc as Rc<dyn Model<Data = SearchResultItem>>,
    ));
}

fn clear_panel_slots(window: &AppWindow) {
    let empty_actions: Rc<VecModel<ButtonSpec>> = Rc::new(VecModel::from(Vec::<ButtonSpec>::new()));
    window.set_actions(ModelRc::from(
        empty_actions as Rc<dyn Model<Data = ButtonSpec>>,
    ));
    let empty_builder: Rc<VecModel<BuilderNode>> =
        Rc::new(VecModel::from(Vec::<BuilderNode>::new()));
    window.set_builder_nodes(ModelRc::from(
        empty_builder as Rc<dyn Model<Data = BuilderNode>>,
    ));
    window.set_builder_node_count(0);
    window.set_builder_source(SharedString::new());
    window.set_inspector_tree(SharedString::new());
    let empty_nodes: Rc<VecModel<InspectorNode>> =
        Rc::new(VecModel::from(Vec::<InspectorNode>::new()));
    window.set_inspector_nodes(ModelRc::from(
        empty_nodes as Rc<dyn Model<Data = InspectorNode>>,
    ));
    window.set_selected_component(SharedString::new());
    let empty_rows: Rc<VecModel<FieldRow>> = Rc::new(VecModel::from(Vec::<FieldRow>::new()));
    window.set_field_rows(ModelRc::from(empty_rows as Rc<dyn Model<Data = FieldRow>>));
    let empty_palette: Rc<VecModel<ComponentPaletteItem>> =
        Rc::new(VecModel::from(Vec::<ComponentPaletteItem>::new()));
    window.set_component_palette(ModelRc::from(
        empty_palette as Rc<dyn Model<Data = ComponentPaletteItem>>,
    ));
}

fn push_explorer_nodes(
    window: &AppWindow,
    apps: &[PrismApp],
    shell_view: &ShellView,
    expanded: &HashSet<String>,
) {
    let tree = crate::explorer::build_explorer_tree(apps, shell_view, expanded);
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
    let model = Rc::new(VecModel::from(items));
    window.set_explorer_nodes(ModelRc::from(
        model as Rc<dyn Model<Data = ExplorerNodeItem>>,
    ));
}

fn push_menu_defs(
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
    let model = Rc::new(VecModel::from(defs));
    window.set_menu_defs(ModelRc::from(model as Rc<dyn Model<Data = MenuDef>>));
}

fn push_app_cards(window: &AppWindow, apps: &[PrismApp]) {
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
    let model = Rc::new(VecModel::from(items));
    window.set_app_cards(ModelRc::from(model as Rc<dyn Model<Data = AppCardItem>>));
}

fn push_builder_preview(window: &AppWindow, doc: &BuilderDocument, selection: &SelectionModel) {
    let items = flatten_builder_nodes(doc.root.as_ref(), selection);
    let count = items.len() as i32;
    let model = Rc::new(VecModel::from(items));
    window.set_builder_nodes(ModelRc::from(model as Rc<dyn Model<Data = BuilderNode>>));
    window.set_builder_node_count(count);
    let palette = component_palette_items();
    let palette_model = Rc::new(VecModel::from(palette));
    window.set_component_palette(ModelRc::from(
        palette_model as Rc<dyn Model<Data = ComponentPaletteItem>>,
    ));
}

fn push_inspector_nodes(window: &AppWindow, doc: &BuilderDocument, selection: &SelectionModel) {
    let items = flatten_inspector_nodes(doc.root.as_ref(), selection);
    let model = Rc::new(VecModel::from(items));
    window.set_inspector_nodes(ModelRc::from(model as Rc<dyn Model<Data = InspectorNode>>));
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

fn push_property_rows(
    window: &AppWindow,
    doc: &BuilderDocument,
    registry: &ComponentRegistry,
    selection: &SelectionModel,
) {
    let selected = selection.as_option();
    let component_id = PropertiesPanel::selected_component(doc, &selected);
    window.set_selected_component(SharedString::from(component_id));
    let rows: Vec<FieldRow> = PropertiesPanel::rows(doc, registry, &selected)
        .into_iter()
        .map(|r| field_row_data_to_slint(&r))
        .collect();
    let model = Rc::new(VecModel::from(rows));
    window.set_field_rows(ModelRc::from(model as Rc<dyn Model<Data = FieldRow>>));
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
        | "panel.properties" | "panel.explorer" | "view.file_explorer" => {
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
            let panel_id = panel_id_for_slint(&s.store.state().workspace);
            update_panel_schemes(&mut s.input, panel_id);
        }
        "panel.code_editor" => {
            let mut s = shared.borrow_mut();
            s.store.mutate(|state| {
                state.workspace.switch_page_by_id("code");
            });
            let panel_id = panel_id_for_slint(&s.store.state().workspace);
            update_panel_schemes(&mut s.input, panel_id);
        }
        "selection.delete" => {
            let mut s = shared.borrow_mut();
            let selected_id = s.store.state().selection.primary().cloned();
            if let Some(ref target_id) = selected_id {
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
                    let _ = live.insert_node_in_source(
                        parent_id.as_deref(),
                        &new_node.component,
                        &new_node.id,
                        &new_node.props,
                    );
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
                    let _ = live.insert_node_in_source(
                        Some(&target_id),
                        &new_node.component,
                        &new_node.id,
                        &new_node.props,
                    );
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
        "file.save" => {}
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

fn push_dock_layout(window: &AppWindow, workspace: &prism_dock::DockWorkspace) {
    let w = window.get_dock_area_width();
    let h = window.get_dock_area_height();
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

    let panel_model = Rc::new(VecModel::from(panels));
    window.set_dock_panels(ModelRc::from(
        panel_model as Rc<dyn Model<Data = DockPanelRect>>,
    ));
    let divider_model = Rc::new(VecModel::from(dividers));
    window.set_dock_dividers(ModelRc::from(
        divider_model as Rc<dyn Model<Data = DockDividerRect>>,
    ));

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
    let page_model = Rc::new(VecModel::from(page_items));
    window.set_workflow_pages(ModelRc::from(
        page_model as Rc<dyn Model<Data = WorkflowPageItem>>,
    ));
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
    });
    for child in &node.children {
        flatten_walk(child, depth + 1, selection, out);
    }
}

fn flatten_builder_nodes(root: Option<&Node>, selection: &SelectionModel) -> Vec<BuilderNode> {
    let mut items = Vec::new();
    if let Some(node) = root {
        flatten_builder_walk(node, 0, selection, &mut items);
    }
    items
}

fn flatten_builder_walk(
    node: &Node,
    depth: i32,
    selection: &SelectionModel,
    out: &mut Vec<BuilderNode>,
) {
    let props = &node.props;
    let str_prop = |key| {
        props
            .get(key)
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string()
    };
    let int_prop =
        |key, default: i64| props.get(key).and_then(|v| v.as_i64()).unwrap_or(default) as i32;
    let bool_prop = |key| props.get(key).and_then(|v| v.as_bool()).unwrap_or(false);

    let text = match node.component.as_str() {
        "heading" | "link" | "button" => str_prop("text"),
        "text" => str_prop("body"),
        "input" => str_prop("name"),
        "card" | "accordion" => str_prop("title"),
        "code" => str_prop("code"),
        "table" => str_prop("headers"),
        "tabs" => str_prop("labels"),
        _ => String::new(),
    };

    let label = match node.component.as_str() {
        "card" => str_prop("body"),
        "table" => str_prop("caption"),
        _ => str_prop("label"),
    };
    let level = match node.component.as_str() {
        "spacer" => int_prop("height", 24),
        "columns" => int_prop("gap", 16),
        _ => int_prop("level", 1),
    };
    let disabled = match node.component.as_str() {
        "list" => bool_prop("ordered"),
        _ => bool_prop("disabled"),
    };

    let display_mode = match &node.layout_mode {
        LayoutMode::Flow(f) => match f.display {
            FlowDisplay::Block => "block",
            FlowDisplay::Flex => "flex",
            FlowDisplay::Grid => "grid",
            FlowDisplay::None => "none",
        },
        LayoutMode::Free => "free",
    };

    out.push(BuilderNode {
        id: SharedString::from(&node.id),
        component_type: SharedString::from(&node.component),
        selected: selection.contains(&node.id),
        depth,
        text: SharedString::from(text),
        level,
        href: SharedString::from(str_prop("href")),
        alt: SharedString::from(str_prop("alt")),
        placeholder: SharedString::from(str_prop("placeholder")),
        label: SharedString::from(label),
        disabled,
        display_mode: SharedString::from(display_mode),
        has_children: !node.children.is_empty(),
    });
    for child in &node.children {
        flatten_builder_walk(child, depth + 1, selection, out);
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
                _ => "text",
            })
            .unwrap_or("text")
            .to_string(),
    )
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

fn default_props_for_component(component: &str) -> serde_json::Value {
    match component {
        "heading" => json!({ "text": "New heading", "level": 2 }),
        "text" => json!({ "body": "New paragraph" }),
        "link" => json!({ "href": "#", "text": "Link" }),
        "image" => json!({ "src": "", "alt": "Image" }),
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
        _ => json!({}),
    }
}

fn component_palette_items() -> Vec<ComponentPaletteItem> {
    [
        ("heading", "Heading", "h1–h6 text heading"),
        ("text", "Text", "Paragraph of body text"),
        ("link", "Link", "Anchor / hyperlink"),
        ("image", "Image", "Image placeholder"),
        ("container", "Container", "Layout wrapper for children"),
        ("card", "Card", "Bordered card with title and body"),
        ("columns", "Columns", "Side-by-side horizontal layout"),
        ("list", "List", "Ordered or unordered list"),
        ("table", "Table", "Data table with column headers"),
        ("tabs", "Tabs", "Tabbed content panels"),
        ("accordion", "Accordion", "Collapsible content section"),
        ("divider", "Divider", "Horizontal separator line"),
        ("spacer", "Spacer", "Vertical spacing element"),
        ("code", "Code", "Preformatted code block"),
        ("form", "Form", "HTML form wrapper"),
        ("input", "Input", "Text / email / password field"),
        ("button", "Button", "Submit / action button"),
    ]
    .into_iter()
    .map(|(ty, label, desc)| ComponentPaletteItem {
        component_type: SharedString::from(ty),
        label: SharedString::from(label),
        description: SharedString::from(desc),
    })
    .collect()
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

fn push_breadcrumbs(window: &AppWindow, doc: &BuilderDocument, selection: &SelectionModel) {
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
    let model = Rc::new(VecModel::from(items));
    window.set_breadcrumbs(ModelRc::from(model as Rc<dyn Model<Data = BreadcrumbItem>>));
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
        column_count: pl.columns.len().max(1) as i32,
        row_count: pl.rows.len() as i32,
        show_grid,
        is_responsive,
        page_size_label: SharedString::from(size_label),
    });

    let content_width = pw - pl.margins.left - pl.margins.right;
    let content_height = ph - pl.margins.top - pl.margins.bottom;

    let col_gutters = compute_gutter_rects(&pl.columns, pl.column_gap, content_width);
    let row_gutters = compute_gutter_rects(&pl.rows, pl.row_gap, content_height);

    let col_model = Rc::new(VecModel::from(col_gutters));
    window.set_column_gutters(ModelRc::from(col_model as Rc<dyn Model<Data = GutterRect>>));

    let row_model = Rc::new(VecModel::from(row_gutters));
    window.set_row_gutters(ModelRc::from(row_model as Rc<dyn Model<Data = GutterRect>>));
}

fn compute_gutter_rects(tracks: &[TrackSize], gap: f32, available: f32) -> Vec<GutterRect> {
    if tracks.len() <= 1 || gap <= 0.0 {
        return Vec::new();
    }

    let num_gaps = tracks.len() - 1;
    let total_gap = gap * num_gaps as f32;
    let track_space = (available - total_gap).max(0.0);

    let total_fr: f32 = tracks
        .iter()
        .map(|t| match t {
            TrackSize::Fr { value } => *value,
            _ => 0.0,
        })
        .sum();

    let fixed_space: f32 = tracks
        .iter()
        .map(|t| match t {
            TrackSize::Fixed { value } => *value,
            TrackSize::Percent { value } => available * value / 100.0,
            TrackSize::MinMax { min, .. } => *min,
            _ => 0.0,
        })
        .sum();

    let fr_available = (track_space - fixed_space).max(0.0);
    let fr_unit = if total_fr > 0.0 {
        fr_available / total_fr
    } else {
        0.0
    };

    let track_sizes: Vec<f32> = tracks
        .iter()
        .map(|t| match t {
            TrackSize::Fixed { value } => *value,
            TrackSize::Fr { value } => fr_unit * value,
            TrackSize::Auto => {
                if total_fr > 0.0 {
                    0.0
                } else {
                    track_space / tracks.len() as f32
                }
            }
            TrackSize::MinMax { min, max } => (fr_unit).clamp(*min, *max),
            TrackSize::Percent { value } => available * value / 100.0,
        })
        .collect();

    let mut gutters = Vec::with_capacity(num_gaps);
    let mut pos: f32 = 0.0;
    for (i, size) in track_sizes.iter().enumerate() {
        pos += size;
        if i < num_gaps {
            gutters.push(GutterRect {
                offset: pos,
                size: gap,
            });
            pos += gap;
        }
    }
    gutters
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
    let parse_usize = |s: &str| s.parse::<usize>().unwrap_or(0);

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
        "columns" => {
            let count = parse_usize(value).max(1);
            pl.columns = (0..count).map(|_| TrackSize::Fr { value: 1.0 }).collect();
        }
        "rows" => {
            let count = parse_usize(value);
            pl.rows = (0..count).map(|_| TrackSize::Auto).collect();
        }
        "column_gap" => pl.column_gap = parse_f32(value),
        "row_gap" => pl.row_gap = parse_f32(value),
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

fn apply_layout_to_node(node: &mut Node, key: &str, value: &str) {
    let parse_f32 = |s: &str| s.parse::<f32>().unwrap_or(0.0);

    let flow = match &mut node.layout_mode {
        LayoutMode::Flow(f) => f,
        LayoutMode::Free => {
            if key == "layout.display" && value != "free" {
                node.layout_mode = LayoutMode::Flow(FlowProps::default());
                match &mut node.layout_mode {
                    LayoutMode::Flow(f) => f,
                    _ => unreachable!(),
                }
            } else {
                return;
            }
        }
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

fn push_layout_rows(window: &AppWindow, doc: &BuilderDocument, selection: &SelectionModel) {
    let selected = selection.as_option();
    let rows: Vec<FieldRow> = PropertiesPanel::layout_rows(doc, &selected)
        .into_iter()
        .map(|r| field_row_data_to_slint(&r))
        .collect();
    let model = Rc::new(VecModel::from(rows));
    window.set_layout_rows(ModelRc::from(model as Rc<dyn Model<Data = FieldRow>>));
}

fn push_composition_data(
    window: &AppWindow,
    doc: &BuilderDocument,
    registry: &prism_builder::ComponentRegistry,
    selection: &SelectionModel,
) {
    let selected = selection.as_option();
    let node = selected
        .as_ref()
        .and_then(|id| doc.root.as_ref().and_then(|r| r.find(id)));

    let modifiers: Vec<ModifierItem> = node
        .map(|n| {
            n.modifiers
                .iter()
                .map(|m| ModifierItem {
                    kind: SharedString::from(
                        serde_json::to_string(&m.kind)
                            .unwrap_or_default()
                            .trim_matches('"'),
                    ),
                    label: SharedString::from(m.kind.label()),
                })
                .collect()
        })
        .unwrap_or_default();
    let model = Rc::new(VecModel::from(modifiers));
    window.set_modifier_items(ModelRc::from(model as Rc<dyn Model<Data = ModifierItem>>));

    let comp = node.and_then(|n| registry.get(&n.component));

    let signals: Vec<SignalItem> = comp
        .as_ref()
        .map(|c| {
            c.signals()
                .into_iter()
                .map(|s| SignalItem {
                    name: SharedString::from(s.name.as_str()),
                    description: SharedString::from(s.description.as_str()),
                })
                .collect()
        })
        .unwrap_or_default();
    let model = Rc::new(VecModel::from(signals));
    window.set_signal_items(ModelRc::from(model as Rc<dyn Model<Data = SignalItem>>));

    let variants: Vec<VariantItem> = comp
        .and_then(|c| {
            let axes = c.variants();
            if axes.is_empty() {
                return None;
            }
            let n = node.unwrap();
            Some(
                axes.into_iter()
                    .map(|axis| {
                        let current = n
                            .props
                            .get(&axis.key)
                            .and_then(|v| v.as_str())
                            .unwrap_or("");
                        let opts: Vec<SharedString> = axis
                            .options
                            .iter()
                            .map(|o| SharedString::from(o.label.as_str()))
                            .collect();
                        VariantItem {
                            key: SharedString::from(axis.key.as_str()),
                            label: SharedString::from(axis.label.as_str()),
                            selected: SharedString::from(current),
                            options: ModelRc::from(
                                Rc::new(VecModel::from(opts)) as Rc<dyn Model<Data = SharedString>>
                            ),
                        }
                    })
                    .collect(),
            )
        })
        .unwrap_or_default();
    let model = Rc::new(VecModel::from(variants));
    window.set_variant_items(ModelRc::from(model as Rc<dyn Model<Data = VariantItem>>));

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
    window: &AppWindow,
    doc: &BuilderDocument,
    selection: &SelectionModel,
    viewport_width: f32,
) {
    let pl = &doc.page_layout;
    let cols = pl.columns.len().max(1);
    let rows = pl.rows.len().max(1);

    if pl.columns.is_empty() && pl.rows.is_empty() {
        window.set_grid_cells(ModelRc::default());
        return;
    }

    let resolved = pl.resolved_size();
    let (pw, ph) = resolved
        .map(|s| (s.width, s.height))
        .unwrap_or((viewport_width, viewport_width * 0.625));
    let content_w = pw - pl.margins.left - pl.margins.right;
    let content_h = ph - pl.margins.top - pl.margins.bottom;

    let col_sizes = compute_track_sizes(&pl.columns, pl.column_gap, content_w);
    let row_sizes = compute_track_sizes(&pl.rows, pl.row_gap, content_h);

    let children: Vec<(&str, Option<usize>, Option<usize>, &str)> = doc
        .root
        .as_ref()
        .map(|r| {
            r.children
                .iter()
                .map(|c| {
                    let (gc, gr) = match &c.layout_mode {
                        LayoutMode::Flow(f) => {
                            (f.grid_column.resolved_index(), f.grid_row.resolved_index())
                        }
                        _ => (None, None),
                    };
                    let preview = c
                        .props
                        .get("text")
                        .or_else(|| c.props.get("body"))
                        .or_else(|| c.props.get("title"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    (c.id.as_str(), gc, gr, preview)
                })
                .collect()
        })
        .unwrap_or_default();

    let mut cells: Vec<GridCellItem> = Vec::with_capacity(cols * rows);
    let mut y_off = 0.0_f32;
    for r in 0..rows {
        let rh = row_sizes.get(r).copied().unwrap_or(80.0);
        let mut x_off = 0.0_f32;
        for c in 0..cols {
            let cw = col_sizes.get(c).copied().unwrap_or(content_w / cols as f32);
            let occupant = children
                .iter()
                .find(|(_, gc, gr, _)| gc.unwrap_or(0) == c && gr.unwrap_or(0) == r);
            let (is_empty, node_id, component_type, selected, preview_text) = match occupant {
                Some((id, _, _, preview)) => {
                    let node = doc.root.as_ref().and_then(|root| root.find(id));
                    let ct = node.map(|n| n.component.as_str()).unwrap_or("");
                    (false, *id, ct, selection.contains(id), *preview)
                }
                None => (true, "", "", false, ""),
            };
            cells.push(GridCellItem {
                col: c as i32,
                row: r as i32,
                x: x_off,
                y: y_off,
                width: cw,
                height: rh.max(60.0),
                is_empty,
                node_id: SharedString::from(node_id),
                component_type: SharedString::from(component_type),
                selected,
                preview_text: SharedString::from(preview_text),
            });
            x_off += cw + pl.column_gap;
        }
        y_off += rh.max(60.0) + pl.row_gap;
    }

    let model = Rc::new(VecModel::from(cells));
    window.set_grid_cells(ModelRc::from(model as Rc<dyn Model<Data = GridCellItem>>));
}

fn compute_track_sizes(tracks: &[TrackSize], gap: f32, available: f32) -> Vec<f32> {
    if tracks.is_empty() {
        return vec![available];
    }
    let num_gaps = if tracks.len() > 1 {
        tracks.len() - 1
    } else {
        0
    };
    let total_gap = gap * num_gaps as f32;
    let track_space = (available - total_gap).max(0.0);

    let total_fr: f32 = tracks
        .iter()
        .map(|t| match t {
            TrackSize::Fr { value } => *value,
            _ => 0.0,
        })
        .sum();

    let fixed_space: f32 = tracks
        .iter()
        .map(|t| match t {
            TrackSize::Fixed { value } => *value,
            TrackSize::Percent { value } => available * value / 100.0,
            TrackSize::MinMax { min, .. } => *min,
            _ => 0.0,
        })
        .sum();

    let fr_available = (track_space - fixed_space).max(0.0);
    let fr_unit = if total_fr > 0.0 {
        fr_available / total_fr
    } else {
        0.0
    };

    tracks
        .iter()
        .map(|t| match t {
            TrackSize::Fixed { value } => *value,
            TrackSize::Fr { value } => fr_unit * value,
            TrackSize::Auto => {
                if total_fr > 0.0 {
                    0.0
                } else {
                    track_space / tracks.len() as f32
                }
            }
            TrackSize::MinMax { min, max } => fr_unit.clamp(*min, *max),
            TrackSize::Percent { value } => available * value / 100.0,
        })
        .collect()
}

fn push_style_rows(
    window: &AppWindow,
    app: Option<&PrismApp>,
    selection: &SelectionModel,
    doc: &BuilderDocument,
) {
    let default_style = StyleProperties::default();
    let app_style = app.map(|a| &a.style).unwrap_or(&default_style);
    let page_style = app
        .and_then(|a| a.pages.get(a.active_page))
        .map(|p| &p.style)
        .unwrap_or(&default_style);
    let node_style = selection
        .as_option()
        .and_then(|id| doc.root.as_ref().and_then(|r| r.find(&id)))
        .map(|n| &n.style);
    let resolved = if let Some(ns) = node_style {
        prism_builder::resolve_cascade(app_style, page_style, ns)
    } else {
        prism_builder::resolve_cascade(app_style, page_style, &default_style)
    };

    let mut rows: Vec<FieldRow> = Vec::new();
    let empty_opts: Vec<SharedString> = Vec::new();

    let push_text = |rows: &mut Vec<FieldRow>, key: &str, label: &str, val: &Option<String>| {
        rows.push(FieldRow {
            key: SharedString::from(key),
            label: SharedString::from(label),
            kind: SharedString::from("text"),
            value: SharedString::from(val.as_deref().unwrap_or("")),
            required: false,
            min: 0.0,
            max: 0.0,
            has_bounds: false,
            options: ModelRc::from(
                Rc::new(VecModel::from(empty_opts.clone())) as Rc<dyn Model<Data = SharedString>>
            ),
            swatch: slint::Color::from_argb_u8(0, 0, 0, 0),
        });
    };
    let push_number = |rows: &mut Vec<FieldRow>,
                       key: &str,
                       label: &str,
                       val: &Option<f32>,
                       min: f32,
                       max: f32| {
        rows.push(FieldRow {
            key: SharedString::from(key),
            label: SharedString::from(label),
            kind: SharedString::from("number"),
            value: SharedString::from(val.map(|v| format!("{v}")).unwrap_or_default()),
            required: false,
            min,
            max,
            has_bounds: true,
            options: ModelRc::from(
                Rc::new(VecModel::from(empty_opts.clone())) as Rc<dyn Model<Data = SharedString>>
            ),
            swatch: slint::Color::from_argb_u8(0, 0, 0, 0),
        });
    };
    let push_color = |rows: &mut Vec<FieldRow>, key: &str, label: &str, val: &Option<String>| {
        let hex = val.as_deref().unwrap_or("#000000");
        rows.push(FieldRow {
            key: SharedString::from(key),
            label: SharedString::from(label),
            kind: SharedString::from("color"),
            value: SharedString::from(hex),
            required: false,
            min: 0.0,
            max: 0.0,
            has_bounds: false,
            options: ModelRc::from(
                Rc::new(VecModel::from(empty_opts.clone())) as Rc<dyn Model<Data = SharedString>>
            ),
            swatch: parse_hex_color(hex),
        });
    };

    push_text(
        &mut rows,
        "style.font_family",
        "Font family",
        &resolved.font_family,
    );
    push_number(
        &mut rows,
        "style.font_size",
        "Font size",
        &resolved.font_size,
        6.0,
        120.0,
    );
    push_number(
        &mut rows,
        "style.font_weight",
        "Font weight",
        &resolved.font_weight.map(|w| w as f32),
        100.0,
        900.0,
    );
    push_number(
        &mut rows,
        "style.line_height",
        "Line height",
        &resolved.line_height,
        0.5,
        4.0,
    );
    push_color(&mut rows, "style.color", "Text color", &resolved.color);
    push_color(
        &mut rows,
        "style.background",
        "Background",
        &resolved.background,
    );
    push_color(&mut rows, "style.accent", "Accent", &resolved.accent);
    push_number(
        &mut rows,
        "style.base_spacing",
        "Spacing",
        &resolved.base_spacing,
        0.0,
        64.0,
    );
    push_number(
        &mut rows,
        "style.border_radius",
        "Radius",
        &resolved.border_radius,
        0.0,
        64.0,
    );

    let model = Rc::new(VecModel::from(rows));
    window.set_style_rows(ModelRc::from(model as Rc<dyn Model<Data = FieldRow>>));
}

fn push_editor_data(window: &AppWindow, es: &EditorState) {
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

    let model = Rc::new(VecModel::from(lines));
    window.set_editor_lines(ModelRc::from(model as Rc<dyn Model<Data = EditorLine>>));
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
        assert_eq!(state.workspace.pages().len(), 4);
        let ids: Vec<&str> = state
            .workspace
            .pages()
            .iter()
            .map(|p| p.id.as_str())
            .collect();
        assert_eq!(ids, &["edit", "design", "code", "fusion"]);
    }

    #[test]
    fn panel_id_for_slint_maps_pages() {
        let mut state = AppState::default();
        assert_eq!(panel_id_for_slint(&state.workspace), 1);
        state.workspace.switch_page_by_id("code");
        assert_eq!(panel_id_for_slint(&state.workspace), 2);
        state.workspace.switch_page_by_id("fusion");
        assert_eq!(panel_id_for_slint(&state.workspace), 1);
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
            "heading",
            "text",
            "link",
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
    fn component_palette_has_all_seventeen_types() {
        let items = component_palette_items();
        assert_eq!(items.len(), 17);
    }

    #[test]
    fn toast_data_serializes() {
        let toast = ToastData {
            id: 1,
            title: "Saved".into(),
            body: "Document saved.".into(),
            kind: "success".into(),
        };
        let json = serde_json::to_string(&toast).unwrap();
        let restored: ToastData = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.title, "Saved");
    }

    #[test]
    fn clone_node_with_new_ids_generates_unique_ids() {
        let node = Node {
            id: "original".into(),
            component: "heading".into(),
            props: json!({ "text": "Hello" }),
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
        assert_eq!(cloned.component, "heading");
        assert_eq!(cloned.props["text"], "Hello");
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
}
