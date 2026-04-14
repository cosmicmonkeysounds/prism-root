//! Input event pipeline.
//!
//! Every host (winit on native, a wasm-bindgen shim on web) normalizes
//! its events into one of these variants and hands them to
//! [`dispatch`]. The dispatcher then forwards to Clay's input API and
//! updates [`crate::AppState`] accordingly.

use crate::AppState;

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

#[derive(Debug, Clone, Copy)]
pub enum PointerButton {
    Primary,
    Secondary,
    Middle,
}

pub fn dispatch(_state: &mut AppState, _event: InputEvent) {
    // TODO(phase0-spike-1): forward into Clay's input API once the
    // binding is wired. For now we just accept and drop.
}
