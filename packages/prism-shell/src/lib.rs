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
//! See `docs/dev/clay-migration-plan.md` §17 (Slint tear-out),
//! §19-22 (slot-typed `AppState` port wave), and §24-27 (service
//! registry + write-side port wave) for the full architectural
//! contract. The legacy `app/`, `panels/`, `luau/`, `signals.rs`,
//! `persistence.rs`, `project.rs`, `search.rs`, `help.rs`, `menu.rs`,
//! `command.rs`, `input.rs`, `keyboard.rs`, `keybindings.rs`,
//! `selection.rs`, `panel_props.rs`, `explorer.rs`, `telemetry.rs`,
//! `testing.rs`, and `e2e.rs` modules — all Slint-era — were
//! deleted in the 2026-05-10 Phase 5 cutover; their behaviour
//! lives entirely in the `services/` registry plus the slot-typed
//! `AppState`.

// `assets` plugs into `prism_ui_runtime::images::AssetLoader`, which
// is only compiled when one of the rendering backends is selected.
// The shell's `native` / `web` features each pull in the matching
// runtime feature; library-only builds (neither feature) stay
// asset-free.
#[cfg(any(feature = "native", feature = "web"))]
pub mod assets;
pub mod components;
pub mod events;
pub mod headless;
pub mod props;
pub mod render;
pub mod render_scope;
pub mod seed;
pub mod services;
pub mod state;

mod shell;

pub use render_scope::RenderScope;
pub use shell::{Shell, ShellError};
pub use state::{
    AppState, BuilderSlot, CanvasSlot, CanvasViewport, ChromeSlot, CodeBuffer, CommandPalette,
    CommandResult, CursorKey, FieldFocus, HandleSide, HelpTooltip, InspectorNode, MenuLabel,
    NavButton, NavEdge, NavEdgeKind, NavPage, NavigationSlot, NumberDrag, OverlaySlot,
    PickerCandidate, PickerState, ProjectSlot, PropertyRow, SchemaDoc, SchemaField, SearchHit,
    SearchSlot, SignalConnection, Toast, ToastKind, ToolMode, TransformSnapshot, WorkspaceSlot,
};

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
