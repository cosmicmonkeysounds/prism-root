//! User-remappable keybinding overrides.
//!
//! Loads keybinding customizations from a JSON file and applies them as
//! a top-priority `InputScheme` on the `InputManager`. File format
//! matches `KeybindingContributionDef` from `prism_core::kernel::plugin`.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::input::{InputManager, InputScheme};
use crate::keyboard::KeyCombo;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeybindingOverride {
    pub key: String,
    pub command: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub when: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserKeybindings {
    #[serde(default)]
    pub overrides: Vec<KeybindingOverride>,
}

impl UserKeybindings {
    pub fn empty() -> Self {
        Self {
            overrides: Vec::new(),
        }
    }

    pub fn load(path: &Path) -> Self {
        match std::fs::read_to_string(path) {
            Ok(content) => serde_json::from_str(&content).unwrap_or_else(|e| {
                eprintln!(
                    "prism-shell: failed to parse keybindings at {}: {e}",
                    path.display()
                );
                Self::empty()
            }),
            Err(_) => Self::empty(),
        }
    }

    pub fn default_path() -> PathBuf {
        if let Ok(home) = std::env::var("HOME") {
            PathBuf::from(home).join(".prism").join("keybindings.json")
        } else {
            PathBuf::from(".prism").join("keybindings.json")
        }
    }

    pub fn apply_to(&self, input: &mut InputManager) {
        if self.overrides.is_empty() {
            return;
        }
        let mut builder = InputScheme::builder("user.overrides").label("User Overrides");
        for ov in &self.overrides {
            if KeyCombo::parse(&ov.key).is_none() {
                eprintln!(
                    "prism-shell: invalid key combo in user keybindings: {:?}",
                    ov.key
                );
                continue;
            }
            if let Some(ref when) = ov.when {
                builder = builder.bind_when(&ov.key, &ov.command, when);
            } else {
                builder = builder.bind(&ov.key, &ov.command);
            }
        }
        input.register(builder.build());
        input.push("user.overrides");
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        std::fs::write(path, json)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_overrides_is_noop() {
        let kb = UserKeybindings::empty();
        let mut input = InputManager::new();
        kb.apply_to(&mut input);
        assert!(input.active_schemes().is_empty() || !input.is_active("user.overrides"));
    }

    #[test]
    fn parse_overrides_from_json() {
        let json = r#"{"overrides":[{"key":"ctrl+k","command":"view.zoom_in"},{"key":"ctrl+shift+k","command":"view.zoom_out","when":"focus.canvas"}]}"#;
        let kb: UserKeybindings = serde_json::from_str(json).unwrap();
        assert_eq!(kb.overrides.len(), 2);
        assert_eq!(kb.overrides[0].command, "view.zoom_in");
        assert_eq!(kb.overrides[1].when.as_deref(), Some("focus.canvas"));
    }

    #[test]
    fn apply_pushes_user_scheme() {
        let json = r#"{"overrides":[{"key":"ctrl+k","command":"view.zoom_in"}]}"#;
        let kb: UserKeybindings = serde_json::from_str(json).unwrap();
        let mut input = InputManager::new();
        kb.apply_to(&mut input);
        assert!(input.is_active("user.overrides"));
        let combo = KeyCombo::parse("ctrl+k").unwrap();
        assert_eq!(input.dispatch(&combo), Some("view.zoom_in"));
    }

    #[test]
    fn invalid_key_combo_skipped() {
        let json =
            r#"{"overrides":[{"key":"","command":"noop"},{"key":"ctrl+z","command":"edit.undo"}]}"#;
        let kb: UserKeybindings = serde_json::from_str(json).unwrap();
        let mut input = InputManager::new();
        kb.apply_to(&mut input);
        let combo = KeyCombo::parse("ctrl+z").unwrap();
        assert_eq!(input.dispatch(&combo), Some("edit.undo"));
    }

    #[test]
    fn user_overrides_take_priority() {
        let json = r#"{"overrides":[{"key":"ctrl+z","command":"custom.action"}]}"#;
        let kb: UserKeybindings = serde_json::from_str(json).unwrap();
        let mut input = InputManager::with_defaults();
        kb.apply_to(&mut input);
        let combo = KeyCombo::parse("ctrl+z").unwrap();
        assert_eq!(input.dispatch(&combo), Some("custom.action"));
    }
}
