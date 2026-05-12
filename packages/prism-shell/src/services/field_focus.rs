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
use serde_json::json;

use crate::services::{CommandTable, EventOutcome, MutCtx, ShellService};

/// Wave 2.2 of `docs/dev/composable-builder-plan.md`: arrow-key
/// nudge for the currently focused number / integer field. Reads
/// the focus session's `target_id` + `key`, adds `delta` to the
/// existing prop value, and writes back through `set_node_prop`.
/// `kind == "integer"` rounds to the nearest int; "number" keeps
/// the f64 value.
fn nudge_focused_number(ctx: &mut MutCtx<'_>, delta: f64, kind: &str) {
    let Some(focus) = ctx.state.field_focus.as_ref().cloned() else {
        return;
    };
    let registry = ctx.registry;
    let current = ctx
        .state
        .canvas
        .document
        .root
        .as_ref()
        .and_then(|r| r.find(&focus.target_id))
        .and_then(|n| n.props.get(&focus.key))
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    let next = current + delta;
    let value = if kind == "integer" {
        json!(next.round() as i64)
    } else {
        json!(next)
    };
    let _ = ctx
        .state
        .set_node_prop(&focus.target_id, &focus.key, value, registry);
}

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
                // Wave 2.2: arrow-key nudging for number / integer
                // fields. ±1 by default, ±10 when shift is held.
                // Min / max clamp piggybacks on `set_node_prop`'s
                // `NumberDrag` clamp path indirectly — but since the
                // field-focus path doesn't carry min/max state, we
                // re-read them off the focus prop's `data-min` /
                // `data-max` attrs via the focus session at click time;
                // for now we nudge unbounded, and rely on the
                // drag-scrub path to enforce bounds when scrubbing.
                let focus_kind = ctx
                    .state
                    .field_focus
                    .as_ref()
                    .map(|f| f.kind.clone())
                    .unwrap_or_default();
                if matches!(focus_kind.as_str(), "number" | "integer")
                    && matches!(code.as_str(), "arrowup" | "arrowdown")
                {
                    let step: f64 = if modifiers.shift { 10.0 } else { 1.0 };
                    let signed = if code == "arrowup" { step } else { -step };
                    nudge_focused_number(ctx, signed, &focus_kind);
                    return EventOutcome::Handled;
                }
                match code.as_str() {
                    "backspace" => {
                        ctx.state.backspace_field(registry);
                        EventOutcome::Handled
                    }
                    "enter" => {
                        // Wave 2.1: Shift-Enter inserts a literal
                        // newline for textarea-kind fields (multi-
                        // line text). Plain Enter commits. The
                        // distinction is made on the focus's kind +
                        // the shift modifier, so single-line text
                        // fields still commit on Enter regardless.
                        let is_multiline = ctx
                            .state
                            .field_focus
                            .as_ref()
                            .map(|f| f.kind == "textarea")
                            .unwrap_or(false);
                        if modifiers.shift && is_multiline {
                            ctx.state.type_field_text("\n", registry);
                            EventOutcome::Handled
                        } else {
                            ctx.state.commit_field_focus();
                            EventOutcome::Handled
                        }
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
            modifier_registry: None,
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
            modifier_registry: None,
        };
        let outcome = svc.on_event(&event, &mut ctx, &cmds);
        assert!(matches!(outcome, EventOutcome::Pass));
    }

    /// Wave 2.1: Shift-Enter inserts a literal newline for textarea
    /// fields. Plain Enter still commits.
    #[test]
    fn shift_enter_inserts_newline_for_textarea_kind() {
        let mut state = AppState::default();
        state.canvas.document = seeded_doc();
        state.canvas.selection = Some("target".into());
        state.begin_field_focus("target", "body", "textarea");
        // begin_field_focus prefills the draft with the existing prop
        // value (`"hi"` per seeded_doc), so type_field_text appends.
        state.type_field_text("line1", None);
        let shift_enter = Event::Key {
            code: "enter".into(),
            pressed: true,
            modifiers: Modifiers {
                shift: true,
                ..Default::default()
            },
        };
        fan_out(&mut state, &shift_enter);
        assert!(state.field_focus.is_some(), "Shift-Enter must NOT commit");
        let draft = state.field_focus.as_ref().unwrap().draft.clone();
        assert!(draft.ends_with('\n'), "trailing newline missing: {draft:?}");
        assert!(
            draft.contains("line1\n"),
            "line1 followed by newline: {draft:?}"
        );
        // Plain Enter commits.
        let plain_enter = Event::Key {
            code: "enter".into(),
            pressed: true,
            modifiers: Modifiers::default(),
        };
        fan_out(&mut state, &plain_enter);
        assert!(state.field_focus.is_none());
    }

    /// Wave 2.2: Up/Down arrow on a focused number field nudges
    /// the bound prop by 1, by 10 with shift.
    #[test]
    fn arrow_keys_nudge_focused_number_field() {
        let mut state = AppState::default();
        state.canvas.document = BuilderDocument {
            root: Some(Node {
                id: "root".into(),
                component: "container".into(),
                children: vec![Node {
                    id: "n".into(),
                    component: "container".into(),
                    props: json!({ "padding": 10 }),
                    ..Default::default()
                }],
                ..Default::default()
            }),
            ..Default::default()
        };
        state.canvas.selection = Some("n".into());
        state.begin_field_focus("n", "padding", "integer");

        // ArrowUp: +1
        let up = Event::Key {
            code: "arrowup".into(),
            pressed: true,
            modifiers: Modifiers::default(),
        };
        fan_out(&mut state, &up);
        let pad = state
            .canvas
            .document
            .root
            .as_ref()
            .unwrap()
            .find("n")
            .unwrap()
            .props
            .get("padding")
            .and_then(|v| v.as_i64())
            .unwrap();
        assert_eq!(pad, 11);

        // Shift+ArrowDown: -10
        let shift_down = Event::Key {
            code: "arrowdown".into(),
            pressed: true,
            modifiers: Modifiers {
                shift: true,
                ..Default::default()
            },
        };
        fan_out(&mut state, &shift_down);
        let pad = state
            .canvas
            .document
            .root
            .as_ref()
            .unwrap()
            .find("n")
            .unwrap()
            .props
            .get("padding")
            .and_then(|v| v.as_i64())
            .unwrap();
        assert_eq!(pad, 1);
    }

    /// Wave 2.1: Shift-Enter on a single-line `text` field still
    /// commits — multi-line is opt-in via `kind = "textarea"`.
    #[test]
    fn shift_enter_on_text_kind_still_commits() {
        let mut state = AppState::default();
        state.canvas.document = seeded_doc();
        state.canvas.selection = Some("target".into());
        state.begin_field_focus("target", "body", "text");
        let shift_enter = Event::Key {
            code: "enter".into(),
            pressed: true,
            modifiers: Modifiers {
                shift: true,
                ..Default::default()
            },
        };
        fan_out(&mut state, &shift_enter);
        assert!(
            state.field_focus.is_none(),
            "Shift-Enter commits text fields"
        );
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
