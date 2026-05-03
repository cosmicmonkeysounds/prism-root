use std::cell::RefCell;
use std::rc::Rc;

use prism_builder::app::{AppIcon, NavigationConfig, Page, PrismApp};
use prism_builder::starter::{builtin_prefab, card_prefab_def, materialize_prefab};
use prism_builder::{
    compute_layout, path_from_string, AssetSource, BuilderDocument, CellEdge, FacetDataSource,
    FacetDef, FacetKind, FacetLayout, FacetOutput, FacetTemplate, StyleProperties,
};
use serde_json::json;
use slint::{ComponentHandle, SharedString, Timer, TimerMode};

use super::{
    apply_drag_to_node, apply_facet_edit, apply_node_layout_edit, apply_node_transform_edit,
    apply_page_layout_edit, apply_resize_to_node, apply_style_edit, clear_href_on_node,
    default_props_for_component, deserialize_addr, display_row_to_buffer_line, execute_command,
    field_kind_for_key, find_node_layout_size, find_node_transform, format_slider_value,
    is_preview_mode, mime_from_extension, open_docs_panel, panel_id_for_slint, push_user_swatches,
    resolve_schema_id, slint_source_key_for_edit, sync_ui_from_shared, ContextMenuState,
    DragSnapshot, GapResizeSnapshot, ResizeSnapshot, Shell, ShellInner, ShellView, SourceSnapshot,
};
use crate::input::{combo_from_slint, update_panel_schemes};
use crate::{AppWindow, DocsPanelData, HelpTooltipData};

/// Apply a property edit by routing on the key prefix. Returns `true`
/// if the key matched a known prefix and was handled. Returns `false`
/// for unprefixed keys, leaving caller-specific source-edit handling
/// to the call site (which differs between text and numeric inputs).
fn apply_prefixed_property_edit(
    s: &mut ShellInner,
    selected_id: &Option<prism_builder::NodeId>,
    key: &str,
    value: &str,
) -> bool {
    if key.starts_with("layout.") {
        if let Some(target_id) = selected_id {
            s.push_undo(&format!("Edit {key}"));
            let tid = target_id.clone();
            if let Some(ref mut live) = s.live {
                let _ = live.mutate_document(|doc| {
                    if let Some(ref mut root) = doc.root {
                        apply_node_layout_edit(root, &tid, key, value);
                    }
                });
            }
            s.sync_builder_document();
        }
        true
    } else if key.starts_with("transform.") {
        if let Some(target_id) = selected_id {
            s.push_undo(&format!("Edit {key}"));
            let tid = target_id.clone();
            if let Some(ref mut live) = s.live {
                let _ = live.mutate_document(|doc| {
                    if let Some(ref mut root) = doc.root {
                        apply_node_transform_edit(root, &tid, key, value);
                    }
                });
            }
            s.sync_builder_document();
        }
        true
    } else if key.starts_with("style.") || key.starts_with("inherited.style.") {
        let style_key = key
            .strip_prefix("inherited.style.")
            .or_else(|| key.strip_prefix("style."))
            .unwrap_or(key);
        let sk = style_key.to_string();
        s.push_undo(&format!("Edit style {key}"));
        if let Some(target_id) = selected_id {
            let tid = target_id.clone();
            if let Some(ref mut live) = s.live {
                let _ = live.mutate_document(|doc| {
                    if let Some(ref mut root) = doc.root {
                        if let Some(node) = root.find_mut(&tid) {
                            apply_style_edit(&mut node.style, &sk, value);
                        }
                    }
                });
            }
            s.sync_builder_document();
        } else {
            s.store.mutate(|state| {
                if let Some(app) = state.active_app_mut() {
                    if let Some(page) = app.pages.get_mut(app.active_page) {
                        apply_style_edit(&mut page.style, &sk, value);
                    }
                }
            });
        }
        true
    } else if key.starts_with("facet.") {
        if let Some(target_id) = selected_id {
            let facet_id = s
                .store
                .state()
                .builder_document
                .root
                .as_ref()
                .and_then(|r| r.find(target_id))
                .and_then(|n| n.props.get("facet_id"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            if let Some(fid) = facet_id {
                s.push_undo(&format!("Edit {key}"));
                let fkey = key.strip_prefix("facet.").unwrap_or(key).to_string();
                let val = value.to_string();
                s.store.mutate(|state| {
                    if let Some(doc) = state.active_app_mut().and_then(|a| a.active_document_mut())
                    {
                        if let Some(def) = doc.facets.get_mut(&fid) {
                            apply_facet_edit(def, &fkey, &val);
                        }
                    }
                });
                s.sync_builder_document();
            }
        }
        true
    } else if key.starts_with("schema.") {
        let skey = key.strip_prefix("schema.").unwrap_or(key).to_string();
        let val = value.to_string();
        s.push_undo(&format!("Edit {key}"));
        s.store.mutate(|state| {
            let sid = resolve_schema_id(state);
            if let Some(sid) = sid {
                if let Some(doc) = state.active_app_mut().and_then(|a| a.active_document_mut()) {
                    if let Some(schema) = doc.facet_schemas.get_mut(&sid) {
                        crate::panels::schema::apply_schema_edit(schema, &skey, &val);
                    }
                }
            }
        });
        s.sync_builder_document();
        true
    } else {
        false
    }
}

/// Borrow `inner` mutably for `body`, then upgrade `weak` and push state into
/// the Slint window. Captures the dominant callback shape: take a mutation
/// closure, sync the UI when it returns. Use directly inside Slint callbacks
/// to drop the per-site `let mut s = inner.borrow_mut();` / `if let Some(w) =
/// weak.upgrade()` boilerplate.
fn with_shell<R>(
    inner: &Rc<RefCell<ShellInner>>,
    weak: &slint::Weak<AppWindow>,
    body: impl FnOnce(&mut ShellInner) -> R,
) -> R {
    let result = {
        let mut s = inner.borrow_mut();
        body(&mut s)
    };
    if let Some(w) = weak.upgrade() {
        sync_ui_from_shared(inner, &w);
    }
    result
}

/// Shared body of `on_property_field_edited` and `on_property_field_edited_number`.
/// `numeric` is `Some(raw)` for slider/number-input edits (controls source-value
/// formatting and suppresses the `changed` signal); `None` for text edits.
fn dispatch_property_field_edit(
    inner: &Rc<RefCell<ShellInner>>,
    weak: &slint::Weak<AppWindow>,
    key: &str,
    value: &str,
    numeric: Option<f32>,
) {
    if is_preview_mode(&inner.borrow().store.state().workspace) {
        return;
    }
    if inner.borrow().syncing.get() {
        return;
    }
    {
        let mut s = inner.borrow_mut();
        let selected_id = s.store.state().selection.primary().cloned();
        if !apply_prefixed_property_edit(&mut s, &selected_id, key, value) {
            if let Some(ref target_id) = selected_id {
                let kind = field_kind_for_key(&s, key);
                let (source_key, formatted) = match numeric {
                    Some(raw) => {
                        let formatted = match kind.as_deref() {
                            Some("integer") => format!("{}", raw as i64),
                            Some("number") => format!("{}px", format_slider_value(raw)),
                            _ => format_slider_value(raw),
                        };
                        (key.to_string(), formatted)
                    }
                    None => slint_source_key_for_edit(&s, key, value, kind.as_deref()),
                };
                s.push_undo(&format!("Edit {key}"));
                if let Some(ref mut live) = s.live {
                    let _ = live.edit_prop_in_source(target_id, &source_key, &formatted);
                }
                s.sync_builder_document();
                if numeric.is_none() {
                    s.fire_signal(target_id, "changed", {
                        let mut p = serde_json::Map::new();
                        p.insert("key".into(), serde_json::Value::from(key));
                        p.insert("value".into(), serde_json::Value::from(value));
                        p
                    });
                }
            }
        }
    }
    if let Some(w) = weak.upgrade() {
        sync_ui_from_shared(inner, &w);
    }
}

impl Shell {
    pub(super) fn wire_callbacks(&self) {
        let weak = self.window.as_weak();
        let inner = Rc::clone(&self.inner);

        // Unified key dispatch — replaces hardcoded Slint FocusScope
        self.window.on_dispatch_key({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |text, ctrl, shift, alt, meta| -> bool {
                let cmd_id = {
                    let combo = combo_from_slint(&text, ctrl, shift, alt, meta);
                    match combo {
                        Some(ref c) => inner.borrow().input.dispatch(c).map(String::from),
                        None => None,
                    }
                };
                if let Some(cmd_id) = cmd_id {
                    execute_command(&inner, &weak, &cmd_id);
                    true
                } else {
                    false
                }
            }
        });

        // Menu bar command dispatch
        self.window.on_menu_command({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |cmd_id| {
                execute_command(&inner, &weak, &cmd_id);
            }
        });

        // Panel selection (sidebar + activity bar)
        self.window.on_select_panel({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |id| {
                {
                    let page_id = match id {
                        0 => "edit",
                        2 => "code",
                        3 => "edit",
                        _ => "edit",
                    };
                    let mut s = inner.borrow_mut();
                    let pid = page_id.to_string();
                    s.store.mutate(|state| {
                        state.workspace.switch_page_by_id(&pid);
                    });
                    s.dock_dirty.set(true);
                    let panel_id = panel_id_for_slint(&s.store.state().workspace);
                    update_panel_schemes(&mut s.input, panel_id);
                }
                if let Some(w) = weak.upgrade() {
                    sync_ui_from_shared(&inner, &w);
                }
            }
        });

        // Dock: workflow page clicked
        self.window.on_workflow_page_clicked({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |page_id| {
                with_shell(&inner, &weak, |s| {
                    let pid = page_id.to_string();
                    let was_preview = is_preview_mode(&s.store.state().workspace);
                    s.store.mutate(|state| {
                        state.workspace.switch_page_by_id(&pid);
                    });
                    if was_preview && !is_preview_mode(&s.store.state().workspace) {
                        s.store.mutate(|state| {
                            state.runtime_overrides.clear();
                        });
                        s.sync_builder_document();
                    }
                    s.dock_dirty.set(true);
                    let panel_id = panel_id_for_slint(&s.store.state().workspace);
                    update_panel_schemes(&mut s.input, panel_id);
                });
            }
        });

        // Dock: tab clicked within a panel group
        self.window.on_dock_tab_clicked({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |addr_key, tab_index| {
                {
                    let addr = deserialize_addr(&addr_key);
                    let mut s = inner.borrow_mut();
                    s.store.mutate(|state| {
                        state
                            .workspace
                            .active_dock_mut()
                            .activate_tab(&addr, tab_index as usize);
                    });
                    s.dock_dirty.set(true);
                }
                if let Some(w) = weak.upgrade() {
                    sync_ui_from_shared(&inner, &w);
                }
            }
        });

        // Dock: divider dragged (ratio update)
        self.window.on_dock_divider_dragged({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |addr_key, new_ratio| {
                {
                    let addr = deserialize_addr(&addr_key);
                    let mut s = inner.borrow_mut();
                    s.store.mutate(|state| {
                        state
                            .workspace
                            .active_dock_mut()
                            .set_ratio(&addr, new_ratio);
                    });
                    s.dock_dirty.set(true);
                }
                if let Some(w) = weak.upgrade() {
                    sync_ui_from_shared(&inner, &w);
                }
            }
        });

        // Builder node clicked — select in edit mode, fire signal in preview mode
        self.window.on_builder_node_clicked({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |node_id| {
                eprintln!("[click] builder_node_clicked id={node_id}");
                with_shell(&inner, &weak, |s| {
                    let nid = node_id.to_string();
                    s.store.mutate(|state| {
                        state.selection.select(nid.clone());
                    });
                    if is_preview_mode(&s.store.state().workspace) {
                        s.fire_signal(&nid, "clicked", serde_json::Map::new());
                    } else if let Some(ref live) = s.live {
                        if let Some(sel) = live.select_node(&nid) {
                            s.store.mutate(|state| {
                                state
                                    .editor_state
                                    .set_cursor_position(sel.start_line, sel.start_col);
                                state
                                    .editor_state
                                    .extend_selection_to(sel.end_line, sel.end_col);
                            });
                        }
                    }
                });
            }
        });

        // Builder node double-clicked — fire double-clicked signal in preview mode
        self.window.on_builder_node_double_clicked({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |node_id| {
                {
                    let nid = node_id.to_string();
                    let mut s = inner.borrow_mut();
                    if is_preview_mode(&s.store.state().workspace) {
                        s.fire_signal(&nid, "double-clicked", serde_json::Map::new());
                    }
                }
                if let Some(w) = weak.upgrade() {
                    sync_ui_from_shared(&inner, &w);
                }
            }
        });

        // Builder node hovered — fire hovered signal (preview mode only)
        self.window.on_builder_node_hovered({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |node_id, x, y| {
                let dispatched = {
                    let nid = node_id.to_string();
                    let mut s = inner.borrow_mut();
                    s.fire_signal(&nid, "hovered", {
                        let mut p = serde_json::Map::new();
                        p.insert("x".into(), serde_json::Value::from(x as f64));
                        p.insert("y".into(), serde_json::Value::from(y as f64));
                        p
                    })
                };
                if dispatched {
                    if let Some(w) = weak.upgrade() {
                        sync_ui_from_shared(&inner, &w);
                    }
                }
            }
        });

        self.window.on_builder_node_hover_ended({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |node_id| {
                let dispatched = {
                    let nid = node_id.to_string();
                    let mut s = inner.borrow_mut();
                    s.fire_signal(&nid, "hover-ended", serde_json::Map::new())
                };
                if dispatched {
                    if let Some(w) = weak.upgrade() {
                        sync_ui_from_shared(&inner, &w);
                    }
                }
            }
        });

        // Inline text editing in builder
        self.window.on_builder_text_edited({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |node_id, value| {
                if is_preview_mode(&inner.borrow().store.state().workspace) {
                    return;
                }
                let node_id = node_id.to_string();
                let value = value.to_string();
                with_shell(&inner, &weak, |s| {
                    s.push_undo("Edit text");
                    let key = {
                        let component = s.live.as_mut().and_then(|l| {
                            let doc = l.document();
                            doc.root
                                .as_ref()
                                .and_then(|r| r.find(&node_id).map(|n| n.component.clone()))
                        });
                        match component.as_deref() {
                            Some("text") => "body",
                            Some("card") | Some("accordion") => "title",
                            Some("code") => "code",
                            _ => "text",
                        }
                    };
                    let formatted = format!(
                        "\"{}\"",
                        prism_builder::slint_source::escape_slint_string(&value)
                    );
                    if let Some(ref mut live) = s.live {
                        let _ = live.edit_prop_in_source(&node_id, key, &formatted);
                    }
                    s.sync_builder_document();
                });
            }
        });

        // Delete node from builder
        self.window.on_builder_delete_node({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |node_id| {
                if is_preview_mode(&inner.borrow().store.state().workspace) {
                    return;
                }
                with_shell(&inner, &weak, |s| {
                    let nid = if node_id.is_empty() {
                        match s.store.state().selection.primary().cloned() {
                            Some(id) => id,
                            None => return,
                        }
                    } else {
                        node_id.to_string()
                    };
                    s.fire_signal(&nid, "deleted", serde_json::Map::new());
                    s.push_undo("Delete node");
                    if let Some(ref mut live) = s.live {
                        let _ = live.remove_node_from_source(&nid);
                    }
                    s.store.mutate(|state| {
                        state.selection.clear();
                    });
                    s.sync_builder_document();
                });
            }
        });

        // Add component from palette
        self.window.on_add_component({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |component_type| {
                if is_preview_mode(&inner.borrow().store.state().workspace) {
                    return;
                }
                let ct = component_type.to_string();
                with_shell(&inner, &weak, |s| {
                    s.push_undo(&format!("Add {ct}"));
                    let parent_id = s.store.state().selection.primary().cloned();

                    // Look up user-defined prefabs from the live document.
                    let user_prefab = s.store.state().builder_document.prefabs.get(&ct).cloned();
                    let prefab_def = builtin_prefab(&ct).or(user_prefab);

                    let mounted_id;
                    if let Some(prefab_def) = prefab_def {
                        let mut counter = s.store.state().next_node_id;
                        let tree = materialize_prefab(&prefab_def, &mut counter);
                        let root_id = tree.id.clone();
                        if let Some(ref mut live) = s.live {
                            let _ = live.insert_tree_in_source(parent_id.as_deref(), &tree, None);
                        }
                        mounted_id = root_id.clone();
                        s.store.mutate(|state| {
                            state.next_node_id = counter;
                            state.selection.select(root_id);
                        });
                    } else if ct == "facet" {
                        let counter = s.store.state().next_node_id;
                        let facet_id = format!("facet:n{counter}");
                        let node_id = format!("n{counter}");
                        let props = json!({ "facet_id": facet_id });
                        if let Some(ref mut live) = s.live {
                            let _ = live.insert_node_in_source(
                                parent_id.as_deref(),
                                &ct,
                                &node_id,
                                &props,
                                None,
                            );
                        }
                        mounted_id = node_id.clone();
                        let nid = node_id.clone();
                        s.store.mutate(|state| {
                            state.next_node_id += 1;
                            state.selection.select(nid);
                            if let Some(doc) =
                                state.active_app_mut().and_then(|a| a.active_document_mut())
                            {
                                // Ensure "card" prefab is available for facet rendering.
                                doc.prefabs
                                    .entry("card".into())
                                    .or_insert_with(card_prefab_def);
                                doc.facets.insert(
                                    facet_id.clone(),
                                    FacetDef {
                                        id: facet_id.clone(),
                                        label: "New Facet".into(),
                                        description: String::new(),
                                        kind: FacetKind::List,
                                        schema_id: None,
                                        template: FacetTemplate::default(),
                                        output: FacetOutput::default(),
                                        data: FacetDataSource::Static {
                                            items: vec![],
                                            records: vec![],
                                        },
                                        bindings: vec![],
                                        variant_rules: vec![],
                                        layout: FacetLayout::default(),
                                        resolved_data: None,
                                    },
                                );
                            }
                        });
                    } else {
                        let node_id = format!("n{}", s.store.state().next_node_id);
                        let props = default_props_for_component(&ct);
                        if let Some(ref mut live) = s.live {
                            let _ = live.insert_node_in_source(
                                parent_id.as_deref(),
                                &ct,
                                &node_id,
                                &props,
                                None,
                            );
                        }
                        mounted_id = node_id.clone();
                        let nid = node_id.clone();
                        s.store.mutate(|state| {
                            state.next_node_id += 1;
                            state.selection.select(nid);
                        });
                    }
                    s.sync_builder_document();
                    s.fire_signal(&mounted_id, "mounted", serde_json::Map::new());
                });
            }
        });

        // File browse button — opens native file dialog, imports into VFS
        self.window.on_file_browse_requested({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |key| {
                let key = key.to_string();
                let picked = {
                    #[cfg(feature = "native")]
                    {
                        rfd::FileDialog::new()
                            .add_filter(
                                "Images",
                                &["png", "jpg", "jpeg", "gif", "webp", "svg", "bmp", "ico"],
                            )
                            .add_filter("All files", &["*"])
                            .pick_file()
                    }
                    #[cfg(not(feature = "native"))]
                    {
                        None::<std::path::PathBuf>
                    }
                };
                if let Some(path) = picked {
                    let bytes = match std::fs::read(&path) {
                        Ok(b) => b,
                        Err(e) => {
                            let mut s = inner.borrow_mut();
                            s.add_toast("Import failed", &format!("{e}"), "error");
                            if let Some(w) = weak.upgrade() {
                                sync_ui_from_shared(&inner, &w);
                            }
                            return;
                        }
                    };
                    let filename = path
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_string();
                    let mime = mime_from_extension(
                        path.extension().and_then(|e| e.to_str()).unwrap_or(""),
                    );
                    let bref = {
                        let s = inner.borrow();
                        s.vfs.import_file(&bytes, &filename, mime)
                    };
                    let asset = AssetSource::Vfs {
                        hash: bref.hash,
                        filename: bref.filename,
                        mime_type: bref.mime_type,
                        size: bref.size,
                    };
                    let json_str = serde_json::to_string(&asset.to_prop()).unwrap_or_default();
                    let formatted = format!(
                        "\"{}\"",
                        prism_builder::slint_source::escape_slint_string(&json_str)
                    );
                    {
                        let mut s = inner.borrow_mut();
                        let selected_id = s.store.state().selection.primary().cloned();
                        if let Some(ref target_id) = selected_id {
                            s.push_undo(&format!("Set {key}"));
                            if let Some(ref mut live) = s.live {
                                let _ = live.edit_prop_in_source(target_id, &key, &formatted);
                            }
                            s.sync_builder_document();
                        }
                        s.add_toast("Imported", &filename, "success");
                    }
                }
                if let Some(w) = weak.upgrade() {
                    sync_ui_from_shared(&inner, &w);
                }
            }
        });

        // Unified property field editing (routes by key prefix)
        self.window.on_property_field_edited({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |key, value| {
                dispatch_property_field_edit(&inner, &weak, &key, &value, None);
            }
        });

        // Unified property numeric editing (routes by key prefix)
        self.window.on_property_field_edited_number({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |key, val| {
                let value = format_slider_value(val);
                dispatch_property_field_edit(&inner, &weak, &key, &value, Some(val));
            }
        });

        // Section toggle (manual override of auto-collapse)
        self.window.on_property_section_toggled({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |section_id| {
                let section_id = section_id.to_string();
                with_shell(&inner, &weak, |s| {
                    if s.toggled_sections.contains(&section_id) {
                        s.toggled_sections.remove(&section_id);
                    } else {
                        s.toggled_sections.insert(section_id);
                    }
                });
            }
        });

        // Inspector node selection
        self.window.on_node_clicked({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |node_id| {
                inner.borrow_mut().store.mutate(|state| {
                    state.selection.select(node_id.to_string());
                });
                if let Some(w) = weak.upgrade() {
                    sync_ui_from_shared(&inner, &w);
                }
            }
        });

        // Node reordering
        fn handle_node_move(
            inner: &Rc<RefCell<ShellInner>>,
            weak: &slint::Weak<AppWindow>,
            node_id: &slint::SharedString,
            direction: i32,
            label: &str,
        ) {
            if is_preview_mode(&inner.borrow().store.state().workspace) {
                return;
            }
            {
                let mut s = inner.borrow_mut();
                let nid = if node_id.is_empty() {
                    match s.store.state().selection.primary().cloned() {
                        Some(id) => id,
                        None => return,
                    }
                } else {
                    node_id.to_string()
                };
                s.push_undo(label);
                if let Some(ref mut live) = s.live {
                    let _ = live.move_node_in_source(&nid, direction);
                }
                s.sync_builder_document();
            }
            if let Some(w) = weak.upgrade() {
                sync_ui_from_shared(inner, &w);
            }
        }
        self.window.on_node_move_up({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |node_id| handle_node_move(&inner, &weak, &node_id, -1, "Move node up")
        });
        self.window.on_node_move_down({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |node_id| handle_node_move(&inner, &weak, &node_id, 1, "Move node down")
        });

        // App navigation: open app from launchpad
        self.window.on_open_app({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |app_id| {
                let app_id = app_id.to_string();
                with_shell(&inner, &weak, |s| {
                    s.store.mutate(|state| {
                        state.shell_view = ShellView::App {
                            app_id: app_id.clone(),
                        };
                        state.workspace.switch_page_by_id("edit");
                        state.selection.clear();
                        state.sync_document_from_app();
                    });
                    s.load_active_page();
                    s.dock_dirty.set(true);
                });
            }
        });

        // App navigation: go back to launchpad
        self.window.on_go_home({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move || {
                with_shell(&inner, &weak, |s| {
                    s.save_to_active_page();
                    s.store.mutate(|state| {
                        state.shell_view = ShellView::Launchpad;
                        state.selection.clear();
                    });
                    s.live = None;
                    s.dock_dirty.set(true);
                });
            }
        });

        // Explorer: node clicked — navigate to app/page
        self.window.on_explorer_node_clicked({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |node_id| {
                let nid = node_id.to_string();
                with_shell(&inner, &weak, |s| {
                    if let Some(app_id) = nid.strip_prefix("app:") {
                        s.save_to_active_page();
                        s.store.mutate(|state| {
                            state.shell_view = ShellView::App {
                                app_id: app_id.into(),
                            };
                            state.workspace.switch_page_by_id("edit");
                            state.selection.clear();
                            state.sync_document_from_app();
                        });
                        s.load_active_page();
                        s.dock_dirty.set(true);
                    } else if let Some(rest) = nid.strip_prefix("page:") {
                        let parts: Vec<&str> = rest.splitn(2, ':').collect();
                        if parts.len() == 2 {
                            let aid = parts[0].to_string();
                            let pid = parts[1].to_string();
                            s.save_to_active_page();
                            s.store.mutate(|state| {
                                state.shell_view = ShellView::App { app_id: aid };
                                if let Some(app) = state.active_app_mut() {
                                    if let Some(idx) = app.pages.iter().position(|p| p.id == pid) {
                                        app.active_page = idx;
                                    }
                                }
                                state.selection.clear();
                                state.sync_document_from_app();
                            });
                            s.load_active_page();
                            s.dock_dirty.set(true);
                        }
                    }
                });
            }
        });

        // Explorer: toggle expand/collapse
        self.window.on_explorer_toggle_expand({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |node_id| {
                let nid = node_id.to_string();
                inner.borrow_mut().store.mutate(|state| {
                    if state.explorer_expanded.contains(&nid) {
                        state.explorer_expanded.remove(&nid);
                    } else {
                        state.explorer_expanded.insert(nid);
                    }
                });
                if let Some(w) = weak.upgrade() {
                    sync_ui_from_shared(&inner, &w);
                }
            }
        });

        // App navigation: create new app
        self.window.on_create_app({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move || {
                with_shell(&inner, &weak, |s| {
                    s.save_to_active_page();
                    let id_num = s.store.state().next_app_id;
                    let naid = format!("app-{id_num}");
                    s.store.mutate(|state| {
                        state.next_app_id += 1;
                        state.apps.push(PrismApp {
                            id: naid.clone(),
                            name: format!("App {id_num}"),
                            description: "A new Prism app.".into(),
                            icon: AppIcon::Cube,
                            pages: vec![Page {
                                id: "page-1".into(),
                                title: "Home".into(),
                                route: "/".into(),
                                source: String::new(),
                                document: BuilderDocument::page_shell(),
                                style: StyleProperties::default(),
                            }],
                            active_page: 0,
                            navigation: NavigationConfig::default(),
                            style: StyleProperties::default(),
                        });
                        state.shell_view = ShellView::App { app_id: naid };
                        state.workspace.switch_page_by_id("edit");
                        state.selection.clear();
                        state.sync_document_from_app();
                    });
                    s.load_active_page();
                    s.dock_dirty.set(true);
                    s.add_toast(
                        "App created",
                        &format!("App {id_num} is ready to edit"),
                        "success",
                    );
                });
            }
        });

        // Page navigation: switch page within an app
        self.window.on_switch_page({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |page_index| {
                inner.borrow_mut().switch_to_page(page_index as usize);
                if let Some(w) = weak.upgrade() {
                    sync_ui_from_shared(&inner, &w);
                }
            }
        });

        // Page navigation: add new page to active app
        self.window.on_add_page({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move || {
                execute_command(&inner, &weak, "add_page");
            }
        });

        // Command palette
        self.window.on_toggle_command_palette({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move || {
                execute_command(&inner, &weak, "command_palette.toggle");
            }
        });
        self.window.on_command_palette_input({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |query| {
                inner.borrow_mut().store.mutate(|state| {
                    state.command_palette_query = query.to_string();
                });
                if let Some(w) = weak.upgrade() {
                    sync_ui_from_shared(&inner, &w);
                }
            }
        });
        self.window.on_command_selected({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |index| {
                let cmd_id = {
                    let s = inner.borrow();
                    let query = &s.store.state().command_palette_query;
                    let results = s.commands.filter(query);
                    results.get(index as usize).map(|c| c.id.clone())
                };
                if let Some(cmd_id) = cmd_id {
                    execute_command(&inner, &weak, &cmd_id);
                }
            }
        });

        // Notifications
        self.window.on_dismiss_notification({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |toast_id| {
                inner.borrow_mut().store.mutate(|state| {
                    state.toasts.retain(|t| t.id != toast_id as u64);
                });
                if let Some(w) = weak.upgrade() {
                    sync_ui_from_shared(&inner, &w);
                }
            }
        });

        // Undo / redo
        self.window.on_undo_clicked({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move || {
                if is_preview_mode(&inner.borrow().store.state().workspace) {
                    return;
                }
                inner.borrow_mut().perform_undo();
                if let Some(w) = weak.upgrade() {
                    sync_ui_from_shared(&inner, &w);
                }
            }
        });
        self.window.on_redo_clicked({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move || {
                if is_preview_mode(&inner.borrow().store.state().workspace) {
                    return;
                }
                inner.borrow_mut().perform_redo();
                if let Some(w) = weak.upgrade() {
                    sync_ui_from_shared(&inner, &w);
                }
            }
        });

        // Escape (from editor FocusScope and other direct callers)
        self.window.on_escape_pressed({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move || {
                let cmd = if inner.borrow().store.state().command_palette_open {
                    "command_palette.close"
                } else {
                    "navigate.escape"
                };
                execute_command(&inner, &weak, cmd);
            }
        });

        // Search
        self.window.on_search_input({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |query| {
                inner.borrow_mut().store.mutate(|state| {
                    state.search_query = query.to_string();
                });
                if let Some(w) = weak.upgrade() {
                    sync_ui_from_shared(&inner, &w);
                }
            }
        });
        self.window.on_search_result_clicked({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |node_id| {
                inner.borrow_mut().store.mutate(|state| {
                    state.selection.select(node_id.to_string());
                });
                if let Some(w) = weak.upgrade() {
                    sync_ui_from_shared(&inner, &w);
                }
            }
        });

        self.window.on_search_focus({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move || {
                execute_command(&inner, &weak, "search.focus");
            }
        });

        // Help tooltip hover
        let show_timer = Rc::new(Timer::default());
        let hide_timer = Rc::new(Timer::default());

        self.window.on_help_hover({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            let show_timer = Rc::clone(&show_timer);
            let hide_timer = Rc::clone(&hide_timer);
            move |help_id, x, y| {
                let help_id = help_id.to_string();
                hide_timer.stop();

                if help_id == "__tooltip__" {
                    return;
                }

                let active_id = inner.borrow().help_active_id.clone();
                if help_id == active_id {
                    return;
                }

                inner.borrow_mut().help_pending_id = help_id.clone();

                let inner_show = Rc::clone(&inner);
                let weak_show = weak.clone();
                let hide_timer_show = Rc::clone(&hide_timer);
                let x_val = x;
                let y_val = y;
                show_timer.start(
                    TimerMode::SingleShot,
                    std::time::Duration::from_millis(380),
                    move || {
                        let pending = inner_show.borrow().help_pending_id.clone();
                        if pending.is_empty() {
                            return;
                        }
                        let (title, summary, has_docs) = {
                            let s = inner_show.borrow();
                            match s.help.get(&pending) {
                                Some(entry) => (
                                    entry.title.clone(),
                                    entry.summary.clone(),
                                    entry.doc_path.is_some(),
                                ),
                                None => return,
                            }
                        };
                        inner_show.borrow_mut().help_active_id = pending;
                        if let Some(w) = weak_show.upgrade() {
                            w.set_help_tooltip(HelpTooltipData {
                                visible: true,
                                title: SharedString::from(title),
                                summary: SharedString::from(summary),
                                has_docs,
                                tip_x: x_val,
                                tip_y: y_val,
                            });
                        }

                        // Auto-hide after 8 seconds of inactivity
                        let inner_autohide = Rc::clone(&inner_show);
                        let weak_autohide = weak_show.clone();
                        hide_timer_show.start(
                            TimerMode::SingleShot,
                            std::time::Duration::from_secs(8),
                            move || {
                                inner_autohide.borrow_mut().help_active_id.clear();
                                inner_autohide.borrow_mut().help_pending_id.clear();
                                if let Some(w) = weak_autohide.upgrade() {
                                    w.set_help_tooltip(HelpTooltipData {
                                        visible: false,
                                        title: SharedString::new(),
                                        summary: SharedString::new(),
                                        has_docs: false,
                                        tip_x: 0.0,
                                        tip_y: 0.0,
                                    });
                                }
                            },
                        );
                    },
                );
            }
        });

        self.window.on_help_leave({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            let show_timer = Rc::clone(&show_timer);
            let hide_timer = Rc::clone(&hide_timer);
            move || {
                show_timer.stop();
                inner.borrow_mut().help_pending_id.clear();

                let inner_hide = Rc::clone(&inner);
                let weak_hide = weak.clone();
                hide_timer.start(
                    TimerMode::SingleShot,
                    std::time::Duration::from_millis(120),
                    move || {
                        inner_hide.borrow_mut().help_active_id.clear();
                        if let Some(w) = weak_hide.upgrade() {
                            w.set_help_tooltip(HelpTooltipData {
                                visible: false,
                                title: SharedString::new(),
                                summary: SharedString::new(),
                                has_docs: false,
                                tip_x: 0.0,
                                tip_y: 0.0,
                            });
                        }
                    },
                );
            }
        });

        self.window.on_help_docs_clicked({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move || {
                open_docs_panel(&inner, &weak);
            }
        });

        self.window.on_help_entry_clicked({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move || {
                open_docs_panel(&inner, &weak);
            }
        });

        self.window.on_docs_open_full({
            let weak = weak.clone();
            move || {
                if let Some(w) = weak.upgrade() {
                    let sidebar = w.get_docs_panel();
                    w.set_docs_view(DocsPanelData {
                        visible: true,
                        help_id: sidebar.help_id.clone(),
                        title: sidebar.title.clone(),
                        summary: sidebar.summary.clone(),
                        body: sidebar.body.clone(),
                    });
                    w.set_docs_panel(DocsPanelData {
                        visible: false,
                        help_id: SharedString::new(),
                        title: SharedString::new(),
                        summary: SharedString::new(),
                        body: SharedString::new(),
                    });
                }
            }
        });

        self.window.on_docs_panel_close({
            let weak = weak.clone();
            move || {
                if let Some(w) = weak.upgrade() {
                    w.set_docs_panel(DocsPanelData {
                        visible: false,
                        help_id: SharedString::new(),
                        title: SharedString::new(),
                        summary: SharedString::new(),
                        body: SharedString::new(),
                    });
                }
            }
        });

        self.window.on_docs_view_close({
            let weak = weak.clone();
            move || {
                if let Some(w) = weak.upgrade() {
                    w.set_docs_view(DocsPanelData {
                        visible: false,
                        help_id: SharedString::new(),
                        title: SharedString::new(),
                        summary: SharedString::new(),
                        body: SharedString::new(),
                    });
                }
            }
        });

        // Breadcrumb navigation
        self.window.on_breadcrumb_clicked({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |node_id| {
                inner.borrow_mut().store.mutate(|state| {
                    state.selection.select(node_id.to_string());
                });
                if let Some(w) = weak.upgrade() {
                    sync_ui_from_shared(&inner, &w);
                }
            }
        });

        // Clipboard: copy
        self.window.on_copy_clicked({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move || {
                execute_command(&inner, &weak, "edit.copy");
            }
        });

        // Clipboard: paste
        self.window.on_paste_clicked({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move || {
                execute_command(&inner, &weak, "edit.paste");
            }
        });

        // Clipboard: cut
        self.window.on_cut_clicked({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move || {
                execute_command(&inner, &weak, "edit.cut");
            }
        });

        // Clipboard: duplicate
        self.window.on_duplicate_clicked({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move || {
                execute_command(&inner, &weak, "edit.duplicate");
            }
        });

        // Code editor action (special keys)
        self.window.on_editor_action({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |action| {
                let action = action.to_string();
                with_shell(&inner, &weak, |s| {
                    s.store.mutate(|state| {
                        state.editor_state.handle_action(&action);
                    });
                    let text = s.store.state().editor_state.text();
                    if let Some(ref mut live) = s.live {
                        let _ = live.set_source(text);
                    }
                    s.sync_builder_document();
                });
            }
        });

        // Code editor character typed
        self.window.on_editor_char_typed({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |ch| {
                let ch = ch.to_string();
                if let Some(c) = ch.chars().next() {
                    if !c.is_control() {
                        with_shell(&inner, &weak, |s| {
                            s.store.mutate(|state| {
                                state.editor_state.insert_char(c);
                            });
                            let text = s.store.state().editor_state.text();
                            if let Some(ref mut live) = s.live {
                                let _ = live.set_source(text);
                            }
                            s.sync_builder_document();
                        });
                    }
                }
            }
        });

        // Code editor mouse click (display row -> buffer line + select builder node)
        self.window.on_editor_click({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |display_row, col| {
                with_shell(&inner, &weak, |s| {
                    let buf_line = {
                        let state = s.store.state();
                        display_row_to_buffer_line(&state.editor_state, display_row as usize)
                    };
                    let col = col as usize;
                    s.store.mutate(|state| {
                        state.editor_state.set_cursor_position(buf_line, col);
                    });
                    if let Some(ref mut live) = s.live {
                        live.editor.set_cursor_position(buf_line, col);
                        if let Some(node_id) = live.node_at_cursor() {
                            let nid = node_id.to_string();
                            s.store.mutate(|state| {
                                state.selection.select(nid);
                            });
                        }
                    }
                });
            }
        });

        // Code editor mouse drag (selection, display row -> buffer line)
        self.window.on_editor_drag({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |display_row, col| {
                inner.borrow_mut().store.mutate(|state| {
                    let buf_line =
                        display_row_to_buffer_line(&state.editor_state, display_row as usize);
                    state
                        .editor_state
                        .extend_selection_to(buf_line, col as usize);
                });
                if let Some(w) = weak.upgrade() {
                    sync_ui_from_shared(&inner, &w);
                }
            }
        });

        // Code editor fold toggle (gutter click)
        self.window.on_editor_fold_toggle({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |display_row| {
                inner.borrow_mut().store.mutate(|state| {
                    let buf_line =
                        display_row_to_buffer_line(&state.editor_state, display_row as usize);
                    state.editor_state.toggle_fold_at_line(buf_line);
                });
                if let Some(w) = weak.upgrade() {
                    sync_ui_from_shared(&inner, &w);
                }
            }
        });

        // Grid overlay toggle
        self.window.on_toggle_grid_overlay({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move || {
                if inner.borrow().syncing.get() {
                    return;
                }
                inner.borrow_mut().store.mutate(|state| {
                    state.show_grid_overlay = !state.show_grid_overlay;
                });
                if let Some(w) = weak.upgrade() {
                    sync_ui_from_shared(&inner, &w);
                }
            }
        });

        // Page layout editing
        self.window.on_page_layout_edited({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |key, value| {
                if inner.borrow().syncing.get() {
                    return;
                }
                let key = key.to_string();
                let value = value.to_string();
                with_shell(&inner, &weak, |s| {
                    s.push_undo(&format!("Edit page {key}"));
                    s.store.mutate(|state| {
                        apply_page_layout_edit(
                            &mut state.builder_document.page_layout,
                            &key,
                            &value,
                        );
                    });
                });
            }
        });

        // Node drag — snapshot initial transform on press
        self.window.on_node_drag_started({
            let inner = Rc::clone(&inner);
            move |node_id| {
                if is_preview_mode(&inner.borrow().store.state().workspace) {
                    return;
                }
                let nid = node_id.to_string();
                let mut s = inner.borrow_mut();
                if let Some(ref mut live) = s.live {
                    let pre_drag_source = live.source.clone();
                    let doc = live.document();
                    if let Some(ref root) = doc.root {
                        if let Some(t) = find_node_transform(root, &nid) {
                            s.drag_initial_transform = Some(DragSnapshot {
                                node_id: nid.clone(),
                                position: t.position,
                                rotation: t.rotation,
                                scale: t.scale,
                                pre_drag_source,
                            });
                        }
                    }
                }
                s.fire_signal(&nid, "drag-started", serde_json::Map::new());
            }
        });

        // Node drag — live update while dragging (delta from press point)
        self.window.on_node_drag_moved({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |node_id, tool, dx, dy, shift| {
                if is_preview_mode(&inner.borrow().store.state().workspace) {
                    return;
                }
                if inner.borrow().syncing.get() {
                    return;
                }
                let nid = node_id.to_string();
                let tool = tool.to_string();
                with_shell(&inner, &weak, |s| {
                    let snap = s.drag_initial_transform.clone();
                    if let (Some(ref snap), Some(ref mut live)) = (&snap, &mut s.live) {
                        if snap.node_id == nid {
                            let _ = live.mutate_document(|doc| {
                                if let Some(ref mut root) = doc.root {
                                    apply_drag_to_node(root, &tool, dx, dy, shift, snap);
                                }
                            });
                        }
                    }
                    s.sync_builder_document();
                });
            }
        });

        // Node drag — commit final transform with undo
        self.window.on_node_drag_finished({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |node_id, _tool, _dx, _dy, _shift| {
                if is_preview_mode(&inner.borrow().store.state().workspace) {
                    return;
                }
                if inner.borrow().syncing.get() {
                    return;
                }
                let nid = node_id.to_string();
                with_shell(&inner, &weak, |s| {
                    let snap = s.drag_initial_transform.take();
                    if let Some(snap) = snap {
                        if snap.node_id == nid {
                            let desc = match _tool.to_string().as_str() {
                                "rotate" => "Rotate node",
                                "scale" => "Scale node",
                                _ => "Move node",
                            };
                            let selection = s.store.state().selection.clone();
                            s.undo_past.push(SourceSnapshot {
                                description: desc.into(),
                                source: snap.pre_drag_source,
                                selection,
                            });
                            s.undo_future.clear();
                            if s.undo_past.len() > 100 {
                                s.undo_past.remove(0);
                            }
                        }
                    }
                    s.sync_builder_document();
                    s.fire_signal(&nid, "drag-ended", serde_json::Map::new());
                });
            }
        });

        // Node resize — snapshot initial position + dimensions on press
        self.window.on_node_resize_started({
            let inner = Rc::clone(&inner);
            move |node_id, _handle| {
                if is_preview_mode(&inner.borrow().store.state().workspace) {
                    return;
                }
                use prism_core::foundation::geometry::Size2;
                let nid = node_id.to_string();
                let mut s = inner.borrow_mut();
                let state = s.store.state();
                let doc = &state.builder_document;
                if let Some(ref root) = doc.root {
                    let vp = Size2::new(state.viewport_width, 800.0);
                    let layout = compute_layout(doc, vp);
                    if let Some(t) = find_node_transform(root, &nid) {
                        let (w, h) =
                            find_node_layout_size(root, &nid, &layout).unwrap_or((100.0, 100.0));
                        s.resize_initial = Some(ResizeSnapshot {
                            node_id: nid,
                            position: t.position,
                            width: w,
                            height: h,
                        });
                    }
                }
            }
        });

        // Node resize — live update while dragging
        self.window.on_node_resize_moved({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |node_id, handle, dx, dy, shift| {
                if is_preview_mode(&inner.borrow().store.state().workspace) {
                    return;
                }
                if inner.borrow().syncing.get() {
                    return;
                }
                let nid = node_id.to_string();
                let handle = handle.to_string();
                with_shell(&inner, &weak, |s| {
                    let snap = s.resize_initial.clone();
                    if let (Some(ref snap), Some(ref mut live)) = (&snap, &mut s.live) {
                        if snap.node_id == nid {
                            let _ = live.mutate_document(|doc| {
                                if let Some(ref mut root) = doc.root {
                                    apply_resize_to_node(root, &handle, dx, dy, shift, snap);
                                }
                            });
                        }
                    }
                    s.sync_builder_document();
                });
            }
        });

        // Node resize — commit final size with undo
        self.window.on_node_resize_finished({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |node_id, handle, dx, dy, shift| {
                if is_preview_mode(&inner.borrow().store.state().workspace) {
                    return;
                }
                if inner.borrow().syncing.get() {
                    return;
                }
                let nid = node_id.to_string();
                let handle = handle.to_string();
                with_shell(&inner, &weak, |s| {
                    let snap = s.resize_initial.take();
                    if let Some(snap) = snap {
                        if snap.node_id == nid {
                            s.push_undo("Resize node");
                            if let Some(ref mut live) = s.live {
                                let _ = live.mutate_document(|doc| {
                                    if let Some(ref mut root) = doc.root {
                                        apply_resize_to_node(root, &handle, dx, dy, shift, &snap);
                                    }
                                });
                            }
                        }
                    }
                    s.sync_builder_document();
                });
            }
        });

        // Grid cell clicked (select node or open palette for empty cell)
        self.window.on_grid_cell_clicked({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |path| {
                if is_preview_mode(&inner.borrow().store.state().workspace) {
                    return;
                }
                with_shell(&inner, &weak, |s| {
                    let occupant_id = {
                        let doc = &s.store.state().builder_document;
                        let cell_path = path_from_string(path.as_str());
                        doc.page_layout
                            .grid
                            .as_ref()
                            .and_then(|g| g.at(&cell_path))
                            .and_then(|c| c.node_id().map(String::from))
                    };
                    if let Some(id) = occupant_id {
                        s.store.mutate(|state| {
                            state.selection.select(id);
                        });
                    }
                });
            }
        });

        // Grid add at edge (recursive subdivision)
        self.window.on_grid_add_at_edge({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |cell_path, edge| {
                if is_preview_mode(&inner.borrow().store.state().workspace) {
                    return;
                }
                with_shell(&inner, &weak, |s| {
                    s.push_undo("Add cell");
                    let path = path_from_string(cell_path.as_str());
                    let cell_edge = match edge.as_str() {
                        "top" => CellEdge::Top,
                        "bottom" => CellEdge::Bottom,
                        "left" => CellEdge::Left,
                        "right" => CellEdge::Right,
                        _ => return,
                    };
                    s.store.mutate(|state| {
                        let _ = state
                            .builder_document
                            .page_layout
                            .insert_at_edge(&path, cell_edge);
                    });
                });
            }
        });

        // Gap resize — snapshot track sizes on press
        self.window.on_grid_gap_resize_started({
            let inner = Rc::clone(&inner);
            move |parent_path, gap_index| {
                if is_preview_mode(&inner.borrow().store.state().workspace) {
                    return;
                }
                let mut s = inner.borrow_mut();
                s.push_undo("Resize gap");
                let pp = path_from_string(parent_path.as_str());
                let gi = gap_index as usize;
                let doc = &s.store.state().builder_document;
                let resolved = doc.page_layout.resolved_size();
                let vw = s.store.state().viewport_width;
                let (pw, ph) = resolved
                    .map(|sz| (sz.width, sz.height))
                    .unwrap_or((vw, vw * 0.625));
                let content_w = pw - doc.page_layout.margins.left - doc.page_layout.margins.right;
                let content_h = ph - doc.page_layout.margins.top - doc.page_layout.margins.bottom;

                if let Some(grid) = &doc.page_layout.grid {
                    let parent = if pp.is_empty() {
                        Some(grid)
                    } else {
                        grid.at(&pp)
                    };
                    if let Some(prism_builder::GridCell::Split {
                        direction, tracks, ..
                    }) = parent
                    {
                        if gi + 1 < tracks.len() {
                            let available = match direction {
                                prism_builder::SplitDirection::Horizontal => content_w,
                                prism_builder::SplitDirection::Vertical => content_h,
                            };
                            s.gap_resize_snapshot = Some(GapResizeSnapshot {
                                parent_path: pp,
                                gap_index: gi,
                                track_a: tracks[gi],
                                track_b: tracks[gi + 1],
                                available,
                            });
                        }
                    }
                }
            }
        });

        // Gap resize — live update while dragging (cumulative delta from press)
        self.window.on_grid_gap_resize_moved({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |_parent_path, _gap_index, delta| {
                if is_preview_mode(&inner.borrow().store.state().workspace) {
                    return;
                }
                if inner.borrow().syncing.get() {
                    return;
                }
                with_shell(&inner, &weak, |s| {
                    let snap = s.gap_resize_snapshot.clone();
                    if let Some(ref snap) = snap {
                        s.store.mutate(|state| {
                            let layout = &mut state.builder_document.page_layout;
                            let grid = match layout.grid.as_mut() {
                                Some(g) => g,
                                None => return,
                            };
                            let parent = if snap.parent_path.is_empty() {
                                grid
                            } else {
                                match grid.at_mut(&snap.parent_path) {
                                    Some(p) => p,
                                    None => return,
                                }
                            };
                            if let prism_builder::GridCell::Split { tracks, .. } = parent {
                                if snap.gap_index + 1 < tracks.len() {
                                    tracks[snap.gap_index] = snap.track_a;
                                    tracks[snap.gap_index + 1] = snap.track_b;
                                }
                            }
                            let _ = layout.resize_gap(
                                &snap.parent_path,
                                snap.gap_index,
                                delta,
                                snap.available,
                            );
                        });
                    }
                });
            }
        });

        // Gap resize — finished
        self.window.on_grid_gap_resize_finished({
            let inner = Rc::clone(&inner);
            move || {
                inner.borrow_mut().gap_resize_snapshot = None;
            }
        });

        // Inspector delete-track (parses "cell:<path>" id format)
        self.window.on_inspector_delete_track({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |id| {
                if is_preview_mode(&inner.borrow().store.state().workspace) {
                    return;
                }
                let id_str = id.as_str();
                if let Some(path_str) = id_str.strip_prefix("cell:") {
                    let path = path_from_string(path_str);
                    with_shell(&inner, &weak, |s| {
                        s.push_undo("Remove cell");
                        s.store.mutate(|state| {
                            let _ = state.builder_document.page_layout.remove_cell(&path);
                        });
                    });
                }
            }
        });

        // Grid cell add component
        self.window.on_grid_cell_add_component({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |component_type, path| {
                if is_preview_mode(&inner.borrow().store.state().workspace) {
                    return;
                }
                let ct = component_type.to_string();
                let path_str = path.to_string();
                with_shell(&inner, &weak, |s| {
                    s.push_undo(&format!("Add {ct} at {path_str}"));

                    let user_prefab_grid =
                        s.store.state().builder_document.prefabs.get(&ct).cloned();
                    if let Some(prefab_def) = builtin_prefab(&ct).or(user_prefab_grid) {
                        let mut counter = s.store.state().next_node_id;
                        let tree = materialize_prefab(&prefab_def, &mut counter);
                        let root_id = tree.id.clone();
                        if let Some(ref mut live) = s.live {
                            let _ = live.insert_tree_in_source(Some("root"), &tree, None);
                        }
                        let cell_path = path_from_string(&path_str);
                        s.store.mutate(|state| {
                            state.next_node_id = counter;
                            state.selection.select(root_id.clone());
                            state
                                .builder_document
                                .page_layout
                                .place_node_at(&cell_path, root_id)
                                .ok();
                        });
                    } else {
                        let node_id = format!("n{}", s.store.state().next_node_id);
                        let props = default_props_for_component(&ct);
                        if let Some(ref mut live) = s.live {
                            let _ = live.insert_node_in_source(
                                Some("root"),
                                &ct,
                                &node_id,
                                &props,
                                None,
                            );
                        }
                        let nid = node_id.clone();
                        let cell_path = path_from_string(&path_str);
                        s.store.mutate(|state| {
                            state.next_node_id += 1;
                            state.selection.select(nid.clone());
                            state
                                .builder_document
                                .page_layout
                                .place_node_at(&cell_path, nid)
                                .ok();
                        });
                    }
                    s.sync_builder_document();
                    s.drag_component_type.clear();
                    s.pending_picker = None;
                });
            }
        });

        // Picker show — sets position + visibility via deferred sync
        self.window.on_picker_show({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |path, x, y| {
                if is_preview_mode(&inner.borrow().store.state().workspace) {
                    return;
                }
                with_shell(&inner, &weak, |s| {
                    s.pending_picker = Some((path.to_string(), x, y));
                });
            }
        });

        // Picker dismiss
        self.window.on_picker_dismiss({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move || {
                {
                    inner.borrow_mut().pending_picker = None;
                }
                if let Some(w) = weak.upgrade() {
                    sync_ui_from_shared(&inner, &w);
                }
            }
        });

        // Palette drag toggle (place mode)
        self.window.on_palette_drag_toggle({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |component_type| {
                if is_preview_mode(&inner.borrow().store.state().workspace) {
                    return;
                }
                with_shell(&inner, &weak, |s| {
                    if s.drag_component_type == component_type.as_str() {
                        s.drag_component_type.clear();
                    } else {
                        s.drag_component_type = component_type.to_string();
                    }
                });
            }
        });

        // Context menu show — select target and build context-sensitive items
        self.window.on_context_menu_show({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |target_kind, target_id, x, y| {
                with_shell(&inner, &weak, |s| {
                    let kind = target_kind.to_string();
                    let id = target_id.to_string();
                    // Select the right-clicked item so commands operate on it
                    if matches!(
                        kind.as_str(),
                        "inspector-node" | "grid-cell" | "builder-node"
                    ) {
                        let id_clone = id.clone();
                        if !s.store.state().selection.contains(&id_clone) {
                            s.store.mutate(|state| {
                                state.selection.select(id_clone);
                            });
                        }
                    }
                    s.pending_context_menu = Some(ContextMenuState {
                        target_kind: kind,
                        target_id: id,
                        x,
                        y,
                    });
                });
            }
        });

        // Context menu dismiss
        self.window.on_context_menu_dismiss({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move || {
                inner.borrow_mut().pending_context_menu = None;
                if let Some(w) = weak.upgrade() {
                    sync_ui_from_shared(&inner, &w);
                }
            }
        });

        // Context menu command — dismiss menu and execute the command
        self.window.on_context_menu_command({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |cmd_id| {
                {
                    inner.borrow_mut().pending_context_menu = None;
                }
                if let Some(w) = weak.upgrade() {
                    sync_ui_from_shared(&inner, &w);
                }
                execute_command(&inner, &weak, &cmd_id);
            }
        });

        // Save a user color swatch
        self.window.on_save_color_swatch({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |hex| {
                let hex = hex.to_string();
                if hex.is_empty() {
                    return;
                }
                {
                    let mut s = inner.borrow_mut();
                    if !s.user_color_swatches.contains(&hex) {
                        s.user_color_swatches.push(hex);
                    }
                }
                if let Some(w) = weak.upgrade() {
                    push_user_swatches(&inner, &w);
                }
            }
        });

        // Remove a user color swatch by index
        self.window.on_remove_color_swatch({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |index| {
                {
                    let mut s = inner.borrow_mut();
                    let idx = index as usize;
                    if idx < s.user_color_swatches.len() {
                        s.user_color_swatches.remove(idx);
                    }
                }
                if let Some(w) = weak.upgrade() {
                    push_user_swatches(&inner, &w);
                }
            }
        });

        // Remove a signal connection by id
        self.window.on_signal_connection_remove({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |connection_id| {
                with_shell(&inner, &weak, |s| {
                    let cid = connection_id.to_string();
                    s.push_undo("Remove signal connection");
                    s.store.mutate(|state| {
                        if let Some(doc) =
                            state.active_app_mut().and_then(|a| a.active_document_mut())
                        {
                            crate::panels::signals::SignalsPanel::remove_connection(doc, &cid);
                        }
                    });
                    s.sync_builder_document();
                });
            }
        });

        // Add a signal connection from the signals panel quick-add
        self.window.on_signal_connection_add({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |signal_name, target_idx, action_idx| {
                with_shell(&inner, &weak, |s| {
                    let source = match s.store.state().selection.primary().cloned() {
                        Some(id) => id,
                        None => return,
                    };
                    let sig = signal_name.to_string();
                    let target = if target_idx <= 0 {
                        source.clone()
                    } else {
                        let doc = &s.store.state().builder_document;
                        let targets = crate::panels::signals::SignalsPanel::available_targets(doc);
                        let ti = (target_idx - 1) as usize;
                        targets
                            .get(ti)
                            .map(|t| t.node_id.clone())
                            .unwrap_or_else(|| source.clone())
                    };
                    let has_duplicate = s
                        .store
                        .state()
                        .active_app()
                        .and_then(|a| a.active_document())
                        .map(|doc| {
                            crate::panels::signals::SignalsPanel::has_duplicate(
                                doc, &source, &sig, &target,
                            )
                        })
                        .unwrap_or(false);
                    if has_duplicate {
                        s.add_toast(
                            "Duplicate connection",
                            "A connection with the same source signal and target already exists.",
                            "warning",
                        );
                        return;
                    }
                    let action = crate::panels::signals::action_kind_from_index(action_idx, &sig);
                    let conn_id = format!("conn-{}", s.store.state().next_node_id);
                    s.push_undo("Add signal connection");
                    s.store.mutate(|state| {
                        state.next_node_id += 1;
                        if let Some(doc) =
                            state.active_app_mut().and_then(|a| a.active_document_mut())
                        {
                            let conn = crate::panels::signals::SignalsPanel::create_connection(
                                &conn_id, &source, &sig, &target, action,
                            );
                            doc.connections.push(conn);
                        }
                    });
                    s.sync_builder_document();
                });
            }
        });

        // ── Navigation panel callbacks ──────────────────────────────
        self.window.on_nav_add_page({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move || {
                execute_command(&inner, &weak, "add_page");
            }
        });
        self.window.on_nav_delete_page({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |page_index| {
                with_shell(&inner, &weak, |s| {
                    s.save_to_active_page();
                    let deleted = s.store.state().active_app().is_some_and(|app| {
                        app.pages.len() > 1 && (page_index as usize) < app.pages.len()
                    });
                    if deleted {
                        s.push_undo("Delete page");
                        s.store.mutate(|state| {
                            if let Some(app) = state.active_app_mut() {
                                crate::panels::navigation::NavigationPanel::delete_page(
                                    app,
                                    page_index as usize,
                                );
                            }
                            state.selection.clear();
                            state.sync_document_from_app();
                        });
                        s.load_active_page();
                    }
                });
            }
        });
        self.window.on_nav_rename_page({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |page_index, new_title| {
                with_shell(&inner, &weak, |s| {
                    s.push_undo("Rename page");
                    s.store.mutate(|state| {
                        if let Some(app) = state.active_app_mut() {
                            crate::panels::navigation::NavigationPanel::rename_page(
                                app,
                                page_index as usize,
                                &new_title,
                            );
                        }
                    });
                });
            }
        });
        self.window.on_nav_set_route({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |page_index, new_route| {
                with_shell(&inner, &weak, |s| {
                    s.push_undo("Set page route");
                    s.store.mutate(|state| {
                        if let Some(app) = state.active_app_mut() {
                            crate::panels::navigation::NavigationPanel::set_page_route(
                                app,
                                page_index as usize,
                                &new_route,
                            );
                        }
                    });
                });
            }
        });
        self.window.on_nav_move_page_up({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |page_index| {
                with_shell(&inner, &weak, |s| {
                    s.save_to_active_page();
                    s.push_undo("Move page up");
                    s.store.mutate(|state| {
                        if let Some(app) = state.active_app_mut() {
                            crate::panels::navigation::NavigationPanel::move_page_up(
                                app,
                                page_index as usize,
                            );
                        }
                    });
                });
            }
        });
        self.window.on_nav_move_page_down({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |page_index| {
                with_shell(&inner, &weak, |s| {
                    s.save_to_active_page();
                    s.push_undo("Move page down");
                    s.store.mutate(|state| {
                        if let Some(app) = state.active_app_mut() {
                            crate::panels::navigation::NavigationPanel::move_page_down(
                                app,
                                page_index as usize,
                            );
                        }
                    });
                });
            }
        });
        self.window.on_nav_select_page({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |page_index| {
                inner.borrow_mut().switch_to_page(page_index as usize);
                if let Some(w) = weak.upgrade() {
                    sync_ui_from_shared(&inner, &w);
                }
            }
        });
        self.window.on_nav_cycle_style({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move || {
                with_shell(&inner, &weak, |s| {
                    s.push_undo("Change navigation style");
                    s.store.mutate(|state| {
                        if let Some(app) = state.active_app_mut() {
                            use prism_builder::app::NavigationStyle;
                            let next = match app.navigation.style {
                                NavigationStyle::Tabs => NavigationStyle::Sidebar,
                                NavigationStyle::Sidebar => NavigationStyle::BottomBar,
                                NavigationStyle::BottomBar => NavigationStyle::None,
                                NavigationStyle::None => NavigationStyle::Tabs,
                            };
                            crate::panels::navigation::NavigationPanel::set_navigation_style(
                                app, next,
                            );
                        }
                    });
                });
            }
        });

        // Navigation graph: click node = select page
        self.window.on_nav_graph_node_clicked({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |page_index| {
                inner.borrow_mut().switch_to_page(page_index as usize);
                if let Some(w) = weak.upgrade() {
                    sync_ui_from_shared(&inner, &w);
                }
            }
        });

        // Navigation graph: begin link drag
        self.window.on_nav_graph_link_start({
            let inner = Rc::clone(&inner);
            move |source_idx| {
                inner.borrow_mut().nav_link_source = Some(source_idx as usize);
            }
        });

        // Navigation graph: complete link (add NavigateTo connection)
        self.window.on_nav_graph_link_end({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |target_idx| {
                let source_idx;
                {
                    let mut s = inner.borrow_mut();
                    source_idx = s.nav_link_source.take();
                }
                let Some(src) = source_idx else { return };
                let tgt = target_idx as usize;
                if src == tgt {
                    return;
                }
                with_shell(&inner, &weak, |s| {
                    let target_route = s
                        .store
                        .state()
                        .active_app()
                        .and_then(|app| app.pages.get(tgt))
                        .map(|p| p.route.clone());
                    if let Some(route) = target_route {
                        use prism_builder::signal::{ActionKind, Connection};
                        let conn_id = format!("nav-{src}-{tgt}");
                        s.push_undo("Add navigation link");
                        s.store.mutate(|state| {
                            if let Some(app) = state.active_app_mut() {
                                if let Some(page) = app.pages.get_mut(src) {
                                    let already = page.document.connections.iter().any(|c| {
                                        matches!(&c.action, ActionKind::NavigateTo { target } if target == &route)
                                    });
                                    if !already {
                                        let source_node = page
                                            .document
                                            .root
                                            .as_ref()
                                            .map(|r| r.id.clone())
                                            .unwrap_or_default();
                                        page.document.connections.push(Connection {
                                            id: conn_id,
                                            source_node,
                                            signal: "clicked".into(),
                                            target_node: String::new(),
                                            action: ActionKind::NavigateTo {
                                                target: route,
                                            },
                                            params: serde_json::Value::Null,
                                        });
                                    }
                                }
                            }
                        });
                    }
                });
            }
        });

        // Navigation graph: remove a link edge
        self.window.on_nav_graph_link_remove({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |edge_id| {
                let edge_id = edge_id.to_string();
                with_shell(&inner, &weak, |s| {
                    s.push_undo("Remove navigation link");
                    if edge_id.starts_with("href:") {
                        // href:<page_idx>:<node_id> — clear the href prop
                        let parts: Vec<&str> = edge_id.splitn(3, ':').collect();
                        if parts.len() == 3 {
                            if let Ok(page_idx) = parts[1].parse::<usize>() {
                                let node_id = parts[2].to_string();
                                s.store.mutate(|state| {
                                    if let Some(app) = state.active_app_mut() {
                                        if let Some(page) = app.pages.get_mut(page_idx) {
                                            if let Some(root) = &mut page.document.root {
                                                clear_href_on_node(root, &node_id);
                                            }
                                        }
                                    }
                                });
                            }
                        }
                    } else {
                        // Signal connection — remove by connection ID
                        s.store.mutate(|state| {
                            if let Some(app) = state.active_app_mut() {
                                for page in &mut app.pages {
                                    page.document.connections.retain(|c| c.id != edge_id);
                                }
                            }
                        });
                    }
                });
            }
        });

        // Schema designer: select a schema
        self.window.on_schema_select({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |schema_id| {
                let sid = schema_id.to_string();
                inner.borrow_mut().store.mutate(|state| {
                    state.selected_schema_id = if sid.is_empty() { None } else { Some(sid) };
                });
                if let Some(w) = weak.upgrade() {
                    sync_ui_from_shared(&inner, &w);
                }
            }
        });

        // Widget toolbar action
        self.window.on_widget_toolbar_action({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |action_id| {
                let action_id = action_id.to_string();
                with_shell(&inner, &weak, |s| {
                    let sel = s.store.state().selection.clone();
                    let Some(node_id) = sel.primary() else {
                        return;
                    };
                    let node_id = node_id.clone();
                    let comp = s
                        .store
                        .state()
                        .builder_document
                        .root
                        .as_ref()
                        .and_then(|n| n.find(&node_id))
                        .and_then(|node| s.registry.get(&node.component));
                    let Some(comp) = comp else {
                        return;
                    };
                    let actions = comp.toolbar_actions();
                    let Some(action) = actions.iter().find(|a| a.id == action_id) else {
                        return;
                    };
                    use prism_core::widget::ToolbarActionKind;
                    match &action.kind {
                        ToolbarActionKind::Signal { signal } => {
                            s.fire_signal(&node_id, signal, serde_json::Map::new());
                        }
                        ToolbarActionKind::SetConfig { key, value } => {
                            s.push_undo("Set widget config");
                            let key = key.clone();
                            let value = value.clone();
                            s.store.mutate(|state| {
                                if let Some(node) = state
                                    .builder_document
                                    .root
                                    .as_mut()
                                    .and_then(|n| n.find_mut(&node_id))
                                {
                                    if let Some(obj) = node.props.as_object_mut() {
                                        obj.insert(key, value);
                                    }
                                }
                            });
                        }
                        ToolbarActionKind::ToggleConfig { key } => {
                            s.push_undo("Toggle widget config");
                            let key = key.clone();
                            s.store.mutate(|state| {
                                if let Some(node) = state
                                    .builder_document
                                    .root
                                    .as_mut()
                                    .and_then(|n| n.find_mut(&node_id))
                                {
                                    if let Some(obj) = node.props.as_object_mut() {
                                        let current = obj
                                            .get(&key)
                                            .and_then(|v| v.as_bool())
                                            .unwrap_or(false);
                                        obj.insert(key, serde_json::Value::Bool(!current));
                                    }
                                }
                            });
                        }
                        ToolbarActionKind::Custom { action_type } => {
                            s.fire_signal(
                                &node_id,
                                &format!("custom:{action_type}"),
                                serde_json::Map::new(),
                            );
                        }
                    }
                });
            }
        });

        // Viewport preset changed
        self.window.on_viewport_preset_changed({
            let inner = Rc::clone(&inner);
            let weak = weak.clone();
            move |preset| {
                let width = match preset.as_str() {
                    "Tablet" => 768.0,
                    "Mobile" => 375.0,
                    _ => 1280.0,
                };
                with_shell(&inner, &weak, |s| {
                    s.store.mutate(|state| {
                        state.viewport_width = width;
                    });
                });
            }
        });
    }
}
