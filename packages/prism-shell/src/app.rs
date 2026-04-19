//! Root application state + Slint binding layer.
//!
//! Everything reloadable lives behind a single [`AppState`] so §7's
//! hot-reload story is exactly one serde call. Mutation goes through
//! the [`Shell`] wrapper, which owns both a
//! `prism_core::Store<AppState>` and the root `AppWindow` Slint
//! handle. Shell state that callbacks need to mutate lives behind
//! `Rc<RefCell<ShellInner>>` so Slint closures can borrow it.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use prism_builder::{
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
use crate::keyboard::KeyboardModel;
use crate::panels::{
    editor::CodeEditorPanel, identity::IdentityPanel, properties::PropertiesPanel, Panel,
};
use crate::search::SearchIndex;
use crate::selection::SelectionModel;
use crate::telemetry::FirstPaint;
use crate::{
    AppWindow, BreadcrumbItem, BuilderNode, ButtonSpec, CommandItem, ComponentPaletteItem,
    DocsPanelData, FieldRow, GutterRect, HelpTooltipData, InspectorNode, ModifierItem,
    PageLayoutData, SearchResultItem, SignalItem, TabItem, ToastItem, VariantItem,
};

// ── Reloadable state ───────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppState {
    pub tokens: DesignTokens,
    pub context: ShellModeContext,
    pub active_panel: ActivePanel,
    pub builder_document: BuilderDocument,
    pub selection: SelectionModel,
    pub tabs: Vec<Tab>,
    pub active_tab: i32,
    pub command_palette_open: bool,
    pub command_palette_query: String,
    pub search_query: String,
    pub editor_state: EditorState,
    pub toasts: Vec<ToastData>,
    pub show_grid_overlay: bool,
    next_toast_id: u64,
    next_node_id: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tab {
    pub id: i32,
    pub title: String,
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
}

impl ActivePanel {
    pub fn as_id(self) -> i32 {
        match self {
            ActivePanel::Identity => 0,
            ActivePanel::Edit => 1,
            ActivePanel::CodeEditor => 2,
        }
    }

    pub fn from_id(id: i32) -> Self {
        match id {
            0 => ActivePanel::Identity,
            2 => ActivePanel::CodeEditor,
            _ => ActivePanel::Edit,
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            tokens: DEFAULT_TOKENS,
            context: ShellModeContext {
                shell_mode: ShellMode::Build,
                permission: Permission::Dev,
            },
            active_panel: ActivePanel::Edit,
            builder_document: sample_document(),
            selection: SelectionModel::single("hero".into()),
            tabs: vec![Tab {
                id: 0,
                title: "Welcome".into(),
            }],
            active_tab: 0,
            command_palette_open: false,
            command_palette_query: String::new(),
            search_query: String::new(),
            editor_state: EditorState::with_text("// Welcome to Prism Code Editor\n// Start typing to edit\n\nfn main() {\n    println!(\"Hello, Prism!\");\n}\n"),
            toasts: Vec::new(),
            show_grid_overlay: true,
            next_toast_id: 0,
            next_node_id: 100,
        }
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
    keyboard: KeyboardModel,
    commands: CommandRegistry,
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
            keyboard: KeyboardModel::with_defaults(),
            commands: CommandRegistry::with_builtins(),
            undo_past: Vec::new(),
            undo_future: Vec::new(),
            clipboard: None,
            help_pending_id: String::new(),
            help_active_id: String::new(),
        }));
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

        // Panel selection (sidebar + activity bar)
        self.window.on_select_panel({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |id| {
                inner.borrow_mut().store.mutate(|state| {
                    state.active_panel = ActivePanel::from_id(id);
                });
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

        // Tab management
        self.window.on_tab_activated({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |tab_id| {
                inner.borrow_mut().store.mutate(|state| {
                    state.active_tab = tab_id;
                });
                if let Some(w) = weak.upgrade() {
                    sync_ui_from_shared(&inner, &w);
                }
            }
        });
        self.window.on_tab_closed({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |tab_id| {
                inner.borrow_mut().store.mutate(|state| {
                    state.tabs.retain(|t| t.id != tab_id);
                    if state.active_tab == tab_id {
                        state.active_tab = state.tabs.first().map(|t| t.id).unwrap_or(0);
                    }
                });
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
                {
                    let mut s = inner.borrow_mut();
                    let open = s.store.state().command_palette_open;
                    s.store.mutate(|state| {
                        state.command_palette_open = !open;
                        if open {
                            state.command_palette_query.clear();
                        }
                    });
                    s.keyboard.set_context("commandPaletteOpen", !open);
                }
                if let Some(w) = weak.upgrade() {
                    sync_ui_from_shared(&inner, &w);
                }
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

        // Escape
        self.window.on_escape_pressed({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move || {
                let was_palette;
                let was_tooltip;
                {
                    let mut s = inner.borrow_mut();
                    was_tooltip = !s.help_active_id.is_empty();
                    if was_tooltip {
                        s.help_active_id.clear();
                        s.help_pending_id.clear();
                    }
                    was_palette = s.store.state().command_palette_open;
                    if was_palette {
                        s.store.mutate(|state| {
                            state.command_palette_open = false;
                            state.command_palette_query.clear();
                        });
                        s.keyboard.set_context("commandPaletteOpen", false);
                    } else if !was_tooltip {
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
                    sync_ui_from_shared(&inner, &w);
                }
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

        self.window.on_search_focus(|| {});

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

        // Code editor text changed
        self.window.on_editor_text_changed({
            let inner = Rc::clone(&inner);
            move |text| {
                inner.borrow_mut().store.mutate(|state| {
                    state.editor_state.set_text(&text);
                });
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
    let inner = shared.borrow();
    sync_ui_impl(&inner, window);
}

fn sync_ui_impl(inner: &ShellInner, window: &AppWindow) {
    let state = inner.store.state();

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

    // Fill active panel
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
            push_game_engine_data(
                window,
                &state.builder_document,
                &inner.registry,
                &state.selection,
            );
        }
        ActivePanel::CodeEditor => {
            window.set_editor_text(SharedString::from(state.editor_state.text()));
        }
    }

    // Tabs
    let tab_items: Vec<TabItem> = state
        .tabs
        .iter()
        .map(|t| TabItem {
            id: t.id,
            title: SharedString::from(&t.title),
            active: t.id == state.active_tab,
        })
        .collect();
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
        "command_palette.toggle" | "command_palette.close" => {
            let mut s = shared.borrow_mut();
            let open = s.store.state().command_palette_open;
            s.store.mutate(|state| {
                state.command_palette_open = !open;
                if open {
                    state.command_palette_query.clear();
                }
            });
            s.keyboard.set_context("commandPaletteOpen", !open);
        }
        "panel.identity" => {
            shared
                .borrow_mut()
                .store
                .mutate(|s| s.active_panel = ActivePanel::Identity);
        }
        "panel.edit" | "panel.builder" | "panel.inspector" | "panel.properties" => {
            shared
                .borrow_mut()
                .store
                .mutate(|s| s.active_panel = ActivePanel::Edit);
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
        _ => {
            eprintln!("prism-shell: unknown command {command_id}");
        }
    }
    // Close palette after command execution
    {
        let mut s = shared.borrow_mut();
        if s.store.state().command_palette_open {
            s.store.mutate(|state| {
                state.command_palette_open = false;
                state.command_palette_query.clear();
            });
            s.keyboard.set_context("commandPaletteOpen", false);
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
        "layout.margin" => {
            let vals = parse_edge_values(value);
            flow.margin = vals;
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

fn push_game_engine_data(
    window: &AppWindow,
    doc: &BuilderDocument,
    registry: &prism_builder::ComponentRegistry,
    selection: &SelectionModel,
) {
    let selected = selection.as_option();

    // Modifiers on the selected node
    let modifiers: Vec<ModifierItem> = selected
        .as_ref()
        .and_then(|id| doc.root.as_ref().and_then(|r| r.find(id)))
        .map(|node| {
            node.modifiers
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

    // Signals declared by the selected component
    let signals: Vec<SignalItem> = selected
        .as_ref()
        .and_then(|id| doc.root.as_ref().and_then(|r| r.find(id)))
        .and_then(|node| registry.get(&node.component))
        .map(|comp| {
            comp.signals()
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

    // Variant axes declared by the selected component
    let variants: Vec<VariantItem> = selected
        .as_ref()
        .and_then(|id| doc.root.as_ref().and_then(|r| r.find(id)))
        .and_then(|node| {
            let comp = registry.get(&node.component)?;
            let axes = comp.variants();
            if axes.is_empty() {
                return None;
            }
            Some(
                axes.into_iter()
                    .map(|axis| {
                        let current = node
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

    // Document-level counts
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

// ── Tests ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_app_state_starts_on_edit_panel() {
        let state = AppState::default();
        assert!(matches!(state.active_panel, ActivePanel::Edit));
    }

    #[test]
    fn default_state_seeds_sample_document() {
        let state = AppState::default();
        assert!(state.builder_document.root.is_some());
        assert_eq!(state.selection.primary(), Some(&"hero".to_string()));
    }

    #[test]
    fn store_snapshot_restore_round_trips_app_state() {
        let mut store: Store<AppState> = Store::new(AppState::default());
        store.mutate(|s| s.active_panel = ActivePanel::Identity);
        let bytes = store.snapshot().expect("snapshot");
        let mut fresh: Store<AppState> = Store::new(AppState::default());
        fresh.restore(&bytes).expect("restore");
        assert!(matches!(fresh.state().active_panel, ActivePanel::Identity));
        assert!(fresh.state().builder_document.root.is_some());
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
    fn selection_model_replaces_selected_node() {
        let state = AppState::default();
        assert_eq!(state.selection.as_option(), Some("hero".to_string()));
    }

    #[test]
    fn tabs_default_to_one_welcome_tab() {
        let state = AppState::default();
        assert_eq!(state.tabs.len(), 1);
        assert_eq!(state.tabs[0].title, "Welcome");
        assert_eq!(state.active_tab, 0);
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
