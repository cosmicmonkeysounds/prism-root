//! Panel modules. One file per panel, each exposing a data-provider
//! struct that feeds Slint property values (title, hint text,
//! sidebar actions, etc.). Panels don't own a `Window` anymore —
//! under Slint the root `AppWindow` is built once by [`crate::Shell`]
//! and panels just populate slots on it.
//!
//! Phase 3 lands four panels — Identity (the legacy Phase-0 spike),
//! Builder (live preview of the generated `.slint` DSL), Inspector
//! (indented node-tree dump), and Properties (field-row editor for
//! the currently selected node). Each panel is a pure data provider
//! keyed by the active [`crate::AppState::active_panel`] variant;
//! [`crate::Shell::sync_ui`] dispatches on the variant and pushes the
//! matching props into the Slint window.

pub mod builder;
pub mod editor;
pub mod identity;
pub mod inspector;
pub mod properties;
pub mod signals;

use prism_core::help::HelpEntry;

/// Panel metadata every concrete panel exposes: the title + hint
/// shown in the panel header and the stable id the sidebar uses to
/// dispatch `select_panel(int)` callbacks back into Rust. Kept tiny
/// on purpose — the panel-specific data (builder source, inspector
/// tree, field rows) lives on each panel's own provider struct.
pub trait Panel {
    fn id(&self) -> i32;
    fn label(&self) -> &'static str;
    fn title(&self) -> &'static str;
    fn hint(&self) -> &'static str;

    /// Optional help entry for this panel. Override to provide
    /// context-sensitive tooltip content in the activity bar.
    fn help_entry(&self) -> Option<HelpEntry> {
        None
    }
}
