//! `prism-shell` — single source of truth for the Prism UI tree.
//!
//! Every Studio panel, the page builder, every lens — all of it lives
//! behind a single Slint component tree whose properties are bound
//! from the reloadable [`AppState`] struct. The [`Shell`] wrapper
//! owns both a `prism_core::Store<AppState>` and the root `AppWindow`
//! instance and funnels Slint callbacks back into the store so
//! subscribers (inspector overlays, the IPC bridge, future panels)
//! see every mutation.
//!
//! Phase 0 ships one hard-coded panel (`panels::identity`) with a
//! sidebar of three buttons. Panels grow fan-out-style in Phase 1;
//! adding one means: register its data in `panels/<name>.rs`, wire
//! it into the dock workspace, and add a branch in `Shell::sync_ui`.
//!
//! ## Backend choice
//!
//! Slint (1.8) owns layout, renderer, and windowing. The native
//! build runs on winit + femtovg; the web build runs on winit's
//! wasm32-unknown-unknown backend, also via femtovg over WebGL.

pub mod app;
pub mod command;
pub mod e2e;
pub mod explorer;
pub mod help;
pub mod input;
pub mod keyboard;
pub mod menu;
pub mod panels;
pub mod search;
pub mod selection;
pub mod telemetry;
pub mod testing;

pub use app::{AppState, Shell};
pub use command::{CommandEntry, CommandRegistry};
pub use input::{combo_from_slint, FocusRegion, InputManager, InputScheme, InputSchemeBuilder};
pub use keyboard::{KeyBinding, KeyCombo, KeyboardModel, Modifiers};
pub use search::{SearchIndex, SearchResult};
pub use selection::SelectionModel;
pub use telemetry::FirstPaint;

// `slint::include_modules!()` inlines the Rust code generated from
// `ui/app.slint` by `build.rs`. It exposes `AppWindow` + the
// `ButtonSpec` struct used by the sidebar model.
slint::include_modules!();

/// Browser entry point. `wasm-bindgen` calls this automatically via
/// its `(start)` attribute so the HTML loader only has to import the
/// generated JS module and invoke `init()`.
#[cfg(all(feature = "web", target_arch = "wasm32"))]
#[wasm_bindgen::prelude::wasm_bindgen(start)]
pub fn web_start() -> Result<(), wasm_bindgen::JsValue> {
    console_error_panic_hook::set_once();
    let shell = Shell::new().map_err(|e| wasm_bindgen::JsValue::from_str(&e.to_string()))?;
    shell
        .run()
        .map_err(|e| wasm_bindgen::JsValue::from_str(&e.to_string()))?;
    Ok(())
}
