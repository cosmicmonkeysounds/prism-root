use std::rc::Rc;

use slint::ComponentHandle;

use super::super::{
    apply_page_layout_edit, deserialize_addr, execute_command, is_preview_mode, panel_id_for_slint,
    sync_ui_from_shared, Shell,
};
use super::with_shell;
use crate::input::{combo_from_slint, update_panel_schemes};

impl Shell {
    pub(super) fn wire_chrome_callbacks(&self) {
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
