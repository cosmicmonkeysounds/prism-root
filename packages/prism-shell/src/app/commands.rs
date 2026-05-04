use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use prism_builder::{FacetDataSource, FacetTemplate, PrefabDef};
#[cfg(feature = "native")]
use prism_core::foundation::persistence::CollectionStore;
use slint::SharedString;

use crate::input::{update_panel_schemes, FocusRegion};
use crate::persistence::PersistenceError;
use crate::{AppWindow, DocsPanelData, HelpTooltipData};

use super::{
    auto_expose_slots, clone_node_with_new_ids, collect_node_ids, page_id_for_panel,
    panel_id_for_slint, sample_apps, sync_ui_from_shared, ShellInner, ShellView, TransformTool,
};

pub(super) struct ContextMenuItemDef {
    pub(super) label: String,
    pub(super) shortcut: String,
    pub(super) command_id: String,
    pub(super) enabled: bool,
    pub(super) is_separator: bool,
}

impl ContextMenuItemDef {
    pub(super) fn action(label: &str, command_id: &str, shortcut: &str, enabled: bool) -> Self {
        Self {
            label: label.into(),
            shortcut: shortcut.into(),
            command_id: command_id.into(),
            enabled,
            is_separator: false,
        }
    }

    pub(super) fn separator() -> Self {
        Self {
            label: String::new(),
            shortcut: String::new(),
            command_id: String::new(),
            enabled: false,
            is_separator: true,
        }
    }
}

pub(super) fn build_context_menu_items(
    inner: &ShellInner,
    target_kind: &str,
    target_id: &str,
) -> Vec<ContextMenuItemDef> {
    let has_selection = !inner.store.state().selection.is_empty();
    let has_clipboard = inner.clipboard.is_some();

    let selected_component = if !target_id.is_empty() {
        inner
            .store
            .state()
            .builder_document
            .root
            .as_ref()
            .and_then(|r| r.find(target_id))
            .map(|n| n.component.clone())
    } else {
        None
    };
    let is_facet = selected_component.as_deref() == Some("facet");

    match target_kind {
        "inspector-node" | "grid-cell" | "builder-node" => {
            let mut items = vec![
                ContextMenuItemDef::action("Cut", "edit.cut", "Ctrl+X", has_selection),
                ContextMenuItemDef::action("Copy", "edit.copy", "Ctrl+C", has_selection),
                ContextMenuItemDef::action("Paste", "edit.paste", "Ctrl+V", has_clipboard),
                ContextMenuItemDef::action("Duplicate", "edit.duplicate", "Ctrl+D", has_selection),
                ContextMenuItemDef::separator(),
                ContextMenuItemDef::action("Move Up", "navigate.inspector_prev", "", has_selection),
                ContextMenuItemDef::action(
                    "Move Down",
                    "navigate.inspector_next",
                    "",
                    has_selection,
                ),
                ContextMenuItemDef::separator(),
                ContextMenuItemDef::action("Delete", "selection.delete", "", has_selection),
            ];
            if is_facet {
                items.push(ContextMenuItemDef::separator());
                items.push(ContextMenuItemDef::action(
                    "Add Item",
                    "facet.add_item",
                    "",
                    true,
                ));
                items.push(ContextMenuItemDef::action(
                    "Clear Items",
                    "facet.clear_items",
                    "",
                    true,
                ));
                items.push(ContextMenuItemDef::action(
                    "Refresh Data",
                    "facet.refresh",
                    "",
                    true,
                ));
                let app_state = inner.store.state();
                let is_inline = app_state
                    .active_app()
                    .and_then(|a| a.active_document())
                    .and_then(|doc| {
                        let node = doc.root.as_ref()?.find(app_state.selection.primary()?)?;
                        let fid = node.props.get("facet_id")?.as_str()?;
                        Some(doc.facets.get(fid)?.is_inline())
                    })
                    .unwrap_or(false);
                if is_inline {
                    items.push(ContextMenuItemDef::action(
                        "Save Template as Component",
                        "facet.promote",
                        "",
                        true,
                    ));
                }
            } else if has_selection {
                items.push(ContextMenuItemDef::separator());
                items.push(ContextMenuItemDef::action(
                    "Save as Prefab",
                    "prefab.save_from_selection",
                    "",
                    true,
                ));
            }
            items
        }
        "inspector-row" => {
            vec![ContextMenuItemDef::action(
                "Delete Track",
                "selection.delete",
                "",
                true,
            )]
        }
        "grid-cell-empty" => {
            vec![
                ContextMenuItemDef::action("Paste", "edit.paste", "Ctrl+V", has_clipboard),
                ContextMenuItemDef::separator(),
                ContextMenuItemDef::action("Select All", "selection.all", "Ctrl+A", true),
            ]
        }
        "explorer-app" | "explorer-page" | "explorer-node" => {
            let mut items = vec![ContextMenuItemDef::action("Open", "panel.edit", "", true)];
            if target_kind == "explorer-page" {
                items.push(ContextMenuItemDef::separator());
                items.push(ContextMenuItemDef::action("Add Page", "add_page", "", true));
            }
            items
        }
        _ => vec![],
    }
}

// ── Docs panel ────────────────────────────────────────────────────

pub(super) fn open_docs_panel(shared: &Rc<RefCell<ShellInner>>, weak: &slint::Weak<AppWindow>) {
    let active_id = shared.borrow().help_active_id.clone();
    if active_id.is_empty() {
        return;
    }
    let (title, summary, body) = {
        let s = shared.borrow();
        match s.help.get(&active_id) {
            Some(entry) => (
                entry.title.clone(),
                entry.summary.clone(),
                entry.body.clone().unwrap_or_default(),
            ),
            None => return,
        }
    };
    {
        let mut s = shared.borrow_mut();
        s.help_active_id.clear();
        s.help_pending_id.clear();
    }
    if let Some(w) = weak.upgrade() {
        w.set_help_tooltip(HelpTooltipData {
            visible: false,
            title: SharedString::new(),
            summary: SharedString::new(),
            has_docs: false,
            tip_x: 0.0,
            tip_y: 0.0,
        });
        w.set_docs_panel(DocsPanelData {
            visible: true,
            help_id: SharedString::from(&active_id),
            title: SharedString::from(title),
            summary: SharedString::from(summary),
            body: SharedString::from(body),
        });
    }
}

// ── Command dispatch ───────────────────────────────────────────────

pub(super) fn execute_command(
    shared: &Rc<RefCell<ShellInner>>,
    weak: &slint::Weak<AppWindow>,
    command_id: &str,
) {
    match command_id {
        "edit.undo" => {
            shared.borrow_mut().perform_undo();
        }
        "edit.redo" => {
            shared.borrow_mut().perform_redo();
        }
        "command_palette.toggle" => {
            let mut s = shared.borrow_mut();
            let open = s.store.state().command_palette_open;
            s.store.mutate(|state| {
                state.command_palette_open = !open;
                if open {
                    state.command_palette_query.clear();
                }
            });
            s.input.set_context("commandPaletteOpen", !open);
        }
        "command_palette.close" => {
            let mut s = shared.borrow_mut();
            s.store.mutate(|state| {
                state.command_palette_open = false;
                state.command_palette_query.clear();
            });
            s.input.set_context("commandPaletteOpen", false);
        }
        "navigate.escape" => {
            let was_tooltip;
            {
                let mut s = shared.borrow_mut();
                was_tooltip = !s.help_active_id.is_empty();
                if was_tooltip {
                    s.help_active_id.clear();
                    s.help_pending_id.clear();
                } else {
                    s.store.mutate(|state| {
                        state.selection.clear();
                    });
                }
            }
            if let Some(w) = weak.upgrade() {
                if was_tooltip {
                    w.set_help_tooltip(HelpTooltipData {
                        visible: false,
                        title: SharedString::new(),
                        summary: SharedString::new(),
                        has_docs: false,
                        tip_x: 0.0,
                        tip_y: 0.0,
                    });
                }
                let empty_docs = DocsPanelData {
                    visible: false,
                    help_id: SharedString::new(),
                    title: SharedString::new(),
                    summary: SharedString::new(),
                    body: SharedString::new(),
                };
                if w.get_docs_view().visible {
                    w.set_docs_view(empty_docs);
                } else if w.get_docs_panel().visible {
                    w.set_docs_panel(empty_docs);
                }
            }
        }
        "panel.identity" | "panel.edit" | "panel.builder" | "panel.inspector"
        | "panel.properties" | "panel.explorer" | "panel.navigation" | "view.file_explorer" => {
            let page_id = page_id_for_panel(
                command_id
                    .strip_prefix("panel.")
                    .or_else(|| command_id.strip_prefix("view.file_"))
                    .unwrap_or("edit"),
            );
            let mut s = shared.borrow_mut();
            let pid = page_id.to_string();
            s.store.mutate(|state| {
                state.workspace.switch_page_by_id(&pid);
            });
            s.dock_dirty.set(true);
            let panel_id = panel_id_for_slint(&s.store.state().workspace);
            update_panel_schemes(&mut s.input, panel_id);
        }
        "panel.code_editor" => {
            let mut s = shared.borrow_mut();
            s.store.mutate(|state| {
                state.workspace.switch_page_by_id("code");
            });
            s.dock_dirty.set(true);
            let panel_id = panel_id_for_slint(&s.store.state().workspace);
            update_panel_schemes(&mut s.input, panel_id);
        }
        "add_page" => {
            let mut s = shared.borrow_mut();
            s.save_to_active_page();
            s.push_undo("Add page");
            s.store.mutate(|state| {
                if let Some(app) = state.active_app_mut() {
                    crate::panels::navigation::NavigationPanel::create_page(app);
                }
                state.selection.clear();
                state.sync_document_from_app();
            });
            s.load_active_page();
            s.dock_dirty.set(true);
        }
        "selection.delete" => {
            let mut s = shared.borrow_mut();
            let selected_id = s.store.state().selection.primary().cloned();
            if let Some(ref target_id) = selected_id {
                s.fire_signal(target_id, "deleted", serde_json::Map::new());
                s.push_undo("Delete node");
                if let Some(ref mut live) = s.live {
                    let _ = live.remove_node_from_source(target_id);
                }
                s.store.mutate(|state| {
                    state.selection.clear();
                });
                s.sync_builder_document();
            }
        }
        "selection.all" => {
            let mut s = shared.borrow_mut();
            let all_ids = collect_node_ids(s.store.state().builder_document.root.as_ref());
            s.store.mutate(|state| {
                state.selection.clear();
                for id in all_ids {
                    state.selection.extend(id);
                }
            });
        }
        "edit.copy" => {
            let mut s = shared.borrow_mut();
            let target_id = s.store.state().selection.primary().cloned();
            let node = target_id.and_then(|tid| {
                s.live.as_mut().and_then(|l| {
                    let doc = l.document();
                    doc.root.as_ref().and_then(|n| n.find(&tid)).cloned()
                })
            });
            if let Some(node) = node {
                let comp = node.component.clone();
                s.clipboard = Some(node);
                s.add_toast("Copied", &format!("{comp} copied"), "info");
            }
        }
        "edit.paste" => {
            let mut s = shared.borrow_mut();
            let clip = s.clipboard.clone();
            if let Some(ref clip) = clip {
                s.push_undo("Paste");
                let mut next_id = s.store.state().next_node_id;
                let new_node = clone_node_with_new_ids(clip, &mut next_id);
                let new_id = new_node.id.clone();
                let parent_id = s.store.state().selection.primary().cloned();
                if let Some(ref mut live) = s.live {
                    let _ = live.insert_tree_in_source(parent_id.as_deref(), &new_node, None);
                }
                s.store.mutate(|state| {
                    state.next_node_id = next_id;
                    state.selection.select(new_id);
                });
                s.sync_builder_document();
            }
        }
        "edit.cut" => {
            let mut s = shared.borrow_mut();
            let target_id = s.store.state().selection.primary().cloned();
            let target_and_node = target_id.and_then(|tid| {
                s.live.as_mut().and_then(|l| {
                    let doc = l.document();
                    let node = doc.root.as_ref().and_then(|n| n.find(&tid))?.clone();
                    Some((tid, node))
                })
            });
            if let Some((target_id, node)) = target_and_node {
                s.clipboard = Some(node);
                s.push_undo("Cut");
                if let Some(ref mut live) = s.live {
                    let _ = live.remove_node_from_source(&target_id);
                }
                s.store.mutate(|state| {
                    state.selection.clear();
                });
                s.sync_builder_document();
            }
        }
        "edit.duplicate" => {
            let mut s = shared.borrow_mut();
            let sel_id = s.store.state().selection.primary().cloned();
            let next_id_start = s.store.state().next_node_id;
            let target_and_node = sel_id.and_then(|tid| {
                s.live.as_mut().and_then(|l| {
                    let doc = l.document();
                    let node = doc.root.as_ref().and_then(|n| n.find(&tid))?.clone();
                    Some((tid, node, next_id_start))
                })
            });
            if let Some((target_id, node, mut next_id)) = target_and_node {
                s.push_undo("Duplicate");
                let new_node = clone_node_with_new_ids(&node, &mut next_id);
                let new_id = new_node.id.clone();
                if let Some(ref mut live) = s.live {
                    let _ = live.insert_tree_in_source(Some(&target_id), &new_node, None);
                }
                s.store.mutate(|state| {
                    state.next_node_id = next_id;
                    state.selection.select(new_id);
                });
                s.sync_builder_document();
            }
        }
        "notification.dismiss_all" => {
            shared.borrow_mut().store.mutate(|state| {
                state.toasts.clear();
            });
        }
        // ── Navigation ────────────────────────────────────────────
        "navigate.next_tab" => {
            let mut s = shared.borrow_mut();
            let page_count = s
                .store
                .state()
                .active_app()
                .map(|a| a.pages.len())
                .unwrap_or(0);
            if page_count > 1 {
                s.save_to_active_page();
                s.store.mutate(|state| {
                    if let Some(app) = state.active_app_mut() {
                        app.active_page = (app.active_page + 1) % app.pages.len();
                    }
                    state.selection.clear();
                    state.sync_document_from_app();
                });
                s.load_active_page();
            }
        }
        "navigate.prev_tab" => {
            let mut s = shared.borrow_mut();
            let page_count = s
                .store
                .state()
                .active_app()
                .map(|a| a.pages.len())
                .unwrap_or(0);
            if page_count > 1 {
                s.save_to_active_page();
                s.store.mutate(|state| {
                    if let Some(app) = state.active_app_mut() {
                        let len = app.pages.len();
                        app.active_page = if app.active_page == 0 {
                            len - 1
                        } else {
                            app.active_page - 1
                        };
                    }
                    state.selection.clear();
                    state.sync_document_from_app();
                });
                s.load_active_page();
            }
        }
        "navigate.inspector_prev" => {
            let mut s = shared.borrow_mut();
            let ids = collect_node_ids(s.store.state().builder_document.root.as_ref());
            if let Some(current) = s.store.state().selection.primary().cloned() {
                if let Some(idx) = ids.iter().position(|id| *id == current) {
                    if idx > 0 {
                        let new_id = ids[idx - 1].clone();
                        s.store.mutate(|state| state.selection.select(new_id));
                    }
                }
            } else if !ids.is_empty() {
                let first = ids[0].clone();
                s.store.mutate(|state| state.selection.select(first));
            }
        }
        "navigate.inspector_next" => {
            let mut s = shared.borrow_mut();
            let ids = collect_node_ids(s.store.state().builder_document.root.as_ref());
            if let Some(current) = s.store.state().selection.primary().cloned() {
                if let Some(idx) = ids.iter().position(|id| *id == current) {
                    if idx + 1 < ids.len() {
                        let new_id = ids[idx + 1].clone();
                        s.store.mutate(|state| state.selection.select(new_id));
                    }
                }
            } else if !ids.is_empty() {
                let first = ids[0].clone();
                s.store.mutate(|state| state.selection.select(first));
            }
        }
        "search.focus" => {
            let mut s = shared.borrow_mut();
            if s.store.state().workspace.active_page().id != "edit" {
                s.store.mutate(|state| {
                    state.workspace.switch_page_by_id("edit");
                });
                s.dock_dirty.set(true);
                let panel_id = panel_id_for_slint(&s.store.state().workspace);
                update_panel_schemes(&mut s.input, panel_id);
            }
            s.input.set_focus(FocusRegion::Search);
        }
        "view.toggle_left_sidebar" | "navigate.sidebar_toggle" => {
            shared.borrow_mut().store.mutate(|state| {
                state.show_left_sidebar = !state.show_left_sidebar;
            });
        }
        "view.toggle_right_sidebar" => {
            shared.borrow_mut().store.mutate(|state| {
                state.show_right_sidebar = !state.show_right_sidebar;
            });
        }
        "view.toggle_activity_bar" => {
            shared.borrow_mut().store.mutate(|state| {
                state.show_activity_bar = !state.show_activity_bar;
            });
        }
        "view.toggle_grid" => {
            shared.borrow_mut().store.mutate(|state| {
                state.show_grid_overlay = !state.show_grid_overlay;
            });
        }
        "view.zoom_in" => {
            if let Some(w) = weak.upgrade() {
                let cur = w.get_canvas_zoom();
                w.set_canvas_zoom((cur + 0.1).min(3.0));
            }
        }
        "view.zoom_out" => {
            if let Some(w) = weak.upgrade() {
                let cur = w.get_canvas_zoom();
                w.set_canvas_zoom((cur - 0.1).max(0.25));
            }
        }
        "view.zoom_reset" => {
            if let Some(w) = weak.upgrade() {
                w.set_canvas_zoom(1.0);
            }
        }
        "view.zoom_to_fit" => {
            if let Some(w) = weak.upgrade() {
                let pl = w.get_page_layout();
                let pw = pl.page_width;
                let ph = pl.page_height;
                if pw > 0.0 && ph > 0.0 {
                    let s = shared.borrow();
                    let (dock_w, dock_h) = s.dock_area_dims;
                    let canvas_w = (dock_w * 0.5).max(200.0);
                    let canvas_h = (dock_h * 0.85).max(200.0);
                    let fit = (canvas_w / pw).min(canvas_h / ph).clamp(0.25, 3.0);
                    drop(s);
                    w.set_canvas_zoom(fit);
                }
            }
        }
        "tool.move" => {
            shared
                .borrow_mut()
                .store
                .mutate(|s| s.transform_tool = TransformTool::Move);
        }
        "tool.rotate" => {
            shared
                .borrow_mut()
                .store
                .mutate(|s| s.transform_tool = TransformTool::Rotate);
        }
        "tool.scale" => {
            shared
                .borrow_mut()
                .store
                .mutate(|s| s.transform_tool = TransformTool::Scale);
        }
        "file.save" => {
            let mut s = shared.borrow_mut();
            s.save_to_active_page();
            #[cfg(feature = "native")]
            if let Some(ref mut proj) = s.project {
                match proj.save() {
                    Ok(_) => {}
                    Err(e) => s.add_toast("Vault save failed", &e.to_string(), "error"),
                }
            }
            let apps = s.store.state().apps.clone();
            let reg = Arc::clone(&s.registry);
            let tokens = s.store.state().tokens;
            let result = s.persistence.save(&apps, &reg, &tokens);
            match result {
                Ok(path) => {
                    let name = path
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_else(|| path.display().to_string());
                    s.add_toast("Saved", &format!("Project saved to {name}"), "success");
                }
                Err(PersistenceError::NoPath) => {
                    drop(s);
                    execute_command(shared, weak, "file.save_as");
                    return;
                }
                Err(PersistenceError::Cancelled) => {}
                Err(e) => s.add_toast("Save failed", &e.to_string(), "error"),
            }
        }
        "file.save_as" => {
            let mut s = shared.borrow_mut();
            s.save_to_active_page();
            let apps = s.store.state().apps.clone();
            let reg = Arc::clone(&s.registry);
            let tokens = s.store.state().tokens;
            match s.persistence.save_as(&apps, &reg, &tokens) {
                Ok(path) => {
                    let name = path
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_else(|| path.display().to_string());
                    s.add_toast("Saved", &format!("Project saved to {name}"), "success");
                }
                Err(PersistenceError::Cancelled) => {}
                Err(e) => s.add_toast("Save failed", &e.to_string(), "error"),
            }
        }
        "file.open" => {
            {
                let s = shared.borrow();
                if s.persistence.is_dirty() && !crate::persistence::confirm_discard_changes() {
                    return;
                }
            }
            let mut s = shared.borrow_mut();
            s.save_to_active_page();
            match s.persistence.open() {
                Ok(apps) => {
                    let name = s
                        .persistence
                        .project_name()
                        .unwrap_or_else(|| "project".into());
                    s.store.mutate(|state| {
                        state.apps = apps;
                        state.shell_view = ShellView::Launchpad;
                        state.selection.clear();
                    });
                    s.live = None;
                    s.undo_past.clear();
                    s.undo_future.clear();
                    s.add_toast("Opened", &format!("Loaded {name}"), "success");
                }
                Err(PersistenceError::Cancelled) => {}
                Err(e) => s.add_toast("Open failed", &e.to_string(), "error"),
            }
        }
        "file.new" => {
            {
                let s = shared.borrow();
                if s.persistence.is_dirty() && !crate::persistence::confirm_discard_changes() {
                    return;
                }
            }
            let mut s = shared.borrow_mut();
            s.persistence.clear_path();
            s.store.mutate(|state| {
                state.apps = sample_apps();
                state.shell_view = ShellView::Launchpad;
                state.selection.clear();
            });
            s.live = None;
            s.undo_past.clear();
            s.undo_future.clear();
            s.add_toast("New Project", "Started a new project", "info");
        }
        "file.revert" => {
            let mut s = shared.borrow_mut();
            let path = s.persistence.current_path().cloned();
            match path {
                Some(p) => {
                    if !s.persistence.is_dirty() {
                        s.add_toast("Revert", "No changes to revert", "info");
                        return;
                    }
                    if !crate::persistence::confirm_discard_changes() {
                        return;
                    }
                    match s.persistence.open_path(&p) {
                        Ok(apps) => {
                            let name = s
                                .persistence
                                .project_name()
                                .unwrap_or_else(|| "project".into());
                            s.store.mutate(|state| {
                                state.apps = apps;
                                state.shell_view = ShellView::Launchpad;
                                state.selection.clear();
                            });
                            s.live = None;
                            s.undo_past.clear();
                            s.undo_future.clear();
                            s.add_toast("Reverted", &format!("Reloaded {name}"), "success");
                        }
                        Err(e) => s.add_toast("Revert failed", &e.to_string(), "error"),
                    }
                }
                None => {
                    s.add_toast("Revert", "No saved file to revert to", "info");
                }
            }
        }
        #[cfg(feature = "native")]
        "project.open_folder" => {
            let folder = rfd::FileDialog::new()
                .set_title("Open Project Folder")
                .pick_folder();
            if let Some(path) = folder {
                let mut s = shared.borrow_mut();
                match crate::project::ProjectManager::open(&path) {
                    Ok(mut proj) => {
                        let objects = proj.collection().list_objects(None);
                        {
                            let mut col = s.collection.borrow_mut();
                            for obj in &objects {
                                let _ = col.put_object(obj);
                            }
                        }
                        let edges = proj.collection().list_edges(None);
                        {
                            let mut col = s.collection.borrow_mut();
                            for edge in &edges {
                                let _ = col.put_edge(edge);
                            }
                        }
                        let name = path
                            .file_name()
                            .map(|n| n.to_string_lossy().into_owned())
                            .unwrap_or_else(|| path.display().to_string());
                        s.project = Some(proj);
                        s.add_toast(
                            "Folder opened",
                            &format!("{name} — {} files", objects.len()),
                            "success",
                        );
                    }
                    Err(e) => {
                        s.add_toast("Open folder failed", &e.to_string(), "error");
                    }
                }
            }
        }
        #[cfg(feature = "native")]
        "project.close" => {
            let mut s = shared.borrow_mut();
            if s.project.is_some() {
                s.project = None;
                *s.collection.borrow_mut() = CollectionStore::new();
                s.add_toast("Folder closed", "Project folder closed", "info");
            }
        }
        "facet.add_item" => {
            let mut s = shared.borrow_mut();
            let selected = s.store.state().selection.primary().cloned();
            if let Some(ref node_id) = selected {
                let facet_id = s
                    .store
                    .state()
                    .builder_document
                    .root
                    .as_ref()
                    .and_then(|r| r.find(node_id))
                    .and_then(|n| n.props.get("facet_id"))
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                if let Some(fid) = facet_id {
                    s.push_undo("Add facet item");
                    s.store.mutate(|state| {
                        if let Some(doc) =
                            state.active_app_mut().and_then(|a| a.active_document_mut())
                        {
                            if let Some(def) = doc.facets.get_mut(&fid) {
                                if let FacetDataSource::Static {
                                    ref mut items,
                                    ref mut records,
                                } = def.data
                                {
                                    if let Some(schema_id) = &def.schema_id {
                                        if let Some(schema) = doc.facet_schemas.get(schema_id) {
                                            let rec_id = format!("rec:{}", records.len() + 1);
                                            records.push(schema.default_record(rec_id));
                                        } else {
                                            items.push(serde_json::json!({}));
                                        }
                                    } else {
                                        items.push(serde_json::json!({}));
                                    }
                                }
                            }
                        }
                    });
                    s.sync_builder_document();
                }
            }
        }
        "facet.clear_items" => {
            let mut s = shared.borrow_mut();
            let selected = s.store.state().selection.primary().cloned();
            if let Some(ref node_id) = selected {
                let facet_id = s
                    .store
                    .state()
                    .builder_document
                    .root
                    .as_ref()
                    .and_then(|r| r.find(node_id))
                    .and_then(|n| n.props.get("facet_id"))
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                if let Some(fid) = facet_id {
                    s.push_undo("Clear facet items");
                    s.store.mutate(|state| {
                        if let Some(doc) =
                            state.active_app_mut().and_then(|a| a.active_document_mut())
                        {
                            if let Some(def) = doc.facets.get_mut(&fid) {
                                if let FacetDataSource::Static {
                                    ref mut items,
                                    ref mut records,
                                } = def.data
                                {
                                    items.clear();
                                    records.clear();
                                }
                            }
                        }
                    });
                    s.sync_builder_document();
                }
            }
        }
        "facet.refresh" => {
            let mut s = shared.borrow_mut();
            s.sync_builder_document();
            s.add_toast("Facet", "Data refreshed", "success");
        }
        "facet.promote" => {
            let mut s = shared.borrow_mut();
            let info: Option<(String, String, prism_builder::Node)> = s
                .store
                .state()
                .active_app()
                .and_then(|a| a.active_document())
                .and_then(|doc| {
                    let selected = s.store.state().selection.primary()?;
                    let node = doc.root.as_ref()?.find(selected)?;
                    if node.component != "facet" {
                        return None;
                    }
                    let fid = node.props.get("facet_id")?.as_str()?;
                    let def = doc.facets.get(fid)?;
                    if let FacetTemplate::Inline { root } = &def.template {
                        Some((fid.to_string(), selected.to_string(), *root.clone()))
                    } else {
                        None
                    }
                });
            if let Some((fid, _node_id, root)) = info {
                let (prefab, bindings) = prism_builder::promote_inline_to_component(&fid, &root);
                let component_id = prefab.id.clone();
                s.push_undo("Promote to component");
                let cid = component_id.clone();
                s.store.mutate(|state| {
                    if let Some(doc) = state.active_app_mut().and_then(|a| a.active_document_mut())
                    {
                        doc.prefabs.insert(cid.clone(), prefab);
                        if let Some(def) = doc.facets.get_mut(&fid) {
                            def.template =
                                prism_builder::FacetTemplate::ComponentRef { component_id: cid };
                            def.bindings = bindings;
                        }
                    }
                });
                s.sync_builder_document();
                s.add_toast("Facet", &format!("Promoted to {component_id}"), "success");
            } else {
                s.add_toast("Facet", "Select an inline-template facet first", "warning");
            }
        }
        "schema.create" => {
            let mut s = shared.borrow_mut();
            let counter = s.store.state().next_node_id;
            let schema_id = format!("schema:n{counter}");
            s.push_undo("Create schema");
            let sid = schema_id.clone();
            s.store.mutate(|state| {
                state.next_node_id += 1;
                if let Some(doc) = state.active_app_mut().and_then(|a| a.active_document_mut()) {
                    doc.facet_schemas.insert(
                        sid.clone(),
                        prism_builder::FacetSchema {
                            id: sid.clone(),
                            label: "New Schema".into(),
                            description: String::new(),
                            fields: vec![
                                prism_core::widget::FieldSpec::text("title", "Title").required()
                            ],
                        },
                    );
                }
                state.selected_schema_id = Some(sid);
            });
            s.sync_builder_document();
            s.add_toast("Schema created", &schema_id, "success");
        }
        "schema.delete" => {
            let mut s = shared.borrow_mut();
            let target_id = s.store.state().selected_schema_id.clone().or_else(|| {
                s.store
                    .state()
                    .active_app()
                    .and_then(|a| a.active_document())
                    .and_then(|doc| doc.facet_schemas.keys().next().cloned())
            });
            if let Some(sid) = target_id {
                let label = sid.clone();
                s.push_undo("Delete schema");
                s.store.mutate(|state| {
                    if let Some(doc) = state.active_app_mut().and_then(|a| a.active_document_mut())
                    {
                        doc.facet_schemas.shift_remove(&sid);
                        for facet in doc.facets.values_mut() {
                            if facet.schema_id.as_deref() == Some(sid.as_str()) {
                                facet.schema_id = None;
                            }
                        }
                    }
                    state.selected_schema_id = state
                        .active_app()
                        .and_then(|a| a.active_document())
                        .and_then(|doc| doc.facet_schemas.keys().next().cloned());
                });
                s.sync_builder_document();
                s.add_toast("Schema deleted", &label, "success");
            }
        }
        "schema.add_field" => {
            let mut s = shared.borrow_mut();
            let sid = s.store.state().selected_schema_id.clone().or_else(|| {
                s.store
                    .state()
                    .active_app()
                    .and_then(|a| a.active_document())
                    .and_then(|doc| doc.facet_schemas.keys().next().cloned())
            });
            if let Some(sid) = sid {
                s.push_undo("Add schema field");
                s.store.mutate(|state| {
                    if let Some(doc) = state.active_app_mut().and_then(|a| a.active_document_mut())
                    {
                        if let Some(schema) = doc.facet_schemas.get_mut(&sid) {
                            let n = schema.fields.len() + 1;
                            schema.fields.push(prism_core::widget::FieldSpec::text(
                                format!("field_{n}"),
                                format!("Field {n}"),
                            ));
                        }
                    }
                });
                s.sync_builder_document();
            }
        }
        "schema.delete_field" => {
            let mut s = shared.borrow_mut();
            let sid = s.store.state().selected_schema_id.clone().or_else(|| {
                s.store
                    .state()
                    .active_app()
                    .and_then(|a| a.active_document())
                    .and_then(|doc| doc.facet_schemas.keys().next().cloned())
            });
            if let Some(sid) = sid {
                s.push_undo("Delete schema field");
                s.store.mutate(|state| {
                    if let Some(doc) = state.active_app_mut().and_then(|a| a.active_document_mut())
                    {
                        if let Some(schema) = doc.facet_schemas.get_mut(&sid) {
                            if schema.fields.len() > 1 {
                                schema.fields.pop();
                            }
                        }
                    }
                });
                s.sync_builder_document();
            }
        }
        "panel.schema_designer" => {
            let mut s = shared.borrow_mut();
            s.store.mutate(|state| {
                state.workspace.switch_page_by_id("data");
            });
        }
        "prefab.save_from_selection" => {
            let mut s = shared.borrow_mut();
            let selected = s.store.state().selection.primary().cloned();
            if let Some(ref node_id) = selected {
                let node_snapshot = s
                    .store
                    .state()
                    .builder_document
                    .root
                    .as_ref()
                    .and_then(|r| r.find(node_id))
                    .cloned();
                if let Some(node) = node_snapshot {
                    let counter = s.store.state().next_node_id;
                    let prefab_id = format!("prefab:n{counter}");
                    let label = {
                        let c = &node.component;
                        let mut chars = c.chars();
                        match chars.next() {
                            Some(first) => {
                                first.to_uppercase().collect::<String>() + chars.as_str()
                            }
                            None => c.clone(),
                        }
                    };
                    let label = format!("{label} Prefab");
                    let exposed = auto_expose_slots(&node);
                    let def = PrefabDef {
                        id: prefab_id.clone(),
                        label: label.clone(),
                        description: String::new(),
                        root: node,
                        exposed,
                        variants: vec![],
                        thumbnail: None,
                    };
                    s.push_undo("Save as prefab");
                    s.store.mutate(|state| {
                        state.next_node_id += 1;
                        if let Some(doc) =
                            state.active_app_mut().and_then(|a| a.active_document_mut())
                        {
                            doc.prefabs.insert(prefab_id, def);
                        }
                    });
                    s.sync_builder_document();
                    s.add_toast(
                        "Prefab saved",
                        &format!("'{label}' saved to Prefabs"),
                        "success",
                    );
                }
            }
        }
        other => {
            if let Some(n) = other
                .strip_prefix("navigate.tab.")
                .and_then(|s| s.parse::<usize>().ok())
            {
                let mut s = shared.borrow_mut();
                let page_count = s
                    .store
                    .state()
                    .active_app()
                    .map(|a| a.pages.len())
                    .unwrap_or(0);
                if n >= 1 && n <= page_count {
                    s.save_to_active_page();
                    s.store.mutate(|state| {
                        if let Some(app) = state.active_app_mut() {
                            app.active_page = n - 1;
                        }
                        state.selection.clear();
                        state.sync_document_from_app();
                    });
                    s.load_active_page();
                }
            } else {
                eprintln!("prism-shell: unknown command {other}");
            }
        }
    }
    // Close palette after non-palette command execution
    if !matches!(
        command_id,
        "command_palette.toggle" | "command_palette.close"
    ) {
        let mut s = shared.borrow_mut();
        if s.store.state().command_palette_open {
            s.store.mutate(|state| {
                state.command_palette_open = false;
                state.command_palette_query.clear();
            });
            s.input.set_context("commandPaletteOpen", false);
        }
    }
    if let Some(w) = weak.upgrade() {
        sync_ui_from_shared(shared, &w);
    }
}
