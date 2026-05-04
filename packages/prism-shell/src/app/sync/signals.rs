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

pub(crate) fn push_signal_panel_data(
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
