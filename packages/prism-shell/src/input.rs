//! Programmatic input event helper.
//!
//! Slint owns the native input pipeline (keyboard, pointer, scroll,
//! touch) and routes events directly through the Slint component
//! tree. This module is kept as a thin Rust-side bridge for two
//! remaining consumers:
//!
//! * tests that want to poke `AppState` without spinning up a full
//!   Slint window, and
//! * future custom panels that dispatch synthetic events through
//!   the store for determinism.
//!
//! Phase 0 has no reducer-visible input state left on `AppState`, so
//! `dispatch` is a no-op. It stays in the public surface so we have
//! a well-named place to grow panel-specific action routing when
//! Phase 1 panels start needing keyboard-driven state.

use crate::AppState;

/// Normalised input event. Slint delivers native events directly to
/// the component tree, but this enum is still useful for feeding the
/// store from tests or from a future action-dispatch bridge.
#[derive(Debug, Clone, Copy)]
pub enum InputEvent {
    PointerMove {
        x: f32,
        y: f32,
    },
    PointerDown {
        x: f32,
        y: f32,
        button: PointerButton,
    },
    PointerUp {
        x: f32,
        y: f32,
        button: PointerButton,
    },
    Wheel {
        dx: f32,
        dy: f32,
    },
    Key {
        code: u32,
        pressed: bool,
    },
    Resize {
        width: u32,
        height: u32,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PointerButton {
    Primary,
    Secondary,
    Middle,
}

/// Dispatch a synthetic [`InputEvent`] into `state`. Returns `true`
/// if the caller should request a redraw (always `true` today — the
/// helper is a stub until panel-level routing lands).
pub fn dispatch(_state: &mut AppState, _event: InputEvent) -> bool {
    true
}
