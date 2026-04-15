//! Root application state + top-level layout.
//!
//! Everything reloadable lives behind a single [`AppState`] so §7's
//! hot-reload story is exactly one serde call. Mutation goes
//! through the [`Shell`] wrapper, which owns a
//! `prism_core::Store<AppState>` and routes every input event /
//! hot-reload snapshot through it so subscribers (the renderer,
//! the IPC bridge, inspector overlays) see every change.

use prism_core::design_tokens::{DesignTokens, DEFAULT_TOKENS};
use prism_core::shell_mode::{Permission, ShellMode, ShellModeContext};
use prism_core::{Action, Store, Subscription};
use serde::{Deserialize, Serialize};

use crate::input::{self, InputEvent, PointerState, SurfaceSize};

#[cfg(feature = "clay")]
use clay_layout::{math::Dimensions, render_commands::RenderCommand, Clay};

/// The single reloadable root state. Extend by adding fields here;
/// do *not* stash runtime state in `lazy_static`s or `OnceCell`s —
/// that breaks the hot-reload loop.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppState {
    pub tokens: DesignTokens,
    pub context: ShellModeContext,
    pub active_panel: ActivePanel,
    pub pointer: PointerState,
    pub surface: SurfaceSize,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
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
            pointer: PointerState::default(),
            surface: SurfaceSize::default(),
        }
    }
}

/// Reducer-side action wrapping a normalised input event. The input
/// pipeline already has a free-function [`input::dispatch`] that
/// mutates [`AppState`] in place — this action just bridges it into
/// the [`Store`] dispatch loop so subscribers see the mutation.
pub struct InputAction(pub InputEvent);

impl Action<AppState> for InputAction {
    fn apply(self, state: &mut AppState) {
        input::dispatch(state, self.0);
    }
}

/// Owning wrapper around `Store<AppState>`. Hosts (the native dev
/// bin, the Studio entry point, the WASM shim) build one of these
/// on startup and route every input event / redraw tick through it.
/// The store's subscription bus is where renderers hook in.
pub struct Shell {
    store: Store<AppState>,
}

impl Shell {
    /// Build a shell around `AppState::default()`.
    pub fn new() -> Self {
        Self {
            store: Store::new(AppState::default()),
        }
    }

    /// Build a shell around a caller-supplied state. Used by tests
    /// and by the hot-reload restore path.
    pub fn from_state(state: AppState) -> Self {
        Self {
            store: Store::new(state),
        }
    }

    /// Borrow the current state for read-only access (e.g. feeding
    /// [`render_app`]).
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

    /// Push a normalised input event through the store. Returns the
    /// "should redraw" hint [`input::dispatch`] produces so hosts
    /// can skip the `request_redraw` call on no-op events.
    pub fn dispatch_input(&mut self, event: InputEvent) -> bool {
        let mut redraw = false;
        self.store.mutate(|state| {
            redraw = input::dispatch(state, event);
        });
        redraw
    }

    /// Subscribe to state changes. Returns a handle the caller can
    /// feed back to [`Shell::unsubscribe`]. Forwards straight to
    /// the underlying [`Store::subscribe`].
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
        self.store.restore(bytes)
    }
}

impl Default for Shell {
    fn default() -> Self {
        Self::new()
    }
}

/// Declare the active panel into a Clay layout and return the
/// resulting render commands. Called by every backend (native
/// wgpu, the future WASM canvas) once per frame.
///
/// The returned `Vec` borrows from `clay` — Clay owns the backing
/// string storage that [`RenderCommand::Text`] variants point into,
/// so the caller must consume the commands before the next
/// `render_app` call.
#[cfg(feature = "clay")]
pub fn render_app<'clay>(
    state: &AppState,
    clay: &'clay mut Clay,
) -> Vec<RenderCommand<'clay, (), ()>> {
    use crate::panels::{identity::IdentityPanel, Panel};

    let mut scope: clay_layout::ClayLayoutScope<'clay, 'clay, (), ()> = clay.begin();

    match state.active_panel {
        ActivePanel::Identity => IdentityPanel::new().declare(state, &mut scope),
    }

    scope.end().collect()
}

/// Install a placeholder text measurement callback on a fresh `Clay`.
///
/// Clay's C core panics on any layout containing text unless a measurer
/// is registered. Phase 0 has no glyph shaper wired up yet, so we cheat:
/// width ≈ `chars * font_size * 0.5`, height == `font_size`. The real
/// glyphon-backed measurer lands with the wgpu renderer in task #2.
#[cfg(feature = "clay")]
pub fn install_stub_text_measurer(clay: &mut Clay) {
    clay.set_measure_text_function(|text, config| {
        let font_size = config.font_size as f32;
        Dimensions::new(text.chars().count() as f32 * font_size * 0.5, font_size)
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::PointerButton;
    use std::cell::RefCell;
    use std::rc::Rc;

    #[test]
    fn new_shell_starts_at_default_state() {
        let shell = Shell::new();
        assert!(matches!(shell.state().active_panel, ActivePanel::Identity));
        assert_eq!(shell.state().pointer.x, 0.0);
        assert_eq!(shell.state().surface.width, 0);
    }

    #[test]
    fn dispatch_input_updates_state_and_notifies_subscribers() {
        let mut shell = Shell::new();
        let seen = Rc::new(RefCell::new(0usize));
        let seen_clone = seen.clone();
        shell.subscribe(move |_| *seen_clone.borrow_mut() += 1);

        let redraw = shell.dispatch_input(InputEvent::PointerMove { x: 100.0, y: 200.0 });

        assert!(redraw);
        assert_eq!(shell.state().pointer.x, 100.0);
        assert_eq!(shell.state().pointer.y, 200.0);
        assert_eq!(*seen.borrow(), 1);
    }

    #[test]
    fn dispatch_input_tracks_button_state() {
        let mut shell = Shell::new();
        shell.dispatch_input(InputEvent::PointerDown {
            x: 10.0,
            y: 20.0,
            button: PointerButton::Primary,
        });
        assert!(shell.state().pointer.primary_down);
        shell.dispatch_input(InputEvent::PointerUp {
            x: 10.0,
            y: 20.0,
            button: PointerButton::Primary,
        });
        assert!(!shell.state().pointer.primary_down);
    }

    #[test]
    fn resize_event_updates_surface() {
        let mut shell = Shell::new();
        shell.dispatch_input(InputEvent::Resize {
            width: 1280,
            height: 800,
        });
        assert_eq!(shell.state().surface.width, 1280);
        assert_eq!(shell.state().surface.height, 800);
    }

    #[test]
    fn unsubscribe_stops_notifications() {
        let mut shell = Shell::new();
        let calls = Rc::new(RefCell::new(0usize));
        let calls_clone = calls.clone();
        let sub = shell.subscribe(move |_| *calls_clone.borrow_mut() += 1);

        shell.dispatch_input(InputEvent::PointerMove { x: 1.0, y: 1.0 });
        shell.unsubscribe(sub);
        shell.dispatch_input(InputEvent::PointerMove { x: 2.0, y: 2.0 });

        assert_eq!(*calls.borrow(), 1);
    }

    #[test]
    fn snapshot_restore_round_trips_through_shell() {
        let mut shell = Shell::new();
        shell.dispatch_input(InputEvent::Resize {
            width: 1920,
            height: 1080,
        });
        shell.dispatch_input(InputEvent::PointerMove { x: 640.0, y: 480.0 });
        let bytes = shell.snapshot().expect("snapshot");

        shell.dispatch_input(InputEvent::Resize {
            width: 100,
            height: 100,
        });
        assert_eq!(shell.state().surface.width, 100);

        shell.restore(&bytes).expect("restore");
        assert_eq!(shell.state().surface.width, 1920);
        assert_eq!(shell.state().surface.height, 1080);
        assert_eq!(shell.state().pointer.x, 640.0);
    }

    #[test]
    fn restore_notifies_subscribers_once() {
        let mut shell = Shell::new();
        let bytes = shell.snapshot().unwrap();
        let calls = Rc::new(RefCell::new(0usize));
        let calls_clone = calls.clone();
        shell.subscribe(move |_| *calls_clone.borrow_mut() += 1);

        shell.restore(&bytes).expect("restore");

        assert_eq!(*calls.borrow(), 1);
    }
}
