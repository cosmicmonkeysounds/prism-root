//! Root application state + Slint binding layer.
//!
//! Everything reloadable lives behind a single [`AppState`] so §7's
//! hot-reload story is exactly one serde call. Mutation goes through
//! the [`Shell`] wrapper, which owns both a
//! `prism_core::Store<AppState>` and the root `AppWindow` Slint
//! handle. [`Shell::sync_ui`] pushes the current `AppState` into the
//! Slint properties matching the active panel; store subscribers do
//! the same any time state changes outside the UI thread.
//!
//! Phase 3 grew the shell from one panel (Identity) to four — Identity,
//! Builder, Inspector, Properties. The builder/inspector/properties
//! panels all read from a shared [`prism_builder::BuilderDocument`] +
//! [`prism_builder::ComponentRegistry`] owned by the shell. The document
//! is part of `AppState` (so it round-trips through snapshot / restore);
//! the registry is rebuilt from scratch on every boot via
//! [`prism_builder::register_builtins`], which is what [`Shell::from_state`] does.

use std::rc::Rc;
use std::sync::Arc;

use prism_builder::{starter::register_builtins, BuilderDocument, ComponentRegistry, Node, NodeId};
use prism_core::design_tokens::{DesignTokens, Rgba, DEFAULT_TOKENS};
use prism_core::shell_mode::{Permission, ShellMode, ShellModeContext};
use prism_core::{Action, Store, Subscription};
use serde::{Deserialize, Serialize};
use serde_json::json;
use slint::{ComponentHandle, Model, ModelRc, SharedString, VecModel};

use crate::input::{self, InputEvent};
use crate::panels::{
    builder::BuilderPanel, identity::IdentityPanel, inspector::InspectorPanel,
    properties::PropertiesPanel, Panel,
};
use crate::telemetry::FirstPaint;
use crate::{AppWindow, ButtonSpec, FieldRow, PanelNavItem};

/// The single reloadable root state. Extend by adding fields here;
/// do *not* stash runtime state in `lazy_static`s or `OnceCell`s —
/// that breaks the hot-reload loop.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppState {
    pub tokens: DesignTokens,
    pub context: ShellModeContext,
    pub active_panel: ActivePanel,
    /// The builder document every non-identity panel reads from.
    /// Serializable so `Shell::snapshot` / `restore` carries it over
    /// a hot reload.
    pub builder_document: BuilderDocument,
    /// Currently selected node id (used by the Properties panel).
    /// `None` means "nothing selected".
    pub selected_node: Option<NodeId>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ActivePanel {
    Identity,
    Builder,
    Inspector,
    Properties,
}

impl ActivePanel {
    /// Panel id used to bridge between Slint's int callback and the
    /// Rust enum. Matches each panel's `ID` const.
    pub fn as_id(self) -> i32 {
        match self {
            ActivePanel::Identity => IdentityPanel::ID,
            ActivePanel::Builder => BuilderPanel::ID,
            ActivePanel::Inspector => InspectorPanel::ID,
            ActivePanel::Properties => PropertiesPanel::ID,
        }
    }

    /// Reverse of [`Self::as_id`]. Unknown ids fall back to Identity
    /// so an out-of-range click can't put the shell into an invalid
    /// state.
    pub fn from_id(id: i32) -> Self {
        match id {
            x if x == BuilderPanel::ID => ActivePanel::Builder,
            x if x == InspectorPanel::ID => ActivePanel::Inspector,
            x if x == PropertiesPanel::ID => ActivePanel::Properties,
            _ => ActivePanel::Identity,
        }
    }
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
            builder_document: sample_document(),
            selected_node: Some("hero".into()),
        }
    }
}

/// A small starter document so the Builder / Inspector / Properties
/// panels have something to render before any real editing has
/// happened. Mirrors the shape the Sovereign Portal relay seeds its
/// sample "welcome" portal with, so both crates exercise the same
/// component ids end-to-end.
fn sample_document() -> BuilderDocument {
    BuilderDocument {
        root: Some(Node {
            id: "root".into(),
            component: "container".into(),
            props: json!({ "spacing": 16 }),
            children: vec![
                Node {
                    id: "hero".into(),
                    component: "heading".into(),
                    props: json!({ "text": "Welcome to Prism", "level": 1 }),
                    children: vec![],
                },
                Node {
                    id: "intro".into(),
                    component: "text".into(),
                    props: json!({
                        "body": "The distributed visual operating system. Pick a panel to start editing."
                    }),
                    children: vec![],
                },
            ],
        }),
        zones: Default::default(),
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

/// Reducer-side action that switches the active panel. Wired up to
/// the Slint sidebar's `select_panel(int)` callback.
pub struct SelectPanel(pub ActivePanel);

impl Action<AppState> for SelectPanel {
    fn apply(self, state: &mut AppState) {
        state.active_panel = self.0;
    }
}

/// Owning wrapper around `Store<AppState>` + the root Slint window +
/// the component registry. Hosts build one on startup and call
/// [`Shell::run`] to hand control to Slint's event loop; the store's
/// subscription bus stays available for non-UI observers.
pub struct Shell {
    store: Store<AppState>,
    window: AppWindow,
    telemetry: FirstPaint,
    /// Component catalog backing the Builder + Properties panels.
    /// Wrapped in `Arc` so callback closures can share it cheaply
    /// without forcing `Shell` itself to be `Clone`.
    registry: Arc<ComponentRegistry>,
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
        // Start the first-paint timer *before* touching Slint so the
        // measured window actually reflects boot cost (window
        // construction, property sync, callback wiring).
        let telemetry = FirstPaint::start();
        let window = AppWindow::new()?;
        let mut registry = ComponentRegistry::new();
        register_builtins(&mut registry).expect("starter components must register");
        let mut shell = Self {
            store: Store::new(state),
            window,
            telemetry,
            registry: Arc::new(registry),
        };
        shell.sync_ui();
        shell.wire_callbacks();
        Ok(shell)
    }

    /// Borrow the current state for read-only access.
    pub fn state(&self) -> &AppState {
        self.store.state()
    }

    /// Borrow the shell's component registry. Exposed so panels
    /// that register new component types (tests, late-bound plugin
    /// hosts) can look up the existing catalog without rebuilding it.
    pub fn registry(&self) -> &ComponentRegistry {
        &self.registry
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

    /// Clone-handle to the first-paint telemetry slot. Hosts that
    /// drive a Slint rendering notifier themselves can hand the clone
    /// to the notifier closure; [`Shell::run`] wires this up
    /// automatically for the standard native + WASM entry points.
    pub fn telemetry(&self) -> FirstPaint {
        self.telemetry.clone()
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

    /// Switch the active panel and push the new panel's data into
    /// the Slint window. Hosts can call this directly or drive it
    /// via the `select_panel(int)` Slint callback.
    pub fn select_panel(&mut self, panel: ActivePanel) {
        self.store.mutate(|state| {
            state.active_panel = panel;
        });
        self.sync_ui();
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
    ///
    /// Installs a Slint rendering notifier that records first paint
    /// into [`Shell::telemetry`] the moment the first frame is
    /// presented. The closure logs a one-shot boot line (`prism-shell:
    /// first-paint Xms`) so the dev loop surfaces the measurement
    /// without requiring a structured-logging dep.
    pub fn run(self) -> Result<(), slint::PlatformError> {
        let telemetry = self.telemetry.clone();
        let already_logged = Rc::new(std::cell::Cell::new(false));
        if let Err(err) = self.window.window().set_rendering_notifier({
            let telemetry = telemetry.clone();
            move |state, _api| {
                if matches!(state, slint::RenderingState::AfterRendering)
                    && !telemetry.is_recorded()
                {
                    telemetry.record_first_paint();
                    if !already_logged.replace(true) {
                        if let Some(d) = telemetry.duration() {
                            eprintln!("prism-shell: first-paint {}ms", d.as_millis());
                        }
                    }
                }
            }
        }) {
            eprintln!("prism-shell: first-paint telemetry unavailable on this backend: {err}");
        }
        self.window.run()
    }

    /// Push the current `AppState` into the Slint window's
    /// properties. Called once at construction, after every
    /// dispatch, and again after `restore`.
    fn sync_ui(&mut self) {
        let state = self.store.state();
        let tokens = &state.tokens;

        // Design-token-backed palette.
        self.window
            .set_background_color(rgba_to_slint(tokens.colors.background));
        self.window
            .set_sidebar_color(rgba_to_slint(tokens.colors.surface));
        self.window
            .set_sidebar_item_color(rgba_to_slint(tokens.colors.surface_elevated));
        self.window
            .set_sidebar_item_selected_color(rgba_to_slint(tokens.colors.accent));
        self.window
            .set_surface_color(rgba_to_slint(tokens.colors.surface_elevated));
        self.window
            .set_button_color(rgba_to_slint(tokens.colors.accent));
        self.window
            .set_text_color(rgba_to_slint(tokens.colors.text_primary));
        self.window
            .set_text_muted_color(rgba_to_slint(tokens.colors.text_secondary));

        // Sidebar nav — one entry per panel, with `selected` bound
        // to the current `active_panel`.
        let nav_items = nav_model(state.active_panel);
        let nav_model = Rc::new(VecModel::from(nav_items));
        self.window.set_nav_items(ModelRc::from(
            nav_model as Rc<dyn Model<Data = PanelNavItem>>,
        ));

        // Panel title + hint come from each panel's static metadata.
        let (title, hint): (&'static str, &'static str) = match state.active_panel {
            ActivePanel::Identity => {
                let p = IdentityPanel::new();
                (p.title(), p.hint())
            }
            ActivePanel::Builder => {
                let p = BuilderPanel::new();
                (p.title(), p.hint())
            }
            ActivePanel::Inspector => {
                let p = InspectorPanel::new();
                (p.title(), p.hint())
            }
            ActivePanel::Properties => {
                let p = PropertiesPanel::new();
                (p.title(), p.hint())
            }
        };
        self.window.set_panel_title(SharedString::from(title));
        self.window.set_panel_hint(SharedString::from(hint));

        // Panel-specific data. Only the active panel's slot is
        // populated — every other slot is cleared so the Slint `if`
        // guards in `ui/app.slint` hide unrelated surfaces.
        self.clear_panel_slots();
        match state.active_panel {
            ActivePanel::Identity => self.push_identity_actions(),
            ActivePanel::Builder => {
                self.push_builder_source(&state.builder_document, tokens);
            }
            ActivePanel::Inspector => {
                self.push_inspector_tree(&state.builder_document);
            }
            ActivePanel::Properties => {
                self.push_property_rows(&state.builder_document, &state.selected_node);
            }
        }
    }

    fn clear_panel_slots(&self) {
        let empty_actions: Rc<VecModel<ButtonSpec>> =
            Rc::new(VecModel::from(Vec::<ButtonSpec>::new()));
        self.window.set_actions(ModelRc::from(
            empty_actions as Rc<dyn Model<Data = ButtonSpec>>,
        ));
        self.window.set_builder_source(SharedString::new());
        self.window.set_builder_node_count(0);
        self.window.set_inspector_tree(SharedString::new());
        self.window.set_selected_component(SharedString::new());
        let empty_rows: Rc<VecModel<FieldRow>> = Rc::new(VecModel::from(Vec::<FieldRow>::new()));
        self.window
            .set_field_rows(ModelRc::from(empty_rows as Rc<dyn Model<Data = FieldRow>>));
    }

    fn push_identity_actions(&self) {
        let panel = IdentityPanel::new();
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

    fn push_builder_source(&self, doc: &BuilderDocument, tokens: &DesignTokens) {
        let source = BuilderPanel::source(doc, &self.registry, tokens);
        self.window.set_builder_source(SharedString::from(source));
        self.window
            .set_builder_node_count(BuilderPanel::node_count(doc) as i32);
    }

    fn push_inspector_tree(&self, doc: &BuilderDocument) {
        let tree = InspectorPanel::tree(doc);
        self.window.set_inspector_tree(SharedString::from(tree));
    }

    fn push_property_rows(&self, doc: &BuilderDocument, selected: &Option<NodeId>) {
        let component_id = PropertiesPanel::selected_component(doc, selected);
        self.window
            .set_selected_component(SharedString::from(component_id));
        let rows = PropertiesPanel::rows(doc, &self.registry, selected)
            .into_iter()
            .map(|r| FieldRow {
                key: SharedString::from(r.key),
                label: SharedString::from(r.label),
                kind: SharedString::from(r.kind),
                value: SharedString::from(r.value),
                required: r.required,
            })
            .collect::<Vec<_>>();
        let model = Rc::new(VecModel::from(rows));
        self.window
            .set_field_rows(ModelRc::from(model as Rc<dyn Model<Data = FieldRow>>));
    }

    /// Wire Slint callbacks back into the store. The sidebar owns
    /// a `select_panel(int)` callback that flips `AppState::active_panel`
    /// through a weak-`AppWindow` handle; the identity panel owns a
    /// `clicked(int)` callback that just logs for now.
    fn wire_callbacks(&mut self) {
        let weak = self.window.as_weak();
        let registry = Arc::clone(&self.registry);
        self.window.on_select_panel(move |id| {
            let Some(window) = weak.upgrade() else {
                return;
            };
            let panel = ActivePanel::from_id(id);
            push_active_panel(&window, &registry, panel);
        });
        self.window.on_clicked(move |index| {
            eprintln!("prism-shell: identity-panel click index={index}");
        });
    }
}

/// Build the sidebar nav model with `selected` set on the currently
/// active panel.
fn nav_model(active: ActivePanel) -> Vec<PanelNavItem> {
    let entries = [
        (
            IdentityPanel::ID,
            IdentityPanel::new().label(),
            ActivePanel::Identity,
        ),
        (
            BuilderPanel::ID,
            BuilderPanel::new().label(),
            ActivePanel::Builder,
        ),
        (
            InspectorPanel::ID,
            InspectorPanel::new().label(),
            ActivePanel::Inspector,
        ),
        (
            PropertiesPanel::ID,
            PropertiesPanel::new().label(),
            ActivePanel::Properties,
        ),
    ];
    entries
        .into_iter()
        .map(|(id, label, panel)| PanelNavItem {
            id,
            label: SharedString::from(label),
            selected: panel == active,
        })
        .collect()
}

/// Slint callback helper — patches the window's panel-specific
/// properties without going through the full `Shell::sync_ui`. Used
/// by the `select_panel` callback which only has a weak handle to
/// the window and a clone of the registry, not a `&mut Shell`.
fn push_active_panel(window: &AppWindow, registry: &ComponentRegistry, panel: ActivePanel) {
    // Refresh nav highlight.
    let nav_items = nav_model(panel);
    let nav_model_rc = Rc::new(VecModel::from(nav_items));
    window.set_nav_items(ModelRc::from(
        nav_model_rc as Rc<dyn Model<Data = PanelNavItem>>,
    ));

    // Clear every panel slot, then fill in the one the user picked.
    let empty_actions: Rc<VecModel<ButtonSpec>> = Rc::new(VecModel::from(Vec::<ButtonSpec>::new()));
    window.set_actions(ModelRc::from(
        empty_actions as Rc<dyn Model<Data = ButtonSpec>>,
    ));
    window.set_builder_source(SharedString::new());
    window.set_builder_node_count(0);
    window.set_inspector_tree(SharedString::new());
    window.set_selected_component(SharedString::new());
    let empty_rows: Rc<VecModel<FieldRow>> = Rc::new(VecModel::from(Vec::<FieldRow>::new()));
    window.set_field_rows(ModelRc::from(empty_rows as Rc<dyn Model<Data = FieldRow>>));

    let (title, hint): (&'static str, &'static str) = match panel {
        ActivePanel::Identity => {
            let p = IdentityPanel::new();
            let actions = p
                .actions()
                .iter()
                .map(|label| ButtonSpec {
                    label: SharedString::from(*label),
                })
                .collect::<Vec<_>>();
            let model = Rc::new(VecModel::from(actions));
            window.set_actions(ModelRc::from(model as Rc<dyn Model<Data = ButtonSpec>>));
            (p.title(), p.hint())
        }
        ActivePanel::Builder => {
            let p = BuilderPanel::new();
            let doc = sample_document();
            let source = BuilderPanel::source(&doc, registry, &DEFAULT_TOKENS);
            window.set_builder_source(SharedString::from(source));
            window.set_builder_node_count(BuilderPanel::node_count(&doc) as i32);
            (p.title(), p.hint())
        }
        ActivePanel::Inspector => {
            let p = InspectorPanel::new();
            let doc = sample_document();
            let tree = InspectorPanel::tree(&doc);
            window.set_inspector_tree(SharedString::from(tree));
            (p.title(), p.hint())
        }
        ActivePanel::Properties => {
            let p = PropertiesPanel::new();
            let doc = sample_document();
            let selected = Some("hero".to_string());
            let component_id = PropertiesPanel::selected_component(&doc, &selected);
            window.set_selected_component(SharedString::from(component_id));
            let rows = PropertiesPanel::rows(&doc, registry, &selected)
                .into_iter()
                .map(|r| FieldRow {
                    key: SharedString::from(r.key),
                    label: SharedString::from(r.label),
                    kind: SharedString::from(r.kind),
                    value: SharedString::from(r.value),
                    required: r.required,
                })
                .collect::<Vec<_>>();
            let model = Rc::new(VecModel::from(rows));
            window.set_field_rows(ModelRc::from(model as Rc<dyn Model<Data = FieldRow>>));
            (p.title(), p.hint())
        }
    };
    window.set_panel_title(SharedString::from(title));
    window.set_panel_hint(SharedString::from(hint));
}

fn rgba_to_slint(c: Rgba) -> slint::Color {
    slint::Color::from_argb_u8(c.a, c.r, c.g, c.b)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `Shell::new` builds an `AppWindow`, and Slint's winit event
    /// loop must run on the main thread on macOS — `cargo test`
    /// workers panic the moment they touch it. These tests stay on
    /// the pure data + store surface and leave the actual
    /// `Shell::run` path for the integration bin at
    /// `src/bin/native.rs`.
    #[test]
    fn default_app_state_starts_on_identity_panel() {
        let state = AppState::default();
        assert!(matches!(state.active_panel, ActivePanel::Identity));
    }

    #[test]
    fn default_state_seeds_sample_document() {
        let state = AppState::default();
        assert!(state.builder_document.root.is_some());
        assert_eq!(state.selected_node.as_deref(), Some("hero"));
    }

    #[test]
    fn store_snapshot_restore_round_trips_app_state() {
        let mut store: Store<AppState> = Store::new(AppState::default());
        store.mutate(|s| s.active_panel = ActivePanel::Builder);
        let bytes = store.snapshot().expect("snapshot");
        let mut fresh: Store<AppState> = Store::new(AppState::default());
        fresh.restore(&bytes).expect("restore");
        assert!(matches!(fresh.state().active_panel, ActivePanel::Builder));
        assert!(fresh.state().builder_document.root.is_some());
    }

    #[test]
    fn active_panel_roundtrips_through_id() {
        for panel in [
            ActivePanel::Identity,
            ActivePanel::Builder,
            ActivePanel::Inspector,
            ActivePanel::Properties,
        ] {
            assert_eq!(ActivePanel::from_id(panel.as_id()), panel);
        }
    }

    #[test]
    fn unknown_panel_id_falls_back_to_identity() {
        assert_eq!(ActivePanel::from_id(999), ActivePanel::Identity);
        assert_eq!(ActivePanel::from_id(-1), ActivePanel::Identity);
    }

    #[test]
    fn select_panel_action_mutates_state() {
        let mut store: Store<AppState> = Store::new(AppState::default());
        store.dispatch(SelectPanel(ActivePanel::Properties));
        assert!(matches!(
            store.state().active_panel,
            ActivePanel::Properties
        ));
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
