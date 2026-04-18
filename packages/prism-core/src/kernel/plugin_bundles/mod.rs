//! `kernel::plugin_bundles` — built-in plugin bundles.
//!
//! Port of `kernel/plugin-bundles/*` at 8426588 per ADR-002 §Part C.
//! Each bundle encapsulates one vertical (work, crm, finance, life,
//! assets, platform), self-registering entity defs, edge defs,
//! automation presets, and plugin contributions (views, commands,
//! keybindings, activity-bar entries) through the shared
//! [`PluginInstallContext`]. The `flux_types` submodule hosts the
//! narrow Flux surface (entity type strings + `FluxAutomationPreset`)
//! that every bundle needs; it's extracted here so the built-ins can
//! compile without waiting on the full Phase-2b `domain::flux` port.

pub mod assets;
mod builders;
pub mod crm;
pub mod finance;
pub mod flux_types;
pub mod install;
pub mod life;
pub mod platform;
pub mod work;

pub use assets::{create_assets_bundle, AssetsBundle};
pub use crm::{create_crm_bundle, CrmBundle};
pub use finance::{create_finance_bundle, FinanceBundle};
pub use flux_types::{FluxActionKind, FluxAutomationAction, FluxAutomationPreset, FluxTriggerKind};
pub use install::{install_plugin_bundles, PluginBundle, PluginInstallContext};
pub use life::{create_life_bundle, LifeBundle};
pub use platform::{create_platform_bundle, PlatformBundle};
pub use work::{create_work_bundle, WorkBundle};

/// Create all built-in plugin bundles in the canonical registration
/// order (work → finance → crm → life → assets → platform).
pub fn create_builtin_bundles() -> Vec<Box<dyn PluginBundle>> {
    vec![
        create_work_bundle(),
        create_finance_bundle(),
        create_crm_bundle(),
        create_life_bundle(),
        create_assets_bundle(),
        create_platform_bundle(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::foundation::object_model::ObjectRegistry;
    use crate::kernel::plugin::PluginRegistry;

    fn fresh_registries() -> (ObjectRegistry, PluginRegistry) {
        (ObjectRegistry::new(), PluginRegistry::new())
    }

    #[test]
    fn builtin_bundles_list_has_all_six() {
        let bundles = create_builtin_bundles();
        assert_eq!(bundles.len(), 6);
        let ids: Vec<&str> = bundles.iter().map(|b| b.id()).collect();
        assert_eq!(
            ids,
            vec![
                "prism.plugin.work",
                "prism.plugin.finance",
                "prism.plugin.crm",
                "prism.plugin.life",
                "prism.plugin.assets",
                "prism.plugin.platform",
            ]
        );
    }

    #[test]
    fn install_fans_bundles_into_registries() {
        let (mut object_registry, mut plugin_registry) = fresh_registries();
        {
            let mut ctx = PluginInstallContext {
                object_registry: &mut object_registry,
                plugin_registry: &mut plugin_registry,
            };
            install_plugin_bundles(&create_builtin_bundles(), &mut ctx);
        }

        assert_eq!(plugin_registry.len(), 6);
        assert!(plugin_registry.has("prism.plugin.work"));
        assert!(plugin_registry.has("prism.plugin.crm"));
        assert!(plugin_registry.has("prism.plugin.platform"));

        // CRM registers no entity or edge types.
        let work_gig = object_registry.get("work:gig").expect("work:gig");
        assert_eq!(work_gig.label, "Gig");
        assert!(object_registry.get_edge_type("work:tracked-for").is_some());
        assert!(object_registry.get("life:habit").is_some());
        assert!(object_registry.get("assets:collection").is_some());
        assert!(object_registry.get("platform:calendar-event").is_some());
        assert!(object_registry.get_edge_type("finance:funded-by").is_some());
    }

    #[test]
    fn work_bundle_shapes_match_source() {
        let entity_defs = work::build_entity_defs();
        assert_eq!(entity_defs.len(), 3);
        let gig = &entity_defs[0];
        assert_eq!(gig.type_name, "work:gig");
        assert_eq!(gig.nsid.as_deref(), Some("io.prismapp.work.gig"));
        assert_eq!(gig.fields.as_ref().unwrap().len(), 11);

        let edges = work::build_edge_defs();
        assert_eq!(edges.len(), 3);
        assert_eq!(edges[0].relation, "work:tracked-for");
    }

    #[test]
    fn life_bundle_has_seven_entities() {
        let defs = life::build_entity_defs();
        assert_eq!(defs.len(), 7);
        let habit_log = defs
            .iter()
            .find(|d| d.type_name == "life:habit-log")
            .unwrap();
        assert_eq!(habit_log.child_only, Some(true));
    }

    #[test]
    fn finance_bundle_presets_are_three_and_round_trip() {
        let presets = finance::build_automation_presets();
        assert_eq!(presets.len(), 3);
        let s = serde_json::to_string(&presets[0]).unwrap();
        let back: FluxAutomationPreset = serde_json::from_str(&s).unwrap();
        assert_eq!(back.id, presets[0].id);
        assert_eq!(back.trigger, FluxTriggerKind::OnDueDate);
    }

    #[test]
    fn assets_and_platform_plugins_build() {
        let a = assets::build_plugin();
        assert_eq!(a.id, "prism.plugin.assets");
        let contribs = a.contributes.as_ref().unwrap();
        assert_eq!(contribs.views.as_ref().unwrap().len(), 4);
        assert_eq!(contribs.commands.as_ref().unwrap().len(), 3);

        let p = platform::build_plugin();
        assert_eq!(p.id, "prism.plugin.platform");
        let pc = p.contributes.as_ref().unwrap();
        assert_eq!(pc.views.as_ref().unwrap().len(), 4);
        assert_eq!(pc.keybindings.as_ref().unwrap().len(), 2);
    }

    #[test]
    fn flux_types_and_edges_constants_are_namespaced() {
        assert_eq!(flux_types::flux_types::TASK, "flux:task");
        assert_eq!(flux_types::flux_types::CONTACT, "flux:contact");
        assert_eq!(flux_types::flux_edges::ASSIGNED_TO, "flux:assigned-to");
    }

    #[test]
    fn installing_twice_keeps_plugin_count_stable() {
        let (mut object_registry, mut plugin_registry) = fresh_registries();
        let bundles = create_builtin_bundles();
        for _ in 0..2 {
            let mut ctx = PluginInstallContext {
                object_registry: &mut object_registry,
                plugin_registry: &mut plugin_registry,
            };
            install_plugin_bundles(&bundles, &mut ctx);
        }
        assert_eq!(plugin_registry.len(), 6);
    }
}
