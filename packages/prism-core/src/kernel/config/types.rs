//! Config system primitives — scopes, setting definitions, change events,
//! the `ConfigStore` trait, and feature-flag definitions. Port of
//! `@prism/core/kernel/config/config-types.ts`.

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

/// Resolution scopes, ordered from least to most specific. The legacy
/// TS tree had "app" and "team"; Prism is local-first, so only
/// `default → workspace → user` survive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SettingScope {
    Default,
    Workspace,
    User,
}

impl SettingScope {
    /// Least → most specific. The `ConfigModel` walks this list in
    /// reverse during resolution.
    pub const ORDER: [SettingScope; 3] = [
        SettingScope::Default,
        SettingScope::Workspace,
        SettingScope::User,
    ];
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SettingType {
    String,
    Number,
    Boolean,
    Select,
    Object,
    Array,
}

/// Validator fn: returns `Some(message)` if invalid, `None` if valid.
pub type SettingValidator = fn(&JsonValue) -> Option<String>;

/// Full definition of a setting. Register with
/// `ConfigRegistry::register` at startup.
#[derive(Clone)]
pub struct SettingDefinition {
    pub key: String,
    pub ty: SettingType,
    pub default: JsonValue,
    pub label: String,
    pub description: Option<String>,
    /// Which scopes may override this setting. Empty = all scopes.
    pub scopes: Vec<SettingScope>,
    /// For `Select` type: valid options.
    pub options: Vec<JsonValue>,
    pub validate: Option<SettingValidator>,
    /// When true: masked (replaced with `***`) in `to_json` output.
    pub secret: bool,
    pub requires_restart: bool,
    pub tags: Vec<String>,
}

impl SettingDefinition {
    pub fn new(
        key: impl Into<String>,
        ty: SettingType,
        default: JsonValue,
        label: impl Into<String>,
    ) -> Self {
        Self {
            key: key.into(),
            ty,
            default,
            label: label.into(),
            description: None,
            scopes: Vec::new(),
            options: Vec::new(),
            validate: None,
            secret: false,
            requires_restart: false,
            tags: Vec::new(),
        }
    }

    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    pub fn with_scopes(mut self, scopes: Vec<SettingScope>) -> Self {
        self.scopes = scopes;
        self
    }

    pub fn with_options(mut self, options: Vec<JsonValue>) -> Self {
        self.options = options;
        self
    }

    pub fn with_validator(mut self, validate: SettingValidator) -> Self {
        self.validate = Some(validate);
        self
    }

    pub fn secret(mut self) -> Self {
        self.secret = true;
        self
    }

    pub fn requires_restart(mut self) -> Self {
        self.requires_restart = true;
        self
    }

    pub fn with_tags<I, S>(mut self, tags: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.tags = tags.into_iter().map(Into::into).collect();
        self
    }

    /// True if this setting accepts writes to `scope`. Empty scope list
    /// means "all scopes allowed".
    pub fn allows_scope(&self, scope: SettingScope) -> bool {
        self.scopes.is_empty() || self.scopes.contains(&scope)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SettingChange {
    pub key: String,
    pub previous_value: JsonValue,
    pub new_value: JsonValue,
    pub scope: SettingScope,
}

/// A condition that contributes to a feature flag's value. Conditions
/// are evaluated in order; first match wins.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum FeatureFlagCondition {
    Always {
        value: bool,
    },
    Config {
        key: String,
        equals: JsonValue,
        value: bool,
    },
}

#[derive(Debug, Clone, Default)]
pub struct FeatureFlagContext {
    pub config: Option<serde_json::Map<String, JsonValue>>,
}

#[derive(Debug, Clone)]
pub struct FeatureFlagDefinition {
    pub id: String,
    pub label: String,
    pub description: Option<String>,
    pub default: bool,
    pub conditions: Vec<FeatureFlagCondition>,
    /// Config key that can override this flag. When set,
    /// `ConfigModel::get` of this key takes precedence.
    pub setting_key: Option<String>,
}

impl FeatureFlagDefinition {
    pub fn new(id: impl Into<String>, label: impl Into<String>, default: bool) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            description: None,
            default,
            conditions: Vec::new(),
            setting_key: None,
        }
    }

    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    pub fn with_setting_key(mut self, key: impl Into<String>) -> Self {
        self.setting_key = Some(key.into());
        self
    }

    pub fn with_conditions(mut self, conditions: Vec<FeatureFlagCondition>) -> Self {
        self.conditions = conditions;
        self
    }
}

/// Synchronous persistence interface for a single scope's settings.
/// Prism uses Loro CRDT — all operations are synchronous.
pub trait ConfigStore {
    fn load(&self) -> serde_json::Map<String, JsonValue>;
    fn save(&mut self, values: serde_json::Map<String, JsonValue>);
}
