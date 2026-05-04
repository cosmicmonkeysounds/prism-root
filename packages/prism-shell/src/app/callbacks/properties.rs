use std::rc::Rc;

use prism_builder::AssetSource;
use slint::ComponentHandle;

use super::super::{
    format_slider_value, mime_from_extension, push_user_swatches, sync_ui_from_shared, Shell,
};
use super::{dispatch_property_field_edit, with_shell};

impl Shell {
    pub(super) fn wire_properties_callbacks(&self) {
        let weak = self.window.as_weak();
        let inner = Rc::clone(&self.inner);

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
    }
}
