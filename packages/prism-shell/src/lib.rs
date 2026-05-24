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
pub mod app_loader;
pub mod app_registry;
#[cfg(any(feature = "native", feature = "web"))]
pub mod assets;
pub mod components;
pub mod editor_help;
pub mod events;
pub mod headless;
/// C3 closure — see `docs/dev/ui-migration-followups.md`.
#[cfg(feature = "native")]
pub mod hot_reload;
/// Wave H.1/H.6 — filesystem-backed `ImportResolver` (sibling
/// pairing + relative + `prism://` roots) for `prui-luau-fusion.md`
/// §5.4 / §5.9.
#[cfg(not(target_arch = "wasm32"))]
pub mod import_resolver;
/// Wave 7.2 — software rasteriser for the `--screenshot` PNG path.
/// Walks a `RenderCommand` stream into an RGBA buffer the `image`
/// crate's PNG encoder consumes.
pub mod png_paint;
/// Project Vault — folder-backed persistent object graph. See
/// `docs/dev/project-vault.md`. Native-only: rides the `crdt`
/// stack the `native` feature pulls in.
#[cfg(feature = "native")]
pub mod project_manager;
pub mod props;
pub mod render;
pub mod render_scope;
pub mod seed;
pub mod services;
/// A2 closure — see `docs/dev/ui-migration-followups.md`.
pub mod skeleton_bindings;
pub mod state;

mod shell;

#[cfg(feature = "native")]
pub use project_manager::ProjectManager;
pub use render::{Skeleton, Stylesheet, StylesheetReload, StylesheetWatcher};
pub use render_scope::RenderScope;
#[cfg(feature = "native")]
pub use shell::ProbeFire;
pub use shell::{Shell, ShellError, ShellInner};
pub use state::{
    AppState, BuilderSlot, CanvasSlot, CanvasViewport, ChromeSlot, CodeBuffer, CommandPalette,
    CommandResult, CursorKey, FieldFocus, HandleSide, HelpTooltip, InspectorNode, MenuLabel,
    NavButton, NavEdge, NavEdgeKind, NavPage, NavigationSlot, NumberDrag, OverlaySlot,
    PickerCandidate, PickerState, ProjectSlot, PropertyRow, SchemaDoc, SchemaField, SearchHit,
    SearchSlot, SignalConnection, Toast, ToastKind, ToolMode, TransformSnapshot, WorkspaceSlot,
};

/// Browser entry point. Intentionally **not** marked `(start)`: the
/// HTML loader awaits `init()` and then calls [`set_luau_invoker`]
/// (handing in a JS function that trampolines to the emscripten
/// daemon module) before invoking `web_start()`. That ordering is
/// what lets `Shell::new` install [`crate::services::JsLuauHost`]
/// instead of falling back to [`crate::services::NoopLuauHost`].
///
/// Calling `web_start()` without first calling `set_luau_invoker` is
/// supported — the shell boots with [`crate::services::NoopLuauHost`]
/// and `luau.exec` calls are recorded but no-op. That keeps a
/// daemon-less preview working.
#[cfg(all(feature = "web", target_arch = "wasm32"))]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn web_start() -> Result<(), wasm_bindgen::JsValue> {
    console_error_panic_hook::set_once();
    let shell = Shell::new().map_err(|e| wasm_bindgen::JsValue::from_str(&e.to_string()))?;
    shell
        .run()
        .map_err(|e| wasm_bindgen::JsValue::from_str(&e.to_string()))?;
    Ok(())
}

/// Stash a JS-side daemon invoker so [`Shell::new`] can construct a
/// [`crate::services::JsLuauHost`]. The function signature on the JS
/// side is `(commandName: string, payloadJson: string) -> string`,
/// returning the daemon's `{ok, result|error}` envelope as JSON text.
/// See `web/index.html` for the canonical bootstrap that loads the
/// emscripten daemon module and registers a `cwrap`'d
/// `prism_daemon_invoke` here.
///
/// The function is stored in a `thread_local` cell; the wasm module
/// is single-threaded, so the cell never races. Calling this a second
/// time replaces the previous invoker (useful for tests / hot-reload).
#[cfg(all(feature = "web", target_arch = "wasm32"))]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn set_luau_invoker(invoker: js_sys::Function) {
    luau_invoker_slot::install(invoker);
}

/// Process-wide slot for the browser daemon invoker. Single-threaded
/// by construction (wasm modules don't share thread state across
/// workers without explicit `SharedArrayBuffer`), so a `thread_local`
/// `RefCell` is sound.
#[cfg(all(feature = "web", target_arch = "wasm32"))]
pub(crate) mod luau_invoker_slot {
    use std::cell::RefCell;

    thread_local! {
        static SLOT: RefCell<Option<js_sys::Function>> = const { RefCell::new(None) };
    }

    pub fn install(invoker: js_sys::Function) {
        SLOT.with(|cell| cell.borrow_mut().replace(invoker));
    }

    pub fn take() -> Option<js_sys::Function> {
        SLOT.with(|cell| cell.borrow_mut().take())
    }
}
