#![allow(unused_imports)]

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use slint::ComponentHandle;

use super::sync::{build_handler_script, sync_ui_from_shared};
use super::*;

impl ShellInner {
    pub(crate) fn push_undo(&mut self, description: &str) {
        let source = self
            .live
            .as_ref()
            .map(|l| l.source.clone())
            .unwrap_or_default();
        let selection = self.store.state().selection.clone();
        self.undo_past.push(SourceSnapshot {
            description: description.into(),
            source,
            selection,
        });
        self.undo_future.clear();
        if self.undo_past.len() > 100 {
            self.undo_past.remove(0);
        }
        self.persistence.mark_dirty();
    }

    pub(crate) fn perform_undo(&mut self) {
        let Some(snapshot) = self.undo_past.pop() else {
            return;
        };
        let current_source = self
            .live
            .as_ref()
            .map(|l| l.source.clone())
            .unwrap_or_default();
        let current_sel = self.store.state().selection.clone();
        self.undo_future.push(SourceSnapshot {
            description: snapshot.description.clone(),
            source: current_source,
            selection: current_sel,
        });
        if let Some(ref mut live) = self.live {
            let _ = live.set_source(snapshot.source);
        }
        let sel = snapshot.selection;
        self.store.mutate(|state| {
            state.selection = sel;
        });
        self.sync_builder_document();
    }

    pub(crate) fn perform_redo(&mut self) {
        let Some(snapshot) = self.undo_future.pop() else {
            return;
        };
        let current_source = self
            .live
            .as_ref()
            .map(|l| l.source.clone())
            .unwrap_or_default();
        let current_sel = self.store.state().selection.clone();
        self.undo_past.push(SourceSnapshot {
            description: snapshot.description.clone(),
            source: current_source,
            selection: current_sel,
        });
        if let Some(ref mut live) = self.live {
            let _ = live.set_source(snapshot.source);
        }
        let sel = snapshot.selection;
        self.store.mutate(|state| {
            state.selection = sel;
        });
        self.sync_builder_document();
    }

    pub(crate) fn fire_signal(
        &mut self,
        source_node: &str,
        signal: &str,
        payload: serde_json::Map<String, serde_json::Value>,
    ) -> bool {
        let is_preview = is_preview_mode(&self.store.state().workspace);
        eprintln!("[signal] fire_signal source={source_node} signal={signal} preview={is_preview}");
        if !is_preview {
            return false;
        }
        let connections = self
            .store
            .state()
            .active_app()
            .and_then(|a| a.active_document())
            .map(|d| d.connections.clone())
            .unwrap_or_default();
        eprintln!("[signal] connections count={}", connections.len());
        if connections.is_empty() {
            return false;
        }
        let results = SignalRuntime::fire(source_node, signal, payload, &connections);
        eprintln!("[signal] dispatch results={}", results.len());
        if results.is_empty() {
            return false;
        }
        let mut cascading: Vec<(String, String)> = Vec::new();
        let mut navigate_target: Option<String> = None;
        let mut custom_handlers: Vec<(String, serde_json::Map<String, serde_json::Value>)> =
            Vec::new();
        self.store.mutate(|state| {
            for result in &results {
                match result {
                    DispatchResult::SetProperty {
                        target_node,
                        key,
                        value,
                    } => {
                        state
                            .runtime_overrides
                            .entry(target_node.clone())
                            .or_default()
                            .insert(key.clone(), value.clone());
                    }
                    DispatchResult::ToggleVisibility { target_node } => {
                        let current = state
                            .runtime_overrides
                            .get(target_node.as_str())
                            .and_then(|m| m.get("visible"))
                            .and_then(|v| v.as_bool())
                            .or_else(|| {
                                state
                                    .builder_document
                                    .root
                                    .as_ref()
                                    .and_then(|r| r.find(target_node))
                                    .and_then(|n| n.props.get("visible"))
                                    .and_then(|v| v.as_bool())
                            })
                            .unwrap_or(true);
                        state
                            .runtime_overrides
                            .entry(target_node.clone())
                            .or_default()
                            .insert("visible".into(), serde_json::Value::from(!current));
                    }
                    DispatchResult::PlayAnimation {
                        target_node,
                        animation,
                    } => {
                        let entry = state
                            .runtime_overrides
                            .entry(target_node.clone())
                            .or_default();
                        entry.insert("animating".into(), serde_json::Value::from(true));
                        entry.insert(
                            "animation".into(),
                            serde_json::Value::from(animation.as_str()),
                        );
                    }
                    DispatchResult::EmitSignal {
                        target_node,
                        signal,
                    } => {
                        cascading.push((target_node.clone(), signal.clone()));
                    }
                    DispatchResult::NavigateTo { target } => {
                        navigate_target = Some(target.clone());
                    }
                    DispatchResult::Custom { handler, payload } => {
                        custom_handlers.push((handler.clone(), payload.clone()));
                    }
                }
            }
        });
        // If no explicit NavigateTo from connections, check for href prop on clicked nodes
        if navigate_target.is_none() && signal == "clicked" {
            let href = self
                .store
                .state()
                .builder_document
                .root
                .as_ref()
                .and_then(|r| r.find(source_node))
                .and_then(|n| n.props.get("href"))
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string());
            if let Some(h) = href {
                navigate_target = Some(h);
            }
        }
        if let Some(route) = navigate_target {
            self.store.mutate(|state| {
                state.runtime_overrides.clear();
                if let Some(app) = state.active_app_mut() {
                    // Match by route first, then by page ID
                    let idx = app
                        .find_page_by_route(&route)
                        .or_else(|| app.find_page_by_id(&route));
                    if let Some(idx) = idx {
                        app.active_page = idx;
                    }
                }
                state.sync_document_from_app();
            });
            self.load_active_page();
        }
        #[cfg(feature = "native")]
        if !custom_handlers.is_empty() {
            self.exec_custom_handlers(&custom_handlers, source_node, signal);
        }
        const MAX_CASCADE_DEPTH: usize = 8;
        for (i, (target, sig)) in cascading.into_iter().enumerate() {
            if i >= MAX_CASCADE_DEPTH {
                break;
            }
            self.fire_signal(&target, &sig, serde_json::Map::new());
        }
        self.store.mutate(|state| {
            Self::apply_runtime_overrides_to_doc(
                &mut state.builder_document,
                &state.runtime_overrides,
            );
        });
        true
    }

    pub(crate) fn apply_runtime_overrides_to_doc(
        doc: &mut BuilderDocument,
        overrides: &HashMap<String, serde_json::Map<String, serde_json::Value>>,
    ) {
        if overrides.is_empty() {
            return;
        }
        if let Some(ref mut root) = doc.root {
            Self::apply_overrides_to_tree(root, overrides);
        }
    }

    pub(crate) fn apply_overrides_to_tree(
        node: &mut Node,
        overrides: &HashMap<String, serde_json::Map<String, serde_json::Value>>,
    ) {
        if let Some(node_overrides) = overrides.get(&node.id) {
            if let Some(obj) = node.props.as_object_mut() {
                for (key, value) in node_overrides {
                    obj.insert(key.clone(), value.clone());
                }
            }
        }
        for child in &mut node.children {
            Self::apply_overrides_to_tree(child, overrides);
        }
    }

    #[cfg(feature = "native")]
    pub(crate) fn exec_custom_handlers(
        &mut self,
        handlers: &[(String, serde_json::Map<String, serde_json::Value>)],
        source_node: &str,
        signal: &str,
    ) {
        let page_source = self
            .store
            .state()
            .active_app()
            .and_then(|a| a.pages.get(a.active_page))
            .map(|p| p.source.clone())
            .unwrap_or_default();
        for (handler_name, payload) in handlers {
            let mut args = payload.clone();
            args.insert("_source_node".into(), serde_json::Value::from(source_node));
            args.insert("_signal".into(), serde_json::Value::from(signal));
            let script = build_handler_script(&page_source, handler_name);
            let mut call_args = serde_json::Map::new();
            call_args.insert("event".into(), serde_json::Value::Object(args));
            // Phase 4: scripts in the shell see a live collection so
            // `prism.objects` / `prism.edges` mutations land in the
            // same store the UI mutates from. The daemon's bare
            // `luau.exec` keeps the default (no collection) context
            // and surfaces a typed error when scripts reach for the
            // instance API there.
            let ctx = prism_daemon::modules::prism_context::PrismContext::default()
                .with_collection(self.collection.clone());
            match prism_daemon::modules::luau_module::exec_with_context(
                &script,
                Some(&call_args),
                ctx,
            ) {
                Ok(result) => {
                    self.apply_luau_result(&result);
                }
                Err(e) => {
                    self.add_toast(
                        &format!("Signal handler '{handler_name}' error"),
                        &e,
                        "error",
                    );
                }
            }
        }
    }

    #[cfg(feature = "native")]
    pub(crate) fn apply_luau_result(&mut self, result: &serde_json::Value) {
        use serde_json::Value;
        let Some(obj) = result.as_object() else {
            return;
        };
        let mut needs_sync = false;
        if let Some(Value::Array(actions)) = obj.get("_actions") {
            for action in actions {
                let Some(action_obj) = action.as_object() else {
                    continue;
                };
                match action_obj.get("type").and_then(|v| v.as_str()) {
                    Some("set_property") => {
                        let node_id = action_obj.get("node_id").and_then(|v| v.as_str());
                        let key = action_obj.get("key").and_then(|v| v.as_str());
                        let value = action_obj.get("value");
                        if let (Some(node_id), Some(key), Some(value)) = (node_id, key, value) {
                            if let Some(ref mut live) = self.live {
                                let formatted = match value {
                                    Value::String(s) => format!(
                                        "\"{}\"",
                                        prism_builder::slint_source::escape_slint_string(s)
                                    ),
                                    Value::Bool(b) => b.to_string(),
                                    Value::Number(n) => n.to_string(),
                                    _ => continue,
                                };
                                let _ = live.edit_prop_in_source(node_id, key, &formatted);
                                needs_sync = true;
                            }
                        }
                    }
                    Some("toggle_visibility") => {
                        let node_id = action_obj.get("node_id").and_then(|v| v.as_str());
                        if let Some(node_id) = node_id {
                            self.store.mutate(|state| {
                                if let Some(doc) =
                                    state.active_app_mut().and_then(|a| a.active_document_mut())
                                {
                                    let result = DispatchResult::ToggleVisibility {
                                        target_node: node_id.into(),
                                    };
                                    SignalRuntime::apply_result(&result, doc);
                                }
                            });
                            needs_sync = true;
                        }
                    }
                    Some("navigate") => {
                        let route = action_obj.get("route").and_then(|v| v.as_str());
                        if let Some(route) = route {
                            let route = route.to_string();
                            self.store.mutate(|state| {
                                if let Some(app) = state.active_app_mut() {
                                    let idx = app
                                        .find_page_by_route(&route)
                                        .or_else(|| app.find_page_by_id(&route));
                                    if let Some(idx) = idx {
                                        app.active_page = idx;
                                    }
                                }
                                state.sync_document_from_app();
                            });
                            self.load_active_page();
                            return;
                        }
                    }
                    Some("emit_signal") => {
                        let node_id = action_obj.get("node_id").and_then(|v| v.as_str());
                        let sig = action_obj.get("signal").and_then(|v| v.as_str());
                        if let (Some(node_id), Some(sig)) = (node_id, sig) {
                            self.fire_signal(node_id, sig, serde_json::Map::new());
                        }
                    }
                    _ => {}
                }
            }
        }
        if needs_sync {
            self.sync_builder_document();
        }
    }

    pub(crate) fn load_active_page(&mut self) {
        self.store.mutate(|state| {
            state.runtime_overrides.clear();
        });
        let state = self.store.state();
        if let Some(app) = state.active_app() {
            if let Some(page) = app.pages.get(app.active_page) {
                let registry = Arc::clone(&self.registry);
                let tokens = state.tokens;
                if page.source.is_empty() {
                    let live = LiveDocument::from_document(page.document.clone(), registry, tokens);
                    self.live = Some(live);
                } else {
                    let live = LiveDocument::from_source(page.source.clone(), registry, tokens);
                    self.live = Some(live);
                }
                self.sync_builder_document();
            }
        }
        self.undo_past.clear();
        self.undo_future.clear();
    }

    pub(crate) fn save_to_active_page(&mut self) {
        if let Some(ref mut live) = self.live {
            let source = live.source.clone();
            let mut doc = live.document().clone();
            self.store.mutate(|state| {
                state.sync_source_to_app(&source);
                doc.page_layout = state.builder_document.page_layout.clone();
                if let Some(app_doc) = state.active_app().and_then(|a| a.active_document()) {
                    doc.connections = app_doc.connections.clone();
                    doc.facets = app_doc.facets.clone();
                    doc.facet_schemas = app_doc.facet_schemas.clone();
                    doc.resources = app_doc.resources.clone();
                    doc.prefabs = app_doc.prefabs.clone();
                }
                Self::apply_runtime_overrides_to_doc(&mut doc, &state.runtime_overrides);
                state.builder_document = doc;
                state.sync_document_to_app();
            });
        }
    }

    pub(crate) fn sync_builder_document(&mut self) {
        if let Some(ref mut live) = self.live {
            let mut doc = live.document().clone();
            let source = live.source.clone();
            self.store.mutate(|state| {
                doc.page_layout = state.builder_document.page_layout.clone();
                if let Some(app_doc) = state.active_app().and_then(|a| a.active_document()) {
                    doc.connections = app_doc.connections.clone();
                    doc.facets = app_doc.facets.clone();
                    doc.facet_schemas = app_doc.facet_schemas.clone();
                    doc.resources = app_doc.resources.clone();
                    doc.prefabs = app_doc.prefabs.clone();
                }
                Self::apply_runtime_overrides_to_doc(&mut doc, &state.runtime_overrides);
                state.builder_document = doc;
                if state.editor_state.text() != source {
                    let cursor = state.editor_state.cursor.position;
                    state.editor_state.set_text(&source);
                    state.editor_state.language = "slint".into();
                    state
                        .editor_state
                        .set_cursor_position(cursor.line, cursor.col);
                }
            });
        }
    }

    pub(crate) fn switch_to_page(&mut self, page_index: usize) {
        self.save_to_active_page();
        self.store.mutate(|state| {
            if let Some(app) = state.active_app_mut() {
                app.active_page = page_index;
            }
            state.selection.clear();
            state.sync_document_from_app();
        });
        self.load_active_page();
        self.dock_dirty.set(true);
    }

    pub(crate) fn add_toast(&mut self, title: &str, body: &str, kind: &str) {
        self.store.mutate(|state| {
            let id = state.next_toast_id;
            state.next_toast_id += 1;
            state.toasts.push(ToastData {
                id,
                title: title.into(),
                body: body.into(),
                kind: kind.into(),
                created_at: Some(Instant::now()),
            });
            if state.toasts.len() > 5 {
                state.toasts.remove(0);
            }
        });
    }
}
