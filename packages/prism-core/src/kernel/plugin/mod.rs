//! `kernel::plugin` — plugin contribution data model + registries.
//!
//! Port of `kernel/plugin/*.ts` at 8426588 per ADR-002 §Part C. The
//! [`PluginRegistry`] owns four inner [`ContributionRegistry`] instances
//! (views, commands, keybindings, context menus); registering a
//! [`PrismPlugin`] auto-fans its `contributes` declarations into the
//! right bucket. Registration / unregistration fires a synchronous
//! listener bus.

pub mod contribution_registry;
pub mod registry;
pub mod types;

pub use contribution_registry::{ContributionEntry, ContributionRegistry};
pub use registry::{
    PluginRegistry, PluginRegistryEvent, PluginRegistryEventType, PluginRegistryListener,
};
pub use types::{
    plugin_id, ActivityBarContributionDef, ActivityBarPosition, CommandContributionDef,
    ContextMenuContributionDef, KeybindingContributionDef, PluginContributions, PluginId,
    PrismPlugin, SettingsContributionDef, StatusBarContributionDef, StatusBarPosition,
    ToolbarContributionDef, ToolbarPosition, ViewContributionDef, ViewZone,
    WeakRefProviderContributionDef,
};

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    fn sample_plugin(id: &str) -> PrismPlugin {
        PrismPlugin::new(id, "Sample").with_contributes(PluginContributions {
            views: Some(vec![ViewContributionDef {
                id: format!("{id}.view"),
                label: "Sample".into(),
                zone: ViewZone::Content,
                component_id: "SampleView".into(),
                icon: None,
                default_visible: None,
                description: None,
                tags: None,
            }]),
            commands: Some(vec![CommandContributionDef {
                id: format!("{id}.cmd"),
                label: "Sample Command".into(),
                category: "Sample".into(),
                shortcut: None,
                description: None,
                action: "sample.action".into(),
                payload: None,
                when: None,
            }]),
            keybindings: Some(vec![KeybindingContributionDef {
                command: format!("{id}.cmd"),
                key: "ctrl+shift+p".into(),
                when: None,
            }]),
            context_menus: Some(vec![ContextMenuContributionDef {
                id: format!("{id}.menu"),
                label: "Sample Menu".into(),
                context: "explorer".into(),
                when: None,
                action: "sample.action".into(),
                shortcut: None,
                separator_before: None,
                danger: None,
            }]),
            activity_bar: None,
            settings: None,
            toolbar: None,
            status_bar: None,
            weak_ref_providers: None,
            immersive: None,
        })
    }

    #[test]
    fn register_fans_out_contributions() {
        let mut reg = PluginRegistry::new();
        reg.register(sample_plugin("a"));
        assert_eq!(reg.len(), 1);
        assert_eq!(reg.views.len(), 1);
        assert_eq!(reg.commands.len(), 1);
        assert_eq!(reg.keybindings.len(), 1);
        assert_eq!(reg.context_menus.len(), 1);
    }

    #[test]
    fn unregister_removes_all_contributions() {
        let mut reg = PluginRegistry::new();
        reg.register(sample_plugin("a"));
        reg.register(sample_plugin("b"));
        assert_eq!(reg.len(), 2);
        assert!(reg.unregister("a"));
        assert_eq!(reg.len(), 1);
        assert!(!reg.has("a"));
        assert_eq!(reg.views.len(), 1);
        assert_eq!(reg.views.by_plugin("b").len(), 1);
    }

    #[test]
    fn register_is_idempotent_on_same_id() {
        let mut reg = PluginRegistry::new();
        reg.register(sample_plugin("a"));
        reg.register(sample_plugin("a"));
        assert_eq!(reg.len(), 1);
        assert_eq!(reg.views.len(), 1);
    }

    #[test]
    fn listeners_fire_for_register_and_unregister() {
        let events: Arc<Mutex<Vec<(PluginRegistryEventType, String)>>> =
            Arc::new(Mutex::new(Vec::new()));
        let events_cb = events.clone();
        let mut reg = PluginRegistry::new();
        reg.subscribe(move |ev| {
            events_cb
                .lock()
                .unwrap()
                .push((ev.event_type.clone(), ev.plugin_id.clone()));
        });
        reg.register(sample_plugin("a"));
        reg.unregister("a");
        let events = events.lock().unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].0, PluginRegistryEventType::Registered);
        assert_eq!(events[1].0, PluginRegistryEventType::Unregistered);
    }

    #[test]
    fn contribution_registry_key_replaces() {
        let mut reg = ContributionRegistry::new(|v: &ViewContributionDef| v.id.clone());
        let def = ViewContributionDef {
            id: "view.x".into(),
            label: "A".into(),
            zone: ViewZone::Content,
            component_id: "X".into(),
            icon: None,
            default_visible: None,
            description: None,
            tags: None,
        };
        reg.register(def.clone(), "p1");
        let mut def2 = def.clone();
        def2.label = "B".into();
        reg.register(def2, "p1");
        assert_eq!(reg.len(), 1);
        assert_eq!(reg.get("view.x").unwrap().label, "B");
    }

    #[test]
    fn contribution_registry_query_and_by_plugin() {
        let mut reg = ContributionRegistry::new(|c: &CommandContributionDef| c.id.clone());
        for i in 0..3 {
            reg.register(
                CommandContributionDef {
                    id: format!("cmd.{i}"),
                    label: format!("C{i}"),
                    category: "Test".into(),
                    shortcut: None,
                    description: None,
                    action: "act".into(),
                    payload: None,
                    when: None,
                },
                if i == 2 { "b" } else { "a" },
            );
        }
        assert_eq!(reg.by_plugin("a").len(), 2);
        assert_eq!(reg.by_plugin("b").len(), 1);
        assert_eq!(reg.query(|c| c.id.ends_with("1")).len(), 1);
    }

    #[test]
    fn plugin_serde_roundtrip() {
        let p = sample_plugin("a");
        let s = serde_json::to_string(&p).unwrap();
        let back: PrismPlugin = serde_json::from_str(&s).unwrap();
        assert_eq!(back.id, p.id);
        assert_eq!(
            back.contributes.as_ref().unwrap().views.as_ref().unwrap()[0].id,
            "a.view"
        );
    }
}
