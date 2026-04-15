//! Root application state + Slint binding layer.
//!
//! Everything reloadable lives behind a single [`AppState`] so §7's
//! hot-reload story is exactly one serde call. Mutation goes through
//! the [`Shell`] wrapper, which owns both a
//! `prism_core::Store<AppState>` and the root `AppWindow` Slint
//! handle. [`Shell::sync_ui`] pushes the current `AppState` into
//! Slint properties; store subscribers do the same any time state
//! changes outside the UI thread.

use std::rc::Rc;

use prism_core::design_tokens::{DesignTokens, Rgba, DEFAULT_TOKENS};
use prism_core::shell_mode::{Permission, ShellMode, ShellModeContext};
use prism_core::{Action, Store, Subscription};
use serde::{Deserialize, Serialize};
use slint::{ComponentHandle, Model, ModelRc, SharedString, VecModel};

use crate::input::{self, InputEvent};
use crate::panels::{self, identity::IdentityPanel, Panel};
use crate::{AppWindow, ButtonSpec};

/// The single reloadable root state. Extend by adding fields here;
/// do *not* stash runtime state in `lazy_static`s or `OnceCell`s —
/// that breaks the hot-reload loop.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppState {
    pub tokens: DesignTokens,
    pub context: ShellModeContext,
    pub active_panel: ActivePanel,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ActivePanel {
    Identity,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            tokens: DEFAULT_TOKENS,
            context: ShellModeContext {
                shell_mode: ShellMode::Build,
                permission: Permission::Dev,
            },
            active_panel: ActivePanel::Identity,
        }
    }
}

/// Reducer-side action wrapping a normalised input event. Kept as a
/// bridge between the older free-function [`input::dispatch`] API
/// and the Store so hosts that want structured action dispatch can
/// still feed inputs through `store.dispatch(InputAction(evt))`
/// instead of the Slint callback path.
pub struct InputAction(pub InputEvent);

impl Action<AppState> for InputAction {
    fn apply(self, state: &mut AppState) {
        input::dispatch(state, self.0);
    }
}

/// Owning wrapper around `Store<AppState>` + the root Slint window.
/// Hosts build one on startup and call [`Shell::run`] to hand control
/// to Slint's event loop; the store's subscription bus stays
/// available for non-UI observers (inspectors, IPC bridges).
pub struct Shell {
    store: Store<AppState>,
    window: AppWindow,
}

impl Shell {
    /// Build a shell around `AppState::default()` and a fresh
    /// `AppWindow`. Returns an error if Slint cannot construct the
    /// window (e.g. missing platform backend).
    pub fn new() -> Result<Self, slint::PlatformError> {
        Self::from_state(AppState::default())
    }

    /// Build a shell around a caller-supplied state. Used by tests
    /// and by the hot-reload restore path.
    pub fn from_state(state: AppState) -> Result<Self, slint::PlatformError> {
        let window = AppWindow::new()?;
        let mut shell = Self {
            store: Store::new(state),
            window,
        };
        shell.sync_ui();
        shell.wire_callbacks();
        Ok(shell)
    }

    /// Borrow the current state for read-only access.
    pub fn state(&self) -> &AppState {
        self.store.state()
    }

    /// Mutable access to the underlying store. Expose it so hosts
    /// can register / drop subscribers, dispatch custom actions, or
    /// run hot-reload snapshot/restore without going through a
    /// bespoke API for every call site.
    pub fn store_mut(&mut self) -> &mut Store<AppState> {
        &mut self.store
    }

    /// Borrow the underlying Slint window. Hosts that want to hook
    /// into extra callbacks or pump additional properties can reach
    /// through this.
    pub fn window(&self) -> &AppWindow {
        &self.window
    }

    /// Push a normalised input event through the store. Kept as an
    /// ergonomic helper for tests and hosts that want to drive state
    /// without going through the Slint event loop.
    pub fn dispatch_input(&mut self, event: InputEvent) -> bool {
        let mut redraw = false;
        self.store.mutate(|state| {
            redraw = input::dispatch(state, event);
        });
        self.sync_ui();
        redraw
    }

    /// Subscribe to state changes. Returns a handle the caller can
    /// feed back to [`Shell::unsubscribe`]. Forwards straight to the
    /// underlying [`Store::subscribe`].
    pub fn subscribe<F>(&mut self, listener: F) -> Subscription
    where
        F: FnMut(&AppState) + 'static,
    {
        self.store.subscribe(listener)
    }

    /// Drop a previously registered subscription.
    pub fn unsubscribe(&mut self, subscription: Subscription) {
        self.store.unsubscribe(subscription);
    }

    /// Serialise the current state for §7 hot-reload.
    pub fn snapshot(&self) -> Result<Vec<u8>, serde_json::Error> {
        self.store.snapshot()
    }

    /// Restore state from a snapshot produced by [`Shell::snapshot`].
    /// Notifies subscribers exactly once on success.
    pub fn restore(&mut self, bytes: &[u8]) -> Result<(), serde_json::Error> {
        self.store.restore(bytes)?;
        self.sync_ui();
        Ok(())
    }

    /// Block on Slint's event loop until the window closes. Native
    /// hosts call this from `main`; the WASM entry point in
    /// `lib.rs` also routes through it.
    pub fn run(self) -> Result<(), slint::PlatformError> {
        self.window.run()
    }

    /// Push the current `AppState` into the Slint window's
    /// properties. Called once at construction, after every
    /// dispatch, and again after `restore`.
    fn sync_ui(&mut self) {
        let state = self.store.state();
        let tokens = &state.tokens;

        self.window
            .set_background_color(rgba_to_slint(tokens.colors.background));
        self.window
            .set_sidebar_color(rgba_to_slint(tokens.colors.surface));
        self.window
            .set_button_color(rgba_to_slint(tokens.colors.accent));
        self.window
            .set_text_color(rgba_to_slint(tokens.colors.text_primary));

        match state.active_panel {
            ActivePanel::Identity => {
                let panel = IdentityPanel::new();
                self.window
                    .set_panel_title(SharedString::from(panel.title()));
                self.window.set_panel_hint(SharedString::from(panel.hint()));
                let actions = panel
                    .actions()
                    .iter()
                    .map(|label| ButtonSpec {
                        label: SharedString::from(*label),
                    })
                    .collect::<Vec<_>>();
                let model = Rc::new(VecModel::from(actions));
                self.window
                    .set_actions(ModelRc::from(model as Rc<dyn Model<Data = ButtonSpec>>));
            }
        }
    }

    /// Wire Slint callbacks back into the store. Today only the
    /// sidebar `clicked(int)` callback exists; more will land as
    /// panels grow their interactive surface.
    fn wire_callbacks(&mut self) {
        self.window.on_clicked(move |index| {
            panels::on_sidebar_click(index as usize);
        });
    }
}

fn rgba_to_slint(c: Rgba) -> slint::Color {
    slint::Color::from_argb_u8(c.a, c.r, c.g, c.b)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn try_new_shell() -> Option<Shell> {
        // Slint's event loop requires a platform backend; in `cargo
        // test` runs there may be none (headless CI). Gracefully
        // skip those tests by returning `None`.
        Shell::new().ok()
    }

    #[test]
    fn new_shell_starts_at_default_state() {
        let Some(shell) = try_new_shell() else {
            return;
        };
        assert!(matches!(shell.state().active_panel, ActivePanel::Identity));
    }

    #[test]
    fn snapshot_restore_round_trips_through_shell() {
        let Some(mut shell) = try_new_shell() else {
            return;
        };
        let bytes = shell.snapshot().expect("snapshot");
        shell.restore(&bytes).expect("restore");
        assert!(matches!(shell.state().active_panel, ActivePanel::Identity));
    }

    #[test]
    fn rgba_to_slint_preserves_channels() {
        let c = rgba_to_slint(Rgba::new(0x3c, 0x78, 0xdc, 0xff));
        assert_eq!(c.red(), 0x3c);
        assert_eq!(c.green(), 0x78);
        assert_eq!(c.blue(), 0xdc);
        assert_eq!(c.alpha(), 0xff);
    }
}
