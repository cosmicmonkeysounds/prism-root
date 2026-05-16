//! `FieldFocusService` — owns text-input keyboard routing while
//! `AppState::field_focus` is `Some`.
//!
//! Most of the wiring lives in the shared
//! [`text_input::dispatch_text_input`] helper. This service owns the
//! pieces that are property-row specific:
//!
//! * `Event::Key { code: "enter" }` — commit (clear focus). With
//!   Shift held on a `textarea` / `code` kind, inserts a literal `\n`
//!   instead.
//! * `Event::Key { code: "escape" }` — cancel (restore the original
//!   value and clear focus).
//! * Number / integer field arrow-key nudges (±1, ±10 with shift).
//! * Flush-to-bound-prop after every editor mutation so the live doc
//!   stays in sync without a separate "commit on blur" path.
//!
//! Every other event short-circuits with `EventOutcome::Handled` while
//! focus is active — typing should never bleed into global shortcuts
//! (Ctrl+S etc.) or modal overlays. The service registers *before*
//! `InputService` so it wins the fan-out race.

use prism_ui_runtime::event::Event;
use serde_json::json;

use crate::services::text_input::{dispatch_text_input, TextInputBindings, TextInputOutcome};
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
        let focus_kind = ctx
            .state
            .field_focus
            .as_ref()
            .map(|f| f.kind.clone())
            .unwrap_or_default();

        // Number / integer fields never touch the text editor — their
        // bound prop is an f64, not a string. Drag-scrub / typed
        // digits go through a different path entirely. Handle the
        // tiny per-kind key set inline and short-circuit.
        if matches!(focus_kind.as_str(), "number" | "integer") {
            return handle_number_field(event, ctx, &focus_kind);
        }

        let is_multiline = matches!(focus_kind.as_str(), "textarea" | "code");
        let bindings = TextInputBindings {
            passthrough_modifier_keys: &[],
            // Commit / cancel keys — the dispatch surfaces them as
            // Ignored so we can run our own commit-or-shift-enter
            // logic.
            passthrough_plain_keys: PLAIN_PASSTHROUGH_KEYS,
        };

        // Route through the shared dispatch first. The borrow on
        // `field_focus.editor` ends before we touch `state.set_node_prop`,
        // so the flush-to-prop path can re-borrow `&mut self`.
        let outcome = if let Some(focus) = ctx.state.field_focus.as_mut() {
            dispatch_text_input(event, &mut focus.editor, ctx.clipboard, &bindings)
        } else {
            return EventOutcome::Pass;
        };

        match outcome {
            TextInputOutcome::BufferMutated => {
                ctx.state.flush_focus_to_prop(registry);
                EventOutcome::Handled
            }
            TextInputOutcome::DisplayMutated => EventOutcome::Handled,
            TextInputOutcome::Inert => EventOutcome::Handled,
            TextInputOutcome::PassToGlobal => EventOutcome::Pass,
            // Caller-owned keys surface here — Enter (commit, or
            // shift+Enter newline for textarea/code), Escape (cancel).
            TextInputOutcome::Ignored => {
                let Event::Key {
                    code,
                    pressed: true,
                    modifiers,
                } = event
                else {
                    return EventOutcome::Pass;
                };
                match code.as_str() {
                    "enter" | "return" => {
                        if is_multiline && modifiers.shift {
                            ctx.state.type_field_text("\n", registry);
                        } else {
                            ctx.state.commit_field_focus();
                        }
                        EventOutcome::Handled
                    }
                    "escape" => {
                        ctx.state.cancel_field_focus(registry);
                        EventOutcome::Handled
                    }
                    _ => EventOutcome::Pass,
                }
            }
        }
    }
}

/// Plain (non-modifier) keys the shared dispatch should surface as
/// Ignored so this service can run its own commit / cancel logic.
const PLAIN_PASSTHROUGH_KEYS: &[&str] = &["enter", "return", "escape"];

/// Number / integer field key handler. Arrow keys nudge the bound
/// prop; Enter commits, Escape cancels, everything else is swallowed.
fn handle_number_field(event: &Event, ctx: &mut MutCtx<'_>, focus_kind: &str) -> EventOutcome {
    let Event::Key {
        code,
        pressed: true,
        modifiers,
    } = event
    else {
        // Number fields swallow every other text-shaped event so
        // non-digit chars don't dump into the bound prop.
        return EventOutcome::Handled;
    };
    if matches!(code.as_str(), "arrowup" | "arrowdown") {
        let step: f64 = if modifiers.shift { 10.0 } else { 1.0 };
        let signed = if code == "arrowup" { step } else { -step };
        nudge_focused_number(ctx, signed, focus_kind);
        return EventOutcome::Handled;
    }
    // Modifier-bearing combos (Ctrl+S etc.) need to reach the global
    // shortcuts even with a number field focused.
    if modifiers.ctrl || modifiers.meta || modifiers.alt {
        return EventOutcome::Pass;
    }
    if matches!(code.as_str(), "enter" | "escape") {
        if code == "enter" {
            ctx.state.commit_field_focus();
        } else {
            ctx.state.cancel_field_focus(ctx.registry);
        }
        return EventOutcome::Handled;
    }
    EventOutcome::Handled
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
        assert_eq!(state.field_focus.as_ref().unwrap().draft(), "hiya");
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
        assert_eq!(state.field_focus.as_ref().unwrap().draft(), "h");
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
        let draft = state.field_focus.as_ref().unwrap().draft().to_string();
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

    /// New editor wiring: a focused text field's arrow keys move the
    /// caret inside the draft, they don't bleed into global
    /// shortcuts. The bound prop stays put (caret nav doesn't mutate
    /// text).
    #[test]
    fn arrow_left_inside_focused_text_field_moves_caret_without_mutating_prop() {
        let mut state = AppState::default();
        state.canvas.document = seeded_doc();
        state.canvas.selection = Some("target".into());
        state.begin_field_focus("target", "body", "text");
        // Caret starts at end of original draft ("hi", caret=2).
        let left = Event::Key {
            code: "arrowleft".into(),
            pressed: true,
            modifiers: Modifiers::default(),
        };
        fan_out(&mut state, &left);
        let focus = state.field_focus.as_ref().unwrap();
        assert_eq!(focus.draft(), "hi");
        assert_eq!(focus.caret_byte(), 1);
    }

    /// New editor wiring: Ctrl+A from inside a focused field selects
    /// the entire draft. Subsequent typing replaces it.
    #[test]
    fn ctrl_a_then_type_replaces_focused_field_value() {
        let mut state = AppState::default();
        state.canvas.document = seeded_doc();
        state.canvas.selection = Some("target".into());
        state.begin_field_focus("target", "body", "text");
        let ctrl_a = Event::Key {
            code: "a".into(),
            pressed: true,
            modifiers: Modifiers {
                ctrl: true,
                ..Default::default()
            },
        };
        fan_out(&mut state, &ctrl_a);
        assert_eq!(
            state.field_focus.as_ref().unwrap().selection(),
            Some((0, 2))
        );
        fan_out(&mut state, &Event::Text { text: "X".into() });
        assert_eq!(state.field_focus.as_ref().unwrap().draft(), "X");
        // And the bound prop reflects the replacement (per-keystroke flush).
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
        assert_eq!(body, "X");
    }

    /// New editor wiring: Ctrl+Z undoes a typing run.
    #[test]
    fn ctrl_z_inside_focused_field_undoes_typing() {
        let mut state = AppState::default();
        state.canvas.document = seeded_doc();
        state.canvas.selection = Some("target".into());
        state.begin_field_focus("target", "body", "text");
        fan_out(&mut state, &Event::Text { text: "abc".into() });
        assert_eq!(state.field_focus.as_ref().unwrap().draft(), "hiabc");
        let ctrl_z = Event::Key {
            code: "z".into(),
            pressed: true,
            modifiers: Modifiers {
                ctrl: true,
                ..Default::default()
            },
        };
        fan_out(&mut state, &ctrl_z);
        assert_eq!(state.field_focus.as_ref().unwrap().draft(), "hi");
        // And the prop flushes back to "hi" too.
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
        assert_eq!(body, "hi");
    }

    /// Backspace deletes at the caret, not at the end. Pre-editor
    /// implementation always popped the last char; this is the new
    /// behaviour readers actually expect.
    #[test]
    fn backspace_deletes_at_caret_not_at_end() {
        let mut state = AppState::default();
        state.canvas.document = seeded_doc();
        state.canvas.selection = Some("target".into());
        state.begin_field_focus("target", "body", "text");
        // Type "abc" → draft = "hiabc", caret at end (5).
        fan_out(&mut state, &Event::Text { text: "abc".into() });
        // Move caret left twice → caret at byte 3 (after "hia").
        fan_out(
            &mut state,
            &Event::Key {
                code: "arrowleft".into(),
                pressed: true,
                modifiers: Modifiers::default(),
            },
        );
        fan_out(
            &mut state,
            &Event::Key {
                code: "arrowleft".into(),
                pressed: true,
                modifiers: Modifiers::default(),
            },
        );
        // Backspace removes the "a" at byte 2 (between "hi" and "bc").
        fan_out(
            &mut state,
            &Event::Key {
                code: "backspace".into(),
                pressed: true,
                modifiers: Modifiers::default(),
            },
        );
        assert_eq!(state.field_focus.as_ref().unwrap().draft(), "hibc");
    }

    /// IME preedit shows inline in a focused text field without
    /// mutating the bound prop.
    #[test]
    fn ime_preedit_in_text_field_doesnt_mutate_prop() {
        let mut state = AppState::default();
        state.canvas.document = seeded_doc();
        state.canvas.selection = Some("target".into());
        state.begin_field_focus("target", "body", "text");
        fan_out(
            &mut state,
            &Event::ImePreedit {
                text: "ん".into(),
                cursor_byte: None,
            },
        );
        // The bound prop stays at the original draft.
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
        assert_eq!(body, "hi");
        // Editor surfaces the preedit.
        let focus = state.field_focus.as_ref().unwrap();
        assert!(focus.editor.has_preedit());
    }

    /// IME commit flushes the finalised composition through to the
    /// bound prop.
    #[test]
    fn ime_commit_in_text_field_writes_through_to_prop() {
        let mut state = AppState::default();
        state.canvas.document = seeded_doc();
        state.canvas.selection = Some("target".into());
        state.begin_field_focus("target", "body", "text");
        fan_out(&mut state, &Event::ImeCommit { text: "漢".into() });
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
        assert_eq!(body, "hi漢");
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
