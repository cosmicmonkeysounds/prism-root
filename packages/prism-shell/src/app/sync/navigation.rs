#![allow(unused_imports)]

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

use super::super::commands::build_context_menu_items;
use super::super::{
    panel_id_for_slint, sync_model, AppState, PersistentModels, ShellInner, ShellView,
    TransformTool,
};
use super::{
    clear_panel_slots, clone_node_with_new_ids, collect_split_bounds, default_props_for_component,
    deserialize_addr, field_kind_for_key, field_row_data_to_slint, format_slider_value,
    format_value_for_source, mime_from_extension, panel_metadata_from_workspace, parse_hex_color,
    push_dock_layout, push_user_swatches, resolve_schema_id, serialize_addr,
    slint_source_key_for_edit, sync_ui_from_shared, sync_ui_impl,
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

pub(crate) fn push_explorer_nodes(
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
    sync_model(&models.explorer_nodes, &items, |c| {
        window.set_explorer_nodes_count(c)
    });
}

pub(crate) fn push_menu_defs(
    models: &PersistentModels,
    window: &AppWindow,
    menus: &crate::menu::MenuRegistry,
    commands: &super::super::CommandRegistry,
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
    sync_model(&models.menu_defs, &defs, |c| window.set_menu_defs_count(c));
}

pub(crate) fn push_app_cards(models: &PersistentModels, window: &AppWindow, apps: &[PrismApp]) {
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
    sync_model(&models.app_cards, &items, |c| window.set_app_cards_count(c));
}

pub(crate) fn push_navigation_panel_data(
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
    sync_model(&models.nav_pages, &nav_items, |c| {
        window.set_nav_pages_count(c)
    });

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
    sync_model(&models.nav_graph_nodes, &graph_nodes, |c| {
        window.set_nav_graph_nodes_count(c)
    });

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
    sync_model(&models.nav_graph_edges, &graph_edges, |c| {
        window.set_nav_graph_edges_count(c)
    });
}

pub(crate) fn clear_href_on_node(node: &mut prism_builder::document::Node, target_id: &str) {
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

pub(crate) fn push_breadcrumbs(
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
    sync_model(&models.breadcrumbs, &items, |c| {
        window.set_breadcrumbs_count(c)
    });
}

pub(crate) fn find_path_to_node(
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

pub(crate) fn collect_node_ids(root: Option<&Node>) -> Vec<NodeId> {
    let mut ids = Vec::new();
    if let Some(node) = root {
        collect_ids_walk(node, &mut ids);
    }
    ids
}

pub(crate) fn collect_ids_walk(node: &Node, ids: &mut Vec<NodeId>) {
    ids.push(node.id.clone());
    for child in &node.children {
        collect_ids_walk(child, ids);
    }
}

// ── Page layout data ──────────────────────────────────────────────
