//! `plugin::contribution_registry` — generic typed registry for the
//! "plugins declare, shell consumes" pattern.
//!
//! Port of `kernel/plugin/contribution-registry.ts`. The `key_fn` closure
//! projects a unique string key out of each item; insertion order is
//! preserved so iteration matches the legacy JS `Map` semantics.

type KeyFn<T> = Box<dyn Fn(&T) -> String + Send + Sync>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContributionEntry<T: Clone> {
    pub item: T,
    pub plugin_id: String,
    pub key: String,
}

pub struct ContributionRegistry<T: Clone + 'static> {
    entries: Vec<ContributionEntry<T>>,
    key_fn: KeyFn<T>,
}

impl<T: Clone + 'static> ContributionRegistry<T> {
    pub fn new<F>(key_fn: F) -> Self
    where
        F: Fn(&T) -> String + Send + Sync + 'static,
    {
        Self {
            entries: Vec::new(),
            key_fn: Box::new(key_fn),
        }
    }

    pub fn register(&mut self, item: T, plugin_id: impl Into<String>) {
        let key = (self.key_fn)(&item);
        let plugin_id = plugin_id.into();
        self.entries.retain(|e| e.key != key);
        self.entries.push(ContributionEntry {
            item,
            plugin_id,
            key,
        });
    }

    pub fn register_all(&mut self, items: Option<&[T]>, plugin_id: &str) {
        let Some(items) = items else { return };
        for item in items {
            self.register(item.clone(), plugin_id);
        }
    }

    pub fn unregister(&mut self, key: &str) -> bool {
        let before = self.entries.len();
        self.entries.retain(|e| e.key != key);
        before != self.entries.len()
    }

    pub fn unregister_by_plugin(&mut self, plugin_id: &str) -> usize {
        let before = self.entries.len();
        self.entries.retain(|e| e.plugin_id != plugin_id);
        before - self.entries.len()
    }

    pub fn get(&self, key: &str) -> Option<&T> {
        self.get_entry(key).map(|e| &e.item)
    }

    pub fn get_entry(&self, key: &str) -> Option<&ContributionEntry<T>> {
        self.entries.iter().find(|e| e.key == key)
    }

    pub fn has(&self, key: &str) -> bool {
        self.entries.iter().any(|e| e.key == key)
    }

    pub fn all(&self) -> Vec<T> {
        self.entries.iter().map(|e| e.item.clone()).collect()
    }

    pub fn all_entries(&self) -> Vec<ContributionEntry<T>> {
        self.entries.clone()
    }

    pub fn by_plugin(&self, plugin_id: &str) -> Vec<T> {
        self.entries
            .iter()
            .filter(|e| e.plugin_id == plugin_id)
            .map(|e| e.item.clone())
            .collect()
    }

    pub fn query<F: Fn(&T) -> bool>(&self, predicate: F) -> Vec<T> {
        self.entries
            .iter()
            .filter(|e| predicate(&e.item))
            .map(|e| e.item.clone())
            .collect()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }
}
