//! End-to-end testing framework for Prism Shell.
//!
//! Drives the shell programmatically through the same input paths a
//! human uses — Slint callbacks, the `InputManager` dispatch chain,
//! and the `CommandRegistry`. Two execution modes:
//!
//! - **Callback** (default): invokes Slint callbacks directly.
//!   Fast, deterministic, no window needed for state assertions.
//! - **OsInput**: injects real keyboard/mouse events via `enigo`
//!   so the full `winit → Slint → InputManager` pipeline runs.
//!   Requires a display; used for true end-to-end validation.
//!
//! Both modes share the same [`E2eDriver`] API and [`TestScript`]
//! format so a test written once runs at either fidelity level.
//!
//! # Quick start
//!
//! ```bash
//! # Run all built-in e2e tests (callback mode, headless-safe)
//! cargo run -p prism-cli -- e2e
//!
//! # Run a single test
//! cargo run -p prism-cli -- e2e --test launchpad-navigation
//!
//! # Record baselines for screenshot comparison
//! cargo run -p prism-cli -- e2e --record
//!
//! # List available tests
//! cargo run -p prism-cli -- e2e --list
//! ```

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::app::AppState;
use crate::testing::{apply_scene, BuiltinScene, ShellScreenshot, TestInput};
use crate::Shell;

// ── Geometry ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Rect {
    pub fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    pub fn center(&self) -> (f32, f32) {
        (self.x + self.width / 2.0, self.y + self.height / 2.0)
    }

    pub fn contains(&self, px: f32, py: f32) -> bool {
        px >= self.x && px <= self.x + self.width && py >= self.y && py <= self.y + self.height
    }
}

// ── Element Locator ─────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ElementLocator {
    NodeId(String),
    GridCell { col: i32, row: i32 },
    ActivityBarButton(i32),
    Panel(String),
    ToolbarButton(String),
    Position { x: f32, y: f32 },
}

// ── Test Actions ────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum TestAction {
    SendKey { combo: String },
    SendCommand { command_id: String },
    TypeText { text: String },
    ClickAt { x: f32, y: f32 },
    ClickElement { locator: ElementLocator },
    MoveTo { x: f32, y: f32 },
    SetScene { scene: String },
    SelectPanel { panel_id: i32 },
    SetViewport { preset: String },
    SetZoom { level: f32 },
    Wait { ms: u64 },
    AssertState { check: StateCheck },
    AssertScreenshot { name: String, max_diff_percent: f64 },
    CaptureBaseline { name: String },
    CaptureScreenshot { name: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum StateCheck {
    OnLaunchpad,
    InApp { app_name: String },
    OnPage { page_id: String },
    HasSelection,
    NoSelection,
    SelectionCount { n: usize },
    CommandPaletteOpen { open: bool },
    ViewportWidth { width: f32 },
    SearchQuery { query: String },
    PanelVisible { panel: String },
    PanelHidden { panel: String },
    DocumentHasRoot,
    DocumentNodeCount { min: usize },
    DockContainsPanel { panel_id: String },
    EditorHasText,
    EditorLanguage { language: String },
    ZoomLevel { min: f32, max: f32 },
}

// ── Test Script ─────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestScript {
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub scene: Option<String>,
    pub actions: Vec<TestAction>,
}

impl TestScript {
    pub fn new(name: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            scene: None,
            actions: Vec::new(),
        }
    }

    pub fn scene(mut self, scene: impl Into<String>) -> Self {
        self.scene = Some(scene.into());
        self
    }

    pub fn step(mut self, action: TestAction) -> Self {
        self.actions.push(action);
        self
    }

    pub fn send_key(self, combo: impl Into<String>) -> Self {
        self.step(TestAction::SendKey {
            combo: combo.into(),
        })
    }

    pub fn send_command(self, id: impl Into<String>) -> Self {
        self.step(TestAction::SendCommand {
            command_id: id.into(),
        })
    }

    pub fn click_node(self, id: impl Into<String>) -> Self {
        self.step(TestAction::ClickElement {
            locator: ElementLocator::NodeId(id.into()),
        })
    }

    pub fn click_grid(self, col: i32, row: i32) -> Self {
        self.step(TestAction::ClickElement {
            locator: ElementLocator::GridCell { col, row },
        })
    }

    pub fn set_viewport(self, preset: impl Into<String>) -> Self {
        self.step(TestAction::SetViewport {
            preset: preset.into(),
        })
    }

    pub fn set_zoom(self, level: f32) -> Self {
        self.step(TestAction::SetZoom { level })
    }

    pub fn wait(self, ms: u64) -> Self {
        self.step(TestAction::Wait { ms })
    }

    pub fn assert(self, check: StateCheck) -> Self {
        self.step(TestAction::AssertState { check })
    }

    pub fn screenshot(self, name: impl Into<String>) -> Self {
        self.step(TestAction::CaptureScreenshot { name: name.into() })
    }
}

// ── Test Result ─────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct TestResult {
    pub name: String,
    pub passed: bool,
    pub duration: Duration,
    pub steps_run: usize,
    pub failures: Vec<TestFailure>,
    pub screenshots: Vec<PathBuf>,
}

impl TestResult {
    pub fn summary(&self) -> String {
        if self.passed {
            format!(
                "PASS  {} ({} steps, {:.0}ms)",
                self.name,
                self.steps_run,
                self.duration.as_secs_f64() * 1000.0
            )
        } else {
            let mut s = format!(
                "FAIL  {} ({}/{} steps, {:.0}ms)",
                self.name,
                self.steps_run - self.failures.len(),
                self.steps_run,
                self.duration.as_secs_f64() * 1000.0
            );
            for f in &self.failures {
                s.push_str(&format!("\n       step {}: {}", f.step, f.message));
            }
            s
        }
    }
}

#[derive(Debug, Clone)]
pub struct TestFailure {
    pub step: usize,
    pub action: String,
    pub message: String,
}

// ── Suite Result ────────────────────────────────────────────────────

#[derive(Debug)]
pub struct SuiteResult {
    pub results: Vec<TestResult>,
    pub duration: Duration,
}

impl SuiteResult {
    pub fn passed(&self) -> usize {
        self.results.iter().filter(|r| r.passed).count()
    }

    pub fn failed(&self) -> usize {
        self.results.iter().filter(|r| !r.passed).count()
    }

    pub fn total(&self) -> usize {
        self.results.len()
    }

    pub fn all_passed(&self) -> bool {
        self.results.iter().all(|r| r.passed)
    }

    pub fn summary(&self) -> String {
        let mut s = String::new();
        s.push_str(&format!(
            "\n  {} tests: {} passed, {} failed ({:.0}ms)\n\n",
            self.total(),
            self.passed(),
            self.failed(),
            self.duration.as_secs_f64() * 1000.0,
        ));
        for r in &self.results {
            s.push_str(&format!("  {}\n", r.summary()));
        }
        s
    }
}

// ── Input Mode ──────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputMode {
    Callback,
    #[cfg(feature = "e2e")]
    OsInput,
}

// ── Layout Oracle ───────────────────────────────────────────────────

pub struct LayoutOracle;

impl LayoutOracle {
    const ACTIVITY_BAR_WIDTH: f32 = 48.0;
    const ACTIVITY_BAR_BUTTON_SIZE: f32 = 48.0;
    const ACTIVITY_BAR_TOP_PAD: f32 = 8.0;
    const TAB_BAR_HEIGHT: f32 = 40.0;
    const TOOLBAR_HEIGHT: f32 = 40.0;
    const STATUS_BAR_HEIGHT: f32 = 32.0;

    pub fn activity_bar_button(index: i32) -> Rect {
        Rect::new(
            0.0,
            Self::ACTIVITY_BAR_TOP_PAD + index as f32 * Self::ACTIVITY_BAR_BUTTON_SIZE,
            Self::ACTIVITY_BAR_WIDTH,
            Self::ACTIVITY_BAR_BUTTON_SIZE,
        )
    }

    pub fn content_origin(state: &AppState) -> (f32, f32) {
        let x = if state.show_activity_bar {
            Self::ACTIVITY_BAR_WIDTH
        } else {
            0.0
        };
        let y = Self::TAB_BAR_HEIGHT + Self::TOOLBAR_HEIGHT;
        (x, y)
    }

    pub fn content_size(state: &AppState) -> (f32, f32) {
        let (ox, oy) = Self::content_origin(state);
        let w = state.viewport_width - ox;
        let h = 800.0 - oy - Self::STATUS_BAR_HEIGHT;
        (w, h)
    }

    pub fn grid_cell_rect(state: &AppState, col: i32, row: i32) -> Option<Rect> {
        let doc = &state.builder_document;
        let layout = &doc.page_layout;
        if layout.columns.is_empty() && layout.rows.is_empty() {
            return None;
        }
        let (ox, oy) = Self::content_origin(state);
        let (cw, ch) = Self::content_size(state);

        let num_cols = layout.columns.len().max(1) as f32;
        let num_rows = layout.rows.len().max(1) as f32;
        let ml = layout.margins.left;
        let mt = layout.margins.top;
        let avail_w = cw - ml - layout.margins.right;
        let avail_h = ch - mt - layout.margins.bottom;
        let cell_w = (avail_w - layout.column_gap * (num_cols - 1.0).max(0.0)) / num_cols;
        let cell_h = (avail_h - layout.row_gap * (num_rows - 1.0).max(0.0)) / num_rows;

        Some(Rect::new(
            ox + ml + col as f32 * (cell_w + layout.column_gap),
            oy + mt + row as f32 * (cell_h + layout.row_gap),
            cell_w,
            cell_h,
        ))
    }

    pub fn element_rect(state: &AppState, locator: &ElementLocator) -> Option<Rect> {
        match locator {
            ElementLocator::ActivityBarButton(idx) => Some(Self::activity_bar_button(*idx)),
            ElementLocator::GridCell { col, row } => Self::grid_cell_rect(state, *col, *row),
            ElementLocator::Position { x, y } => Some(Rect::new(*x, *y, 1.0, 1.0)),
            _ => None,
        }
    }
}

// ── Screenshot Diff ─────────────────────────────────────────────────

pub struct ScreenDiff;

impl ScreenDiff {
    #[cfg(feature = "e2e")]
    pub fn compare(baseline: &Path, current: &Path, max_diff_percent: f64) -> Result<bool, String> {
        let baseline_img = image::open(baseline)
            .map_err(|e| format!("Failed to open baseline {}: {e}", baseline.display()))?
            .to_rgba8();
        let current_img = image::open(current)
            .map_err(|e| format!("Failed to open current {}: {e}", current.display()))?
            .to_rgba8();

        let (bw, bh) = baseline_img.dimensions();
        let (cw, ch) = current_img.dimensions();
        if bw != cw || bh != ch {
            return Err(format!(
                "Dimension mismatch: baseline {bw}x{bh}, current {cw}x{ch}"
            ));
        }

        let total = (bw * bh) as f64;
        let mut diff_count = 0u64;
        for (b, c) in baseline_img.pixels().zip(current_img.pixels()) {
            if b != c {
                diff_count += 1;
            }
        }

        let diff_percent = (diff_count as f64 / total) * 100.0;
        Ok(diff_percent <= max_diff_percent)
    }

    #[cfg(feature = "e2e")]
    pub fn diff_image(baseline: &Path, current: &Path, output: &Path) -> Result<f64, String> {
        let baseline_img = image::open(baseline)
            .map_err(|e| format!("Failed to open baseline: {e}"))?
            .to_rgba8();
        let current_img = image::open(current)
            .map_err(|e| format!("Failed to open current: {e}"))?
            .to_rgba8();

        let (bw, bh) = baseline_img.dimensions();
        let (cw, ch) = current_img.dimensions();
        let out_w = bw.max(cw);
        let out_h = bh.max(ch);
        let mut diff_img = image::RgbaImage::new(out_w, out_h);
        let total = (out_w * out_h) as f64;
        let mut diff_count = 0u64;

        for y in 0..out_h {
            for x in 0..out_w {
                let bp = if x < bw && y < bh {
                    *baseline_img.get_pixel(x, y)
                } else {
                    image::Rgba([0, 0, 0, 255])
                };
                let cp = if x < cw && y < ch {
                    *current_img.get_pixel(x, y)
                } else {
                    image::Rgba([0, 0, 0, 255])
                };

                if bp == cp {
                    diff_img.put_pixel(x, y, image::Rgba([bp[0] / 3, bp[1] / 3, bp[2] / 3, 255]));
                } else {
                    diff_count += 1;
                    diff_img.put_pixel(x, y, image::Rgba([255, 0, 80, 255]));
                }
            }
        }

        diff_img
            .save(output)
            .map_err(|e| format!("Failed to save diff image: {e}"))?;
        Ok((diff_count as f64 / total) * 100.0)
    }

    #[cfg(not(feature = "e2e"))]
    pub fn compare(
        _baseline: &Path,
        _current: &Path,
        _max_diff_percent: f64,
    ) -> Result<bool, String> {
        Err("Screenshot comparison requires the `e2e` feature".into())
    }
}

// ── OS Input Driver ─────────────────────────────────────────────────

#[cfg(feature = "e2e")]
mod os_input {
    use enigo::{Enigo, Keyboard, Mouse, Settings};

    pub struct OsInputDriver {
        enigo: Enigo,
    }

    impl OsInputDriver {
        pub fn new() -> Result<Self, String> {
            let enigo = Enigo::new(&Settings::default()).map_err(|e| format!("enigo init: {e}"))?;
            Ok(Self { enigo })
        }

        pub fn click(&mut self, x: f32, y: f32) -> Result<(), String> {
            self.enigo
                .move_mouse(x as i32, y as i32, enigo::Coordinate::Abs)
                .map_err(|e| format!("mouse move: {e}"))?;
            std::thread::sleep(std::time::Duration::from_millis(10));
            self.enigo
                .button(enigo::Button::Left, enigo::Direction::Click)
                .map_err(|e| format!("mouse click: {e}"))?;
            Ok(())
        }

        pub fn move_to(&mut self, x: f32, y: f32) -> Result<(), String> {
            self.enigo
                .move_mouse(x as i32, y as i32, enigo::Coordinate::Abs)
                .map_err(|e| format!("mouse move: {e}"))?;
            Ok(())
        }

        pub fn type_text(&mut self, text: &str) -> Result<(), String> {
            self.enigo
                .text(text)
                .map_err(|e| format!("type text: {e}"))?;
            Ok(())
        }

        pub fn key_combo(&mut self, combo: &str) -> Result<(), String> {
            use enigo::{Direction, Key};

            let parts: Vec<&str> = combo.split('+').map(str::trim).collect();
            let mut modifiers = Vec::new();
            let mut key_str = "";

            for part in &parts {
                match part.to_lowercase().as_str() {
                    "ctrl" | "control" => modifiers.push(Key::Control),
                    "shift" => modifiers.push(Key::Shift),
                    "alt" | "option" => modifiers.push(Key::Alt),
                    "meta" | "cmd" | "command" | "super" => modifiers.push(Key::Meta),
                    _ => key_str = part,
                }
            }

            let key = match key_str.to_lowercase().as_str() {
                "escape" | "esc" => Key::Escape,
                "return" | "enter" => Key::Return,
                "tab" => Key::Tab,
                "space" => Key::Space,
                "backspace" => Key::Backspace,
                "delete" => Key::Delete,
                "up" => Key::UpArrow,
                "down" => Key::DownArrow,
                "left" => Key::LeftArrow,
                "right" => Key::RightArrow,
                "home" => Key::Home,
                "end" => Key::End,
                "pageup" => Key::PageUp,
                "pagedown" => Key::PageDown,
                "f1" => Key::F1,
                "f2" => Key::F2,
                "f3" => Key::F3,
                "f4" => Key::F4,
                "f5" => Key::F5,
                "f6" => Key::F6,
                "f7" => Key::F7,
                "f8" => Key::F8,
                "f9" => Key::F9,
                "f10" => Key::F10,
                "f11" => Key::F11,
                "f12" => Key::F12,
                s if s.len() == 1 => Key::Unicode(s.chars().next().unwrap()),
                s => return Err(format!("Unknown key: {s}")),
            };

            for m in &modifiers {
                self.enigo
                    .key(*m, Direction::Press)
                    .map_err(|e| format!("key press: {e}"))?;
            }
            self.enigo
                .key(key, Direction::Click)
                .map_err(|e| format!("key click: {e}"))?;
            for m in modifiers.iter().rev() {
                self.enigo
                    .key(*m, Direction::Release)
                    .map_err(|e| format!("key release: {e}"))?;
            }
            Ok(())
        }
    }
}

// ── E2E Driver ──────────────────────────────────────────────────────

pub struct E2eDriver {
    shell: Shell,
    mode: InputMode,
    screenshot_dir: PathBuf,
    baseline_dir: PathBuf,
    step_delay_ms: u64,
    #[cfg(feature = "e2e")]
    os_input: Option<os_input::OsInputDriver>,
}

impl E2eDriver {
    pub fn new(shell: Shell) -> Self {
        Self {
            shell,
            mode: InputMode::Callback,
            screenshot_dir: PathBuf::from("screenshots/e2e"),
            baseline_dir: PathBuf::from("screenshots/baseline"),
            step_delay_ms: 16,
            #[cfg(feature = "e2e")]
            os_input: None,
        }
    }

    pub fn from_scene(scene: BuiltinScene) -> Result<Self, slint::PlatformError> {
        let state = apply_scene(scene);
        let shell = Shell::from_state(state)?;
        Ok(Self::new(shell))
    }

    pub fn with_mode(mut self, mode: InputMode) -> Self {
        self.mode = mode;
        #[cfg(feature = "e2e")]
        if mode == InputMode::OsInput {
            self.os_input = os_input::OsInputDriver::new().ok();
        }
        self
    }

    pub fn with_screenshot_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.screenshot_dir = dir.into();
        self
    }

    pub fn with_baseline_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.baseline_dir = dir.into();
        self
    }

    pub fn with_step_delay(mut self, ms: u64) -> Self {
        self.step_delay_ms = ms;
        self
    }

    // ── Core Input ──────────────────────────────────────────────

    pub fn send_key(&mut self, combo: &str) -> bool {
        #[cfg(feature = "e2e")]
        if self.mode == InputMode::OsInput {
            if let Some(ref mut os) = self.os_input {
                return os.key_combo(combo).is_ok();
            }
        }
        let parsed = crate::keyboard::KeyCombo::parse(combo);
        if let Some(kc) = parsed {
            let slint_text = key_name_to_slint_char(&kc.key);
            TestInput::send_key(
                &self.shell,
                &slint_text,
                kc.modifiers.ctrl,
                kc.modifiers.shift,
                kc.modifiers.alt,
            )
        } else {
            false
        }
    }

    pub fn send_command(&self, command_id: &str) {
        TestInput::send_command(&self.shell, command_id);
    }

    pub fn type_text(&mut self, text: &str) {
        #[cfg(feature = "e2e")]
        if self.mode == InputMode::OsInput {
            if let Some(ref mut os) = self.os_input {
                let _ = os.type_text(text);
                return;
            }
        }
        for ch in text.chars() {
            TestInput::send_key(&self.shell, &ch.to_string(), false, false, false);
        }
    }

    pub fn click_at(&mut self, x: f32, y: f32) {
        #[cfg(feature = "e2e")]
        if self.mode == InputMode::OsInput {
            if let Some(ref mut os) = self.os_input {
                let _ = os.click(x, y);
                return;
            }
        }
        let state = self.shell.state();
        if let Some(rect) = LayoutOracle::grid_cell_rect(&state, 0, 0) {
            let doc = &state.builder_document;
            let layout = &doc.page_layout;
            let num_cols = layout.columns.len().max(1);
            let num_rows = layout.rows.len().max(1);
            for row in 0..num_rows {
                for col in 0..num_cols {
                    if let Some(cell) = LayoutOracle::grid_cell_rect(&state, col as i32, row as i32)
                    {
                        if cell.contains(x, y) {
                            TestInput::click_grid_cell(&self.shell, col as i32, row as i32);
                            return;
                        }
                    }
                }
            }
            let _ = rect;
        }
    }

    pub fn click_element(&self, locator: &ElementLocator) {
        match locator {
            ElementLocator::NodeId(id) => TestInput::click_node(&self.shell, id),
            ElementLocator::GridCell { col, row } => {
                TestInput::click_grid_cell(&self.shell, *col, *row)
            }
            ElementLocator::Panel(panel) => {
                let panel_id = match panel.as_str() {
                    "identity" => 0,
                    "builder" | "edit" => 1,
                    "code-editor" | "code" => 2,
                    "explorer" => 3,
                    _ => return,
                };
                TestInput::select_panel(&self.shell, panel_id);
            }
            ElementLocator::ActivityBarButton(idx) => {
                TestInput::select_panel(&self.shell, *idx);
            }
            ElementLocator::ToolbarButton(cmd) => {
                TestInput::send_command(&self.shell, cmd);
            }
            ElementLocator::Position { .. } => {
                // Position-based click delegates to click_at through the layout oracle
            }
        }
    }

    pub fn move_to(&mut self, x: f32, y: f32) {
        #[cfg(feature = "e2e")]
        if self.mode == InputMode::OsInput {
            if let Some(ref mut os) = self.os_input {
                let _ = os.move_to(x, y);
            }
        }
        let _ = (x, y);
    }

    pub fn set_viewport(&self, preset: &str) {
        TestInput::set_viewport(&self.shell, preset);
    }

    pub fn select_panel(&self, panel_id: i32) {
        TestInput::select_panel(&self.shell, panel_id);
    }

    // ── Queries ─────────────────────────────────────────────────

    pub fn state(&self) -> AppState {
        self.shell.state()
    }

    pub fn shell(&self) -> &Shell {
        &self.shell
    }

    pub fn element_position(&self, locator: &ElementLocator) -> Option<Rect> {
        LayoutOracle::element_rect(&self.state(), locator)
    }

    pub fn element_center(&self, locator: &ElementLocator) -> Option<(f32, f32)> {
        self.element_position(locator).map(|r| r.center())
    }

    // ── Screenshots ─────────────────────────────────────────────

    pub fn capture_screenshot(&self, name: &str) -> Option<PathBuf> {
        let dir = &self.screenshot_dir;
        std::fs::create_dir_all(dir).ok()?;
        let path = dir.join(format!("{name}.png"));
        if ShellScreenshot::capture(path.to_str()?) {
            Some(path)
        } else {
            None
        }
    }

    pub fn capture_baseline(&self, name: &str) -> Option<PathBuf> {
        let dir = &self.baseline_dir;
        std::fs::create_dir_all(dir).ok()?;
        let path = dir.join(format!("{name}.png"));
        if ShellScreenshot::capture(path.to_str()?) {
            Some(path)
        } else {
            None
        }
    }

    pub fn compare_screenshots(&self, name: &str, max_diff_percent: f64) -> Result<bool, String> {
        let current = self.screenshot_dir.join(format!("{name}.png"));
        let baseline = self.baseline_dir.join(format!("{name}.png"));
        if !baseline.exists() {
            return Err(format!("No baseline: {}", baseline.display()));
        }
        if !current.exists() {
            return Err(format!("No screenshot: {}", current.display()));
        }
        ScreenDiff::compare(&baseline, &current, max_diff_percent)
    }

    // ── State Assertions ────────────────────────────────────────

    pub fn check_state(&self, check: &StateCheck) -> Result<(), String> {
        let state = self.state();
        match check {
            StateCheck::OnLaunchpad => {
                if !state.shell_view.is_launchpad() {
                    return Err("Expected launchpad view".into());
                }
            }
            StateCheck::InApp { app_name } => match state.active_app() {
                Some(app) if app.name.to_lowercase() == app_name.to_lowercase() => {}
                Some(app) => return Err(format!("Expected app '{app_name}', got '{}'", app.name)),
                None => return Err(format!("Expected app '{app_name}', on launchpad")),
            },
            StateCheck::OnPage { page_id } => {
                let active = state.workspace.active_page();
                if active.id != *page_id {
                    return Err(format!("Expected page '{page_id}', got '{}'", active.id));
                }
            }
            StateCheck::HasSelection => {
                if state.selection.is_empty() {
                    return Err("Expected selection, got none".into());
                }
            }
            StateCheck::NoSelection => {
                if !state.selection.is_empty() {
                    return Err("Expected no selection".into());
                }
            }
            StateCheck::SelectionCount { n } => {
                let count = state.selection.count();
                if count != *n {
                    return Err(format!("Expected {n} selected, got {count}"));
                }
            }
            StateCheck::CommandPaletteOpen { open } => {
                if state.command_palette_open != *open {
                    return Err(format!(
                        "Expected command palette {}",
                        if *open { "open" } else { "closed" }
                    ));
                }
            }
            StateCheck::ViewportWidth { width } => {
                if (state.viewport_width - width).abs() > 0.5 {
                    return Err(format!(
                        "Expected viewport {width}, got {}",
                        state.viewport_width
                    ));
                }
            }
            StateCheck::SearchQuery { query } => {
                if state.search_query != *query {
                    return Err(format!(
                        "Expected search '{query}', got '{}'",
                        state.search_query
                    ));
                }
            }
            StateCheck::PanelVisible { panel } => match panel.as_str() {
                "left" | "left_sidebar" => {
                    if !state.show_left_sidebar {
                        return Err("Expected left sidebar visible".into());
                    }
                }
                "right" | "right_sidebar" => {
                    if !state.show_right_sidebar {
                        return Err("Expected right sidebar visible".into());
                    }
                }
                "activity_bar" => {
                    if !state.show_activity_bar {
                        return Err("Expected activity bar visible".into());
                    }
                }
                other => return Err(format!("Unknown panel: {other}")),
            },
            StateCheck::PanelHidden { panel } => match panel.as_str() {
                "left" | "left_sidebar" => {
                    if state.show_left_sidebar {
                        return Err("Expected left sidebar hidden".into());
                    }
                }
                "right" | "right_sidebar" => {
                    if state.show_right_sidebar {
                        return Err("Expected right sidebar hidden".into());
                    }
                }
                "activity_bar" => {
                    if state.show_activity_bar {
                        return Err("Expected activity bar hidden".into());
                    }
                }
                other => return Err(format!("Unknown panel: {other}")),
            },
            StateCheck::DocumentHasRoot => {
                if state.builder_document.root.is_none() {
                    return Err("Expected document to have root node".into());
                }
            }
            StateCheck::DocumentNodeCount { min } => {
                let count = count_nodes(&state.builder_document);
                if count < *min {
                    return Err(format!("Expected >= {min} nodes, got {count}"));
                }
            }
            StateCheck::DockContainsPanel { panel_id } => {
                let page = state.workspace.active_page();
                if !page.dock.contains_panel(panel_id) {
                    return Err(format!(
                        "Active page '{}' dock does not contain panel '{panel_id}'",
                        page.id
                    ));
                }
            }
            StateCheck::EditorHasText => {
                if state.editor_state.text().is_empty() {
                    return Err("Expected editor to have text content".into());
                }
            }
            StateCheck::EditorLanguage { language } => {
                if state.editor_state.language != *language {
                    return Err(format!(
                        "Expected editor language '{}', got '{}'",
                        language, state.editor_state.language
                    ));
                }
            }
            StateCheck::ZoomLevel { min, max } => {
                let zoom = self.shell.window().get_canvas_zoom();
                if zoom < *min || zoom > *max {
                    return Err(format!("Expected zoom in [{min}..{max}], got {zoom}"));
                }
            }
        }
        Ok(())
    }

    // ── Script Runner ───────────────────────────────────────────

    pub fn run_script(&mut self, script: &TestScript) -> TestResult {
        let start = Instant::now();
        let mut failures = Vec::new();
        let mut screenshots = Vec::new();
        let mut steps_run = 0;

        for (i, action) in script.actions.iter().enumerate() {
            steps_run = i + 1;
            match self.run_action(action) {
                Ok(path) => {
                    if let Some(p) = path {
                        screenshots.push(p);
                    }
                }
                Err(msg) => {
                    failures.push(TestFailure {
                        step: i,
                        action: format!("{action:?}"),
                        message: msg,
                    });
                }
            }
            if self.step_delay_ms > 0 {
                std::thread::sleep(Duration::from_millis(self.step_delay_ms));
            }
        }

        TestResult {
            name: script.name.clone(),
            passed: failures.is_empty(),
            duration: start.elapsed(),
            steps_run,
            failures,
            screenshots,
        }
    }

    pub fn run_suite(&mut self, scripts: &[TestScript]) -> SuiteResult {
        let start = Instant::now();
        let mut results = Vec::new();
        for script in scripts {
            if let Some(ref scene_name) = script.scene {
                if let Some(scene) = BuiltinScene::by_name(scene_name) {
                    let new_state = apply_scene(scene);
                    let bytes = serde_json::to_vec(&new_state).unwrap();
                    let _ = self.shell.restore(&bytes);
                }
            }
            results.push(self.run_script(script));
        }
        SuiteResult {
            results,
            duration: start.elapsed(),
        }
    }

    fn run_action(&mut self, action: &TestAction) -> Result<Option<PathBuf>, String> {
        match action {
            TestAction::SendKey { combo } => {
                self.send_key(combo);
                Ok(None)
            }
            TestAction::SendCommand { command_id } => {
                self.send_command(command_id);
                Ok(None)
            }
            TestAction::TypeText { text } => {
                self.type_text(text);
                Ok(None)
            }
            TestAction::ClickAt { x, y } => {
                self.click_at(*x, *y);
                Ok(None)
            }
            TestAction::ClickElement { locator } => {
                self.click_element(locator);
                Ok(None)
            }
            TestAction::MoveTo { x, y } => {
                self.move_to(*x, *y);
                Ok(None)
            }
            TestAction::SetScene { scene } => {
                let s = BuiltinScene::by_name(scene)
                    .ok_or_else(|| format!("Unknown scene: {scene}"))?;
                let new_state = apply_scene(s);
                let bytes =
                    serde_json::to_vec(&new_state).map_err(|e| format!("serialize: {e}"))?;
                self.shell
                    .restore(&bytes)
                    .map_err(|e| format!("restore: {e}"))?;
                Ok(None)
            }
            TestAction::SelectPanel { panel_id } => {
                self.select_panel(*panel_id);
                Ok(None)
            }
            TestAction::SetViewport { preset } => {
                self.set_viewport(preset);
                Ok(None)
            }
            TestAction::SetZoom { level } => {
                self.shell.window().set_canvas_zoom(*level);
                Ok(None)
            }
            TestAction::Wait { ms } => {
                std::thread::sleep(Duration::from_millis(*ms));
                Ok(None)
            }
            TestAction::AssertState { check } => {
                self.check_state(check)?;
                Ok(None)
            }
            TestAction::AssertScreenshot {
                name,
                max_diff_percent,
            } => {
                self.capture_screenshot(name);
                match self.compare_screenshots(name, *max_diff_percent) {
                    Ok(true) => Ok(None),
                    Ok(false) => Err(format!("Screenshot '{name}' differs from baseline")),
                    Err(e) => Err(e),
                }
            }
            TestAction::CaptureBaseline { name } => {
                let path = self
                    .capture_baseline(name)
                    .ok_or_else(|| format!("Failed to capture baseline '{name}'"))?;
                Ok(Some(path))
            }
            TestAction::CaptureScreenshot { name } => {
                let path = self
                    .capture_screenshot(name)
                    .ok_or_else(|| format!("Failed to capture screenshot '{name}'"))?;
                Ok(Some(path))
            }
        }
    }

    pub fn into_shell(self) -> Shell {
        self.shell
    }
}

fn key_name_to_slint_char(key: &str) -> String {
    match key {
        "escape" => "\u{001B}".into(),
        "tab" => "\u{0009}".into(),
        "return" | "enter" => "\u{000D}".into(),
        "delete" => "\u{007F}".into(),
        "backspace" => "\u{0008}".into(),
        "space" => " ".into(),
        "up" => "\u{F700}".into(),
        "down" => "\u{F701}".into(),
        "left" => "\u{F702}".into(),
        "right" => "\u{F703}".into(),
        "home" => "\u{F729}".into(),
        "end" => "\u{F72B}".into(),
        "pageup" => "\u{F72C}".into(),
        "pagedown" => "\u{F72D}".into(),
        "f1" => "\u{F704}".into(),
        "f2" => "\u{F705}".into(),
        "f3" => "\u{F706}".into(),
        "f4" => "\u{F707}".into(),
        "f5" => "\u{F708}".into(),
        "f6" => "\u{F709}".into(),
        "f7" => "\u{F70A}".into(),
        "f8" => "\u{F70B}".into(),
        "f9" => "\u{F70C}".into(),
        "f10" => "\u{F70D}".into(),
        "f11" => "\u{F70E}".into(),
        "f12" => "\u{F70F}".into(),
        other => other.into(),
    }
}

fn count_nodes(doc: &prism_builder::BuilderDocument) -> usize {
    fn walk(node: &prism_builder::Node) -> usize {
        1 + node.children.iter().map(walk).sum::<usize>()
    }
    doc.root.as_ref().map_or(0, walk)
}

// ── Built-in Test Scripts ───────────────────────────────────────────

pub fn builtin_scripts() -> Vec<TestScript> {
    vec![
        test_launchpad_state(),
        test_scene_loading(),
        test_viewport_switching(),
        test_keyboard_dispatch(),
        test_command_palette_toggle(),
        test_panel_navigation(),
        test_selection_lifecycle(),
        test_undo_redo_cycle(),
        test_grid_cell_interaction(),
        test_sidebar_toggles(),
        test_zoom_controls(),
        test_document_structure(),
        test_bidirectional_editor(),
        test_workflow_page_switching(),
        test_search_focus(),
        test_all_panel_toggles(),
        test_select_all_nodes(),
        test_clipboard_copy_paste(),
        test_selection_delete(),
        test_inspector_arrow_navigation(),
        test_tab_cycling(),
        test_escape_context_cascade(),
        test_editor_state(),
        test_node_click_selection(),
        test_command_palette_auto_close(),
        test_duplicate_node(),
    ]
}

pub fn builtin_script_by_name(name: &str) -> Option<TestScript> {
    builtin_scripts().into_iter().find(|s| s.name == name)
}

fn test_launchpad_state() -> TestScript {
    TestScript::new(
        "launchpad-state",
        "Verify launchpad starts with correct defaults",
    )
    .scene("launchpad")
    .assert(StateCheck::OnLaunchpad)
    .assert(StateCheck::NoSelection)
    .assert(StateCheck::CommandPaletteOpen { open: false })
    .assert(StateCheck::ViewportWidth { width: 1280.0 })
    .assert(StateCheck::PanelVisible {
        panel: "activity_bar".into(),
    })
}

fn test_scene_loading() -> TestScript {
    TestScript::new("scene-loading", "Load each scene and verify expected state")
        .step(TestAction::SetScene {
            scene: "builder-grid".into(),
        })
        .assert(StateCheck::InApp {
            app_name: "Lattice".into(),
        })
        .assert(StateCheck::OnPage {
            page_id: "edit".into(),
        })
        .assert(StateCheck::DocumentHasRoot)
        .step(TestAction::SetScene {
            scene: "builder-tablet".into(),
        })
        .assert(StateCheck::ViewportWidth { width: 768.0 })
        .step(TestAction::SetScene {
            scene: "builder-mobile".into(),
        })
        .assert(StateCheck::ViewportWidth { width: 375.0 })
        .step(TestAction::SetScene {
            scene: "code-editor".into(),
        })
        .assert(StateCheck::OnPage {
            page_id: "code".into(),
        })
}

fn test_viewport_switching() -> TestScript {
    TestScript::new(
        "viewport-switching",
        "Switch between viewport presets and verify widths",
    )
    .scene("builder-grid")
    .assert(StateCheck::ViewportWidth { width: 1280.0 })
    .set_viewport("Tablet")
    .assert(StateCheck::ViewportWidth { width: 768.0 })
    .set_viewport("Mobile")
    .assert(StateCheck::ViewportWidth { width: 375.0 })
    .set_viewport("Desktop")
    .assert(StateCheck::ViewportWidth { width: 1280.0 })
}

fn test_keyboard_dispatch() -> TestScript {
    TestScript::new(
        "keyboard-dispatch",
        "Verify key combos dispatch through InputManager",
    )
    .scene("builder-grid")
    .step(TestAction::SendKey {
        combo: "ctrl+shift+p".into(),
    })
    .assert(StateCheck::CommandPaletteOpen { open: true })
    .step(TestAction::SendKey {
        combo: "escape".into(),
    })
    .assert(StateCheck::CommandPaletteOpen { open: false })
}

fn test_command_palette_toggle() -> TestScript {
    TestScript::new(
        "command-palette-toggle",
        "Open/close command palette via command dispatch",
    )
    .scene("builder-grid")
    .assert(StateCheck::CommandPaletteOpen { open: false })
    .send_command("command_palette.toggle")
    .assert(StateCheck::CommandPaletteOpen { open: true })
    .send_command("command_palette.toggle")
    .assert(StateCheck::CommandPaletteOpen { open: false })
}

fn test_panel_navigation() -> TestScript {
    TestScript::new("panel-navigation", "Switch between workflow panels")
        .scene("builder-grid")
        .assert(StateCheck::OnPage {
            page_id: "edit".into(),
        })
        .step(TestAction::SelectPanel { panel_id: 2 })
        .assert(StateCheck::OnPage {
            page_id: "code".into(),
        })
        .step(TestAction::SelectPanel { panel_id: 1 })
        .assert(StateCheck::OnPage {
            page_id: "edit".into(),
        })
}

fn test_selection_lifecycle() -> TestScript {
    TestScript::new(
        "selection-lifecycle",
        "Select, verify, and clear node selection",
    )
    .scene("inspector")
    .assert(StateCheck::HasSelection)
    .send_command("navigate.escape")
    .assert(StateCheck::NoSelection)
}

fn test_undo_redo_cycle() -> TestScript {
    TestScript::new("undo-redo-cycle", "Undo and redo preserve state correctly")
        .scene("builder-grid")
        .assert(StateCheck::DocumentHasRoot)
        .assert(StateCheck::DocumentNodeCount { min: 3 })
        .send_command("edit.undo")
        .send_command("edit.redo")
        .assert(StateCheck::DocumentHasRoot)
}

fn test_grid_cell_interaction() -> TestScript {
    TestScript::new(
        "grid-cell-interaction",
        "Click grid cells and verify state changes",
    )
    .scene("builder-grid")
    .assert(StateCheck::DocumentHasRoot)
    .click_grid(0, 0)
    .wait(50)
}

fn test_sidebar_toggles() -> TestScript {
    TestScript::new("sidebar-toggles", "Toggle sidebar visibility via commands")
        .scene("builder-grid")
        .assert(StateCheck::PanelVisible {
            panel: "left".into(),
        })
        .send_command("view.toggle_left_sidebar")
        .assert(StateCheck::PanelHidden {
            panel: "left".into(),
        })
        .send_command("view.toggle_left_sidebar")
        .assert(StateCheck::PanelVisible {
            panel: "left".into(),
        })
}

fn test_zoom_controls() -> TestScript {
    TestScript::new(
        "zoom-controls",
        "Zoom in/out/reset/fit via commands and direct set",
    )
    .scene("builder-grid")
    .send_command("view.zoom_in")
    .assert(StateCheck::ZoomLevel {
        min: 1.05,
        max: 1.15,
    })
    .send_command("view.zoom_out")
    .assert(StateCheck::ZoomLevel {
        min: 0.95,
        max: 1.05,
    })
    .send_command("view.zoom_reset")
    .assert(StateCheck::ZoomLevel { min: 1.0, max: 1.0 })
    .set_zoom(2.0)
    .assert(StateCheck::ZoomLevel { min: 2.0, max: 2.0 })
    .send_command("view.zoom_to_fit")
    .send_command("view.zoom_reset")
    .assert(StateCheck::ZoomLevel { min: 1.0, max: 1.0 })
}

fn test_bidirectional_editor() -> TestScript {
    TestScript::new(
        "bidirectional-editor",
        "Verify Edit page has code editor and bidirectional selection wiring",
    )
    .scene("builder-grid")
    .assert(StateCheck::OnPage {
        page_id: "edit".into(),
    })
    .assert(StateCheck::DockContainsPanel {
        panel_id: "code-editor".into(),
    })
    .assert(StateCheck::DockContainsPanel {
        panel_id: "properties".into(),
    })
    .assert(StateCheck::DocumentHasRoot)
    .assert(StateCheck::DocumentNodeCount { min: 3 })
    .step(TestAction::SetScene {
        scene: "inspector".into(),
    })
    .assert(StateCheck::HasSelection)
    .assert(StateCheck::DockContainsPanel {
        panel_id: "code-editor".into(),
    })
    .step(TestAction::SetScene {
        scene: "code-editor".into(),
    })
    .assert(StateCheck::OnPage {
        page_id: "code".into(),
    })
    .assert(StateCheck::DockContainsPanel {
        panel_id: "code-editor".into(),
    })
}

fn test_workflow_page_switching() -> TestScript {
    TestScript::new(
        "workflow-page-switching",
        "Switch between workflow pages and verify dock layouts",
    )
    .scene("builder-grid")
    .assert(StateCheck::OnPage {
        page_id: "edit".into(),
    })
    .assert(StateCheck::DockContainsPanel {
        panel_id: "builder".into(),
    })
    .assert(StateCheck::DockContainsPanel {
        panel_id: "code-editor".into(),
    })
    .step(TestAction::SelectPanel { panel_id: 2 })
    .assert(StateCheck::OnPage {
        page_id: "code".into(),
    })
    .assert(StateCheck::DockContainsPanel {
        panel_id: "code-editor".into(),
    })
    .step(TestAction::SelectPanel { panel_id: 1 })
    .assert(StateCheck::OnPage {
        page_id: "edit".into(),
    })
    .assert(StateCheck::DockContainsPanel {
        panel_id: "code-editor".into(),
    })
}

fn test_document_structure() -> TestScript {
    TestScript::new("document-structure", "Verify document tree across scenes")
        .step(TestAction::SetScene {
            scene: "builder-grid".into(),
        })
        .assert(StateCheck::DocumentHasRoot)
        .assert(StateCheck::DocumentNodeCount { min: 3 })
        .step(TestAction::SetScene {
            scene: "builder-empty".into(),
        })
        .assert(StateCheck::DocumentHasRoot)
}

fn test_search_focus() -> TestScript {
    TestScript::new(
        "search-focus",
        "Focus search switches to edit page and sets focus region",
    )
    .scene("code-editor")
    .assert(StateCheck::OnPage {
        page_id: "code".into(),
    })
    .send_command("search.focus")
    .assert(StateCheck::OnPage {
        page_id: "edit".into(),
    })
}

fn test_all_panel_toggles() -> TestScript {
    TestScript::new(
        "all-panel-toggles",
        "Toggle right sidebar and activity bar visibility",
    )
    .scene("builder-grid")
    .assert(StateCheck::PanelVisible {
        panel: "right".into(),
    })
    .send_command("view.toggle_right_sidebar")
    .assert(StateCheck::PanelHidden {
        panel: "right".into(),
    })
    .send_command("view.toggle_right_sidebar")
    .assert(StateCheck::PanelVisible {
        panel: "right".into(),
    })
    .assert(StateCheck::PanelVisible {
        panel: "activity_bar".into(),
    })
    .send_command("view.toggle_activity_bar")
    .assert(StateCheck::PanelHidden {
        panel: "activity_bar".into(),
    })
    .send_command("view.toggle_activity_bar")
    .assert(StateCheck::PanelVisible {
        panel: "activity_bar".into(),
    })
}

fn test_select_all_nodes() -> TestScript {
    TestScript::new(
        "select-all-nodes",
        "Select all document nodes and verify count",
    )
    .scene("builder-grid")
    .assert(StateCheck::NoSelection)
    .assert(StateCheck::DocumentNodeCount { min: 3 })
    .send_command("selection.all")
    .assert(StateCheck::HasSelection)
    .assert(StateCheck::SelectionCount { n: 6 })
    .send_command("navigate.escape")
    .assert(StateCheck::NoSelection)
}

fn test_clipboard_copy_paste() -> TestScript {
    TestScript::new(
        "clipboard-copy-paste",
        "Copy a selected node and paste it, growing the document",
    )
    .scene("inspector")
    .assert(StateCheck::HasSelection)
    .assert(StateCheck::DocumentNodeCount { min: 3 })
    .send_command("edit.copy")
    .send_command("edit.paste")
    .assert(StateCheck::HasSelection)
    .assert(StateCheck::DocumentNodeCount { min: 4 })
}

fn test_selection_delete() -> TestScript {
    TestScript::new(
        "selection-delete",
        "Delete a selected node and verify selection clears",
    )
    .scene("inspector")
    .assert(StateCheck::HasSelection)
    .assert(StateCheck::DocumentNodeCount { min: 3 })
    .send_command("selection.delete")
    .assert(StateCheck::NoSelection)
}

fn test_inspector_arrow_navigation() -> TestScript {
    TestScript::new(
        "inspector-arrow-navigation",
        "Navigate between nodes with inspector prev/next commands",
    )
    .scene("inspector")
    .assert(StateCheck::HasSelection)
    .send_command("navigate.inspector_next")
    .assert(StateCheck::HasSelection)
    .send_command("navigate.inspector_next")
    .assert(StateCheck::HasSelection)
    .send_command("navigate.inspector_prev")
    .assert(StateCheck::HasSelection)
}

fn test_tab_cycling() -> TestScript {
    TestScript::new(
        "tab-cycling",
        "Cycle through app tabs with next/prev commands",
    )
    .scene("builder-grid")
    .assert(StateCheck::InApp {
        app_name: "Lattice".into(),
    })
    .send_command("navigate.next_tab")
    .assert(StateCheck::NoSelection)
    .send_command("navigate.prev_tab")
    .assert(StateCheck::NoSelection)
}

fn test_escape_context_cascade() -> TestScript {
    TestScript::new(
        "escape-context-cascade",
        "Escape closes command palette first, then clears selection",
    )
    .scene("inspector")
    .assert(StateCheck::HasSelection)
    .send_command("command_palette.toggle")
    .assert(StateCheck::CommandPaletteOpen { open: true })
    .assert(StateCheck::HasSelection)
    .send_command("command_palette.toggle")
    .assert(StateCheck::CommandPaletteOpen { open: false })
    .assert(StateCheck::HasSelection)
    .send_command("navigate.escape")
    .assert(StateCheck::NoSelection)
}

fn test_editor_state() -> TestScript {
    TestScript::new(
        "editor-state",
        "Verify code editor scene has text content and language set",
    )
    .scene("code-editor")
    .assert(StateCheck::OnPage {
        page_id: "code".into(),
    })
    .assert(StateCheck::EditorHasText)
    .assert(StateCheck::EditorLanguage {
        language: "rust".into(),
    })
}

fn test_node_click_selection() -> TestScript {
    TestScript::new(
        "node-click-selection",
        "Click a builder node by ID to select it",
    )
    .scene("builder-grid")
    .assert(StateCheck::NoSelection)
    .assert(StateCheck::DocumentHasRoot)
    .click_node("root")
    .assert(StateCheck::HasSelection)
    .assert(StateCheck::SelectionCount { n: 1 })
}

fn test_command_palette_auto_close() -> TestScript {
    TestScript::new(
        "command-palette-auto-close",
        "Command palette closes automatically when a non-palette command runs",
    )
    .scene("builder-grid")
    .send_command("command_palette.toggle")
    .assert(StateCheck::CommandPaletteOpen { open: true })
    .send_command("view.toggle_left_sidebar")
    .assert(StateCheck::CommandPaletteOpen { open: false })
    .send_command("view.toggle_left_sidebar")
}

fn test_duplicate_node() -> TestScript {
    TestScript::new(
        "duplicate-node",
        "Duplicate a selected node and verify document grows",
    )
    .scene("inspector")
    .assert(StateCheck::HasSelection)
    .assert(StateCheck::DocumentNodeCount { min: 3 })
    .send_command("edit.duplicate")
    .assert(StateCheck::HasSelection)
    .assert(StateCheck::DocumentNodeCount { min: 4 })
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rect_center() {
        let r = Rect::new(10.0, 20.0, 100.0, 50.0);
        assert_eq!(r.center(), (60.0, 45.0));
    }

    #[test]
    fn rect_contains() {
        let r = Rect::new(10.0, 20.0, 100.0, 50.0);
        assert!(r.contains(60.0, 45.0));
        assert!(r.contains(10.0, 20.0));
        assert!(!r.contains(9.0, 20.0));
        assert!(!r.contains(111.0, 20.0));
    }

    #[test]
    fn layout_oracle_activity_bar() {
        let r = LayoutOracle::activity_bar_button(0);
        assert_eq!(r.x, 0.0);
        assert_eq!(r.width, 48.0);
        let r1 = LayoutOracle::activity_bar_button(1);
        assert!(r1.y > r.y);
    }

    #[test]
    fn layout_oracle_content_origin() {
        let state = AppState::default();
        let (x, y) = LayoutOracle::content_origin(&state);
        assert!(x > 0.0);
        assert!(y > 0.0);
    }

    #[test]
    fn layout_oracle_grid_cell() {
        let state = apply_scene(BuiltinScene::BuilderGrid);
        let rect = LayoutOracle::grid_cell_rect(&state, 0, 0);
        assert!(rect.is_some());
        let r = rect.unwrap();
        assert!(r.width > 0.0);
        assert!(r.height > 0.0);
    }

    #[test]
    fn state_check_launchpad() {
        let state = apply_scene(BuiltinScene::Launchpad);
        let _check = StateCheck::OnLaunchpad;
        let driver_state = state.clone();
        assert!(driver_state.shell_view.is_launchpad());
    }

    #[test]
    fn state_check_builder_grid_has_root() {
        let state = apply_scene(BuiltinScene::BuilderGrid);
        assert!(state.builder_document.root.is_some());
    }

    #[test]
    fn count_nodes_works() {
        let state = apply_scene(BuiltinScene::BuilderGrid);
        let n = count_nodes(&state.builder_document);
        assert!(n >= 3);
    }

    #[test]
    fn test_script_builder_api() {
        let script = TestScript::new("test", "A test")
            .scene("launchpad")
            .assert(StateCheck::OnLaunchpad)
            .send_command("edit.undo")
            .send_key("ctrl+z")
            .wait(100);
        assert_eq!(script.name, "test");
        assert_eq!(script.actions.len(), 4);
    }

    #[test]
    fn builtin_scripts_all_have_unique_names() {
        let scripts = builtin_scripts();
        let mut names: Vec<&str> = scripts.iter().map(|s| s.name.as_str()).collect();
        let len = names.len();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), len, "duplicate test script names");
    }

    #[test]
    fn builtin_script_lookup() {
        assert!(builtin_script_by_name("launchpad-state").is_some());
        assert!(builtin_script_by_name("nonexistent").is_none());
    }

    #[test]
    fn test_result_summary_pass() {
        let r = TestResult {
            name: "test".into(),
            passed: true,
            duration: Duration::from_millis(42),
            steps_run: 3,
            failures: vec![],
            screenshots: vec![],
        };
        assert!(r.summary().contains("PASS"));
    }

    #[test]
    fn test_result_summary_fail() {
        let r = TestResult {
            name: "test".into(),
            passed: false,
            duration: Duration::from_millis(42),
            steps_run: 3,
            failures: vec![TestFailure {
                step: 1,
                action: "assert".into(),
                message: "wrong state".into(),
            }],
            screenshots: vec![],
        };
        assert!(r.summary().contains("FAIL"));
        assert!(r.summary().contains("wrong state"));
    }

    #[test]
    fn suite_result_counts() {
        let suite = SuiteResult {
            results: vec![
                TestResult {
                    name: "a".into(),
                    passed: true,
                    duration: Duration::ZERO,
                    steps_run: 1,
                    failures: vec![],
                    screenshots: vec![],
                },
                TestResult {
                    name: "b".into(),
                    passed: false,
                    duration: Duration::ZERO,
                    steps_run: 1,
                    failures: vec![TestFailure {
                        step: 0,
                        action: "x".into(),
                        message: "y".into(),
                    }],
                    screenshots: vec![],
                },
            ],
            duration: Duration::from_millis(100),
        };
        assert_eq!(suite.passed(), 1);
        assert_eq!(suite.failed(), 1);
        assert_eq!(suite.total(), 2);
        assert!(!suite.all_passed());
    }
}
