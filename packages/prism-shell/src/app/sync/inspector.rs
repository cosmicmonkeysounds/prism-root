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

pub(crate) fn push_inspector_nodes(
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

pub(crate) fn flatten_inspector_nodes(
    root: Option<&Node>,
    selection: &SelectionModel,
) -> Vec<InspectorNode> {
    let mut items = Vec::new();
    if let Some(node) = root {
        flatten_walk(node, 0, selection, &mut items);
    }
    items
}

pub(crate) fn flatten_walk(
    node: &Node,
    depth: i32,
    selection: &SelectionModel,
    out: &mut Vec<InspectorNode>,
) {
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

pub(crate) fn flatten_inspector_grid(
    doc: &BuilderDocument,
    selection: &SelectionModel,
) -> Vec<InspectorNode> {
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

pub(crate) fn inspect_grid_cell(
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
