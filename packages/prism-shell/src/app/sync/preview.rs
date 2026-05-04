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


pub(crate) fn push_builder_preview(models: &PersistentModels, window: &AppWindow, doc: &BuilderDocument) {
    let node_count = count_nodes(doc.root.as_ref());
    window.set_builder_node_count(node_count);
    let palette = component_palette_items(doc);
    let count = sync_model(&models.component_palette, &palette);
    window.set_component_palette_count(count);
}

pub(crate) fn count_nodes(root: Option<&Node>) -> i32 {
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

pub(crate) fn push_live_preview(
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

pub(crate) fn push_wysiwyg_preview(
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

pub(crate) fn materialize_vfs_assets(
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

pub(crate) fn mime_to_extension(mime: &str) -> &'static str {
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

pub(crate) fn extract_preview_text(
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

pub(crate) fn component_palette_items(doc: &BuilderDocument) -> Vec<ComponentPaletteItem> {
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
