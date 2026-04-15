//! `config` — layered config system with scope cascade, validation,
//! watchers, pluggable stores, and feature flags. Port of
//! `@prism/core/kernel/config/*`.
//!
//! Scopes cascade `default → workspace → user` (most specific wins).
//! The legacy `app` / `team` scopes are deliberately not ported —
//! Prism is local-first.
//!
//! Submodules:
//! - [`types`]         — `SettingScope` / `SettingDefinition` /
//!   `SettingChange` / `ConfigStore` trait / feature-flag primitives.
//! - [`registry`]      — `ConfigRegistry` with the built-in UI/editor/
//!   sync/AI/notification settings and the `ai-features` / `sync`
//!   feature flags.
//! - [`store`]         — `MemoryConfigStore`. File-backed stores live
//!   in the host crates.
//! - [`schema`]        — lightweight JSON Schema subset for validation
//!   and env-var coercion.
//! - [`model`]         — `ConfigModel`: layered resolution, watchers,
//!   attached stores, hot-reload via `load`.
//! - [`feature_flags`] — `FeatureFlags`: boolean toggles evaluated
//!   against a `ConfigModel`, with live change watchers.

pub mod feature_flags;
pub mod model;
pub mod registry;
pub mod schema;
pub mod store;
pub mod types;

pub use feature_flags::{FeatureFlags, FlagSubscription};
pub use model::{ChangeListener, ConfigModel, SettingWatcher, Subscription};
pub use registry::ConfigRegistry;
pub use schema::{
    coerce_config_value, object_schema, schema_to_message, validate_config, ArraySchema,
    ConfigSchema, NumberSchema, ObjectSchema, StringSchema, ValidationError, ValidationResult,
};
pub use store::MemoryConfigStore;
pub use types::{
    ConfigStore, FeatureFlagCondition, FeatureFlagContext, FeatureFlagDefinition, SettingChange,
    SettingDefinition, SettingScope, SettingType, SettingValidator,
};
