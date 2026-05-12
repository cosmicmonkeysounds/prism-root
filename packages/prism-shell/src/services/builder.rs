//! `BuilderService` — commands wired to the builder-toolbar /
//! inspector-row chrome buttons. Owns three command families that all
//! act on `CanvasSlot` (or `NavigationSlot`):
//!
//! * **Tree edits** — `builder.move-selected-up`,
//!   `builder.move-selected-down`, `builder.delete-selected`.
//!   Dispatched from the inspector-row chevron / trash buttons when a
//!   tree node is selected.
//! * **Viewport edits** — `view.zoom-in`, `view.zoom-out`. The +/−
//!   icon buttons in the builder-toolbar cluster fire these; the
//!   `100%` pill in the same cluster keeps its declarative
//!   `data-role="toolbar-zoom-reset"` route because reset has no
//!   schema-driven step.
//! * **Alignment** — `builder.align-left`, `builder.align-center`,
//!   `builder.align-right`. Set the `text-align` prop on the
//!   selected canvas node.
//! * **Navigation** — `navigation.add-page`. The menu-bar-row "+"
//!   button fires this; the slot appends a fresh untitled page and
//!   activates it.
//!
//! Each handler returns no value; the existing `route_on_click` path
//! treats any registered command as having "acted" and triggers a
//! redraw. Where the command is a no-op against current state (no
//! selection, zoom already clamped) the dispatch still succeeds — the
//! command body decides whether the mutation is meaningful.

use crate::cmd;
use crate::services::{CommandSpec, ShellService};

#[derive(Default)]
pub struct BuilderService;

impl ShellService for BuilderService {
    fn id(&self) -> &'static str {
        "builder"
    }

    fn commands(&self) -> Vec<CommandSpec> {
        vec![
            cmd!(
                "builder.move-selected-up",
                "Move Selection Up in Tree",
                "Edit",
                |ctx| {
                    if ctx.state.canvas.reorder_selection(-1) {
                        ctx.state.resync_builder_for_selection(ctx.registry);
                    }
                }
            ),
            cmd!(
                "builder.move-selected-down",
                "Move Selection Down in Tree",
                "Edit",
                |ctx| {
                    if ctx.state.canvas.reorder_selection(1) {
                        ctx.state.resync_builder_for_selection(ctx.registry);
                    }
                }
            ),
            cmd!(
                "builder.delete-selected",
                "Delete Selection",
                "Edit",
                |ctx| {
                    ctx.state.canvas.delete_selection();
                    ctx.state.resync_builder_for_selection(ctx.registry);
                }
            ),
            cmd!("builder.align-left", "Align Left", "Edit", |ctx| {
                if ctx.state.canvas.set_selection_align("left") {
                    ctx.state.resync_builder_for_selection(ctx.registry);
                }
            }),
            cmd!("builder.align-center", "Align Center", "Edit", |ctx| {
                if ctx.state.canvas.set_selection_align("center") {
                    ctx.state.resync_builder_for_selection(ctx.registry);
                }
            }),
            cmd!("builder.align-right", "Align Right", "Edit", |ctx| {
                if ctx.state.canvas.set_selection_align("right") {
                    ctx.state.resync_builder_for_selection(ctx.registry);
                }
            }),
            cmd!("view.zoom-in", "Zoom In", "View", |ctx| {
                ctx.state.canvas.zoom_by(1.25);
            }),
            cmd!("view.zoom-out", "Zoom Out", "View", |ctx| {
                ctx.state.canvas.zoom_by(0.8);
            }),
            cmd!("navigation.add-page", "Add Page", "Navigation", |ctx| {
                ctx.state.navigation.add_page();
            }),
            cmd!(
                "navigation.move-page-up",
                "Move Page Up",
                "Navigation",
                |ctx| {
                    ctx.state.navigation.reorder_selected(-1);
                }
            ),
            cmd!(
                "navigation.move-page-down",
                "Move Page Down",
                "Navigation",
                |ctx| {
                    ctx.state.navigation.reorder_selected(1);
                }
            ),
            cmd!(
                "navigation.delete-selected-page",
                "Delete Selected Page",
                "Navigation",
                |ctx| {
                    ctx.state.navigation.delete_selected();
                }
            ),
            cmd!(
                "schema.delete-selected-field",
                "Delete Selected Schema Field",
                "Schema",
                |ctx| {
                    ctx.state.builder.delete_selected_schema_field();
                }
            ),
            cmd!(
                "signals.delete-selected-connection",
                "Delete Selected Connection",
                "Signals",
                |ctx| {
                    ctx.state.builder.delete_selected_signal_connection();
                }
            ),
            // Wave 4.3 — connection picker open/close/cycle/confirm.
            // Each command is a single mutator call; the click
            // dispatch happens through `route_on_click` so the
            // picker rows + footer button thread their `command`
            // attribute through the existing chain.
            cmd!(
                "signals.open-connection-picker",
                "Add Connection",
                "Signals",
                |ctx| {
                    ctx.state.open_connection_picker();
                }
            ),
            cmd!(
                "signals.close-connection-picker",
                "Close Connection Picker",
                "Signals",
                |ctx| {
                    ctx.state.close_connection_picker();
                }
            ),
            cmd!(
                "signals.cycle-connection-picker-action-kind",
                "Cycle Connection Action Kind",
                "Signals",
                |ctx| {
                    ctx.state.cycle_connection_picker_action_kind();
                }
            ),
            cmd!(
                "signals.confirm-connection-picker",
                "Add Connection",
                "Signals",
                |ctx| {
                    ctx.state.confirm_connection_picker(ctx.registry);
                }
            ),
        ]
    }
}

#[cfg(test)]
mod tests {
    use crate::services::vfs::test_support::InMemVfs;
    use crate::services::{Clipboard, MutCtx, NoopLuauHost, ServiceRegistry, UndoStack};
    use crate::AppState;
    use prism_builder::{BuilderDocument, Node};
    use prism_ui_runtime::layout::Viewport;

    fn three_child_doc() -> BuilderDocument {
        BuilderDocument {
            root: Some(Node {
                id: "root".into(),
                component: "container".into(),
                children: vec![
                    Node {
                        id: "a".into(),
                        component: "text".into(),
                        ..Default::default()
                    },
                    Node {
                        id: "b".into(),
                        component: "text".into(),
                        ..Default::default()
                    },
                    Node {
                        id: "c".into(),
                        component: "text".into(),
                        ..Default::default()
                    },
                ],
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    fn run(state: &mut AppState, id: &str) -> bool {
        let reg = ServiceRegistry::with_builtins();
        let mut undo = UndoStack::default();
        let mut vfs = InMemVfs::default();
        let mut luau = NoopLuauHost::default();
        let mut clipboard = Clipboard::default();
        let mut ctx = MutCtx {
            state,
            viewport: Viewport {
                width: 0.0,
                height: 0.0,
            },
            undo: &mut undo,
            vfs: &mut vfs,
            luau: &mut luau,
            clipboard: &mut clipboard,
            registry: None,
            modifier_registry: None,
        };
        reg.commands().run(id, &mut ctx)
    }

    fn sibling_ids(state: &AppState) -> Vec<String> {
        state
            .canvas
            .document
            .root
            .as_ref()
            .unwrap()
            .children
            .iter()
            .map(|n| n.id.clone())
            .collect()
    }

    #[test]
    fn move_selected_up_swaps_with_previous_sibling() {
        let mut state = AppState::default();
        state.canvas.document = three_child_doc();
        state.canvas.selection = Some("b".into());
        assert!(run(&mut state, "builder.move-selected-up"));
        assert_eq!(sibling_ids(&state), vec!["b", "a", "c"]);
    }

    #[test]
    fn move_selected_down_swaps_with_next_sibling() {
        let mut state = AppState::default();
        state.canvas.document = three_child_doc();
        state.canvas.selection = Some("b".into());
        assert!(run(&mut state, "builder.move-selected-down"));
        assert_eq!(sibling_ids(&state), vec!["a", "c", "b"]);
    }

    #[test]
    fn move_selected_up_at_first_position_is_a_no_op() {
        let mut state = AppState::default();
        state.canvas.document = three_child_doc();
        state.canvas.selection = Some("a".into());
        assert!(run(&mut state, "builder.move-selected-up"));
        assert_eq!(sibling_ids(&state), vec!["a", "b", "c"]);
    }

    #[test]
    fn delete_selected_removes_node_and_clears_cursor() {
        let mut state = AppState::default();
        state.canvas.document = three_child_doc();
        state.canvas.selection = Some("b".into());
        assert!(run(&mut state, "builder.delete-selected"));
        assert_eq!(sibling_ids(&state), vec!["a", "c"]);
        assert!(state.canvas.selection.is_none());
    }

    #[test]
    fn align_commands_set_text_align_prop_on_selection() {
        let mut state = AppState::default();
        state.canvas.document = three_child_doc();
        state.canvas.selection = Some("a".into());
        for (cmd_id, expected) in [
            ("builder.align-left", "left"),
            ("builder.align-center", "center"),
            ("builder.align-right", "right"),
        ] {
            assert!(run(&mut state, cmd_id));
            let node = state
                .canvas
                .document
                .root
                .as_ref()
                .unwrap()
                .find("a")
                .unwrap();
            assert_eq!(
                node.props.get("text-align").and_then(|v| v.as_str()),
                Some(expected),
                "after `{cmd_id}`"
            );
        }
    }

    #[test]
    fn zoom_in_and_out_step_the_canvas_viewport_zoom() {
        let mut state = AppState::default();
        let start = state.canvas.viewport.zoom;
        assert!(run(&mut state, "view.zoom-in"));
        assert!(state.canvas.viewport.zoom > start);
        assert!(run(&mut state, "view.zoom-out"));
        // 1.0 * 1.25 * 0.8 = 1.0 — should round-trip back to start.
        assert!((state.canvas.viewport.zoom - start).abs() < 1e-3);
    }

    #[test]
    fn zoom_clamps_at_the_schema_declared_bounds() {
        let mut state = AppState::default();
        // Push past the upper bound (8.0).
        state.canvas.viewport.zoom = 8.0;
        run(&mut state, "view.zoom-in");
        assert_eq!(state.canvas.viewport.zoom, 8.0);
        // Push past the lower bound (0.1).
        state.canvas.viewport.zoom = 0.1;
        run(&mut state, "view.zoom-out");
        assert_eq!(state.canvas.viewport.zoom, 0.1);
    }

    #[test]
    fn delete_selected_schema_field_removes_cursor_row_and_clears_cursor() {
        use crate::state::{SchemaDoc, SchemaField};
        let mut state = AppState::default();
        state.builder.schema = SchemaDoc {
            fields: vec![
                SchemaField {
                    name: "title".into(),
                    kind: "text".into(),
                    required: false,
                },
                SchemaField {
                    name: "body".into(),
                    kind: "rich-text".into(),
                    required: false,
                },
            ],
            ..Default::default()
        };
        assert!(state.builder.select_schema_field("body"));
        assert!(run(&mut state, "schema.delete-selected-field"));
        let remaining: Vec<String> = state
            .builder
            .schema
            .fields
            .iter()
            .map(|f| f.name.clone())
            .collect();
        assert_eq!(remaining, vec!["title"]);
        assert!(state.builder.schema.selected_field.is_none());
    }

    #[test]
    fn delete_selected_schema_field_with_no_cursor_is_a_no_op() {
        let mut state = AppState::default();
        // No fields, no cursor — dispatch still succeeds, document untouched.
        assert!(run(&mut state, "schema.delete-selected-field"));
        assert!(state.builder.schema.fields.is_empty());
    }

    #[test]
    fn delete_selected_signal_connection_removes_cursor_row_and_clears_cursor() {
        use crate::state::SignalConnection;
        let mut state = AppState::default();
        state.builder.signal_connections.push(SignalConnection {
            id: "c1".into(),
            source_signal: "clicked".into(),
            action_kind: "EmitSignal".into(),
            target_label: "x".into(),
        });
        state.builder.signal_connections.push(SignalConnection {
            id: "c2".into(),
            source_signal: "hovered".into(),
            action_kind: "SetProperty".into(),
            target_label: "y".into(),
        });
        assert!(state.builder.select_signal_connection("c1"));
        assert!(run(&mut state, "signals.delete-selected-connection"));
        let remaining: Vec<String> = state
            .builder
            .signal_connections
            .iter()
            .map(|c| c.id.clone())
            .collect();
        assert_eq!(remaining, vec!["c2"]);
        assert!(state.builder.selected_connection.is_none());
    }

    #[test]
    fn navigation_add_page_appends_and_activates() {
        let mut state = AppState::default();
        assert!(run(&mut state, "navigation.add-page"));
        assert_eq!(state.navigation.pages.len(), 1);
        assert!(state.navigation.pages[0].is_active);
        assert!(run(&mut state, "navigation.add-page"));
        assert_eq!(state.navigation.pages.len(), 2);
        // Only the latest page is active.
        assert!(!state.navigation.pages[0].is_active);
        assert!(state.navigation.pages[1].is_active);
        // Ids are stable and non-colliding.
        assert_ne!(state.navigation.pages[0].id, state.navigation.pages[1].id);
    }
}
