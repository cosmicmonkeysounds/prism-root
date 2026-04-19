use std::collections::HashMap;
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Modifiers {
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
    pub meta: bool,
}

impl Modifiers {
    pub fn none() -> Self {
        Self::default()
    }

    pub fn ctrl() -> Self {
        Self {
            ctrl: true,
            ..Default::default()
        }
    }

    pub fn is_empty(&self) -> bool {
        !self.ctrl && !self.shift && !self.alt && !self.meta
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct KeyCombo {
    pub key: String,
    pub modifiers: Modifiers,
}

impl KeyCombo {
    pub fn new(key: impl Into<String>, modifiers: Modifiers) -> Self {
        Self {
            key: key.into().to_lowercase(),
            modifiers,
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        if s.is_empty() {
            return None;
        }
        let parts: Vec<&str> = s.split('+').map(str::trim).collect();
        let mut modifiers = Modifiers::default();
        let mut key = None;
        for part in &parts {
            match part.to_lowercase().as_str() {
                "ctrl" | "control" => modifiers.ctrl = true,
                "shift" => modifiers.shift = true,
                "alt" | "option" => modifiers.alt = true,
                "meta" | "cmd" | "command" | "super" | "win" => modifiers.meta = true,
                k => key = Some(k.to_string()),
            }
        }
        key.map(|k| Self { key: k, modifiers })
    }
}

impl fmt::Display for KeyCombo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut parts = Vec::new();
        if self.modifiers.ctrl {
            parts.push("Ctrl");
        }
        if self.modifiers.shift {
            parts.push("Shift");
        }
        if self.modifiers.alt {
            parts.push("Alt");
        }
        if self.modifiers.meta {
            parts.push("Cmd");
        }
        parts.push(&self.key);
        write!(f, "{}", parts.join("+"))
    }
}

#[derive(Debug, Clone)]
pub struct KeyBinding {
    pub combo: KeyCombo,
    pub command: String,
    pub when: Option<String>,
}

impl KeyBinding {
    pub fn new(combo: KeyCombo, command: impl Into<String>) -> Self {
        Self {
            combo,
            command: command.into(),
            when: None,
        }
    }

    pub fn when(mut self, condition: impl Into<String>) -> Self {
        self.when = Some(condition.into());
        self
    }
}

pub struct KeyboardModel {
    bindings: Vec<KeyBinding>,
    context: HashMap<String, bool>,
}

impl KeyboardModel {
    pub fn new() -> Self {
        Self {
            bindings: Vec::new(),
            context: HashMap::new(),
        }
    }

    pub fn with_defaults() -> Self {
        let mut model = Self::new();
        model.register(KeyBinding::new(
            KeyCombo::parse("ctrl+shift+p").unwrap(),
            "command_palette.toggle",
        ));
        model.register(KeyBinding::new(
            KeyCombo::parse("ctrl+z").unwrap(),
            "edit.undo",
        ));
        model.register(KeyBinding::new(
            KeyCombo::parse("ctrl+shift+z").unwrap(),
            "edit.redo",
        ));
        model.register(KeyBinding::new(
            KeyCombo::parse("ctrl+y").unwrap(),
            "edit.redo",
        ));
        model.register(KeyBinding::new(
            KeyCombo::parse("ctrl+s").unwrap(),
            "file.save",
        ));
        model.register(KeyBinding::new(
            KeyCombo::parse("ctrl+f").unwrap(),
            "search.focus",
        ));
        model.register(
            KeyBinding::new(KeyCombo::parse("escape").unwrap(), "command_palette.close")
                .when("commandPaletteOpen"),
        );
        model.register(
            KeyBinding::new(KeyCombo::parse("delete").unwrap(), "selection.delete")
                .when("hasSelection"),
        );
        model.register(KeyBinding::new(
            KeyCombo::parse("ctrl+a").unwrap(),
            "selection.all",
        ));
        model.register(
            KeyBinding::new(KeyCombo::parse("ctrl+c").unwrap(), "edit.copy").when("hasSelection"),
        );
        model.register(
            KeyBinding::new(KeyCombo::parse("ctrl+v").unwrap(), "edit.paste").when("hasClipboard"),
        );
        model.register(
            KeyBinding::new(KeyCombo::parse("ctrl+x").unwrap(), "edit.cut").when("hasSelection"),
        );
        model.register(
            KeyBinding::new(KeyCombo::parse("ctrl+d").unwrap(), "edit.duplicate")
                .when("hasSelection"),
        );
        model
    }

    pub fn register(&mut self, binding: KeyBinding) {
        self.bindings.push(binding);
    }

    pub fn set_context(&mut self, key: impl Into<String>, value: bool) {
        self.context.insert(key.into(), value);
    }

    pub fn resolve(&self, combo: &KeyCombo) -> Option<&str> {
        for binding in self.bindings.iter().rev() {
            if binding.combo != *combo {
                continue;
            }
            if let Some(ref when) = binding.when {
                let negated = when.starts_with('!');
                let ctx_key = if negated { &when[1..] } else { when.as_str() };
                let ctx_val = self.context.get(ctx_key).copied().unwrap_or(false);
                let passes = if negated { !ctx_val } else { ctx_val };
                if !passes {
                    continue;
                }
            }
            return Some(&binding.command);
        }
        None
    }

    pub fn bindings(&self) -> &[KeyBinding] {
        &self.bindings
    }

    pub fn bindings_for_command(&self, command: &str) -> Vec<&KeyBinding> {
        self.bindings
            .iter()
            .filter(|b| b.command == command)
            .collect()
    }

    pub fn shortcut_label(&self, command: &str) -> Option<String> {
        self.bindings_for_command(command)
            .first()
            .map(|b| b.combo.to_string())
    }
}

impl Default for KeyboardModel {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_simple_key() {
        let combo = KeyCombo::parse("a").unwrap();
        assert_eq!(combo.key, "a");
        assert!(combo.modifiers.is_empty());
    }

    #[test]
    fn parse_ctrl_shift_p() {
        let combo = KeyCombo::parse("ctrl+shift+p").unwrap();
        assert_eq!(combo.key, "p");
        assert!(combo.modifiers.ctrl);
        assert!(combo.modifiers.shift);
        assert!(!combo.modifiers.alt);
        assert!(!combo.modifiers.meta);
    }

    #[test]
    fn parse_cmd_alias() {
        let combo = KeyCombo::parse("cmd+s").unwrap();
        assert!(combo.modifiers.meta);
        assert_eq!(combo.key, "s");
    }

    #[test]
    fn parse_empty_returns_none() {
        assert!(KeyCombo::parse("").is_none());
    }

    #[test]
    fn parse_only_modifiers_returns_none() {
        assert!(KeyCombo::parse("ctrl+shift").is_none());
    }

    #[test]
    fn display_roundtrips() {
        let combo = KeyCombo::parse("ctrl+shift+p").unwrap();
        assert_eq!(combo.to_string(), "Ctrl+Shift+p");
    }

    #[test]
    fn resolve_finds_matching_binding() {
        let mut model = KeyboardModel::new();
        model.register(KeyBinding::new(
            KeyCombo::parse("ctrl+z").unwrap(),
            "edit.undo",
        ));
        let combo = KeyCombo::parse("ctrl+z").unwrap();
        assert_eq!(model.resolve(&combo), Some("edit.undo"));
    }

    #[test]
    fn resolve_returns_none_for_unbound() {
        let model = KeyboardModel::new();
        let combo = KeyCombo::parse("ctrl+z").unwrap();
        assert_eq!(model.resolve(&combo), None);
    }

    #[test]
    fn resolve_respects_when_context() {
        let mut model = KeyboardModel::new();
        model.register(
            KeyBinding::new(KeyCombo::parse("escape").unwrap(), "palette.close")
                .when("paletteOpen"),
        );
        let esc = KeyCombo::parse("escape").unwrap();

        assert_eq!(model.resolve(&esc), None);

        model.set_context("paletteOpen", true);
        assert_eq!(model.resolve(&esc), Some("palette.close"));

        model.set_context("paletteOpen", false);
        assert_eq!(model.resolve(&esc), None);
    }

    #[test]
    fn resolve_respects_negated_when() {
        let mut model = KeyboardModel::new();
        model.register(
            KeyBinding::new(KeyCombo::parse("escape").unwrap(), "deselect").when("!paletteOpen"),
        );
        let esc = KeyCombo::parse("escape").unwrap();

        assert_eq!(model.resolve(&esc), Some("deselect"));

        model.set_context("paletteOpen", true);
        assert_eq!(model.resolve(&esc), None);
    }

    #[test]
    fn later_binding_wins() {
        let mut model = KeyboardModel::new();
        model.register(KeyBinding::new(KeyCombo::parse("ctrl+z").unwrap(), "first"));
        model.register(KeyBinding::new(
            KeyCombo::parse("ctrl+z").unwrap(),
            "second",
        ));
        let combo = KeyCombo::parse("ctrl+z").unwrap();
        assert_eq!(model.resolve(&combo), Some("second"));
    }

    #[test]
    fn bindings_for_command() {
        let mut model = KeyboardModel::new();
        model.register(KeyBinding::new(
            KeyCombo::parse("ctrl+z").unwrap(),
            "edit.undo",
        ));
        model.register(KeyBinding::new(
            KeyCombo::parse("ctrl+shift+z").unwrap(),
            "edit.redo",
        ));
        let undo = model.bindings_for_command("edit.undo");
        assert_eq!(undo.len(), 1);
        assert_eq!(undo[0].combo.key, "z");
    }

    #[test]
    fn shortcut_label() {
        let model = KeyboardModel::with_defaults();
        let label = model.shortcut_label("edit.undo");
        assert!(label.is_some());
    }

    #[test]
    fn with_defaults_registers_standard_bindings() {
        let model = KeyboardModel::with_defaults();
        assert!(!model.bindings().is_empty());
        let combo = KeyCombo::parse("ctrl+z").unwrap();
        assert_eq!(model.resolve(&combo), Some("edit.undo"));
    }
}
