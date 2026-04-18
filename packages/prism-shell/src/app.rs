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
    starter::register_builtins, BuilderDocument, ComponentRegistry, FieldKind, Node, NodeId,
};
use prism_core::design_tokens::{DesignTokens, DEFAULT_TOKENS};
use prism_core::shell_mode::{Permission, ShellMode, ShellModeContext};
use prism_core::{Action, Store, Subscription};
use serde::{Deserialize, Serialize};
use serde_json::json;
use slint::{ComponentHandle, Model, ModelRc, SharedString, VecModel};

use crate::command::CommandRegistry;
use crate::input::{self, InputEvent};
use crate::keyboard::KeyboardModel;
use crate::panels::{
    builder::BuilderPanel, identity::IdentityPanel, inspector::InspectorPanel,
    properties::PropertiesPanel, Panel,
};
use crate::search::SearchIndex;
use crate::selection::SelectionModel;
use crate::telemetry::FirstPaint;
use crate::{
    AppWindow, BuilderNode, ButtonSpec, CommandItem, ComponentPaletteItem, FieldRow, InspectorNode,
    PanelNavItem, SearchResultItem, TabItem, ToastItem,
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
    pub toasts: Vec<ToastData>,
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
    Builder,
    Inspector,
    Properties,
}

impl ActivePanel {
    pub fn as_id(self) -> i32 {
        match self {
            ActivePanel::Identity => IdentityPanel::ID,
            ActivePanel::Builder => BuilderPanel::ID,
            ActivePanel::Inspector => InspectorPanel::ID,
            ActivePanel::Properties => PropertiesPanel::ID,
        }
    }

    pub fn from_id(id: i32) -> Self {
        match id {
            x if x == BuilderPanel::ID => ActivePanel::Builder,
            x if x == InspectorPanel::ID => ActivePanel::Inspector,
            x if x == PropertiesPanel::ID => ActivePanel::Properties,
            _ => ActivePanel::Identity,
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
            active_panel: ActivePanel::Identity,
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
            toasts: Vec::new(),
            next_toast_id: 0,
            next_node_id: 100,
        }
    }
}

fn sample_document() -> BuilderDocument {
    BuilderDocument {
        root: Some(Node {
            id: "root".into(),
            component: "container".into(),
            props: json!({ "spacing": 16 }),
            children: vec![
                Node {
                    id: "hero".into(),
                    component: "heading".into(),
                    props: json!({ "text": "Welcome to Prism", "level": 1 }),
                    children: vec![],
                },
                Node {
                    id: "intro".into(),
                    component: "text".into(),
                    props: json!({
                        "body": "The distributed visual operating system. Pick a panel to start editing."
                    }),
                    children: vec![],
                },
            ],
        }),
        zones: Default::default(),
    }
}

// ── Actions ────────────────────────────────────────────────────────

pub struct InputAction(pub InputEvent);

impl Action<AppState> for InputAction {
    fn apply(self, state: &mut AppState) {
        input::dispatch(state, self.0);
    }
}

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
    keyboard: KeyboardModel,
    commands: CommandRegistry,
    undo_past: Vec<DocumentSnapshot>,
    undo_future: Vec<DocumentSnapshot>,
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
        let inner = Rc::new(RefCell::new(ShellInner {
            store: Store::new(state),
            registry: Arc::new(registry),
            keyboard: KeyboardModel::with_defaults(),
            commands: CommandRegistry::with_builtins(),
            undo_past: Vec::new(),
            undo_future: Vec::new(),
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

    pub fn dispatch_input(&self, event: InputEvent) -> bool {
        let mut inner = self.inner.borrow_mut();
        let redraw = {
            let mut result = false;
            inner.store.mutate(|state| {
                result = input::dispatch(state, event);
            });
            result
        };
        drop(inner);
        sync_ui_from_shared(&self.inner, &self.window);
        redraw
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

        // Identity panel click
        self.window.on_clicked(move |index| {
            eprintln!("prism-shell: identity-panel click index={index}");
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
                            let node = find_node(Some(&*root), &node_id);
                            let key = match node.map(|n| n.component.as_str()) {
                                Some("text") => "body",
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
                    s.push_undo("Delete node");
                    let nid = node_id.to_string();
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
        self.window.on_node_move_up({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |node_id| {
                {
                    let mut s = inner.borrow_mut();
                    s.push_undo("Move node up");
                    s.store.mutate(|state| {
                        move_node_in_siblings(&mut state.builder_document, &node_id, -1);
                    });
                }
                if let Some(w) = weak.upgrade() {
                    sync_ui_from_shared(&inner, &w);
                }
            }
        });
        self.window.on_node_move_down({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |node_id| {
                {
                    let mut s = inner.borrow_mut();
                    s.push_undo("Move node down");
                    s.store.mutate(|state| {
                        move_node_in_siblings(&mut state.builder_document, &node_id, 1);
                    });
                }
                if let Some(w) = weak.upgrade() {
                    sync_ui_from_shared(&inner, &w);
                }
            }
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
                {
                    let mut s = inner.borrow_mut();
                    was_palette = s.store.state().command_palette_open;
                    if was_palette {
                        s.store.mutate(|state| {
                            state.command_palette_open = false;
                            state.command_palette_query.clear();
                        });
                        s.keyboard.set_context("commandPaletteOpen", false);
                    } else {
                        s.store.mutate(|state| {
                            state.selection.clear();
                        });
                    }
                }
                if let Some(w) = weak.upgrade() {
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
                    state.active_panel = ActivePanel::Properties;
                });
                if let Some(w) = weak.upgrade() {
                    sync_ui_from_shared(&inner, &w);
                }
            }
        });

        self.window.on_search_focus(|| {});
    }
}

// ── Sync UI ────────────────────────────────────────────────────────

fn sync_ui_from_shared(shared: &Rc<RefCell<ShellInner>>, window: &AppWindow) {
    let inner = shared.borrow();
    sync_ui_impl(&inner, window);
}

fn sync_ui_impl(inner: &ShellInner, window: &AppWindow) {
    let state = inner.store.state();

    // Sidebar nav (activity bar items)
    let nav_items = nav_model(state.active_panel);
    let nav_rc = Rc::new(VecModel::from(nav_items));
    window.set_nav_items(ModelRc::from(nav_rc as Rc<dyn Model<Data = PanelNavItem>>));

    // Panel title + hint
    let (title, hint) = panel_metadata(state.active_panel);
    window.set_panel_title(SharedString::from(title));
    window.set_panel_hint(SharedString::from(hint));

    // Clear all panel slots
    clear_panel_slots(window);

    // Fill active panel
    match state.active_panel {
        ActivePanel::Identity => push_identity_actions(window),
        ActivePanel::Builder => {
            push_builder_preview(window, &state.builder_document, &state.selection)
        }
        ActivePanel::Inspector => {
            push_inspector_nodes(window, &state.builder_document, &state.selection)
        }
        ActivePanel::Properties => push_property_rows(
            window,
            &state.builder_document,
            &inner.registry,
            &state.selection,
        ),
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
        .map(|r| FieldRow {
            key: SharedString::from(r.key),
            label: SharedString::from(r.label),
            kind: SharedString::from(r.kind),
            value: SharedString::from(r.value),
            required: r.required,
        })
        .collect();
    let model = Rc::new(VecModel::from(rows));
    window.set_field_rows(ModelRc::from(model as Rc<dyn Model<Data = FieldRow>>));
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
        "panel.builder" => {
            shared
                .borrow_mut()
                .store
                .mutate(|s| s.active_panel = ActivePanel::Builder);
        }
        "panel.inspector" => {
            shared
                .borrow_mut()
                .store
                .mutate(|s| s.active_panel = ActivePanel::Inspector);
        }
        "panel.properties" => {
            shared
                .borrow_mut()
                .store
                .mutate(|s| s.active_panel = ActivePanel::Properties);
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
        ActivePanel::Builder => (BuilderPanel::new().title(), BuilderPanel::new().hint()),
        ActivePanel::Inspector => (InspectorPanel::new().title(), InspectorPanel::new().hint()),
        ActivePanel::Properties => (
            PropertiesPanel::new().title(),
            PropertiesPanel::new().hint(),
        ),
    }
}

fn nav_model(active: ActivePanel) -> Vec<PanelNavItem> {
    [
        (IdentityPanel::ID, "Id", ActivePanel::Identity),
        (BuilderPanel::ID, "Bu", ActivePanel::Builder),
        (InspectorPanel::ID, "In", ActivePanel::Inspector),
        (PropertiesPanel::ID, "Pr", ActivePanel::Properties),
    ]
    .into_iter()
    .map(|(id, label, panel)| PanelNavItem {
        id,
        label: SharedString::from(label),
        selected: panel == active,
    })
    .collect()
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
    });
    for child in &node.children {
        flatten_builder_walk(child, depth + 1, selection, out);
    }
}

fn find_node<'a>(root: Option<&'a Node>, target: &str) -> Option<&'a Node> {
    let node = root?;
    if node.id == target {
        return Some(node);
    }
    for child in &node.children {
        if let Some(hit) = find_node(Some(child), target) {
            return Some(hit);
        }
    }
    None
}

fn field_kind_for_key(inner: &ShellInner, key: &str) -> Option<String> {
    let state = inner.store.state();
    let selected = state.selection.primary()?;
    let node = find_node(state.builder_document.root.as_ref(), selected)?;
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

fn insert_child(node: &mut Node, parent_id: &str, child: Node) -> bool {
    if node.id == parent_id {
        node.children.push(child);
        return true;
    }
    for c in &mut node.children {
        if insert_child(c, parent_id, child.clone()) {
            return true;
        }
    }
    false
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

// ── Tests ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_app_state_starts_on_identity_panel() {
        let state = AppState::default();
        assert!(matches!(state.active_panel, ActivePanel::Identity));
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
        store.mutate(|s| s.active_panel = ActivePanel::Builder);
        let bytes = store.snapshot().expect("snapshot");
        let mut fresh: Store<AppState> = Store::new(AppState::default());
        fresh.restore(&bytes).expect("restore");
        assert!(matches!(fresh.state().active_panel, ActivePanel::Builder));
        assert!(fresh.state().builder_document.root.is_some());
    }

    #[test]
    fn active_panel_roundtrips_through_id() {
        for panel in [
            ActivePanel::Identity,
            ActivePanel::Builder,
            ActivePanel::Inspector,
            ActivePanel::Properties,
        ] {
            assert_eq!(ActivePanel::from_id(panel.as_id()), panel);
        }
    }

    #[test]
    fn unknown_panel_id_falls_back_to_identity() {
        assert_eq!(ActivePanel::from_id(999), ActivePanel::Identity);
        assert_eq!(ActivePanel::from_id(-1), ActivePanel::Identity);
    }

    #[test]
    fn select_panel_action_mutates_state() {
        let mut store: Store<AppState> = Store::new(AppState::default());
        store.dispatch(SelectPanel(ActivePanel::Properties));
        assert!(matches!(
            store.state().active_panel,
            ActivePanel::Properties
        ));
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
        let hero = find_node(doc.root.as_ref(), "hero").unwrap();
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
        let hero = find_node(doc.root.as_ref(), "hero").unwrap();
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
        assert_eq!(children_before, vec!["hero", "intro"]);

        move_node_in_siblings(&mut doc, "hero", 1);
        let children_after: Vec<String> = doc
            .root
            .as_ref()
            .unwrap()
            .children
            .iter()
            .map(|n| n.id.clone())
            .collect();
        assert_eq!(children_after, vec!["intro", "hero"]);
    }

    #[test]
    fn delete_node_removes_child() {
        let mut doc = sample_document();
        delete_node(&mut doc, "hero");
        assert_eq!(doc.root.as_ref().unwrap().children.len(), 1);
        assert_eq!(doc.root.as_ref().unwrap().children[0].id, "intro");
    }

    #[test]
    fn flatten_inspector_nodes_produces_correct_depths() {
        let doc = sample_document();
        let sel = SelectionModel::single("hero".into());
        let items = flatten_inspector_nodes(doc.root.as_ref(), &sel);
        assert_eq!(items.len(), 3);
        assert_eq!(items[0].depth, 0);
        assert_eq!(items[0].id, "root");
        assert!(!items[0].selected);
        assert_eq!(items[1].depth, 1);
        assert_eq!(items[1].id, "hero");
        assert!(items[1].selected);
        assert_eq!(items[2].depth, 1);
        assert_eq!(items[2].id, "intro");
    }

    #[test]
    fn collect_node_ids_walks_full_tree() {
        let doc = sample_document();
        let ids = collect_node_ids(doc.root.as_ref());
        assert_eq!(ids, vec!["root", "hero", "intro"]);
    }

    #[test]
    fn add_node_to_selected_parent() {
        let mut doc = sample_document();
        let new_node = Node {
            id: "n100".into(),
            component: "heading".into(),
            props: json!({ "text": "Added", "level": 2 }),
            children: vec![],
        };
        add_node_to_document(&mut doc, Some("root"), new_node);
        assert_eq!(doc.root.as_ref().unwrap().children.len(), 3);
        assert_eq!(doc.root.as_ref().unwrap().children[2].id, "n100");
    }

    #[test]
    fn add_node_to_empty_document() {
        let mut doc = BuilderDocument::default();
        let new_node = Node {
            id: "first".into(),
            component: "heading".into(),
            props: json!({ "text": "First" }),
            children: vec![],
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
}
