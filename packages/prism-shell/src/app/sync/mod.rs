use std::cell::RefCell;
use std::rc::Rc;

use prism_builder::{FacetKind, FieldKind, Node};
use prism_luau_derive::SlintBinding;
use slint::{ComponentHandle, Model, ModelRc, SharedString, TimerMode, VecModel};

/// Push-only mirror of the Slint `in` properties that drive shell chrome
/// visibility and viewport sizing. Names match the Slint properties on
/// `AppWindow` 1:1 — adding a new chrome flag is one line on each side.
#[derive(SlintBinding)]
#[slint(global = "AppWindow", push_only)]
struct ChromeBindings {
    show_activity_bar: bool,
    show_left_sidebar: bool,
    show_right_sidebar: bool,
    viewport_width: f32,
}

impl ChromeBindings {
    fn from(state: &AppState) -> Self {
        Self {
            show_activity_bar: state.show_activity_bar,
            show_left_sidebar: state.show_left_sidebar,
            show_right_sidebar: state.show_right_sidebar,
            viewport_width: state.viewport_width,
        }
    }
}

/// Workflow / app-level scalar properties: launchpad gate, active app and
/// page names, viewport preset label, panel id, drag-component type, and
/// transform tool. The `active_app_name` / `active_page_name` fields are
/// blank on the launchpad — derive that here so the call site is one bind.
#[derive(SlintBinding)]
#[slint(global = "AppWindow", push_only)]
struct WorkflowBindings {
    is_launchpad: bool,
    preview_mode: bool,
    active_app_name: SharedString,
    active_page_name: SharedString,
    viewport_preset: SharedString,
    active_panel_id: i32,
    drag_component_type: SharedString,
    transform_tool: SharedString,
}

impl WorkflowBindings {
    fn from_shell(state: &AppState, inner: &ShellInner) -> Self {
        let is_launchpad = state.shell_view.is_launchpad();
        let (app_name, page_name) = if is_launchpad {
            (SharedString::new(), SharedString::new())
        } else {
            let app = state.active_app();
            (
                SharedString::from(app.map(|a| a.name.as_str()).unwrap_or("")),
                SharedString::from(
                    app.and_then(|a| a.pages.get(a.active_page))
                        .map(|p| p.title.as_str())
                        .unwrap_or(""),
                ),
            )
        };
        let preset = match state.viewport_width as u32 {
            768 => "Tablet",
            375 => "Mobile",
            _ => "Desktop",
        };
        let tool = match state.transform_tool {
            TransformTool::Move => "move",
            TransformTool::Rotate => "rotate",
            TransformTool::Scale => "scale",
        };
        Self {
            is_launchpad,
            preview_mode: super::is_preview_mode(&state.workspace),
            active_app_name: app_name,
            active_page_name: page_name,
            viewport_preset: SharedString::from(preset),
            active_panel_id: panel_id_for_slint(&state.workspace),
            drag_component_type: SharedString::from(inner.drag_component_type.as_str()),
            transform_tool: SharedString::from(tool),
        }
    }
}

/// Project name + dirty state from the persistence and (native-only)
/// `ProjectManager` surfaces. The branching is around platform features,
/// not state shape, so the constructor handles it once.
#[derive(SlintBinding)]
#[slint(global = "AppWindow", push_only)]
struct ProjectBindings {
    project_name: SharedString,
    project_dirty: bool,
}

impl ProjectBindings {
    fn from_shell(inner: &ShellInner) -> Self {
        let mut name = inner.persistence.project_name().unwrap_or_default();
        let mut dirty = inner.persistence.is_dirty();
        #[cfg(feature = "native")]
        if let Some(ref proj) = inner.project {
            if name.is_empty() {
                name = proj
                    .root()
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
            }
            dirty = dirty || proj.is_dirty();
        }
        Self {
            project_name: SharedString::from(name),
            project_dirty: dirty,
        }
    }
}

/// Toolbar state — selection / clipboard flags + active panel title and
/// hint string derived from the dock workspace.
#[derive(SlintBinding)]
#[slint(global = "AppWindow", push_only)]
struct ToolbarBindings {
    has_selection: bool,
    has_clipboard: bool,
    panel_title: SharedString,
    panel_hint: SharedString,
}

impl ToolbarBindings {
    fn from_shell(state: &AppState, inner: &ShellInner) -> Self {
        let (title, hint) = panel_metadata_from_workspace(&state.workspace);
        Self {
            has_selection: !state.selection.is_empty(),
            has_clipboard: inner.clipboard.is_some(),
            panel_title: SharedString::from(title),
            panel_hint: SharedString::from(hint),
        }
    }
}

/// Undo/redo button state — enabled flags plus the description label of
/// the snapshot at the top of each stack.
#[derive(SlintBinding)]
#[slint(global = "AppWindow", push_only)]
struct UndoBindings {
    can_undo: bool,
    can_redo: bool,
    undo_label: SharedString,
    redo_label: SharedString,
}

impl UndoBindings {
    fn from_shell(inner: &ShellInner) -> Self {
        Self {
            can_undo: !inner.undo_past.is_empty(),
            can_redo: !inner.undo_future.is_empty(),
            undo_label: SharedString::from(
                inner
                    .undo_past
                    .last()
                    .map(|s| s.description.as_str())
                    .unwrap_or(""),
            ),
            redo_label: SharedString::from(
                inner
                    .undo_future
                    .last()
                    .map(|s| s.description.as_str())
                    .unwrap_or(""),
            ),
        }
    }
}

/// Component-picker overlay state. When `pending_picker` is `None` the
/// fields default to empty / zero / hidden so a single push covers both
/// branches of the original `if let Some(...)`.
#[derive(SlintBinding)]
#[slint(global = "AppWindow", push_only)]
struct PickerBindings {
    pending_add_path: SharedString,
    pending_add_x: f32,
    pending_add_y: f32,
    show_component_picker: bool,
}

impl PickerBindings {
    fn from_shell(inner: &ShellInner) -> Self {
        if let Some((ref path, x, y)) = inner.pending_picker {
            Self {
                pending_add_path: SharedString::from(path.as_str()),
                pending_add_x: x,
                pending_add_y: y,
                show_component_picker: true,
            }
        } else {
            Self {
                pending_add_path: SharedString::new(),
                pending_add_x: 0.0,
                pending_add_y: 0.0,
                show_component_picker: false,
            }
        }
    }
}

/// Scalar half of the context-menu overlay (visibility + position +
/// item count). The item-list `VecModel` is pushed separately because it
/// is a model, not a Slint property.
#[derive(SlintBinding)]
#[slint(global = "AppWindow", push_only)]
struct ContextMenuBindings {
    show_context_menu: bool,
    context_menu_x: f32,
    context_menu_y: f32,
    context_menu_items_count: i32,
}

use super::commands::build_context_menu_items;
use super::{
    panel_id_for_slint, sync_model, AppState, PersistentModels, ShellInner, TransformTool,
};
use crate::panels::{editor::CodeEditorPanel, Panel};
use crate::search::SearchIndex;
use crate::{
    AppWindow, ColorPreset, CommandItem, DockDividerRect, DockPanelRect, DockTabItem, FieldRow,
    MenuItem, SearchResultItem, TabItem, ToastItem, WorkflowPageItem,
};

mod editor;
mod facets;
mod grid;
mod inspector;
mod navigation;
mod preview;
mod properties;
mod schema;
mod signals;
mod widget;

pub(crate) use editor::{display_row_to_buffer_line, push_editor_data};
#[cfg(feature = "native")]
pub(crate) use facets::{build_handler_script, resolve_facet_data, resolve_widget_data};
pub(crate) use grid::{
    push_composition_counts, push_grid_cells, push_grid_edge_handles, push_page_layout_data,
};
#[cfg(test)]
pub(crate) use inspector::flatten_inspector_nodes;
pub(crate) use inspector::push_inspector_nodes;
#[cfg(test)]
pub(crate) use navigation::find_path_to_node;
pub(crate) use navigation::{
    clear_href_on_node, collect_node_ids, push_app_cards, push_breadcrumbs, push_explorer_nodes,
    push_menu_defs, push_navigation_panel_data,
};
#[cfg(test)]
pub(crate) use preview::component_palette_items;
pub(crate) use preview::{push_builder_preview, push_live_preview, push_wysiwyg_preview};
pub(crate) use properties::push_property_sections;
pub(crate) use schema::push_schema_list;
pub(crate) use signals::push_signal_panel_data;
pub(crate) use widget::push_widget_toolbar;

pub(super) fn push_user_swatches(shared: &Rc<RefCell<ShellInner>>, window: &AppWindow) {
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

pub(super) fn sync_ui_from_shared(shared: &Rc<RefCell<ShellInner>>, window: &AppWindow) {
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

pub(super) fn sync_ui_impl(inner: &ShellInner, window: &AppWindow) {
    let state = inner.store.state();

    // Launchpad vs App view
    let is_launchpad = state.shell_view.is_launchpad();
    if is_launchpad {
        push_app_cards(&inner.models, window, &state.apps);
    }

    // Workflow / app-level scalar state — launchpad gate, app/page names,
    // viewport preset, panel id, drag-component, transform tool.
    WorkflowBindings::from_shell(state, inner).bind_to(window);

    // Project name + dirty state.
    ProjectBindings::from_shell(inner).bind_to(window);

    // Shell chrome visibility + viewport — pushed through the typed
    // `ChromeBindings` mirror so a renamed AppState field that no longer
    // matches a Slint property fails to compile instead of silently
    // dropping the push.
    ChromeBindings::from(state).bind_to(window);

    // Menu bar
    push_menu_defs(&inner.models, window, &inner.menus, &inner.commands);

    // Component picker overlay
    PickerBindings::from_shell(inner).bind_to(window);

    // Context menu overlay — scalar half via the mirror struct, item
    // model still pushed manually because it's a `VecModel`.
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
        ContextMenuBindings {
            show_context_menu: true,
            context_menu_x: ctx.x,
            context_menu_y: ctx.y,
            context_menu_items_count: items.len() as i32,
        }
        .bind_to(window);
    } else {
        ContextMenuBindings {
            show_context_menu: false,
            context_menu_x: 0.0,
            context_menu_y: 0.0,
            context_menu_items_count: 0,
        }
        .bind_to(window);
    }

    // Toolbar + panel header (selection / clipboard flags + workspace title).
    ToolbarBindings::from_shell(state, inner).bind_to(window);

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
    UndoBindings::from_shell(inner).bind_to(window);

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

pub(super) fn field_row_data_to_slint(r: &crate::panels::properties::FieldRowData) -> FieldRow {
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

pub(super) fn parse_hex_color(hex: &str) -> slint::Color {
    let hex = hex.trim_start_matches('#');
    let r = u8::from_str_radix(hex.get(0..2).unwrap_or("00"), 16).unwrap_or(0);
    let g = u8::from_str_radix(hex.get(2..4).unwrap_or("00"), 16).unwrap_or(0);
    let b = u8::from_str_radix(hex.get(4..6).unwrap_or("00"), 16).unwrap_or(0);
    let a = u8::from_str_radix(hex.get(6..8).unwrap_or("ff"), 16).unwrap_or(255);
    slint::Color::from_argb_u8(a, r, g, b)
}

pub(super) fn panel_metadata_from_workspace(
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

pub(super) fn serialize_addr(addr: &prism_dock::NodeAddress) -> String {
    addr.0
        .iter()
        .map(|&b| if b { "1" } else { "0" })
        .collect::<Vec<_>>()
        .join(".")
}

pub(super) fn deserialize_addr(s: &str) -> prism_dock::NodeAddress {
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

pub(super) fn push_dock_layout(
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

pub(super) fn field_kind_for_key(inner: &ShellInner, key: &str) -> Option<String> {
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

pub(super) fn resolve_schema_id(state: &AppState) -> Option<String> {
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

pub(super) fn format_slider_value(val: f32) -> String {
    if val.fract() == 0.0 && val.is_finite() {
        format!("{}", val as i64)
    } else {
        format!("{:.2}", val)
    }
}

pub(super) fn format_value_for_source(value: &str, kind: Option<&str>) -> String {
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
pub(super) fn slint_source_key_for_edit(
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

pub(super) fn default_props_for_component(component: &str) -> serde_json::Value {
    match component {
        "text" => serde_json::json!({ "body": "New paragraph", "level": "paragraph" }),
        "image" => serde_json::json!({ "src": "", "alt": "Image", "fit": "cover" }),
        "container" => serde_json::json!({ "spacing": 12 }),
        "form" => serde_json::json!({ "method": "post" }),
        "input" => {
            serde_json::json!({ "name": "field", "type": "text", "placeholder": "Enter value" })
        }
        "button" => serde_json::json!({ "text": "Button" }),
        "card" => serde_json::json!({ "title": "Card", "body": "" }),
        "code" => serde_json::json!({ "code": "// code here", "language": "" }),
        "divider" => serde_json::json!({}),
        "spacer" => serde_json::json!({ "height": 24 }),
        "columns" => serde_json::json!({ "gap": 16 }),
        "list" => serde_json::json!({ "ordered": false }),
        "table" => serde_json::json!({ "headers": "Column 1, Column 2" }),
        "tabs" => serde_json::json!({ "labels": "Tab 1, Tab 2" }),
        "accordion" => serde_json::json!({ "title": "Section", "open": true }),
        "facet" => serde_json::json!({ "facet_id": "" }),
        _ => {
            for c in prism_builder::collect_all_contributions() {
                if c.id == component {
                    if !c.default_config.is_null() {
                        return c.default_config;
                    }
                    break;
                }
            }
            serde_json::json!({})
        }
    }
}

pub(super) fn clone_node_with_new_ids(node: &Node, counter: &mut u64) -> Node {
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
