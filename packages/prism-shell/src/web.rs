//! Web entry point. Called from a `trunk`-built bundle; wires a
//! `<canvas>` element into the shared layout + input pipeline.
//!
//! Stub until Phase 0 spike #1 wires the Clay HTML / Canvas2D
//! renderer. The `#[wasm_bindgen(start)]` attribute is the thing
//! `wasm-pack` looks for.

use wasm_bindgen::prelude::*;

use crate::AppState;

#[wasm_bindgen(start)]
pub fn start() -> Result<(), JsValue> {
    console_error_panic_hook::set_once();
    let state = AppState::default();
    let _count = crate::render_app(&state);
    // TODO(phase0-spike-1): attach canvas, forward DOM events.
    Ok(())
}
