//! `ConfigModel` — live runtime config with layered scope resolution.
//! Port of `@prism/core/kernel/config/config-model.ts`.
//!
//! Resolution walks scopes from most specific (user) to least
//! (default/registry). Watchers fire when a key's *resolved* value
//! changes.

use std::cell::RefCell;
use std::collections::{BTreeSet, HashMap};
use std::rc::Rc;

use indexmap::IndexMap;
use serde_json::{Map as JsonMap, Value as JsonValue};

use super::registry::ConfigRegistry;
use super::types::{ConfigStore, SettingChange, SettingScope};

pub type SettingWatcher = Box<dyn FnMut(&JsonValue, &SettingChange)>;
pub type ChangeListener = Box<dyn FnMut(&SettingChange)>;

/// Opaque handle returned by `watch` / `on_change`. Dropping it does
/// **not** unsubscribe; call `ConfigModel::unwatch` / `unlisten`
/// explicitly. This mirrors the `Subscription` pattern already used by
/// `kernel::store`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Subscription(u64);

struct Inner {
    registry: Rc<ConfigRegistry>,
    layers: HashMap<SettingScope, IndexMap<String, JsonValue>>,
    watchers: HashMap<String, Vec<(Subscription, SettingWatcher)>>,
    change_listeners: Vec<(Subscription, ChangeListener)>,
    stores: HashMap<SettingScope, Box<dyn ConfigStore>>,
    next_id: u64,
}

impl Inner {
    fn new(registry: Rc<ConfigRegistry>) -> Self {
        let mut layers = HashMap::new();
        for scope in SettingScope::ORDER {
            layers.insert(scope, IndexMap::new());
        }
        Self {
            registry,
            layers,
            watchers: HashMap::new(),
            change_listeners: Vec::new(),
            stores: HashMap::new(),
            next_id: 1,
        }
    }

    fn alloc_id(&mut self) -> Subscription {
        let id = self.next_id;
        self.next_id += 1;
        Subscription(id)
    }

    fn layer(&self, scope: SettingScope) -> &IndexMap<String, JsonValue> {
        self.layers.get(&scope).expect("scope layer missing")
    }

    fn layer_mut(&mut self, scope: SettingScope) -> &mut IndexMap<String, JsonValue> {
        self.layers.get_mut(&scope).expect("scope layer missing")
    }

    fn get(&self, key: &str) -> JsonValue {
        // Walk scopes from most specific to least.
        for scope in SettingScope::ORDER.iter().rev() {
            let def = self.registry.get(key);
            if let Some(d) = def {
                if !d.allows_scope(*scope) {
                    continue;
                }
            }
            if let Some(v) = self.layer(*scope).get(key) {
                return v.clone();
            }
        }
        self.registry
            .get_default(key)
            .cloned()
            .unwrap_or(JsonValue::Null)
    }

    /// Resolve a key as if `modified_scope`'s current layer were
    /// replaced by `previous_layer`. Used by `load` to compute the
    /// pre-load resolved value for every affected key.
    fn resolve_with_previous(
        &self,
        key: &str,
        modified_scope: SettingScope,
        previous_layer: &IndexMap<String, JsonValue>,
    ) -> JsonValue {
        for scope in SettingScope::ORDER.iter().rev() {
            let def = self.registry.get(key);
            if let Some(d) = def {
                if !d.allows_scope(*scope) {
                    continue;
                }
            }
            let layer = if *scope == modified_scope {
                previous_layer
            } else {
                self.layer(*scope)
            };
            if let Some(v) = layer.get(key) {
                return v.clone();
            }
        }
        self.registry
            .get_default(key)
            .cloned()
            .unwrap_or(JsonValue::Null)
    }

    fn notify_change(&mut self, change: SettingChange) {
        for (_, l) in self.change_listeners.iter_mut() {
            l(&change);
        }
        if let Some(watchers) = self.watchers.get_mut(&change.key) {
            for (_, w) in watchers.iter_mut() {
                w(&change.new_value, &change);
            }
        }
    }
}

pub struct ConfigModel {
    inner: Rc<RefCell<Inner>>,
}

impl ConfigModel {
    pub fn new(registry: Rc<ConfigRegistry>) -> Self {
        Self {
            inner: Rc::new(RefCell::new(Inner::new(registry))),
        }
    }

    // ── Loading ───────────────────────────────────────────────────

    /// Load (or replace) all values for a scope. Fires watchers for
    /// every key whose *resolved* value changes.
    pub fn load(&self, scope: SettingScope, values: JsonMap<String, JsonValue>) {
        let mut changes: Vec<SettingChange> = Vec::new();
        {
            let mut inner = self.inner.borrow_mut();
            let previous = inner.layer(scope).clone();

            inner.layer_mut(scope).clear();
            for (k, v) in values.into_iter() {
                inner.layer_mut(scope).insert(k, v);
            }

            let mut affected: BTreeSet<String> = BTreeSet::new();
            for k in previous.keys() {
                affected.insert(k.clone());
            }
            for k in inner.layer(scope).keys() {
                affected.insert(k.clone());
            }

            for key in affected {
                let was = inner.resolve_with_previous(&key, scope, &previous);
                let now = inner.get(&key);
                if was != now {
                    changes.push(SettingChange {
                        key,
                        previous_value: was,
                        new_value: now,
                        scope,
                    });
                }
            }
        }
        for change in changes {
            self.inner.borrow_mut().notify_change(change);
        }
    }

    /// Attach a persistent store to a scope. `load()` is called
    /// immediately and `save()` on every mutation of this scope. The
    /// caller is responsible for pushing external changes via
    /// [`ConfigModel::load`] — the Rust trait intentionally omits
    /// `subscribe` (file watchers / IPC callbacks belong in the app
    /// layer, not here).
    pub fn attach_store(&self, scope: SettingScope, store: Box<dyn ConfigStore>) {
        let initial = store.load();
        self.inner.borrow_mut().stores.insert(scope, store);
        self.load(scope, initial);
    }

    pub fn detach_store(&self, scope: SettingScope) -> Option<Box<dyn ConfigStore>> {
        self.inner.borrow_mut().stores.remove(&scope)
    }

    // ── Reading ───────────────────────────────────────────────────

    pub fn get(&self, key: &str) -> JsonValue {
        self.inner.borrow().get(key)
    }

    pub fn get_at_scope(&self, key: &str, scope: SettingScope) -> Option<JsonValue> {
        self.inner.borrow().layer(scope).get(key).cloned()
    }

    pub fn get_scope(&self, scope: SettingScope) -> JsonMap<String, JsonValue> {
        let inner = self.inner.borrow();
        let mut out = JsonMap::new();
        for (k, v) in inner.layer(scope) {
            out.insert(k.clone(), v.clone());
        }
        out
    }

    /// True if `key` has been explicitly set in any non-default scope.
    pub fn is_overridden(&self, key: &str) -> bool {
        let inner = self.inner.borrow();
        for scope in [SettingScope::Workspace, SettingScope::User] {
            if inner.layer(scope).contains_key(key) {
                return true;
            }
        }
        false
    }

    // ── Writing ───────────────────────────────────────────────────

    /// Set a value in the given scope. Errors if the scope is not
    /// allowed by the definition, or if the validator rejects the
    /// value.
    pub fn set(
        &self,
        key: &str,
        value: JsonValue,
        scope: SettingScope,
    ) -> Result<(), String> {
        let change_opt = {
            let mut inner = self.inner.borrow_mut();
            if let Some(def) = inner.registry.get(key) {
                if !def.allows_scope(scope) {
                    return Err(format!(
                        "Setting '{}' does not allow scope '{:?}'",
                        key, scope
                    ));
                }
                if let Some(validate) = def.validate {
                    if let Some(err) = validate(&value) {
                        return Err(format!("Invalid value for '{}': {}", key, err));
                    }
                }
            }

            let previous = inner.get(key);
            inner.layer_mut(scope).insert(key.to_string(), value);
            let resolved = inner.get(key);

            if previous != resolved {
                Some(SettingChange {
                    key: key.to_string(),
                    previous_value: previous,
                    new_value: resolved,
                    scope,
                })
            } else {
                None
            }
        };

        if let Some(change) = change_opt {
            self.inner.borrow_mut().notify_change(change);
        }
        self.persist_scope(scope);
        Ok(())
    }

    /// Remove a value from a specific scope (falls back to parent
    /// scopes for resolution).
    pub fn reset(&self, key: &str, scope: SettingScope) {
        let change_opt = {
            let mut inner = self.inner.borrow_mut();
            let previous = inner.get(key);
            inner.layer_mut(scope).shift_remove(key);
            let resolved = inner.get(key);
            if previous != resolved {
                Some(SettingChange {
                    key: key.to_string(),
                    previous_value: previous,
                    new_value: resolved,
                    scope,
                })
            } else {
                None
            }
        };
        if let Some(change) = change_opt {
            self.inner.borrow_mut().notify_change(change);
        }
        self.persist_scope(scope);
    }

    fn persist_scope(&self, scope: SettingScope) {
        let values = self.get_scope(scope);
        if let Some(store) = self.inner.borrow_mut().stores.get_mut(&scope) {
            store.save(values);
        }
    }

    // ── Watching ──────────────────────────────────────────────────

    /// Watch a specific key's *resolved* value. Fires immediately with
    /// the current value, then on every change. Returns a
    /// [`Subscription`] handle for [`ConfigModel::unwatch`].
    pub fn watch<F>(&self, key: &str, mut callback: F) -> Subscription
    where
        F: FnMut(&JsonValue, &SettingChange) + 'static,
    {
        let current = self.get(key);
        let immediate = SettingChange {
            key: key.to_string(),
            previous_value: current.clone(),
            new_value: current.clone(),
            scope: SettingScope::Default,
        };
        callback(&current, &immediate);

        let id = self.inner.borrow_mut().alloc_id();
        self.inner
            .borrow_mut()
            .watchers
            .entry(key.to_string())
            .or_default()
            .push((id, Box::new(callback)));
        id
    }

    pub fn unwatch(&self, key: &str, sub: Subscription) {
        let mut inner = self.inner.borrow_mut();
        if let Some(watchers) = inner.watchers.get_mut(key) {
            watchers.retain(|(id, _)| *id != sub);
        }
    }

    /// Listen for any config change. Returns a [`Subscription`] handle
    /// for [`ConfigModel::unlisten`].
    pub fn on_change<F>(&self, callback: F) -> Subscription
    where
        F: FnMut(&SettingChange) + 'static,
    {
        let id = self.inner.borrow_mut().alloc_id();
        self.inner
            .borrow_mut()
            .change_listeners
            .push((id, Box::new(callback)));
        id
    }

    pub fn unlisten(&self, sub: Subscription) {
        let mut inner = self.inner.borrow_mut();
        inner.change_listeners.retain(|(id, _)| *id != sub);
    }

    // ── Serialization ────────────────────────────────────────────

    /// Serialize a scope's values as a plain map. Secret values are
    /// replaced with `"***"`.
    pub fn to_json(&self, scope: SettingScope) -> JsonMap<String, JsonValue> {
        let inner = self.inner.borrow();
        let mut out = JsonMap::new();
        for (k, v) in inner.layer(scope) {
            let masked = inner
                .registry
                .get(k)
                .map(|d| d.secret)
                .unwrap_or(false);
            if masked {
                out.insert(k.clone(), JsonValue::String("***".into()));
            } else {
                out.insert(k.clone(), v.clone());
            }
        }
        out
    }

    /// Access the registry that backs this model — primarily for
    /// `FeatureFlags` to walk flag definitions.
    pub fn registry(&self) -> Rc<ConfigRegistry> {
        self.inner.borrow().registry.clone()
    }
}

impl Clone for ConfigModel {
    /// Cloning yields another handle to the *same* underlying config
    /// state (like `Rc`). Used to share a model between the
    /// `FeatureFlags` instance and the rest of the app.
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::cell::Cell;

    fn registry() -> Rc<ConfigRegistry> {
        Rc::new(ConfigRegistry::new())
    }

    fn jmap(pairs: &[(&str, JsonValue)]) -> JsonMap<String, JsonValue> {
        let mut out = JsonMap::new();
        for (k, v) in pairs {
            out.insert((*k).to_string(), v.clone());
        }
        out
    }

    #[test]
    fn get_falls_back_to_registry_default() {
        let model = ConfigModel::new(registry());
        assert_eq!(model.get("ui.theme"), json!("system"));
        assert_eq!(model.get("editor.fontSize"), json!(14.0));
    }

    #[test]
    fn user_scope_overrides_workspace() {
        let model = ConfigModel::new(registry());
        model.load(SettingScope::Workspace, jmap(&[("ui.theme", json!("dark"))]));
        assert_eq!(model.get("ui.theme"), json!("dark"));
        model.load(SettingScope::User, jmap(&[("ui.theme", json!("light"))]));
        assert_eq!(model.get("ui.theme"), json!("light"));
    }

    #[test]
    fn set_rejects_disallowed_scope() {
        let model = ConfigModel::new(registry());
        // sync.enabled is workspace-only.
        let err = model
            .set("sync.enabled", json!(true), SettingScope::User)
            .unwrap_err();
        assert!(err.contains("does not allow scope"));
    }

    #[test]
    fn set_runs_validator() {
        let model = ConfigModel::new(registry());
        let err = model
            .set("ui.sidebarWidth", json!(50.0), SettingScope::User)
            .unwrap_err();
        assert!(err.contains("Must be between 180 and 600"));
        model
            .set("ui.sidebarWidth", json!(320.0), SettingScope::User)
            .unwrap();
        assert_eq!(model.get("ui.sidebarWidth"), json!(320.0));
    }

    #[test]
    fn reset_falls_back_to_parent_scope() {
        let model = ConfigModel::new(registry());
        model.load(
            SettingScope::Workspace,
            jmap(&[("ui.theme", json!("dark"))]),
        );
        model
            .set("ui.theme", json!("light"), SettingScope::User)
            .unwrap();
        assert_eq!(model.get("ui.theme"), json!("light"));
        model.reset("ui.theme", SettingScope::User);
        assert_eq!(model.get("ui.theme"), json!("dark"));
    }

    #[test]
    fn is_overridden_checks_non_default_scopes() {
        let model = ConfigModel::new(registry());
        assert!(!model.is_overridden("ui.theme"));
        model
            .set("ui.theme", json!("dark"), SettingScope::Workspace)
            .unwrap();
        assert!(model.is_overridden("ui.theme"));
    }

    #[test]
    fn get_at_scope_returns_raw_layer() {
        let model = ConfigModel::new(registry());
        model.load(
            SettingScope::Workspace,
            jmap(&[("ui.theme", json!("dark"))]),
        );
        assert_eq!(
            model.get_at_scope("ui.theme", SettingScope::Workspace),
            Some(json!("dark"))
        );
        assert_eq!(model.get_at_scope("ui.theme", SettingScope::User), None);
    }

    #[test]
    fn watch_fires_immediately_and_on_change() {
        let model = ConfigModel::new(registry());
        let calls = Rc::new(Cell::new(0));
        let calls_clone = calls.clone();
        model.watch("editor.fontSize", move |_, _| {
            calls_clone.set(calls_clone.get() + 1);
        });
        assert_eq!(calls.get(), 1); // immediate.
        model
            .set("editor.fontSize", json!(18.0), SettingScope::User)
            .unwrap();
        assert_eq!(calls.get(), 2);
        // Same value — no new change.
        model
            .set("editor.fontSize", json!(18.0), SettingScope::User)
            .unwrap();
        assert_eq!(calls.get(), 2);
    }

    #[test]
    fn unwatch_detaches_callback() {
        let model = ConfigModel::new(registry());
        let calls = Rc::new(Cell::new(0));
        let calls_clone = calls.clone();
        let sub = model.watch("editor.fontSize", move |_, _| {
            calls_clone.set(calls_clone.get() + 1);
        });
        assert_eq!(calls.get(), 1);
        model.unwatch("editor.fontSize", sub);
        model
            .set("editor.fontSize", json!(20.0), SettingScope::User)
            .unwrap();
        assert_eq!(calls.get(), 1);
    }

    #[test]
    fn on_change_receives_all_mutations() {
        let model = ConfigModel::new(registry());
        let seen: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
        let seen_clone = seen.clone();
        model.on_change(move |change| {
            seen_clone.borrow_mut().push(change.key.clone());
        });
        model
            .set("ui.theme", json!("dark"), SettingScope::User)
            .unwrap();
        model
            .set("editor.fontSize", json!(18.0), SettingScope::User)
            .unwrap();
        assert_eq!(
            *seen.borrow(),
            vec!["ui.theme".to_string(), "editor.fontSize".to_string()]
        );
    }

    #[test]
    fn to_json_masks_secret_values() {
        let model = ConfigModel::new(registry());
        model
            .set("ai.apiKey", json!("sk-abcdef"), SettingScope::Workspace)
            .unwrap();
        let json = model.to_json(SettingScope::Workspace);
        assert_eq!(json.get("ai.apiKey"), Some(&json!("***")));
    }

    #[test]
    fn load_fires_watchers_for_affected_keys() {
        let model = ConfigModel::new(registry());
        let theme_calls = Rc::new(Cell::new(0));
        let theme_clone = theme_calls.clone();
        model.watch("ui.theme", move |_, _| {
            theme_clone.set(theme_clone.get() + 1);
        });
        assert_eq!(theme_calls.get(), 1); // immediate.

        model.load(
            SettingScope::User,
            jmap(&[("ui.theme", json!("dark"))]),
        );
        assert_eq!(theme_calls.get(), 2);

        // Replacing with the same resolved value should not refire.
        model.load(
            SettingScope::User,
            jmap(&[("ui.theme", json!("dark"))]),
        );
        assert_eq!(theme_calls.get(), 2);
    }

    #[test]
    fn attach_store_loads_initial_values() {
        use super::super::store::MemoryConfigStore;
        let model = ConfigModel::new(registry());
        let store = MemoryConfigStore::new(jmap(&[("ui.theme", json!("dark"))]));
        model.attach_store(SettingScope::Workspace, Box::new(store));
        assert_eq!(model.get("ui.theme"), json!("dark"));
    }

    #[test]
    fn set_persists_to_attached_store() {
        use super::super::store::MemoryConfigStore;
        let model = ConfigModel::new(registry());
        model.attach_store(SettingScope::User, Box::new(MemoryConfigStore::default()));
        model
            .set("ui.theme", json!("light"), SettingScope::User)
            .unwrap();
        // There's no public way to read back from the owned store, so
        // detach it and inspect the returned trait object through the
        // known concrete type.
        let store = model.detach_store(SettingScope::User).unwrap();
        let saved = store.load();
        assert_eq!(saved.get("ui.theme"), Some(&json!("light")));
    }
}
