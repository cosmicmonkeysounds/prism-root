//! Panel modules. One file per panel, each exposing a data-provider
//! struct that feeds Slint property values (title, hint text,
//! sidebar actions, etc.). Panels don't own a `Window` anymore —
//! under Slint the root `AppWindow` is built once by [`crate::Shell`]
//! and panels just populate slots on it.
//!
//! The legacy TS tree had ~40 of these; Phase 1 adds them back one
//! at a time behind a new [`ActivePanel`] variant per panel.
//!
//! [`ActivePanel`]: crate::AppState::active_panel

pub mod identity;

/// Panel surface: the minimum each panel needs to provide to drive
/// the Slint `AppWindow` properties. Kept tiny in Phase 0; grows
/// with the sidebar/content split once Phase 1 introduces more than
/// one active panel.
pub trait Panel {
    fn title(&self) -> &'static str;
    fn hint(&self) -> &'static str;
    fn actions(&self) -> &'static [&'static str];
}

/// Panel-level sidebar click router. Slint's `clicked(int)`
/// callback hits this with the index of the tapped button. Phase 0
/// only logs; Phase 1 dispatches into `Store<AppState>`.
pub fn on_sidebar_click(index: usize) {
    eprintln!("prism-shell: sidebar click index={index}");
}
