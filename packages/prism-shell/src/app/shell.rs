#![allow(unused_imports)]

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use slint::ComponentHandle;

use super::sync::{build_handler_script, sync_ui_from_shared};
use super::*;

impl Shell {
    pub fn new() -> Result<Self, slint::PlatformError> {
        Self::from_state(AppState::default())
    }

    pub fn from_state(state: AppState) -> Result<Self, slint::PlatformError> {
        let telemetry = FirstPaint::start();
        let window = AppWindow::new()?;
        let mut registry = ComponentRegistry::new();
        register_builtins(&mut registry)
            .expect("starter components must register");
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
            legacy_luau_protocol_warned: Cell::new(false),
            #[cfg(feature = "native")]
            collection: std::rc::Rc::new(std::cell::RefCell::new(CollectionStore::new())),
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
            let collection = inner.collection.clone();
            let mut borrow = collection.borrow_mut();
            f(&mut borrow)
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
        {
            let mut col = inner.collection.borrow_mut();
            for obj in &objects {
                let _ = col.put_object(obj);
            }
        }
        let edges = proj.collection().list_edges(None);
        {
            let mut col = inner.collection.borrow_mut();
            for edge in &edges {
                let _ = col.put_edge(edge);
            }
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
