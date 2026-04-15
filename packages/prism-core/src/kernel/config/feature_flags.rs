//! `FeatureFlags` — boolean toggles evaluated against `ConfigModel`
//! values. Port of `@prism/core/kernel/config/feature-flags.ts`.
//!
//! Resolution order:
//!   1. `config.get(flag.setting_key)` if set and boolean.
//!   2. Evaluate conditions in order; first match wins.
//!   3. `flag.default`.

use std::cell::RefCell;
use std::rc::Rc;

use serde_json::Value as JsonValue;

use super::model::ConfigModel;
use super::types::{FeatureFlagCondition, FeatureFlagContext};

pub type FlagWatcher = Box<dyn FnMut(bool)>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FlagSubscription(u64);

struct WatcherEntry {
    id: FlagSubscription,
    flag_id: String,
    callback: FlagWatcher,
    context: FeatureFlagContext,
}

struct Inner {
    config: ConfigModel,
    watchers: Vec<WatcherEntry>,
    next_id: u64,
}

pub struct FeatureFlags {
    inner: Rc<RefCell<Inner>>,
    _config_sub: super::model::Subscription,
}

impl FeatureFlags {
    pub fn new(config: ConfigModel) -> Self {
        let inner = Rc::new(RefCell::new(Inner {
            config: config.clone(),
            watchers: Vec::new(),
            next_id: 1,
        }));

        // Bind to config.on("change") so flag watchers re-fire when
        // their underlying setting key changes.
        let inner_weak = Rc::downgrade(&inner);
        let config_sub = config.on_change(move |change| {
            let Some(inner_rc) = inner_weak.upgrade() else {
                return;
            };

            // Collect the list of watchers that should fire — we can't
            // call `is_enabled` while holding a mut borrow, so snapshot
            // IDs first.
            let to_notify: Vec<(FlagSubscription, String, FeatureFlagContext)> = {
                let inner_ref = inner_rc.borrow();
                let registry = inner_ref.config.registry();
                let mut out = Vec::new();
                for w in &inner_ref.watchers {
                    let Some(def) = registry.get_flag(&w.flag_id) else {
                        continue;
                    };
                    if def.setting_key.as_deref() == Some(change.key.as_str()) {
                        out.push((w.id, w.flag_id.clone(), w.context.clone()));
                    }
                }
                out
            };

            for (id, flag_id, ctx) in to_notify {
                let enabled = {
                    let inner_ref = inner_rc.borrow();
                    Self::is_enabled_on(&inner_ref.config, &flag_id, &ctx)
                };
                let mut inner_mut = inner_rc.borrow_mut();
                if let Some(entry) = inner_mut.watchers.iter_mut().find(|w| w.id == id) {
                    (entry.callback)(enabled);
                }
            }
        });

        Self {
            inner,
            _config_sub: config_sub,
        }
    }

    fn is_enabled_on(config: &ConfigModel, flag_id: &str, ctx: &FeatureFlagContext) -> bool {
        let registry = config.registry();
        let Some(def) = registry.get_flag(flag_id).cloned() else {
            return false;
        };

        // 1. Config override.
        if let Some(key) = &def.setting_key {
            let val = config.get(key);
            if let Some(b) = val.as_bool() {
                return b;
            }
        }

        // 2. Conditions.
        for cond in &def.conditions {
            if let Some(result) = evaluate_condition(cond, ctx) {
                return result;
            }
        }

        // 3. Default.
        def.default
    }

    pub fn is_enabled(&self, flag_id: &str) -> bool {
        let inner = self.inner.borrow();
        Self::is_enabled_on(&inner.config, flag_id, &FeatureFlagContext::default())
    }

    pub fn is_enabled_with(&self, flag_id: &str, ctx: &FeatureFlagContext) -> bool {
        let inner = self.inner.borrow();
        Self::is_enabled_on(&inner.config, flag_id, ctx)
    }

    /// Map of `flag_id → bool` for every registered flag.
    pub fn get_all(&self, ctx: &FeatureFlagContext) -> indexmap::IndexMap<String, bool> {
        let inner = self.inner.borrow();
        let registry = inner.config.registry();
        let mut out = indexmap::IndexMap::new();
        for def in registry.all_flags() {
            out.insert(
                def.id.clone(),
                Self::is_enabled_on(&inner.config, &def.id, ctx),
            );
        }
        out
    }

    /// Watch a feature flag for changes driven by config mutations.
    /// Calls `callback` immediately with the current value, then on
    /// every change. Returns a [`FlagSubscription`] for [`unwatch`].
    pub fn watch<F>(
        &self,
        flag_id: &str,
        mut callback: F,
        context: FeatureFlagContext,
    ) -> FlagSubscription
    where
        F: FnMut(bool) + 'static,
    {
        let current = self.is_enabled_with(flag_id, &context);
        callback(current);

        let mut inner = self.inner.borrow_mut();
        let id = FlagSubscription(inner.next_id);
        inner.next_id += 1;
        inner.watchers.push(WatcherEntry {
            id,
            flag_id: flag_id.to_string(),
            callback: Box::new(callback),
            context,
        });
        id
    }

    pub fn unwatch(&self, sub: FlagSubscription) {
        let mut inner = self.inner.borrow_mut();
        inner.watchers.retain(|w| w.id != sub);
    }
}

fn evaluate_condition(cond: &FeatureFlagCondition, ctx: &FeatureFlagContext) -> Option<bool> {
    match cond {
        FeatureFlagCondition::Always { value } => Some(*value),
        FeatureFlagCondition::Config { key, equals, value } => {
            let Some(config) = &ctx.config else {
                return None;
            };
            let actual = config.get(key).unwrap_or(&JsonValue::Null);
            if actual == equals {
                Some(*value)
            } else {
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::model::ConfigModel;
    use super::super::registry::ConfigRegistry;
    use super::super::types::{FeatureFlagCondition, SettingScope};
    use super::*;
    use serde_json::json;
    use std::cell::Cell;

    fn fresh() -> (FeatureFlags, ConfigModel) {
        let registry = Rc::new(ConfigRegistry::new());
        let model = ConfigModel::new(registry);
        let flags = FeatureFlags::new(model.clone());
        (flags, model)
    }

    #[test]
    fn unknown_flag_returns_false() {
        let (flags, _) = fresh();
        assert!(!flags.is_enabled("nonexistent"));
    }

    #[test]
    fn default_is_respected() {
        let (flags, _) = fresh();
        // ai-features default = true
        assert!(flags.is_enabled("ai-features"));
        // sync default = false
        assert!(!flags.is_enabled("sync"));
    }

    #[test]
    fn setting_key_override_wins() {
        let (flags, model) = fresh();
        model
            .set("ai.enabled", json!(false), SettingScope::Workspace)
            .unwrap();
        assert!(!flags.is_enabled("ai-features"));
    }

    #[test]
    fn watcher_fires_on_setting_change() {
        let (flags, model) = fresh();
        let calls = Rc::new(Cell::new(0));
        let last = Rc::new(Cell::new(true));
        let calls_c = calls.clone();
        let last_c = last.clone();
        flags.watch(
            "ai-features",
            move |enabled| {
                calls_c.set(calls_c.get() + 1);
                last_c.set(enabled);
            },
            FeatureFlagContext::default(),
        );
        assert_eq!(calls.get(), 1);
        assert!(last.get());

        model
            .set("ai.enabled", json!(false), SettingScope::Workspace)
            .unwrap();
        assert_eq!(calls.get(), 2);
        assert!(!last.get());
    }

    #[test]
    fn conditions_evaluated_in_order() {
        use super::super::registry::ConfigRegistry;
        use super::super::types::FeatureFlagDefinition;

        let mut reg = ConfigRegistry::new();
        reg.register_flag(
            FeatureFlagDefinition::new("test", "Test flag", false).with_conditions(vec![
                FeatureFlagCondition::Config {
                    key: "mode".into(),
                    equals: json!("beta"),
                    value: true,
                },
                FeatureFlagCondition::Always { value: false },
            ]),
        );
        let model = ConfigModel::new(Rc::new(reg));
        let flags = FeatureFlags::new(model);

        let mut ctx = FeatureFlagContext::default();
        let mut config = serde_json::Map::new();
        config.insert("mode".into(), json!("beta"));
        ctx.config = Some(config);
        assert!(flags.is_enabled_with("test", &ctx));

        let mut ctx2 = FeatureFlagContext::default();
        let mut config2 = serde_json::Map::new();
        config2.insert("mode".into(), json!("stable"));
        ctx2.config = Some(config2);
        assert!(!flags.is_enabled_with("test", &ctx2));
    }

    #[test]
    fn get_all_returns_every_flag() {
        let (flags, _) = fresh();
        let all = flags.get_all(&FeatureFlagContext::default());
        assert!(all.contains_key("ai-features"));
        assert!(all.contains_key("sync"));
    }

    #[test]
    fn unwatch_detaches_watcher() {
        let (flags, model) = fresh();
        let calls = Rc::new(Cell::new(0));
        let calls_c = calls.clone();
        let sub = flags.watch(
            "ai-features",
            move |_| calls_c.set(calls_c.get() + 1),
            FeatureFlagContext::default(),
        );
        assert_eq!(calls.get(), 1);
        flags.unwatch(sub);
        model
            .set("ai.enabled", json!(false), SettingScope::Workspace)
            .unwrap();
        assert_eq!(calls.get(), 1);
    }
}
