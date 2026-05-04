use std::cell::RefCell;
use std::rc::Rc;

use super::{
    apply_facet_edit, apply_node_layout_edit, apply_node_transform_edit, apply_style_edit,
    field_kind_for_key, format_slider_value, is_preview_mode, resolve_schema_id,
    slint_source_key_for_edit, sync_ui_from_shared, Shell, ShellInner,
};
use crate::AppWindow;

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

mod builder;
mod chrome;
mod editor;
mod navigation;
mod overlay;
mod properties;

impl Shell {
    pub(super) fn wire_callbacks(&self) {
        self.wire_chrome_callbacks();
        self.wire_builder_callbacks();
        self.wire_properties_callbacks();
        self.wire_navigation_callbacks();
        self.wire_editor_callbacks();
        self.wire_overlay_callbacks();
    }
}
