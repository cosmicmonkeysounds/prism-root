//! Concrete `ConfigStore` implementations. Port of
//! `@prism/core/kernel/config/config-store.ts`.
//!
//! Only `MemoryConfigStore` ships in-crate — the file-backed store
//! belongs to the app layer (desktop shell, relay server, …).

use serde_json::{Map as JsonMap, Value as JsonValue};

use super::types::ConfigStore;

#[derive(Debug, Default, Clone)]
pub struct MemoryConfigStore {
    values: JsonMap<String, JsonValue>,
}

impl MemoryConfigStore {
    pub fn new(values: JsonMap<String, JsonValue>) -> Self {
        Self { values }
    }

    /// Direct snapshot for tests.
    pub fn snapshot(&self) -> JsonMap<String, JsonValue> {
        self.values.clone()
    }
}

impl ConfigStore for MemoryConfigStore {
    fn load(&self) -> JsonMap<String, JsonValue> {
        self.values.clone()
    }

    fn save(&mut self, values: JsonMap<String, JsonValue>) {
        self.values = values;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn load_returns_initial_values() {
        let mut initial = JsonMap::new();
        initial.insert("ui.theme".into(), json!("dark"));
        let store = MemoryConfigStore::new(initial);
        let loaded = store.load();
        assert_eq!(loaded.get("ui.theme"), Some(&json!("dark")));
    }

    #[test]
    fn save_replaces_values() {
        let mut store = MemoryConfigStore::default();
        let mut next = JsonMap::new();
        next.insert("ui.theme".into(), json!("light"));
        store.save(next);
        assert_eq!(store.load().get("ui.theme"), Some(&json!("light")));
    }
}
