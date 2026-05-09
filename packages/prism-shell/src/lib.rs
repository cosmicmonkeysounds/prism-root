//! `prism-shell` — single source of truth for the Prism UI tree.
//!
//! Renders through `prism-ui-runtime` (winit + femtovg native, wasm32 +
//! webgl browser). The host-runtime contract is three modules:
//!
//! - [`props`] — `ShellPropBindings` registration table mirroring
//!   [`components::register_shell_builtins`].
//! - [`render`] — per-frame `render_tree`: bindings.snapshot →
//!   fill_compositions → lower.
//! - [`events`] — single `dispatch_event` router from
//!   `prism_ui_runtime::event::Event` into `ShellInner` mutations.
//!
//! See `docs/dev/clay-migration-plan.md` §17 for the full
//! architectural contract.
//!
//! ## Migration status
//!
//! The 2026-05-09 Slint tear-out (§17) deleted `ui/app.slint`,
//! `app/sync/`, `app/callbacks/`, every Slint dep, and the
//! `bind_model!` block in one stroke. The legacy feature surface
//! (`app::commands`, `app::mutations`, `panels::*`, `panel_props`,
//! `signals`, `input`, `command`, `keyboard`, `keybindings`, `menu`,
//! `search`, `selection`, `persistence`, `project`, `explorer`,
//! `help`, `telemetry`, `testing`, `e2e`, `luau`) is *on disk but
//! not in the build* until each module is ported to the new
//! `Shell` / `Surface` / `ShellInner` shape. Re-add them to the
//! `pub mod` list below as each port lands.

pub mod components;
pub mod events;
pub mod props;
pub mod render;

mod shell;

pub use shell::{Shell, ShellError};

/// Minimal placeholder for the per-frame state every binding closure
/// reads. The legacy `app::AppState` (with dock workspace, selection
/// model, command palette, toasts, …) is on disk in `src/app/mod.rs`
/// and will be re-introduced module-by-module as each panel is ported
/// against the new shell shape.
#[derive(Default, Clone)]
pub struct AppState;

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
