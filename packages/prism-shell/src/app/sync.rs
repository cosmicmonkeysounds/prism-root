use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use prism_builder::app::PrismApp;
use prism_builder::layout::{GridCell, PageSize};
use prism_builder::{
    compile_slint_preview, compute_layout, preview_component_factory,
    render_document_slint_preview_with_assets_and_data, BuilderDocument, CellEdge,
    ComponentRegistry, FacetKind, FieldKind, Node, NodeId, ScriptLanguage,
};
use prism_core::design_tokens::DesignTokens;
use prism_core::editor::EditorState;
#[cfg(feature = "native")]
use prism_core::foundation::persistence::{CollectionStore, EdgeFilter, ObjectFilter};
use prism_core::foundation::vfs::VfsManager;
use slint::{ComponentHandle, Model, ModelRc, SharedString, TimerMode, VecModel};

use super::commands::build_context_menu_items;
use super::{
    panel_id_for_slint, sync_model, AppState, PersistentModels, ShellInner, ShellView,
    TransformTool,
};
use crate::panels::{editor::CodeEditorPanel, properties::PropertiesPanel, Panel};
use crate::search::SearchIndex;
use crate::selection::SelectionModel;
use crate::{
    AppCardItem, AppWindow, BreadcrumbItem, ColorPreset, CommandItem, ComponentPaletteItem,
    DockDividerRect, DockPanelRect, DockTabItem, EditorIndentGuide, EditorLine, EditorToken,
    ExplorerNodeItem, FieldRow, GridCellItem, GridEdgeHandle, InspectorNode, MenuDef, MenuItem,
    PageLayoutData, PreviewNode, SearchResultItem, TabItem, ToastItem, WidgetToolbarItem,
    WorkflowPageItem,
};

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
    window.set_is_launchpad(is_launchpad);
    window.set_preview_mode(super::is_preview_mode(&state.workspace));

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
    commands: &super::CommandRegistry,
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

// ── Helpers ────────────────────────────────────────────────────────

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

pub(super) fn clear_href_on_node(node: &mut prism_builder::document::Node, target_id: &str) {
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
pub(super) fn resolve_facet_data(doc: &mut BuilderDocument, collection: &CollectionStore) {
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
pub(super) fn resolve_widget_data(
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
            result.insert(node.id.clone(), serde_json::json!({ data_key: items }));
        }
        for child in &node.children {
            walk_nodes(child, contrib_map, collection, result);
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
pub(super) fn build_handler_script(page_source: &str, handler_name: &str) -> String {
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

pub(super) fn flatten_inspector_nodes(
    root: Option<&Node>,
    selection: &SelectionModel,
) -> Vec<InspectorNode> {
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

pub(super) fn component_palette_items(doc: &BuilderDocument) -> Vec<ComponentPaletteItem> {
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

    // Core engine widgets grouped by WidgetCategory
    let contributions = prism_builder::collect_all_contributions();
    let category_label = |cat: &prism_core::widget::WidgetCategory| -> &'static str {
        use prism_core::widget::WidgetCategory;
        match cat {
            WidgetCategory::Display => "WIDGETS: DISPLAY",
            WidgetCategory::Input => "WIDGETS: INPUT",
            WidgetCategory::Navigation => "WIDGETS: NAVIGATION",
            WidgetCategory::DataTable => "WIDGETS: DATA TABLE",
            WidgetCategory::Temporal => "WIDGETS: TEMPORAL",
            WidgetCategory::Communication => "WIDGETS: COMMUNICATION",
            WidgetCategory::Finance => "WIDGETS: FINANCE",
            WidgetCategory::Layout => "WIDGETS: LAYOUT",
            WidgetCategory::Custom => "WIDGETS: CUSTOM",
        }
    };
    let mut seen_categories = std::collections::HashSet::new();
    let mut grouped: Vec<(&str, Vec<&prism_core::widget::WidgetContribution>)> = Vec::new();
    for c in &contributions {
        let label = category_label(&c.category);
        if let Some(entry) = grouped.iter_mut().find(|(l, _)| *l == label) {
            entry.1.push(c);
        } else {
            grouped.push((label, vec![c]));
        }
    }
    for (cat_label, widgets) in &grouped {
        if seen_categories.insert(*cat_label) {
            items.push(make_header(cat_label));
        }
        for w in widgets {
            items.push(make_item(&w.id, &w.label, &w.description, cat_label));
        }
    }

    items
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

pub(super) fn find_path_to_node(
    node: &Node,
    target: &str,
    path: &mut Vec<(String, String)>,
) -> bool {
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

pub(super) fn collect_node_ids(root: Option<&Node>) -> Vec<NodeId> {
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

pub(super) fn push_editor_data(models: &PersistentModels, window: &AppWindow, es: &EditorState) {
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

pub(super) fn display_row_to_buffer_line(es: &EditorState, display_row: usize) -> usize {
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
