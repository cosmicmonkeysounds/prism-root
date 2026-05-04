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

#[cfg(feature = "native")]
pub(crate) fn resolve_facet_data(doc: &mut BuilderDocument, collection: &CollectionStore) {
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
pub(crate) fn resolve_widget_data(
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
pub(crate) fn build_handler_script(page_source: &str, handler_name: &str) -> String {
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
