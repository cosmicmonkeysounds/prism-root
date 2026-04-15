//! `plugin::registry` — [`PluginRegistry`].
//!
//! Port of `kernel/plugin/plugin-registry.ts`. Stores `PrismPlugin`
//! records, auto-registers their declared contributions into four
//! inner [`ContributionRegistry`] instances (views, commands,
//! keybindings, context menus), and fans out register / unregister
//! notifications through a synchronous listener bus.

use std::collections::HashMap;

use super::contribution_registry::ContributionRegistry;
use super::types::{
    CommandContributionDef, ContextMenuContributionDef, KeybindingContributionDef, PluginId,
    PrismPlugin, ViewContributionDef,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PluginRegistryEventType {
    Registered,
    Unregistered,
}

#[derive(Debug, Clone)]
pub struct PluginRegistryEvent {
    pub event_type: PluginRegistryEventType,
    pub plugin_id: PluginId,
}

pub type PluginRegistryListener = Box<dyn Fn(&PluginRegistryEvent) + Send + Sync>;

pub struct PluginRegistry {
    plugins: HashMap<String, PrismPlugin>,
    order: Vec<String>,
    listeners: Vec<PluginRegistryListener>,
    pub views: ContributionRegistry<ViewContributionDef>,
    pub commands: ContributionRegistry<CommandContributionDef>,
    pub keybindings: ContributionRegistry<KeybindingContributionDef>,
    pub context_menus: ContributionRegistry<ContextMenuContributionDef>,
}

impl Default for PluginRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl PluginRegistry {
    pub fn new() -> Self {
        Self {
            plugins: HashMap::new(),
            order: Vec::new(),
            listeners: Vec::new(),
            views: ContributionRegistry::new(|v: &ViewContributionDef| v.id.clone()),
            commands: ContributionRegistry::new(|c: &CommandContributionDef| c.id.clone()),
            keybindings: ContributionRegistry::new(|k: &KeybindingContributionDef| {
                format!("{}:{}", k.command, k.key)
            }),
            context_menus: ContributionRegistry::new(|m: &ContextMenuContributionDef| m.id.clone()),
        }
    }

    pub fn register(&mut self, plugin: PrismPlugin) {
        let id = plugin.id.clone();
        if !self.plugins.contains_key(&id) {
            self.order.push(id.clone());
        }
        if let Some(contribs) = &plugin.contributes {
            self.views.register_all(contribs.views.as_deref(), &id);
            self.commands
                .register_all(contribs.commands.as_deref(), &id);
            self.keybindings
                .register_all(contribs.keybindings.as_deref(), &id);
            self.context_menus
                .register_all(contribs.context_menus.as_deref(), &id);
        }
        self.plugins.insert(id.clone(), plugin);
        self.emit(&PluginRegistryEvent {
            event_type: PluginRegistryEventType::Registered,
            plugin_id: id,
        });
    }

    pub fn unregister(&mut self, id: &str) -> bool {
        if self.plugins.remove(id).is_none() {
            return false;
        }
        self.order.retain(|p| p != id);
        self.views.unregister_by_plugin(id);
        self.commands.unregister_by_plugin(id);
        self.keybindings.unregister_by_plugin(id);
        self.context_menus.unregister_by_plugin(id);
        self.emit(&PluginRegistryEvent {
            event_type: PluginRegistryEventType::Unregistered,
            plugin_id: id.to_string(),
        });
        true
    }

    pub fn get(&self, id: &str) -> Option<&PrismPlugin> {
        self.plugins.get(id)
    }

    pub fn has(&self, id: &str) -> bool {
        self.plugins.contains_key(id)
    }

    pub fn all(&self) -> Vec<PrismPlugin> {
        self.order
            .iter()
            .filter_map(|id| self.plugins.get(id).cloned())
            .collect()
    }

    pub fn len(&self) -> usize {
        self.plugins.len()
    }

    pub fn is_empty(&self) -> bool {
        self.plugins.is_empty()
    }

    pub fn subscribe<F>(&mut self, listener: F)
    where
        F: Fn(&PluginRegistryEvent) + Send + Sync + 'static,
    {
        self.listeners.push(Box::new(listener));
    }

    fn emit(&self, event: &PluginRegistryEvent) {
        for listener in &self.listeners {
            listener(event);
        }
    }
}
