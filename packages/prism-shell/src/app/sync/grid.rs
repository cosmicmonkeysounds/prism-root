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


pub(crate) fn push_page_layout_data(
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

pub(crate) fn push_composition_counts(
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

pub(crate) fn push_grid_cells(
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

pub(crate) fn push_grid_edge_handles(
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
