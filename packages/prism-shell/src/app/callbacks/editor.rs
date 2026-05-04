use std::rc::Rc;

use slint::ComponentHandle;

use super::super::{display_row_to_buffer_line, sync_ui_from_shared, Shell};
use super::with_shell;

impl Shell {
    pub(super) fn wire_editor_callbacks(&self) {
        let weak = self.window.as_weak();
        let inner = Rc::clone(&self.inner);

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
    }
}
