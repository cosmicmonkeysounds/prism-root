//! `FieldFocusService` — owns text-input keyboard routing while
//! `AppState::field_focus` is `Some`.
//!
//! Three event arms:
//!
//! * `Event::Text { text }` — append typed characters to the focused
//!   field's draft and flush to the bound prop.
//! * `Event::Key { code: "backspace" }` — pop one char.
//! * `Event::Key { code: "enter" }` — commit (clear focus).
//! * `Event::Key { code: "escape" }` — cancel (restore the original
//!   value and clear focus).
//!
//! Every other event short-circuits with `EventOutcome::Handled` while
//! focus is active — typing should never bleed into global shortcuts
//! (Ctrl+S etc.) or modal overlays. The service registers *before*
//! `InputService` so it wins the fan-out race.

use prism_ui_runtime::event::Event;

use crate::services::{CommandTable, EventOutcome, MutCtx, ShellService};

#[derive(Default)]
pub struct FieldFocusService;

impl ShellService for FieldFocusService {
    fn id(&self) -> &'static str {
        "field-focus"
    }

    fn on_event(&self, event: &Event, ctx: &mut MutCtx<'_>, _cmds: &CommandTable) -> EventOutcome {
        if ctx.state.field_focus.is_none() {
            return EventOutcome::Pass;
        }
        let registry = ctx.registry;
        match event {
            Event::Text { text } => {
                ctx.state.type_field_text(text, registry);
                EventOutcome::Handled
            }
            Event::Key {
                code,
                pressed: true,
                modifiers,
            } => {
                // Modifier-bearing key combos pass through so global
                // shortcuts (Ctrl+S etc.) still work while typing.
                // Plain Esc/Enter/Backspace are owned by the focus.
                if modifiers.ctrl || modifiers.meta || modifiers.alt {
                    return EventOutcome::Pass;
                }
                match code.as_str() {
                    "backspace" => {
                        ctx.state.backspace_field(registry);
                        EventOutcome::Handled
                    }
                    "enter" => {
                        ctx.state.commit_field_focus();
                        EventOutcome::Handled
                    }
                    "escape" => {
                        ctx.state.cancel_field_focus(registry);
                        EventOutcome::Handled
                    }
                    // Every other plain-key event terminates here so
                    // it can't fire a global shortcut while the user
                    // is in a field — but doesn't write anything,
                    // the matching `Event::Text` arm above does.
                    _ => EventOutcome::Handled,
                }
            }
            _ => EventOutcome::Pass,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::vfs::test_support::InMemVfs;
    use crate::services::{
        register_shell_services, Clipboard, NoopLuauHost, ServiceRegistry, UndoStack,
    };
    use crate::AppState;
    use prism_builder::{BuilderDocument, Node};
    use prism_ui_runtime::event::{Event, Modifiers};
    use prism_ui_runtime::layout::Viewport;
    use serde_json::json;

    fn seeded_doc() -> BuilderDocument {
        BuilderDocument {
            root: Some(Node {
                id: "root".into(),
                component: "container".into(),
                children: vec![Node {
                    id: "target".into(),
                    component: "text".into(),
                    props: json!({ "body": "hi" }),
                    ..Default::default()
                }],
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    fn fan_out(state: &mut AppState, event: &Event) -> EventOutcome {
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
        };
        reg.fan_out(event, &mut ctx)
    }

    fn registered() -> ServiceRegistry {
        let mut reg = ServiceRegistry::new();
        register_shell_services(&mut reg);
        reg
    }

    #[test]
    fn service_registers_under_id_field_focus() {
        let reg = registered();
        assert!(reg.get("field-focus").is_some());
    }

    #[test]
    fn text_event_appends_to_draft_and_updates_prop() {
        let mut state = AppState::default();
        state.canvas.document = seeded_doc();
        state.canvas.selection = Some("target".into());
        state.begin_field_focus("target", "body", "text");
        let out = fan_out(&mut state, &Event::Text { text: "ya".into() });
        assert!(matches!(out, EventOutcome::Handled));
        assert_eq!(state.field_focus.as_ref().unwrap().draft, "hiya");
        let body = state
            .canvas
            .document
            .root
            .as_ref()
            .and_then(|r| r.find("target"))
            .and_then(|n| n.props.get("body"))
            .and_then(|v| v.as_str())
            .unwrap()
            .to_string();
        assert_eq!(body, "hiya");
    }

    #[test]
    fn backspace_pops_one_char_and_flushes() {
        let mut state = AppState::default();
        state.canvas.document = seeded_doc();
        state.canvas.selection = Some("target".into());
        state.begin_field_focus("target", "body", "text");
        let key = Event::Key {
            code: "backspace".into(),
            pressed: true,
            modifiers: Modifiers::default(),
        };
        fan_out(&mut state, &key);
        assert_eq!(state.field_focus.as_ref().unwrap().draft, "h");
    }

    #[test]
    fn enter_commits_and_clears_focus() {
        let mut state = AppState::default();
        state.canvas.document = seeded_doc();
        state.canvas.selection = Some("target".into());
        state.begin_field_focus("target", "body", "text");
        let enter = Event::Key {
            code: "enter".into(),
            pressed: true,
            modifiers: Modifiers::default(),
        };
        fan_out(&mut state, &enter);
        assert!(state.field_focus.is_none());
    }

    #[test]
    fn escape_cancels_and_restores_original() {
        let mut state = AppState::default();
        state.canvas.document = seeded_doc();
        state.canvas.selection = Some("target".into());
        state.begin_field_focus("target", "body", "text");
        // Type something, then Esc.
        fan_out(&mut state, &Event::Text { text: "X".into() });
        fan_out(
            &mut state,
            &Event::Key {
                code: "escape".into(),
                pressed: true,
                modifiers: Modifiers::default(),
            },
        );
        assert!(state.field_focus.is_none());
        let body = state
            .canvas
            .document
            .root
            .as_ref()
            .and_then(|r| r.find("target"))
            .and_then(|n| n.props.get("body"))
            .and_then(|v| v.as_str())
            .unwrap()
            .to_string();
        assert_eq!(body, "hi", "Esc must restore original");
    }

    #[test]
    fn modifier_keys_pass_through_so_global_shortcuts_still_work() {
        // While focus is open, Ctrl+S must reach InputService so the
        // user can save without first cancelling out of the field.
        let mut state = AppState::default();
        state.canvas.document = seeded_doc();
        state.canvas.selection = Some("target".into());
        state.begin_field_focus("target", "body", "text");
        let event = Event::Key {
            code: "s".into(),
            pressed: true,
            modifiers: Modifiers {
                ctrl: true,
                ..Default::default()
            },
        };
        let svc = FieldFocusService;
        let mut undo = UndoStack::default();
        let mut vfs = InMemVfs::default();
        let mut luau = NoopLuauHost::default();
        let mut clipboard = Clipboard::default();
        let cmds = CommandTable::default();
        let mut ctx = MutCtx {
            state: &mut state,
            viewport: Viewport {
                width: 0.0,
                height: 0.0,
            },
            undo: &mut undo,
            vfs: &mut vfs,
            luau: &mut luau,
            clipboard: &mut clipboard,
            registry: None,
        };
        let outcome = svc.on_event(&event, &mut ctx, &cmds);
        assert!(matches!(outcome, EventOutcome::Pass));
    }

    #[test]
    fn no_focus_means_pass_through() {
        let mut state = AppState::default();
        let out = fan_out(&mut state, &Event::Text { text: "x".into() });
        // No service should mark this Handled when nothing modal is
        // open and no focus is active.
        assert!(matches!(out, EventOutcome::Pass));
    }
}
