//! Input event pipeline.
//!
//! Every host (tao on native, a wasm-bindgen shim on web) normalizes
//! its events into one of these variants and hands them to
//! [`dispatch`]. The dispatcher updates [`crate::AppState`] and
//! forwards into Clay's `pointer_state` / `update_scroll_containers`
//! calls so layout sees the pointer the same way on every target.

use crate::AppState;
use serde::{Deserialize, Serialize};

#[cfg(feature = "clay")]
use clay_layout::{math::Vector2, Clay};

/// Normalised input event. Hosts translate their native event streams
/// into these variants before calling [`dispatch`]; the shell never
/// touches `tao::Event` or `web_sys::MouseEvent` directly.
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
    /// Wheel delta in *pixels*. Hosts pre-scale line-based deltas
    /// against the current line height before calling in.
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

/// Live pointer + scroll state. Lives inside [`AppState`] so it
/// survives hot reloads like the rest of the shell's runtime data.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct PointerState {
    pub x: f32,
    pub y: f32,
    /// True while the *primary* mouse button is held. Clay only tracks
    /// one button; secondary/middle are stored on [`PointerState`] so
    /// panels can read them without going back through the host.
    pub primary_down: bool,
    pub secondary_down: bool,
    pub middle_down: bool,
    /// Wheel delta accumulated since the last frame. Cleared by the
    /// render loop when it pumps Clay's scroll containers.
    pub scroll_delta_x: f32,
    pub scroll_delta_y: f32,
}

/// Surface size in physical pixels. Mirrors the last `Resize` event.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct SurfaceSize {
    pub width: u32,
    pub height: u32,
}

/// Dispatch a normalised [`InputEvent`] into the shell state. Hosts
/// call this from every native event they want the UI tree to see —
/// dropping an event here means layout will never observe it.
///
/// Returns `true` if the caller should request a redraw. This lets
/// hosts skip the `request_redraw` call on events that don't move the
/// pointer visually (e.g. keyboard repeats with no focused widget).
#[cfg_attr(not(feature = "clay"), allow(unused_variables))]
pub fn dispatch(state: &mut AppState, event: InputEvent) -> bool {
    match event {
        InputEvent::PointerMove { x, y } => {
            state.pointer.x = x;
            state.pointer.y = y;
            true
        }
        InputEvent::PointerDown { x, y, button } => {
            state.pointer.x = x;
            state.pointer.y = y;
            match button {
                PointerButton::Primary => state.pointer.primary_down = true,
                PointerButton::Secondary => state.pointer.secondary_down = true,
                PointerButton::Middle => state.pointer.middle_down = true,
            }
            true
        }
        InputEvent::PointerUp { x, y, button } => {
            state.pointer.x = x;
            state.pointer.y = y;
            match button {
                PointerButton::Primary => state.pointer.primary_down = false,
                PointerButton::Secondary => state.pointer.secondary_down = false,
                PointerButton::Middle => state.pointer.middle_down = false,
            }
            true
        }
        InputEvent::Wheel { dx, dy } => {
            state.pointer.scroll_delta_x += dx;
            state.pointer.scroll_delta_y += dy;
            true
        }
        InputEvent::Key { .. } => {
            // Keyboard routing grows with the panel set in Phase 1 —
            // today there's no focused widget to send anything at, so
            // we still want to redraw so repaint-on-input stays honest.
            true
        }
        InputEvent::Resize { width, height } => {
            state.surface.width = width;
            state.surface.height = height;
            true
        }
    }
}

/// Forward the current [`PointerState`] into Clay's input API and
/// drain the accumulated scroll delta into Clay's scroll containers.
///
/// Call exactly once per frame, after host input has been dispatched
/// and before `Clay::begin`. `delta_time_seconds` feeds Clay's inertial
/// scrolling; pass `0.0` if you don't have a frame timer yet.
#[cfg(feature = "clay")]
pub fn pump_clay(state: &mut AppState, clay: &Clay, delta_time_seconds: f32) {
    clay.pointer_state(
        Vector2::new(state.pointer.x, state.pointer.y),
        state.pointer.primary_down,
    );
    if state.pointer.scroll_delta_x != 0.0 || state.pointer.scroll_delta_y != 0.0 {
        clay.update_scroll_containers(
            true,
            Vector2::new(state.pointer.scroll_delta_x, state.pointer.scroll_delta_y),
            delta_time_seconds.max(0.0),
        );
        state.pointer.scroll_delta_x = 0.0;
        state.pointer.scroll_delta_y = 0.0;
    }
}
