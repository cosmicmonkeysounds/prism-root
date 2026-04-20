//! Visual testing harness for prism-shell.
//!
//! Provides predefined test scenes, screenshot capture, and input
//! simulation so the builder canvas can be visually verified without
//! manual click-through. Used by:
//!
//! - `prism-shell --scene <name>` — direct scene loading
//! - `prism test visual` — automated screenshot regression suite
//! - `#[cfg(test)]` harness tests — headless state assertions
//!
//! # Quick start
//!
//! ```bash
//! # List available scenes
//! cargo run -p prism-shell -- --scene list
//!
//! # Open builder with Lattice app at tablet viewport
//! cargo run -p prism-shell -- --app lattice --panel builder --viewport tablet
//!
//! # Take a screenshot of a scene and exit
//! cargo run -p prism-shell -- --scene builder-grid --screenshot /tmp/grid.png
//!
//! # Run the full visual test suite
//! cargo run -p prism-cli -- test visual
//! ```

use slint::ComponentHandle;

use crate::app::{AppState, ShellView};
use crate::Shell;

// ── Built-in scenes ──────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuiltinScene {
    Launchpad,
    BuilderEmpty,
    BuilderGrid,
    BuilderTablet,
    BuilderMobile,
    Inspector,
    CodeEditor,
    Explorer,
}

impl BuiltinScene {
    pub fn all() -> &'static [BuiltinScene] {
        &[
            BuiltinScene::Launchpad,
            BuiltinScene::BuilderEmpty,
            BuiltinScene::BuilderGrid,
            BuiltinScene::BuilderTablet,
            BuiltinScene::BuilderMobile,
            BuiltinScene::Inspector,
            BuiltinScene::CodeEditor,
            BuiltinScene::Explorer,
        ]
    }

    pub fn name(self) -> &'static str {
        match self {
            BuiltinScene::Launchpad => "launchpad",
            BuiltinScene::BuilderEmpty => "builder-empty",
            BuiltinScene::BuilderGrid => "builder-grid",
            BuiltinScene::BuilderTablet => "builder-tablet",
            BuiltinScene::BuilderMobile => "builder-mobile",
            BuiltinScene::Inspector => "inspector",
            BuiltinScene::CodeEditor => "code-editor",
            BuiltinScene::Explorer => "explorer",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            BuiltinScene::Launchpad => "App launcher home screen",
            BuiltinScene::BuilderEmpty => "Builder panel with empty page shell (Lattice app)",
            BuiltinScene::BuilderGrid => "Builder panel with grid content (Lattice app, page 1)",
            BuiltinScene::BuilderTablet => "Builder at 768px tablet viewport",
            BuiltinScene::BuilderMobile => "Builder at 375px mobile viewport",
            BuiltinScene::Inspector => "Inspector panel with selected node",
            BuiltinScene::CodeEditor => "Code editor panel",
            BuiltinScene::Explorer => "File explorer panel",
        }
    }

    pub fn by_name(name: &str) -> Option<BuiltinScene> {
        Self::all().iter().find(|s| s.name() == name).copied()
    }
}

/// Build an `AppState` configured for the given scene.
pub fn apply_scene(scene: BuiltinScene) -> AppState {
    let mut state = AppState::default();

    match scene {
        BuiltinScene::Launchpad => {}

        BuiltinScene::BuilderEmpty => {
            enter_first_app(&mut state);
            state.workspace.switch_page_by_id("edit");
            state.builder_document = prism_builder::BuilderDocument::page_shell();
        }

        BuiltinScene::BuilderGrid => {
            enter_first_app(&mut state);
            state.workspace.switch_page_by_id("edit");
            state.sync_document_from_app_pub();
        }

        BuiltinScene::BuilderTablet => {
            enter_first_app(&mut state);
            state.workspace.switch_page_by_id("edit");
            state.viewport_width = 768.0;
            state.sync_document_from_app_pub();
        }

        BuiltinScene::BuilderMobile => {
            enter_first_app(&mut state);
            state.workspace.switch_page_by_id("edit");
            state.viewport_width = 375.0;
            state.sync_document_from_app_pub();
        }

        BuiltinScene::Inspector => {
            enter_first_app(&mut state);
            state.workspace.switch_page_by_id("edit");
            state.sync_document_from_app_pub();
            if let Some(root) = &state.builder_document.root {
                let first_id = root.id.clone();
                state.selection.select(first_id);
            }
        }

        BuiltinScene::CodeEditor => {
            enter_first_app(&mut state);
            state.workspace.switch_page_by_id("code");
        }

        BuiltinScene::Explorer => {
            enter_first_app(&mut state);
            state.workspace.switch_page_by_id("edit");
        }
    }

    state
}

fn enter_first_app(state: &mut AppState) {
    if let Some(app) = state.apps.first() {
        let app_id = app.id.clone();
        state.shell_view = ShellView::App { app_id };
        state.sync_document_from_app_pub();
    }
}

// ── Screenshot capture ───────────────────────────────────────────

pub struct ShellScreenshot;

impl ShellScreenshot {
    /// Schedule a screenshot after the first paint, then exit.
    ///
    /// Uses a Slint timer to wait for the window to render, then
    /// invokes the platform screenshot tool. On macOS this uses
    /// `screencapture`; on other platforms it falls back to
    /// window-level capture if available.
    pub fn schedule_and_exit(shell: &Shell, path: &str) {
        use slint::{Timer, TimerMode};
        let path = path.to_string();
        let weak = shell.window().as_weak();
        let timer = Timer::default();
        timer.start(
            TimerMode::SingleShot,
            std::time::Duration::from_millis(2000),
            move || {
                if let Some(_w) = weak.upgrade() {
                    let ok = capture_screenshot(&path);
                    if ok {
                        eprintln!("Screenshot saved: {path}");
                    } else {
                        eprintln!("Screenshot capture failed: {path}");
                    }
                }
                slint::quit_event_loop().ok();
            },
        );
        std::mem::forget(timer);
    }

    /// Take a screenshot of the current screen to the given path.
    /// Non-blocking utility for test harnesses.
    pub fn capture(path: &str) -> bool {
        capture_screenshot(path)
    }
}

#[cfg(target_os = "macos")]
fn capture_screenshot(path: &str) -> bool {
    std::process::Command::new("screencapture")
        .args(["-x", path])
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

#[cfg(not(target_os = "macos"))]
fn capture_screenshot(path: &str) -> bool {
    eprintln!("Screenshot capture not implemented for this platform. Path: {path}");
    false
}

// ── Input simulation ─────────────────────────────────────────────

/// Input simulation helpers for test harnesses.
///
/// These work by invoking the same Slint callbacks and command
/// dispatch paths that real input uses — no OS-level event injection
/// needed.
pub struct TestInput;

impl TestInput {
    /// Simulate a keyboard shortcut by dispatching through the
    /// shell's command system. Equivalent to the user pressing the
    /// key combo.
    ///
    /// ```text
    /// TestInput::send_command(&shell, "edit.undo");
    /// TestInput::send_command(&shell, "view.zoom_in");
    /// ```
    pub fn send_command(shell: &Shell, command_id: &str) {
        let window = shell.window();
        window.invoke_menu_command(command_id.into());
    }

    /// Simulate a key event through the Slint dispatch-key callback.
    /// Returns true if the key was handled.
    pub fn send_key(shell: &Shell, text: &str, ctrl: bool, shift: bool, alt: bool) -> bool {
        shell
            .window()
            .invoke_dispatch_key(text.into(), ctrl, shift, alt, false)
    }

    /// Simulate clicking a grid cell by invoking the grid-cell-clicked
    /// callback.
    pub fn click_grid_cell(shell: &Shell, col: i32, row: i32) {
        shell.window().invoke_grid_cell_clicked(col, row);
    }

    /// Simulate clicking a builder node by invoking the
    /// builder-node-clicked callback.
    pub fn click_node(shell: &Shell, node_id: &str) {
        shell.window().invoke_builder_node_clicked(node_id.into());
    }

    /// Simulate selecting a panel by invoking the select-panel callback.
    pub fn select_panel(shell: &Shell, panel_id: i32) {
        shell.window().invoke_select_panel(panel_id);
    }

    /// Simulate changing the viewport preset.
    pub fn set_viewport(shell: &Shell, preset: &str) {
        shell.window().invoke_viewport_preset_changed(preset.into());
    }
}

// ── Test harness ─────────────────────────────────────────────────

/// High-level test harness wrapping a `Shell` with scene loading,
/// input simulation, and state assertions.
///
/// ```text
/// let harness = TestHarness::from_scene(BuiltinScene::BuilderGrid)?;
/// harness.send_command("view.zoom_in");
/// assert!(harness.state().viewport_width == 1280.0);
/// harness.screenshot("/tmp/test.png");
/// ```
pub struct TestHarness {
    shell: Shell,
}

impl TestHarness {
    pub fn new() -> Result<Self, slint::PlatformError> {
        Ok(Self {
            shell: Shell::new()?,
        })
    }

    pub fn from_state(state: AppState) -> Result<Self, slint::PlatformError> {
        Ok(Self {
            shell: Shell::from_state(state)?,
        })
    }

    pub fn from_scene(scene: BuiltinScene) -> Result<Self, slint::PlatformError> {
        Self::from_state(apply_scene(scene))
    }

    pub fn shell(&self) -> &Shell {
        &self.shell
    }

    pub fn state(&self) -> AppState {
        self.shell.state()
    }

    pub fn send_command(&self, command_id: &str) {
        TestInput::send_command(&self.shell, command_id);
    }

    pub fn send_key(&self, text: &str, ctrl: bool, shift: bool, alt: bool) -> bool {
        TestInput::send_key(&self.shell, text, ctrl, shift, alt)
    }

    pub fn set_viewport(&self, preset: &str) {
        TestInput::set_viewport(&self.shell, preset);
    }

    pub fn click_grid_cell(&self, col: i32, row: i32) {
        TestInput::click_grid_cell(&self.shell, col, row);
    }

    pub fn click_node(&self, node_id: &str) {
        TestInput::click_node(&self.shell, node_id);
    }

    pub fn screenshot(&self, path: &str) -> bool {
        ShellScreenshot::capture(path)
    }

    pub fn run(self) -> Result<(), slint::PlatformError> {
        self.shell.run()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_scene_names_are_unique() {
        let mut names: Vec<&str> = BuiltinScene::all().iter().map(|s| s.name()).collect();
        let len = names.len();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), len, "duplicate scene names");
    }

    #[test]
    fn by_name_round_trips() {
        for scene in BuiltinScene::all() {
            assert_eq!(BuiltinScene::by_name(scene.name()), Some(*scene));
        }
    }

    #[test]
    fn by_name_returns_none_for_unknown() {
        assert_eq!(BuiltinScene::by_name("nonexistent"), None);
    }

    #[test]
    fn apply_scene_launchpad_stays_on_launchpad() {
        let state = apply_scene(BuiltinScene::Launchpad);
        assert!(state.shell_view.is_launchpad());
    }

    #[test]
    fn apply_scene_builder_enters_app() {
        let state = apply_scene(BuiltinScene::BuilderGrid);
        assert!(!state.shell_view.is_launchpad());
        assert_eq!(state.workspace.active_page().id, "edit");
    }

    #[test]
    fn apply_scene_tablet_sets_viewport() {
        let state = apply_scene(BuiltinScene::BuilderTablet);
        assert_eq!(state.viewport_width, 768.0);
    }

    #[test]
    fn apply_scene_mobile_sets_viewport() {
        let state = apply_scene(BuiltinScene::BuilderMobile);
        assert_eq!(state.viewport_width, 375.0);
    }

    #[test]
    fn apply_scene_inspector_selects_root() {
        let state = apply_scene(BuiltinScene::Inspector);
        assert!(state.selection.primary().is_some());
    }

    #[test]
    fn apply_scene_code_editor_panel() {
        let state = apply_scene(BuiltinScene::CodeEditor);
        assert_eq!(state.workspace.active_page().id, "code");
    }

    #[test]
    fn apply_scene_explorer_panel() {
        let state = apply_scene(BuiltinScene::Explorer);
        assert_eq!(state.workspace.active_page().id, "edit");
    }

    #[test]
    fn all_scenes_produce_valid_state() {
        for scene in BuiltinScene::all() {
            let state = apply_scene(*scene);
            assert!(!state.apps.is_empty(), "scene {} has no apps", scene.name());
        }
    }
}
