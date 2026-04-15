//! `tool` — canvas tool-mode FSM (Hand / Select / Edit).
//!
//! Port of `kernel/state-machine/tool.machine.ts` at 8426588. The legacy
//! TS version used xstate; the Slint migration plan (§11) pencilled in a
//! `statig` rewrite for parity. In practice the tool FSM is three states
//! and six events, so it drops onto the already-ported flat
//! [`machine::Machine`] without pulling a new dependency into the tree —
//! saving us a runtime crate plus the macro-heavy `statig` setup cost.
//!
//! **States**
//! - `Hand` — pan / zoom the canvas, no node interaction
//! - `Select` — click to select nodes, drag to multi-select (initial)
//! - `Edit` — click into node content (CodeMirror, markdown)
//!
//! **Events**
//! - `SwitchHand`   — from Select or Edit → Hand
//! - `SwitchSelect` — from Hand or Edit → Select
//! - `SwitchEdit`   — from Hand or Select → Edit
//! - `DoubleClickNode` — from Select → Edit
//! - `ClickCanvas` — from Edit → Select
//! - `PressEscape` — from Hand or Edit → Select

use super::machine::{Machine, MachineDefinition, StateNode, Transition, TransitionFrom};

/// Canvas tool mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ToolMode {
    Hand,
    Select,
    Edit,
}

impl ToolMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            ToolMode::Hand => "hand",
            ToolMode::Select => "select",
            ToolMode::Edit => "edit",
        }
    }
}

/// Input events for the tool FSM.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ToolEvent {
    SwitchHand,
    SwitchSelect,
    SwitchEdit,
    DoubleClickNode,
    ClickCanvas,
    PressEscape,
}

/// Build the tool-mode [`MachineDefinition`]. Initial state is
/// [`ToolMode::Select`] — mirrors the legacy xstate machine.
pub fn tool_machine_definition() -> MachineDefinition<ToolMode, ToolEvent> {
    MachineDefinition {
        initial: ToolMode::Select,
        states: vec![
            StateNode::new(ToolMode::Hand),
            StateNode::new(ToolMode::Select),
            StateNode::new(ToolMode::Edit),
        ],
        transitions: vec![
            // From Hand
            Transition::new(
                TransitionFrom::One(ToolMode::Hand),
                ToolEvent::SwitchSelect,
                ToolMode::Select,
            ),
            Transition::new(
                TransitionFrom::One(ToolMode::Hand),
                ToolEvent::SwitchEdit,
                ToolMode::Edit,
            ),
            Transition::new(
                TransitionFrom::One(ToolMode::Hand),
                ToolEvent::PressEscape,
                ToolMode::Select,
            ),
            // From Select
            Transition::new(
                TransitionFrom::One(ToolMode::Select),
                ToolEvent::SwitchHand,
                ToolMode::Hand,
            ),
            Transition::new(
                TransitionFrom::One(ToolMode::Select),
                ToolEvent::SwitchEdit,
                ToolMode::Edit,
            ),
            Transition::new(
                TransitionFrom::One(ToolMode::Select),
                ToolEvent::DoubleClickNode,
                ToolMode::Edit,
            ),
            // From Edit
            Transition::new(
                TransitionFrom::One(ToolMode::Edit),
                ToolEvent::SwitchHand,
                ToolMode::Hand,
            ),
            Transition::new(
                TransitionFrom::One(ToolMode::Edit),
                ToolEvent::SwitchSelect,
                ToolMode::Select,
            ),
            Transition::new(
                TransitionFrom::One(ToolMode::Edit),
                ToolEvent::PressEscape,
                ToolMode::Select,
            ),
            Transition::new(
                TransitionFrom::One(ToolMode::Edit),
                ToolEvent::ClickCanvas,
                ToolMode::Select,
            ),
        ],
    }
}

/// Start a running tool machine at its default initial state
/// ([`ToolMode::Select`]).
pub fn create_tool_machine() -> Machine<ToolMode, ToolEvent> {
    Machine::start(tool_machine_definition())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starts_in_select() {
        let m = create_tool_machine();
        assert_eq!(*m.state(), ToolMode::Select);
    }

    #[test]
    fn select_to_hand_to_edit_to_select() {
        let mut m = create_tool_machine();
        assert!(m.send(&ToolEvent::SwitchHand));
        assert_eq!(*m.state(), ToolMode::Hand);
        assert!(m.send(&ToolEvent::SwitchEdit));
        assert_eq!(*m.state(), ToolMode::Edit);
        assert!(m.send(&ToolEvent::PressEscape));
        assert_eq!(*m.state(), ToolMode::Select);
    }

    #[test]
    fn double_click_jumps_select_to_edit() {
        let mut m = create_tool_machine();
        assert!(m.send(&ToolEvent::DoubleClickNode));
        assert_eq!(*m.state(), ToolMode::Edit);
    }

    #[test]
    fn click_canvas_from_edit_returns_to_select() {
        let mut m = create_tool_machine();
        m.send(&ToolEvent::SwitchEdit);
        assert!(m.send(&ToolEvent::ClickCanvas));
        assert_eq!(*m.state(), ToolMode::Select);
    }

    #[test]
    fn double_click_rejected_from_hand() {
        let mut m = create_tool_machine();
        m.send(&ToolEvent::SwitchHand);
        assert!(!m.send(&ToolEvent::DoubleClickNode));
        assert_eq!(*m.state(), ToolMode::Hand);
    }

    #[test]
    fn click_canvas_noop_from_select() {
        let mut m = create_tool_machine();
        assert!(!m.send(&ToolEvent::ClickCanvas));
        assert_eq!(*m.state(), ToolMode::Select);
    }

    #[test]
    fn press_escape_from_select_is_noop() {
        let mut m = create_tool_machine();
        assert!(!m.send(&ToolEvent::PressEscape));
        assert_eq!(*m.state(), ToolMode::Select);
    }

    #[test]
    fn str_representation_matches_legacy() {
        assert_eq!(ToolMode::Hand.as_str(), "hand");
        assert_eq!(ToolMode::Select.as_str(), "select");
        assert_eq!(ToolMode::Edit.as_str(), "edit");
    }
}
