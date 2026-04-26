//! Layered input manager with composable key-binding schemes.
//!
//! Apps, editors, and panels register [`InputScheme`]s via the builder
//! pattern and push/pop them onto the [`InputManager`]'s active stack.
//! Key events dispatch top-down through the stack until a binding matches,
//! then the matched command ID goes to `execute_command`.

use std::collections::HashMap;

use crate::keyboard::{KeyBinding, KeyCombo, Modifiers};

// ── Focus region ──────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum FocusRegion {
    #[default]
    Shell,
    ActivityBar,
    Sidebar,
    Canvas,
    Inspector,
    Properties,
    TabBar,
    Search,
    CommandPalette,
    CodeEditor,
}

impl FocusRegion {
    fn context_key(self) -> &'static str {
        match self {
            Self::Shell => "focus.shell",
            Self::ActivityBar => "focus.activityBar",
            Self::Sidebar => "focus.sidebar",
            Self::Canvas => "focus.canvas",
            Self::Inspector => "focus.inspector",
            Self::Properties => "focus.properties",
            Self::TabBar => "focus.tabBar",
            Self::Search => "focus.search",
            Self::CommandPalette => "focus.commandPalette",
            Self::CodeEditor => "focus.codeEditor",
        }
    }

    fn all() -> &'static [Self] {
        &[
            Self::Shell,
            Self::ActivityBar,
            Self::Sidebar,
            Self::Canvas,
            Self::Inspector,
            Self::Properties,
            Self::TabBar,
            Self::Search,
            Self::CommandPalette,
            Self::CodeEditor,
        ]
    }
}

// ── InputScheme ───────────────────────────────────────────────────

pub struct InputScheme {
    id: String,
    label: String,
    bindings: Vec<KeyBinding>,
}

impl InputScheme {
    pub fn builder(id: impl Into<String>) -> InputSchemeBuilder {
        let id = id.into();
        InputSchemeBuilder {
            label: id.clone(),
            id,
            bindings: Vec::new(),
        }
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn label(&self) -> &str {
        &self.label
    }

    pub fn bindings(&self) -> &[KeyBinding] {
        &self.bindings
    }

    fn resolve(&self, combo: &KeyCombo, context: &HashMap<String, bool>) -> Option<&str> {
        for binding in self.bindings.iter().rev() {
            if binding.combo != *combo {
                continue;
            }
            if let Some(ref when) = binding.when {
                let negated = when.starts_with('!');
                let ctx_key = if negated { &when[1..] } else { when.as_str() };
                let ctx_val = context.get(ctx_key).copied().unwrap_or(false);
                if negated == ctx_val {
                    continue;
                }
            }
            return Some(&binding.command);
        }
        None
    }
}

// ── Builder ───────────────────────────────────────────────────────

pub struct InputSchemeBuilder {
    id: String,
    label: String,
    bindings: Vec<KeyBinding>,
}

impl InputSchemeBuilder {
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = label.into();
        self
    }

    pub fn bind(mut self, combo: &str, command: impl Into<String>) -> Self {
        if let Some(kc) = KeyCombo::parse(combo) {
            self.bindings.push(KeyBinding::new(kc, command));
        }
        self
    }

    pub fn bind_when(
        mut self,
        combo: &str,
        command: impl Into<String>,
        when: impl Into<String>,
    ) -> Self {
        if let Some(kc) = KeyCombo::parse(combo) {
            self.bindings.push(KeyBinding::new(kc, command).when(when));
        }
        self
    }

    pub fn build(self) -> InputScheme {
        InputScheme {
            id: self.id,
            label: self.label,
            bindings: self.bindings,
        }
    }
}

// ── InputManager ──────────────────────────────────────────────────

pub struct InputManager {
    registry: HashMap<String, InputScheme>,
    active_stack: Vec<String>,
    context: HashMap<String, bool>,
    focus: FocusRegion,
}

impl InputManager {
    pub fn new() -> Self {
        Self {
            registry: HashMap::new(),
            active_stack: Vec::new(),
            context: HashMap::new(),
            focus: FocusRegion::Shell,
        }
    }

    pub fn with_defaults() -> Self {
        let mut mgr = Self::new();

        mgr.register(
            InputScheme::builder("shell.base")
                .label("Shell")
                // Command palette
                .bind("ctrl+shift+p", "command_palette.toggle")
                // Edit operations
                .bind("ctrl+z", "edit.undo")
                .bind("ctrl+shift+z", "edit.redo")
                .bind("ctrl+y", "edit.redo")
                .bind("ctrl+s", "file.save")
                .bind("ctrl+a", "selection.all")
                .bind_when("ctrl+c", "edit.copy", "hasSelection")
                .bind_when("ctrl+v", "edit.paste", "hasClipboard")
                .bind_when("ctrl+x", "edit.cut", "hasSelection")
                .bind_when("ctrl+d", "edit.duplicate", "hasSelection")
                // Search
                .bind("ctrl+f", "search.focus")
                // Tab navigation
                .bind("ctrl+tab", "navigate.next_tab")
                .bind("ctrl+shift+tab", "navigate.prev_tab")
                .bind("ctrl+1", "navigate.tab.1")
                .bind("ctrl+2", "navigate.tab.2")
                .bind("ctrl+3", "navigate.tab.3")
                .bind("ctrl+4", "navigate.tab.4")
                .bind("ctrl+5", "navigate.tab.5")
                .bind("ctrl+6", "navigate.tab.6")
                .bind("ctrl+7", "navigate.tab.7")
                .bind("ctrl+8", "navigate.tab.8")
                .bind("ctrl+9", "navigate.tab.9")
                // Sidebar visibility
                .bind("ctrl+b", "view.toggle_left_sidebar")
                .bind("ctrl+shift+b", "view.toggle_right_sidebar")
                // Zoom
                .bind("ctrl+=", "view.zoom_in")
                .bind("ctrl+-", "view.zoom_out")
                .bind("ctrl+0", "view.zoom_reset")
                .bind("ctrl+shift+0", "view.zoom_to_fit")
                // Escape: palette first, then general
                .bind_when("escape", "command_palette.close", "commandPaletteOpen")
                .bind_when("escape", "navigate.escape", "!commandPaletteOpen")
                .build(),
        );

        mgr.register(
            InputScheme::builder("shell.edit")
                .label("Edit Panel")
                .bind_when("delete", "selection.delete", "hasSelection")
                .bind_when("backspace", "selection.delete", "hasSelection")
                .bind("up", "navigate.inspector_prev")
                .bind("down", "navigate.inspector_next")
                .bind("w", "tool.move")
                .bind("e", "tool.rotate")
                .bind("r", "tool.scale")
                .build(),
        );

        mgr.register(
            InputScheme::builder("shell.search")
                .label("Search")
                .bind("escape", "search.blur")
                .bind("return", "search.confirm")
                .build(),
        );

        mgr.push("shell.base");

        mgr
    }

    // ── Registration ──────────────────────────────────────────────

    pub fn register(&mut self, scheme: InputScheme) {
        self.registry.insert(scheme.id.clone(), scheme);
    }

    pub fn push(&mut self, scheme_id: &str) -> bool {
        if !self.registry.contains_key(scheme_id) {
            return false;
        }
        if !self.active_stack.iter().any(|id| id == scheme_id) {
            self.active_stack.push(scheme_id.into());
        }
        true
    }

    pub fn pop(&mut self, scheme_id: &str) -> bool {
        let before = self.active_stack.len();
        self.active_stack.retain(|id| id != scheme_id);
        self.active_stack.len() != before
    }

    pub fn is_active(&self, scheme_id: &str) -> bool {
        self.active_stack.iter().any(|id| id == scheme_id)
    }

    pub fn active_schemes(&self) -> &[String] {
        &self.active_stack
    }

    // ── Focus ─────────────────────────────────────────────────────

    pub fn set_focus(&mut self, region: FocusRegion) {
        self.focus = region;
        for r in FocusRegion::all() {
            self.context.insert(r.context_key().into(), *r == region);
        }
    }

    pub fn focus(&self) -> FocusRegion {
        self.focus
    }

    // ── Context ───────────────────────────────────────────────────

    pub fn set_context(&mut self, key: impl Into<String>, value: bool) {
        self.context.insert(key.into(), value);
    }

    pub fn context_value(&self, key: &str) -> bool {
        self.context.get(key).copied().unwrap_or(false)
    }

    // ── Dispatch ──────────────────────────────────────────────────

    pub fn dispatch(&self, combo: &KeyCombo) -> Option<&str> {
        for scheme_id in self.active_stack.iter().rev() {
            if let Some(scheme) = self.registry.get(scheme_id) {
                if let Some(cmd) = scheme.resolve(combo, &self.context) {
                    return Some(cmd);
                }
            }
        }
        None
    }

    pub fn bindings_for_command(&self, command: &str) -> Vec<&KeyBinding> {
        let mut result = Vec::new();
        for scheme_id in &self.active_stack {
            if let Some(scheme) = self.registry.get(scheme_id) {
                for binding in &scheme.bindings {
                    if binding.command == command {
                        result.push(binding);
                    }
                }
            }
        }
        result
    }

    pub fn shortcut_label(&self, command: &str) -> Option<String> {
        self.bindings_for_command(command)
            .first()
            .map(|b| b.combo.to_string())
    }

    pub fn scheme(&self, id: &str) -> Option<&InputScheme> {
        self.registry.get(id)
    }

    pub fn registered_schemes(&self) -> Vec<&str> {
        self.registry.keys().map(String::as_str).collect()
    }
}

impl Default for InputManager {
    fn default() -> Self {
        Self::new()
    }
}

// ── Slint key translation ─────────────────────────────────────────

pub fn combo_from_slint(
    text: &str,
    ctrl: bool,
    shift: bool,
    alt: bool,
    meta: bool,
) -> Option<KeyCombo> {
    let ch = text.chars().next()?;
    let modifiers = Modifiers {
        ctrl,
        shift,
        alt,
        meta,
    };

    let key: &str = match ch {
        '\u{001B}' => "escape",
        '\u{0009}' => "tab",
        '\u{000D}' => "return",
        '\u{007F}' | '\u{F728}' => "delete",
        '\u{0008}' => "backspace",
        ' ' => "space",
        '\u{F700}' => "up",
        '\u{F701}' => "down",
        '\u{F702}' => "left",
        '\u{F703}' => "right",
        '\u{F729}' => "home",
        '\u{F72B}' => "end",
        '\u{F72C}' => "pageup",
        '\u{F72D}' => "pagedown",
        c @ '\u{F704}'..='\u{F70F}' => {
            let n = c as u32 - 0xF704 + 1;
            let key = match n {
                1 => "f1",
                2 => "f2",
                3 => "f3",
                4 => "f4",
                5 => "f5",
                6 => "f6",
                7 => "f7",
                8 => "f8",
                9 => "f9",
                10 => "f10",
                11 => "f11",
                12 => "f12",
                _ => return None,
            };
            return Some(KeyCombo::new(key, modifiers));
        }
        c if ctrl && c.is_ascii_control() && (1..=26).contains(&(c as u8)) => {
            let letter = (c as u8 + b'a' - 1) as char;
            return Some(KeyCombo::new(letter.to_string(), modifiers));
        }
        c if c.is_alphanumeric() || c.is_ascii_punctuation() => {
            return Some(KeyCombo::new(c.to_lowercase().to_string(), modifiers));
        }
        _ => return None,
    };

    Some(KeyCombo::new(key, modifiers))
}

// ── Panel scheme management ───────────────────────────────────────

pub fn update_panel_schemes(input: &mut InputManager, panel_id: i32) {
    input.pop("shell.edit");
    input.pop("shell.search");
    if panel_id == 1 {
        input.push("shell.edit");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builder_creates_scheme_with_bindings() {
        let scheme = InputScheme::builder("test")
            .label("Test Scheme")
            .bind("ctrl+z", "undo")
            .bind_when("delete", "delete_item", "hasSelection")
            .build();
        assert_eq!(scheme.id(), "test");
        assert_eq!(scheme.label(), "Test Scheme");
        assert_eq!(scheme.bindings().len(), 2);
    }

    #[test]
    fn builder_skips_invalid_combo() {
        let scheme = InputScheme::builder("test")
            .bind("", "nothing")
            .bind("ctrl+z", "undo")
            .build();
        assert_eq!(scheme.bindings().len(), 1);
    }

    #[test]
    fn register_and_push() {
        let mut mgr = InputManager::new();
        mgr.register(InputScheme::builder("test").bind("ctrl+z", "undo").build());
        assert!(mgr.push("test"));
        assert!(mgr.is_active("test"));
    }

    #[test]
    fn push_unknown_returns_false() {
        let mut mgr = InputManager::new();
        assert!(!mgr.push("nonexistent"));
    }

    #[test]
    fn push_is_idempotent() {
        let mut mgr = InputManager::new();
        mgr.register(InputScheme::builder("test").build());
        mgr.push("test");
        mgr.push("test");
        assert_eq!(mgr.active_schemes().len(), 1);
    }

    #[test]
    fn pop_removes_scheme() {
        let mut mgr = InputManager::new();
        mgr.register(InputScheme::builder("test").build());
        mgr.push("test");
        assert!(mgr.pop("test"));
        assert!(!mgr.is_active("test"));
    }

    #[test]
    fn pop_unknown_returns_false() {
        let mut mgr = InputManager::new();
        assert!(!mgr.pop("nonexistent"));
    }

    #[test]
    fn dispatch_top_layer_wins() {
        let mut mgr = InputManager::new();
        mgr.register(
            InputScheme::builder("base")
                .bind("ctrl+z", "base.undo")
                .build(),
        );
        mgr.register(
            InputScheme::builder("layer")
                .bind("ctrl+z", "layer.undo")
                .build(),
        );
        mgr.push("base");
        mgr.push("layer");
        let combo = KeyCombo::parse("ctrl+z").unwrap();
        assert_eq!(mgr.dispatch(&combo), Some("layer.undo"));
    }

    #[test]
    fn dispatch_falls_through_to_lower_layer() {
        let mut mgr = InputManager::new();
        mgr.register(InputScheme::builder("base").bind("ctrl+z", "undo").build());
        mgr.register(InputScheme::builder("layer").bind("ctrl+x", "cut").build());
        mgr.push("base");
        mgr.push("layer");
        let combo = KeyCombo::parse("ctrl+z").unwrap();
        assert_eq!(mgr.dispatch(&combo), Some("undo"));
    }

    #[test]
    fn dispatch_returns_none_for_unbound() {
        let mgr = InputManager::with_defaults();
        let combo = KeyCombo::parse("ctrl+shift+alt+q").unwrap();
        assert_eq!(mgr.dispatch(&combo), None);
    }

    #[test]
    fn dispatch_respects_when_context() {
        let mut mgr = InputManager::new();
        mgr.register(
            InputScheme::builder("base")
                .bind_when("delete", "sel.delete", "hasSelection")
                .build(),
        );
        mgr.push("base");
        let del = KeyCombo::parse("delete").unwrap();

        assert_eq!(mgr.dispatch(&del), None);

        mgr.set_context("hasSelection", true);
        assert_eq!(mgr.dispatch(&del), Some("sel.delete"));
    }

    #[test]
    fn dispatch_respects_negated_context() {
        let mut mgr = InputManager::new();
        mgr.register(
            InputScheme::builder("base")
                .bind_when("escape", "palette.close", "paletteOpen")
                .bind_when("escape", "deselect", "!paletteOpen")
                .build(),
        );
        mgr.push("base");
        let esc = KeyCombo::parse("escape").unwrap();

        assert_eq!(mgr.dispatch(&esc), Some("deselect"));

        mgr.set_context("paletteOpen", true);
        assert_eq!(mgr.dispatch(&esc), Some("palette.close"));
    }

    #[test]
    fn focus_region_sets_context_flags() {
        let mut mgr = InputManager::new();
        mgr.set_focus(FocusRegion::Inspector);
        assert!(mgr.context_value("focus.inspector"));
        assert!(!mgr.context_value("focus.shell"));
        assert!(!mgr.context_value("focus.codeEditor"));
    }

    #[test]
    fn shortcut_label_lookup() {
        let mgr = InputManager::with_defaults();
        let label = mgr.shortcut_label("edit.undo");
        assert!(label.is_some());
        assert!(label.unwrap().contains("Ctrl"));
    }

    #[test]
    fn bindings_for_command_across_layers() {
        let mut mgr = InputManager::new();
        mgr.register(
            InputScheme::builder("a")
                .bind("ctrl+z", "undo")
                .bind("ctrl+y", "undo")
                .build(),
        );
        mgr.push("a");
        assert_eq!(mgr.bindings_for_command("undo").len(), 2);
    }

    #[test]
    fn with_defaults_has_base_active() {
        let mgr = InputManager::with_defaults();
        assert!(mgr.is_active("shell.base"));
        assert!(!mgr.is_active("shell.edit"));
    }

    #[test]
    fn with_defaults_resolves_ctrl_z() {
        let mgr = InputManager::with_defaults();
        let combo = KeyCombo::parse("ctrl+z").unwrap();
        assert_eq!(mgr.dispatch(&combo), Some("edit.undo"));
    }

    #[test]
    fn with_defaults_tab_navigation() {
        let mgr = InputManager::with_defaults();
        let combo = KeyCombo::parse("ctrl+tab").unwrap();
        assert_eq!(mgr.dispatch(&combo), Some("navigate.next_tab"));
        let combo = KeyCombo::parse("ctrl+shift+tab").unwrap();
        assert_eq!(mgr.dispatch(&combo), Some("navigate.prev_tab"));
    }

    #[test]
    fn with_defaults_tab_numbers() {
        let mgr = InputManager::with_defaults();
        for n in 1..=9 {
            let combo = KeyCombo::parse(&format!("ctrl+{n}")).unwrap();
            assert_eq!(
                mgr.dispatch(&combo),
                Some(format!("navigate.tab.{n}").as_str())
            );
        }
    }

    #[test]
    fn edit_scheme_inspector_nav() {
        let mut mgr = InputManager::with_defaults();
        mgr.push("shell.edit");
        let up = KeyCombo::parse("up").unwrap();
        let down = KeyCombo::parse("down").unwrap();
        assert_eq!(mgr.dispatch(&up), Some("navigate.inspector_prev"));
        assert_eq!(mgr.dispatch(&down), Some("navigate.inspector_next"));
    }

    #[test]
    fn update_panel_schemes_pushes_edit() {
        let mut mgr = InputManager::with_defaults();
        update_panel_schemes(&mut mgr, 1);
        assert!(mgr.is_active("shell.edit"));
        update_panel_schemes(&mut mgr, 2);
        assert!(!mgr.is_active("shell.edit"));
    }

    // ── combo_from_slint tests ────────────────────────────────────

    #[test]
    fn slint_ctrl_z() {
        let combo = combo_from_slint("\u{001a}", true, false, false, false).unwrap();
        assert_eq!(combo.key, "z");
        assert!(combo.modifiers.ctrl);
        assert!(!combo.modifiers.shift);
    }

    #[test]
    fn slint_ctrl_shift_z() {
        let combo = combo_from_slint("\u{001a}", true, true, false, false).unwrap();
        assert_eq!(combo.key, "z");
        assert!(combo.modifiers.ctrl);
        assert!(combo.modifiers.shift);
    }

    #[test]
    fn slint_ctrl_shift_p() {
        let combo = combo_from_slint("P", true, true, false, false).unwrap();
        assert_eq!(combo.key, "p");
        assert!(combo.modifiers.ctrl);
        assert!(combo.modifiers.shift);
    }

    #[test]
    fn slint_escape() {
        let combo = combo_from_slint("\u{001B}", false, false, false, false).unwrap();
        assert_eq!(combo.key, "escape");
        assert!(combo.modifiers.is_empty());
    }

    #[test]
    fn slint_delete() {
        let combo = combo_from_slint("\u{007F}", false, false, false, false).unwrap();
        assert_eq!(combo.key, "delete");
    }

    #[test]
    fn slint_arrow_up() {
        let combo = combo_from_slint("\u{F700}", false, false, false, false).unwrap();
        assert_eq!(combo.key, "up");
    }

    #[test]
    fn slint_arrow_down() {
        let combo = combo_from_slint("\u{F701}", false, false, false, false).unwrap();
        assert_eq!(combo.key, "down");
    }

    #[test]
    fn slint_tab() {
        let combo = combo_from_slint("\u{0009}", false, false, false, false).unwrap();
        assert_eq!(combo.key, "tab");
        assert!(combo.modifiers.is_empty());
    }

    #[test]
    fn slint_ctrl_tab() {
        let combo = combo_from_slint("\u{0009}", true, false, false, false).unwrap();
        assert_eq!(combo.key, "tab");
        assert!(combo.modifiers.ctrl);
    }

    #[test]
    fn slint_ctrl_digit() {
        let combo = combo_from_slint("1", true, false, false, false).unwrap();
        assert_eq!(combo.key, "1");
        assert!(combo.modifiers.ctrl);
    }

    #[test]
    fn slint_plain_letter() {
        let combo = combo_from_slint("a", false, false, false, false).unwrap();
        assert_eq!(combo.key, "a");
        assert!(combo.modifiers.is_empty());
    }

    #[test]
    fn slint_f1() {
        let combo = combo_from_slint("\u{F704}", false, false, false, false).unwrap();
        assert_eq!(combo.key, "f1");
    }

    #[test]
    fn slint_unknown_returns_none() {
        assert!(combo_from_slint("\u{FFFE}", false, false, false, false).is_none());
    }

    #[test]
    fn roundtrip_dispatch() {
        let mgr = InputManager::with_defaults();
        let combo = combo_from_slint("\u{001a}", true, false, false, false).unwrap();
        assert_eq!(mgr.dispatch(&combo), Some("edit.undo"));
    }

    #[test]
    fn roundtrip_ctrl_shift_p() {
        let mgr = InputManager::with_defaults();
        let combo = combo_from_slint("P", true, true, false, false).unwrap();
        assert_eq!(mgr.dispatch(&combo), Some("command_palette.toggle"));
    }

    #[test]
    fn app_registers_custom_scheme() {
        let mut mgr = InputManager::with_defaults();
        mgr.register(
            InputScheme::builder("myapp.editor")
                .label("My App")
                .bind("ctrl+b", "myapp.bold")
                .build(),
        );
        mgr.push("myapp.editor");
        let combo = KeyCombo::parse("ctrl+b").unwrap();
        assert_eq!(mgr.dispatch(&combo), Some("myapp.bold"));
        // base bindings still work
        let combo = KeyCombo::parse("ctrl+z").unwrap();
        assert_eq!(mgr.dispatch(&combo), Some("edit.undo"));
    }

    #[test]
    fn app_scheme_override_and_restore() {
        let mut mgr = InputManager::with_defaults();
        mgr.register(
            InputScheme::builder("myapp")
                .bind("ctrl+z", "myapp.undo")
                .build(),
        );
        mgr.push("myapp");
        let combo = KeyCombo::parse("ctrl+z").unwrap();
        assert_eq!(mgr.dispatch(&combo), Some("myapp.undo"));

        mgr.pop("myapp");
        assert_eq!(mgr.dispatch(&combo), Some("edit.undo"));
    }
}
