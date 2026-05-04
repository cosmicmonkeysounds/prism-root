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

#[allow(clippy::too_many_arguments)]
pub(crate) fn push_property_sections(
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
