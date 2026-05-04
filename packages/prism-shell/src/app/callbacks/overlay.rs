use std::rc::Rc;

use slint::{ComponentHandle, SharedString, Timer, TimerMode};

use super::super::{
    execute_command, is_preview_mode, open_docs_panel, sync_ui_from_shared, ContextMenuState, Shell,
};
use super::with_shell;
use crate::{DocsPanelData, HelpTooltipData};

impl Shell {
    pub(super) fn wire_overlay_callbacks(&self) {
        let weak = self.window.as_weak();
        let inner = Rc::clone(&self.inner);

        let show_timer = Rc::new(Timer::default());
        let hide_timer = Rc::new(Timer::default());

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
    }
}
