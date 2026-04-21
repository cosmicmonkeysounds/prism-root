//! `ConfigRegistry` — central catalog of every known `SettingDefinition`
//! and `FeatureFlagDefinition`. Instance-based, not a singleton; each
//! workspace gets its own. Port of
//! `@prism/core/kernel/config/config-registry.ts`.

use indexmap::IndexMap;
use serde_json::{json, Value as JsonValue};

use super::types::{
    FeatureFlagDefinition, SettingDefinition, SettingScope, SettingType, SettingValidator,
};

pub struct ConfigRegistry {
    settings: IndexMap<String, SettingDefinition>,
    flags: IndexMap<String, FeatureFlagDefinition>,
}

impl ConfigRegistry {
    pub fn new() -> Self {
        let mut reg = Self {
            settings: IndexMap::new(),
            flags: IndexMap::new(),
        };
        reg.reset();
        reg
    }

    pub fn register(&mut self, def: SettingDefinition) {
        self.settings.insert(def.key.clone(), def);
    }

    pub fn register_all(&mut self, defs: impl IntoIterator<Item = SettingDefinition>) {
        for def in defs {
            self.register(def);
        }
    }

    pub fn get(&self, key: &str) -> Option<&SettingDefinition> {
        self.settings.get(key)
    }

    pub fn all(&self) -> impl Iterator<Item = &SettingDefinition> {
        self.settings.values()
    }

    pub fn by_tag<'a>(&'a self, tag: &'a str) -> impl Iterator<Item = &'a SettingDefinition> + 'a {
        self.settings
            .values()
            .filter(move |d| d.tags.iter().any(|t| t == tag))
    }

    pub fn by_scope(&self, scope: SettingScope) -> impl Iterator<Item = &SettingDefinition> {
        self.settings
            .values()
            .filter(move |d| d.allows_scope(scope))
    }

    pub fn get_default(&self, key: &str) -> Option<&JsonValue> {
        self.settings.get(key).map(|d| &d.default)
    }

    pub fn register_flag(&mut self, def: FeatureFlagDefinition) {
        self.flags.insert(def.id.clone(), def);
    }

    pub fn register_all_flags(&mut self, defs: impl IntoIterator<Item = FeatureFlagDefinition>) {
        for def in defs {
            self.register_flag(def);
        }
    }

    pub fn get_flag(&self, id: &str) -> Option<&FeatureFlagDefinition> {
        self.flags.get(id)
    }

    pub fn all_flags(&self) -> impl Iterator<Item = &FeatureFlagDefinition> {
        self.flags.values()
    }

    /// Reset to built-in definitions only.
    pub fn reset(&mut self) {
        self.settings.clear();
        self.flags.clear();
        for def in built_in_settings() {
            self.settings.insert(def.key.clone(), def);
        }
        for def in built_in_flags() {
            self.flags.insert(def.id.clone(), def);
        }
    }
}

impl Default for ConfigRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ── Built-ins ──────────────────────────────────────────────────────────

fn sidebar_width_validator(v: &JsonValue) -> Option<String> {
    match v.as_f64() {
        Some(n) if (180.0..=600.0).contains(&n) => None,
        _ => Some("Must be between 180 and 600".into()),
    }
}

fn editor_font_size_validator(v: &JsonValue) -> Option<String> {
    match v.as_f64() {
        Some(n) if (8.0..=32.0).contains(&n) => None,
        _ => Some("Must be between 8 and 32".into()),
    }
}

fn editor_indent_size_validator(v: &JsonValue) -> Option<String> {
    match v.as_i64() {
        Some(n) if (1..=8).contains(&n) => None,
        _ => Some("Must be an integer between 1 and 8".into()),
    }
}

fn non_negative_number_validator(v: &JsonValue) -> Option<String> {
    match v.as_f64() {
        Some(n) if n >= 0.0 => None,
        _ => Some("Must be >= 0".into()),
    }
}

fn sync_interval_validator(v: &JsonValue) -> Option<String> {
    match v.as_f64() {
        Some(n) if n >= 0.0 => None,
        _ => Some("Must be >= 0 (0 = manual only)".into()),
    }
}

fn zoom_sensitivity_validator(v: &JsonValue) -> Option<String> {
    match v.as_f64() {
        Some(n) if (0.1..=5.0).contains(&n) => None,
        _ => Some("Must be between 0.1 and 5.0".into()),
    }
}

fn select(
    key: &str,
    default: &str,
    label: &str,
    tag: &str,
    options: &[&str],
    scopes: Vec<SettingScope>,
) -> SettingDefinition {
    SettingDefinition::new(key, SettingType::Select, json!(default), label)
        .with_tags([tag])
        .with_options(options.iter().map(|o| json!(o)).collect())
        .with_scopes(scopes)
}

fn boolean(
    key: &str,
    default: bool,
    label: &str,
    tag: &str,
    scopes: Vec<SettingScope>,
) -> SettingDefinition {
    SettingDefinition::new(key, SettingType::Boolean, json!(default), label)
        .with_tags([tag])
        .with_scopes(scopes)
}

fn number(
    key: &str,
    default: f64,
    label: &str,
    tag: &str,
    scopes: Vec<SettingScope>,
    validator: Option<SettingValidator>,
) -> SettingDefinition {
    let mut def = SettingDefinition::new(key, SettingType::Number, json!(default), label)
        .with_tags([tag])
        .with_scopes(scopes);
    if let Some(v) = validator {
        def = def.with_validator(v);
    }
    def
}

fn string_def(
    key: &str,
    default: &str,
    label: &str,
    tag: &str,
    scopes: Vec<SettingScope>,
) -> SettingDefinition {
    SettingDefinition::new(key, SettingType::String, json!(default), label)
        .with_tags([tag])
        .with_scopes(scopes)
}

pub(crate) fn built_in_settings() -> Vec<SettingDefinition> {
    use SettingScope::*;
    vec![
        // ── UI ─────────────────────────────────────────────────────
        select(
            "ui.theme",
            "system",
            "Theme",
            "ui",
            &["light", "dark", "system"],
            vec![Workspace, User],
        ),
        select(
            "ui.density",
            "comfortable",
            "Density",
            "ui",
            &["compact", "comfortable", "spacious"],
            vec![Workspace, User],
        ),
        string_def("ui.language", "en", "Language", "ui", vec![User])
            .with_description("BCP 47 locale code (e.g. en-US, fr-FR)"),
        number(
            "ui.sidebarWidth",
            260.0,
            "Sidebar width (px)",
            "ui",
            vec![User],
            Some(sidebar_width_validator),
        ),
        boolean(
            "ui.showActivityBar",
            true,
            "Show activity bar",
            "ui",
            vec![User],
        ),
        // ── Editor ────────────────────────────────────────────────
        number(
            "editor.fontSize",
            14.0,
            "Editor font size",
            "editor",
            vec![Workspace, User],
            Some(editor_font_size_validator),
        ),
        boolean(
            "editor.lineNumbers",
            true,
            "Show line numbers",
            "editor",
            vec![User],
        ),
        boolean(
            "editor.spellCheck",
            false,
            "Spell check",
            "editor",
            vec![User],
        ),
        number(
            "editor.indentSize",
            2.0,
            "Indent size (spaces)",
            "editor",
            vec![Workspace, User],
            Some(editor_indent_size_validator),
        )
        .with_description("Number of spaces per indent level in code editors."),
        number(
            "editor.autosaveMs",
            1500.0,
            "Autosave delay (ms)",
            "editor",
            vec![Workspace, User],
            Some(non_negative_number_validator),
        ),
        // ── Sync ──────────────────────────────────────────────────
        boolean(
            "sync.enabled",
            false,
            "Enable sync",
            "sync",
            vec![Workspace],
        ),
        number(
            "sync.intervalSeconds",
            300.0,
            "Sync interval (seconds)",
            "sync",
            vec![Workspace],
            Some(sync_interval_validator),
        ),
        // ── AI ────────────────────────────────────────────────────
        boolean(
            "ai.enabled",
            true,
            "Enable AI features",
            "ai",
            vec![Workspace],
        ),
        select(
            "ai.provider",
            "anthropic",
            "AI provider",
            "ai",
            &["anthropic", "openai", "ollama", "custom"],
            vec![Workspace],
        ),
        string_def(
            "ai.modelId",
            "claude-sonnet-4-6",
            "AI model ID",
            "ai",
            vec![Workspace, User],
        ),
        string_def("ai.apiKey", "", "AI API key", "ai", vec![Workspace]).secret(),
        // ── Notifications ─────────────────────────────────────────
        boolean(
            "notifications.inApp",
            true,
            "In-app notifications",
            "notifications",
            vec![User],
        ),
        // ── Canvas / Builder ─────────────────────────────────────
        boolean(
            "canvas.scrollZoom",
            true,
            "Ctrl/Cmd+scroll to zoom canvas",
            "canvas",
            vec![Workspace, User],
        ),
        number(
            "canvas.zoomSensitivity",
            1.0,
            "Zoom sensitivity multiplier",
            "canvas",
            vec![User],
            Some(zoom_sensitivity_validator),
        )
        .with_description("Multiplier for scroll-to-zoom speed (0.1–5.0)."),
    ]
}

pub(crate) fn built_in_flags() -> Vec<FeatureFlagDefinition> {
    vec![
        FeatureFlagDefinition::new("ai-features", "AI Features", true)
            .with_description("AI chat, suggestions, and knowledge base")
            .with_setting_key("ai.enabled"),
        FeatureFlagDefinition::new("sync", "CRDT Sync", false)
            .with_description("Sync workspace data to peer nodes")
            .with_setting_key("sync.enabled"),
    ]
}
