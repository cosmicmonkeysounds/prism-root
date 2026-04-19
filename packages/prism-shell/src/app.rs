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
    BuilderDocument, ComponentRegistry, FieldKind, Node, NodeId,
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
use crate::panels::{
    editor::CodeEditorPanel, identity::IdentityPanel, properties::PropertiesPanel, Panel,
};
use crate::search::SearchIndex;
use crate::selection::SelectionModel;
use crate::telemetry::FirstPaint;
use crate::{
    AppCardItem, AppWindow, BreadcrumbItem, BuilderNode, ButtonSpec, CommandItem,
    ComponentPaletteItem, DocsPanelData, EditorLine, EditorToken, ExplorerNodeItem, FieldRow,
    GutterRect, HelpTooltipData, InspectorNode, MenuDef, MenuItem, ModifierItem, PageLayoutData,
    SearchResultItem, SignalItem, TabItem, ToastItem, VariantItem,
};

// ── Reloadable state ───────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppState {
    pub tokens: DesignTokens,
    pub context: ShellModeContext,
    pub shell_view: ShellView,
    pub active_panel: ActivePanel,
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
    next_toast_id: u64,
    next_node_id: u64,
    next_app_id: u64,
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

    fn sync_document_to_app(&mut self) {
        let doc = self.builder_document.clone();
        if let Some(app) = self.active_app_mut() {
            if let Some(page_doc) = app.active_document_mut() {
                *page_doc = doc;
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToastData {
    pub id: u64,
    pub title: String,
    pub body: String,
    pub kind: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ActivePanel {
    Identity,
    Edit,
    CodeEditor,
    Explorer,
}

impl ActivePanel {
    pub fn as_id(self) -> i32 {
        match self {
            ActivePanel::Identity => 0,
            ActivePanel::Edit => 1,
            ActivePanel::CodeEditor => 2,
            ActivePanel::Explorer => 3,
        }
    }

    pub fn from_id(id: i32) -> Self {
        match id {
            0 => ActivePanel::Identity,
            2 => ActivePanel::CodeEditor,
            3 => ActivePanel::Explorer,
            _ => ActivePanel::Edit,
        }
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
            active_panel: ActivePanel::Edit,
            apps,
            builder_document: BuilderDocument::default(),
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
                    document: sample_document(),
                },
                Page {
                    id: "p2".into(),
                    title: "Dashboard".into(),
                    route: "/dashboard".into(),
                    document: sample_dashboard_document(),
                },
            ],
            active_page: 0,
            navigation: NavigationConfig::default(),
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
                document: BuilderDocument::default(),
            }],
            active_page: 0,
            navigation: NavigationConfig::default(),
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
                    document: BuilderDocument::default(),
                },
                Page {
                    id: "p2".into(),
                    title: "Settings".into(),
                    route: "/settings".into(),
                    document: BuilderDocument::default(),
                },
                Page {
                    id: "p3".into(),
                    title: "Preview".into(),
                    route: "/preview".into(),
                    document: BuilderDocument::default(),
                },
            ],
            active_page: 0,
            navigation: NavigationConfig::default(),
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

pub struct SelectPanel(pub ActivePanel);

impl Action<AppState> for SelectPanel {
    fn apply(self, state: &mut AppState) {
        state.active_panel = self.0;
    }
}

// ── Undo snapshots ─────────────────────────────────────────────────

#[derive(Debug, Clone)]
struct DocumentSnapshot {
    description: String,
    document: BuilderDocument,
    selection: SelectionModel,
}

// ── Shell inner state (shared with callbacks) ──────────────────────

struct ShellInner {
    store: Store<AppState>,
    registry: Arc<ComponentRegistry>,
    help: HelpRegistry,
    input: InputManager,
    commands: CommandRegistry,
    menus: crate::menu::MenuRegistry,
    undo_past: Vec<DocumentSnapshot>,
    undo_future: Vec<DocumentSnapshot>,
    clipboard: Option<Node>,
    help_pending_id: String,
    help_active_id: String,
}

impl ShellInner {
    fn push_undo(&mut self, description: &str) {
        let state = self.store.state();
        self.undo_past.push(DocumentSnapshot {
            description: description.into(),
            document: state.builder_document.clone(),
            selection: state.selection.clone(),
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
        let state = self.store.state();
        self.undo_future.push(DocumentSnapshot {
            description: snapshot.description.clone(),
            document: state.builder_document.clone(),
            selection: state.selection.clone(),
        });
        let doc = snapshot.document;
        let sel = snapshot.selection;
        self.store.mutate(|state| {
            state.builder_document = doc;
            state.selection = sel;
        });
    }

    fn perform_redo(&mut self) {
        let Some(snapshot) = self.undo_future.pop() else {
            return;
        };
        let state = self.store.state();
        self.undo_past.push(DocumentSnapshot {
            description: snapshot.description.clone(),
            document: state.builder_document.clone(),
            selection: state.selection.clone(),
        });
        let doc = snapshot.document;
        let sel = snapshot.selection;
        self.store.mutate(|state| {
            state.builder_document = doc;
            state.selection = sel;
        });
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
            let state = s.store.state();
            let panel = state.active_panel;
            let has_sel = !state.selection.is_empty();
            update_panel_schemes(&mut s.input, panel.as_id());
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

    pub fn select_panel(&self, panel: ActivePanel) {
        self.inner.borrow_mut().store.mutate(|state| {
            state.active_panel = panel;
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
                    let mut s = inner.borrow_mut();
                    s.store.mutate(|state| {
                        state.active_panel = ActivePanel::from_id(id);
                    });
                    update_panel_schemes(&mut s.input, id);
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
                    s.store.mutate(|state| {
                        if let Some(ref mut root) = state.builder_document.root {
                            let node = root.find(&node_id);
                            let key = match node.map(|n| n.component.as_str()) {
                                Some("text") => "body",
                                Some("card") | Some("accordion") => "title",
                                Some("code") => "code",
                                _ => "text",
                            };
                            mutate_node_prop(root, &node_id, key, &value, Some("text"));
                        }
                    });
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
                    s.store.mutate(|state| {
                        delete_node(&mut state.builder_document, &nid);
                        state.selection.clear();
                    });
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
                    let new_node = Node {
                        id: node_id.clone(),
                        component: ct,
                        props,
                        children: vec![],
                        ..Default::default()
                    };
                    let parent_id = s.store.state().selection.primary().cloned();
                    s.store.mutate(|state| {
                        state.next_node_id += 1;
                        add_node_to_document(
                            &mut state.builder_document,
                            parent_id.as_deref(),
                            new_node,
                        );
                        state.selection.select(node_id);
                    });
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
                        s.push_undo(&format!("Edit {key}"));
                        s.store.mutate(|state| {
                            if let Some(ref mut root) = state.builder_document.root {
                                mutate_node_prop(root, target_id, &key, &value, kind.as_deref());
                            }
                        });
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
                        let value = match kind.as_deref() {
                            Some("integer") => format!("{}", val as i64),
                            _ => format_slider_value(val),
                        };
                        s.push_undo(&format!("Edit {key}"));
                        s.store.mutate(|state| {
                            if let Some(ref mut root) = state.builder_document.root {
                                mutate_node_prop(root, target_id, &key, &value, kind.as_deref());
                            }
                        });
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
                s.store.mutate(|state| {
                    move_node_in_siblings(&mut state.builder_document, &nid, direction);
                });
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
                inner.borrow_mut().store.mutate(|state| {
                    state.shell_view = ShellView::App {
                        app_id: app_id.clone(),
                    };
                    state.active_panel = ActivePanel::Edit;
                    state.selection.clear();
                    state.sync_document_from_app();
                });
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
                inner.borrow_mut().store.mutate(|state| {
                    state.sync_document_to_app();
                    state.shell_view = ShellView::Launchpad;
                    state.selection.clear();
                });
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
                        s.store.mutate(|state| {
                            state.sync_document_to_app();
                            state.shell_view = ShellView::App {
                                app_id: app_id.into(),
                            };
                            state.active_panel = ActivePanel::Explorer;
                            state.selection.clear();
                            state.sync_document_from_app();
                        });
                    } else if let Some(rest) = nid.strip_prefix("page:") {
                        let parts: Vec<&str> = rest.splitn(2, ':').collect();
                        if parts.len() == 2 {
                            let aid = parts[0].to_string();
                            let pid = parts[1].to_string();
                            s.store.mutate(|state| {
                                state.sync_document_to_app();
                                state.shell_view = ShellView::App { app_id: aid };
                                if let Some(app) = state.active_app_mut() {
                                    if let Some(idx) = app.pages.iter().position(|p| p.id == pid) {
                                        app.active_page = idx;
                                    }
                                }
                                state.selection.clear();
                                state.sync_document_from_app();
                            });
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
                                document: BuilderDocument::default(),
                            }],
                            active_page: 0,
                            navigation: NavigationConfig::default(),
                        });
                        state.shell_view = ShellView::App { app_id: naid };
                        state.active_panel = ActivePanel::Edit;
                        state.selection.clear();
                        state.sync_document_from_app();
                    });
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
                inner.borrow_mut().store.mutate(|state| {
                    state.sync_document_to_app();
                    if let Some(app) = state.active_app_mut() {
                        app.active_page = page_index as usize;
                    }
                    state.selection.clear();
                    state.sync_document_from_app();
                });
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
                    s.store.mutate(|state| {
                        state.sync_document_to_app();
                        if let Some(app) = state.active_app_mut() {
                            let page_num = app.pages.len() + 1;
                            app.pages.push(Page {
                                id: format!("page-{page_num}"),
                                title: format!("Page {page_num}"),
                                route: format!("/page-{page_num}"),
                                document: BuilderDocument::default(),
                            });
                            app.active_page = app.pages.len() - 1;
                        }
                        state.selection.clear();
                        state.sync_document_from_app();
                    });
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

        // Code editor mouse click
        self.window.on_editor_click({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |line, col| {
                inner.borrow_mut().store.mutate(|state| {
                    state
                        .editor_state
                        .set_cursor_position(line as usize, col as usize);
                });
                if let Some(w) = weak.upgrade() {
                    sync_ui_from_shared(&inner, &w);
                }
            }
        });

        // Code editor mouse drag (selection)
        self.window.on_editor_drag({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |line, col| {
                inner.borrow_mut().store.mutate(|state| {
                    state
                        .editor_state
                        .extend_selection_to(line as usize, col as usize);
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
    }
}

// ── Sync UI ────────────────────────────────────────────────────────

fn sync_ui_from_shared(shared: &Rc<RefCell<ShellInner>>, window: &AppWindow) {
    let mut inner = shared.borrow_mut();
    inner.store.mutate(|state| {
        state.sync_document_to_app();
    });
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

    // Menu bar
    push_menu_defs(window, &inner.menus, &inner.commands);

    // Activity bar panel selection
    window.set_active_panel_id(state.active_panel.as_id());

    // Toolbar state
    window.set_has_selection(!state.selection.is_empty());
    window.set_has_clipboard(inner.clipboard.is_some());

    // Panel title + hint
    let (title, hint) = panel_metadata(state.active_panel);
    window.set_panel_title(SharedString::from(title));
    window.set_panel_hint(SharedString::from(hint));

    // Clear all panel slots
    clear_panel_slots(window);

    // Fill active panel (only when inside an app)
    if !is_launchpad {
        match state.active_panel {
            ActivePanel::Identity => push_identity_actions(window),
            ActivePanel::Edit => {
                push_builder_preview(window, &state.builder_document, &state.selection);
                push_inspector_nodes(window, &state.builder_document, &state.selection);
                push_property_rows(
                    window,
                    &state.builder_document,
                    &inner.registry,
                    &state.selection,
                );
                push_breadcrumbs(window, &state.builder_document, &state.selection);
                push_page_layout_data(window, &state.builder_document, state.show_grid_overlay);
                push_layout_rows(window, &state.builder_document, &state.selection);
                push_composition_data(
                    window,
                    &state.builder_document,
                    &inner.registry,
                    &state.selection,
                );
            }
            ActivePanel::CodeEditor => {
                push_editor_data(window, &state.editor_state);
            }
            ActivePanel::Explorer => {
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

fn push_identity_actions(window: &AppWindow) {
    let panel = IdentityPanel::new();
    let actions: Vec<ButtonSpec> = panel
        .actions()
        .iter()
        .map(|label| ButtonSpec {
            label: SharedString::from(*label),
        })
        .collect();
    let model = Rc::new(VecModel::from(actions));
    window.set_actions(ModelRc::from(model as Rc<dyn Model<Data = ButtonSpec>>));
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
        "panel.identity" => {
            let mut s = shared.borrow_mut();
            s.store
                .mutate(|state| state.active_panel = ActivePanel::Identity);
            update_panel_schemes(&mut s.input, ActivePanel::Identity.as_id());
        }
        "panel.edit" | "panel.builder" | "panel.inspector" | "panel.properties" => {
            let mut s = shared.borrow_mut();
            s.store
                .mutate(|state| state.active_panel = ActivePanel::Edit);
            update_panel_schemes(&mut s.input, ActivePanel::Edit.as_id());
        }
        "panel.code_editor" => {
            let mut s = shared.borrow_mut();
            s.store
                .mutate(|state| state.active_panel = ActivePanel::CodeEditor);
            update_panel_schemes(&mut s.input, ActivePanel::CodeEditor.as_id());
        }
        "panel.explorer" | "view.file_explorer" => {
            let mut s = shared.borrow_mut();
            s.store
                .mutate(|state| state.active_panel = ActivePanel::Explorer);
            update_panel_schemes(&mut s.input, ActivePanel::Explorer.as_id());
        }
        "selection.delete" => {
            let mut s = shared.borrow_mut();
            let selected_id = s.store.state().selection.primary().cloned();
            if let Some(ref target_id) = selected_id {
                s.push_undo("Delete node");
                let tid = target_id.clone();
                s.store.mutate(|state| {
                    delete_node(&mut state.builder_document, &tid);
                    state.selection.clear();
                });
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
            let state = s.store.state();
            if let Some(target_id) = state.selection.primary() {
                if let Some(node) = state
                    .builder_document
                    .root
                    .as_ref()
                    .and_then(|n| n.find(target_id))
                {
                    let node = node.clone();
                    let comp = node.component.clone();
                    s.clipboard = Some(node);
                    s.add_toast("Copied", &format!("{comp} copied"), "info");
                }
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
                s.store.mutate(|state| {
                    state.next_node_id = next_id;
                    let parent_id = state.selection.primary().cloned();
                    add_node_to_document(
                        &mut state.builder_document,
                        parent_id.as_deref(),
                        new_node,
                    );
                    state.selection.select(new_id);
                });
            }
        }
        "edit.cut" => {
            let mut s = shared.borrow_mut();
            let target_and_node = {
                let state = s.store.state();
                state.selection.primary().and_then(|id| {
                    let node = state
                        .builder_document
                        .root
                        .as_ref()
                        .and_then(|n| n.find(id))?
                        .clone();
                    Some((id.clone(), node))
                })
            };
            if let Some((target_id, node)) = target_and_node {
                s.clipboard = Some(node);
                s.push_undo("Cut");
                s.store.mutate(|state| {
                    delete_node(&mut state.builder_document, &target_id);
                    state.selection.clear();
                });
            }
        }
        "edit.duplicate" => {
            let mut s = shared.borrow_mut();
            let target_and_node = {
                let state = s.store.state();
                state.selection.primary().and_then(|id| {
                    let node = state
                        .builder_document
                        .root
                        .as_ref()
                        .and_then(|n| n.find(id))?
                        .clone();
                    Some((id.clone(), node, state.next_node_id))
                })
            };
            if let Some((target_id, node, mut next_id)) = target_and_node {
                s.push_undo("Duplicate");
                let new_node = clone_node_with_new_ids(&node, &mut next_id);
                let new_id = new_node.id.clone();
                s.store.mutate(|state| {
                    state.next_node_id = next_id;
                    insert_after_sibling(&mut state.builder_document, &target_id, new_node);
                    state.selection.select(new_id);
                });
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
                s.store.mutate(|state| {
                    state.sync_document_to_app();
                    if let Some(app) = state.active_app_mut() {
                        app.active_page = (app.active_page + 1) % app.pages.len();
                    }
                    state.selection.clear();
                    state.sync_document_from_app();
                });
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
                s.store.mutate(|state| {
                    state.sync_document_to_app();
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
            if s.store.state().active_panel != ActivePanel::Edit {
                s.store
                    .mutate(|state| state.active_panel = ActivePanel::Edit);
                update_panel_schemes(&mut s.input, ActivePanel::Edit.as_id());
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
                    s.store.mutate(|state| {
                        state.sync_document_to_app();
                        if let Some(app) = state.active_app_mut() {
                            app.active_page = n - 1;
                        }
                        state.selection.clear();
                        state.sync_document_from_app();
                    });
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

fn panel_metadata(panel: ActivePanel) -> (&'static str, &'static str) {
    match panel {
        ActivePanel::Identity => (IdentityPanel::new().title(), IdentityPanel::new().hint()),
        ActivePanel::Edit => ("Editor", "Build your page visually."),
        ActivePanel::CodeEditor => {
            let p = CodeEditorPanel::new();
            (p.title(), p.hint())
        }
        ActivePanel::Explorer => ("Explorer", "Browse apps, pages, and documents."),
    }
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

fn mutate_node_prop(
    root: &mut Node,
    target: &str,
    key: &str,
    value: &str,
    kind: Option<&str>,
) -> bool {
    if root.id == target {
        if let Some(obj) = root.props.as_object_mut() {
            let json_value = match kind {
                Some("number") => value
                    .parse::<f64>()
                    .ok()
                    .and_then(|n| serde_json::Number::from_f64(n).map(serde_json::Value::Number))
                    .unwrap_or_else(|| serde_json::Value::String(value.to_string())),
                Some("integer") => value
                    .parse::<i64>()
                    .map(serde_json::Value::from)
                    .unwrap_or_else(|_| serde_json::Value::String(value.to_string())),
                Some("boolean") => serde_json::Value::Bool(value == "true"),
                _ => serde_json::Value::String(value.to_string()),
            };
            obj.insert(key.to_string(), json_value);
        }
        return true;
    }
    for child in &mut root.children {
        if mutate_node_prop(child, target, key, value, kind) {
            return true;
        }
    }
    false
}

fn move_node_in_siblings(doc: &mut BuilderDocument, node_id: &str, direction: i32) {
    if let Some(ref mut root) = doc.root {
        move_in_children(&mut root.children, node_id, direction);
    }
}

fn move_in_children(children: &mut [Node], target: &str, direction: i32) -> bool {
    if let Some(pos) = children.iter().position(|n| n.id == target) {
        let new_pos = pos as i32 + direction;
        if new_pos >= 0 && (new_pos as usize) < children.len() {
            children.swap(pos, new_pos as usize);
            return true;
        }
    }
    for child in children.iter_mut() {
        if move_in_children(&mut child.children, target, direction) {
            return true;
        }
    }
    false
}

fn delete_node(doc: &mut BuilderDocument, target: &str) {
    if let Some(ref mut root) = doc.root {
        if root.id == target {
            doc.root = None;
            return;
        }
        delete_from_children(&mut root.children, target);
    }
}

fn delete_from_children(children: &mut Vec<Node>, target: &str) {
    if let Some(pos) = children.iter().position(|n| n.id == target) {
        children.remove(pos);
        return;
    }
    for child in children.iter_mut() {
        delete_from_children(&mut child.children, target);
    }
}

fn add_node_to_document(doc: &mut BuilderDocument, parent_id: Option<&str>, new_node: Node) {
    match parent_id {
        Some(pid) => {
            if let Some(ref mut root) = doc.root {
                insert_child(root, pid, new_node);
            }
        }
        None => {
            if let Some(ref mut root) = doc.root {
                root.children.push(new_node);
            } else {
                doc.root = Some(new_node);
            }
        }
    }
}

fn insert_child(node: &mut Node, parent_id: &str, child: Node) -> Option<Node> {
    if node.id == parent_id {
        node.children.push(child);
        return None;
    }
    let mut remaining = child;
    for c in &mut node.children {
        match insert_child(c, parent_id, remaining) {
            None => return None,
            Some(returned) => remaining = returned,
        }
    }
    Some(remaining)
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
    }
}

fn insert_after_sibling(doc: &mut BuilderDocument, sibling_id: &str, new_node: Node) {
    if let Some(ref mut root) = doc.root {
        if let Some(node) = insert_after_in_children(&mut root.children, sibling_id, new_node) {
            root.children.push(node);
        }
    }
}

/// Try to insert `new_node` after `sibling_id` in the tree. Returns
/// `None` on success (node consumed) or `Some(node)` if not found.
fn insert_after_in_children(
    children: &mut Vec<Node>,
    sibling_id: &str,
    new_node: Node,
) -> Option<Node> {
    if let Some(pos) = children.iter().position(|n| n.id == sibling_id) {
        children.insert(pos + 1, new_node);
        return None;
    }
    let mut node = new_node;
    for child in children.iter_mut() {
        match insert_after_in_children(&mut child.children, sibling_id, node) {
            None => return None,
            Some(returned) => node = returned,
        }
    }
    Some(node)
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

fn push_page_layout_data(window: &AppWindow, doc: &BuilderDocument, show_grid: bool) {
    let pl = &doc.page_layout;
    let resolved = pl.resolved_size();
    let is_responsive = resolved.is_none();
    let (pw, ph) = resolved
        .map(|s| (s.width, s.height))
        .unwrap_or((1280.0, 800.0));

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

fn push_editor_data(window: &AppWindow, es: &EditorState) {
    use prism_core::editor::{highlight_line, TokenKind};

    let line_count = es.buffer.line_count();
    let cursor_line = es.cursor.position.line;
    let cursor_col = es.cursor.position.col;

    let (sel_start, sel_end) = es
        .selection
        .as_ref()
        .map(|s| s.ordered_positions(&es.buffer))
        .unzip();

    let mut lines: Vec<EditorLine> = Vec::with_capacity(line_count);
    for i in 0..line_count {
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

        lines.push(EditorLine {
            number: (i + 1) as i32,
            tokens: ModelRc::from(token_model as Rc<dyn Model<Data = EditorToken>>),
            is_current,
            sel_from: sf,
            sel_to: st,
        });
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
        assert!(matches!(state.active_panel, ActivePanel::Edit));
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
        store.mutate(|s| s.active_panel = ActivePanel::Identity);
        let bytes = store.snapshot().expect("snapshot");
        let mut fresh: Store<AppState> = Store::new(AppState::default());
        fresh.restore(&bytes).expect("restore");
        assert!(matches!(fresh.state().active_panel, ActivePanel::Identity));
        assert!(!fresh.state().apps.is_empty());
    }

    #[test]
    fn active_panel_roundtrips_through_id() {
        for panel in [ActivePanel::Identity, ActivePanel::Edit] {
            assert_eq!(ActivePanel::from_id(panel.as_id()), panel);
        }
    }

    #[test]
    fn unknown_panel_id_falls_back_to_edit() {
        assert_eq!(ActivePanel::from_id(999), ActivePanel::Edit);
        assert_eq!(ActivePanel::from_id(-1), ActivePanel::Edit);
    }

    #[test]
    fn select_panel_action_mutates_state() {
        let mut store: Store<AppState> = Store::new(AppState::default());
        store.dispatch(SelectPanel(ActivePanel::Identity));
        assert!(matches!(store.state().active_panel, ActivePanel::Identity));
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
    fn mutate_node_prop_updates_text_value() {
        let mut doc = sample_document();
        let root = doc.root.as_mut().unwrap();
        assert!(mutate_node_prop(
            root,
            "hero",
            "text",
            "Hello",
            Some("text")
        ));
        let hero = doc.root.as_ref().unwrap().find("hero").unwrap();
        assert_eq!(hero.props["text"], "Hello");
    }

    #[test]
    fn mutate_node_prop_updates_integer_value() {
        let mut doc = sample_document();
        let root = doc.root.as_mut().unwrap();
        assert!(mutate_node_prop(
            root,
            "hero",
            "level",
            "3",
            Some("integer")
        ));
        let hero = doc.root.as_ref().unwrap().find("hero").unwrap();
        assert_eq!(hero.props["level"], 3);
    }

    #[test]
    fn mutate_node_prop_returns_false_for_unknown_node() {
        let mut doc = sample_document();
        let root = doc.root.as_mut().unwrap();
        assert!(!mutate_node_prop(root, "nonexistent", "x", "y", None));
    }

    #[test]
    fn move_node_swaps_siblings() {
        let mut doc = sample_document();
        let children_before: Vec<String> = doc
            .root
            .as_ref()
            .unwrap()
            .children
            .iter()
            .map(|n| n.id.clone())
            .collect();
        assert_eq!(children_before, vec!["hero", "intro", "cols"]);

        move_node_in_siblings(&mut doc, "hero", 1);
        let children_after: Vec<String> = doc
            .root
            .as_ref()
            .unwrap()
            .children
            .iter()
            .map(|n| n.id.clone())
            .collect();
        assert_eq!(children_after, vec!["intro", "hero", "cols"]);
    }

    #[test]
    fn delete_node_removes_child() {
        let mut doc = sample_document();
        delete_node(&mut doc, "hero");
        assert_eq!(doc.root.as_ref().unwrap().children.len(), 2);
        assert_eq!(doc.root.as_ref().unwrap().children[0].id, "intro");
        assert_eq!(doc.root.as_ref().unwrap().children[1].id, "cols");
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
    fn add_node_to_selected_parent() {
        let mut doc = sample_document();
        let new_node = Node {
            id: "n100".into(),
            component: "heading".into(),
            props: json!({ "text": "Added", "level": 2 }),
            children: vec![],
            ..Default::default()
        };
        add_node_to_document(&mut doc, Some("root"), new_node);
        assert_eq!(doc.root.as_ref().unwrap().children.len(), 4);
        assert_eq!(doc.root.as_ref().unwrap().children[3].id, "n100");
    }

    #[test]
    fn add_node_to_empty_document() {
        let mut doc = BuilderDocument::default();
        let new_node = Node {
            id: "first".into(),
            component: "heading".into(),
            props: json!({ "text": "First" }),
            children: vec![],
            ..Default::default()
        };
        add_node_to_document(&mut doc, None, new_node);
        assert_eq!(doc.root.as_ref().unwrap().id, "first");
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

    #[test]
    fn insert_after_sibling_places_correctly() {
        let mut doc = sample_document();
        let new_node = Node {
            id: "between".into(),
            component: "divider".into(),
            props: json!({}),
            children: vec![],
            ..Default::default()
        };
        insert_after_sibling(&mut doc, "hero", new_node);
        let children = &doc.root.as_ref().unwrap().children;
        assert_eq!(children.len(), 4);
        assert_eq!(children[0].id, "hero");
        assert_eq!(children[1].id, "between");
        assert_eq!(children[2].id, "intro");
        assert_eq!(children[3].id, "cols");
    }
}
