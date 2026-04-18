//! `plugin_bundles::crm` — CRM plugin bundle.
//!
//! Port of `kernel/plugin-bundles/crm/crm.ts`. Unlike the other bundles
//! CRM contributes **no** new entity or edge types; it's a pure plugin
//! wrapper that layers CRM-specific views, commands, and keybindings
//! on top of the existing Flux `contact` / `organization` types.

use super::install::{PluginBundle, PluginInstallContext};
use crate::kernel::plugin::{
    plugin_id, ActivityBarContributionDef, ActivityBarPosition, CommandContributionDef,
    KeybindingContributionDef, PluginContributions, PrismPlugin, ViewContributionDef, ViewZone,
};

pub fn build_plugin() -> PrismPlugin {
    PrismPlugin::new(plugin_id("prism.plugin.crm"), "CRM").with_contributes(PluginContributions {
        views: Some(vec![
            ViewContributionDef {
                id: "crm:contacts".into(),
                label: "Contacts".into(),
                zone: ViewZone::Content,
                component_id: "ContactListView".into(),
                icon: None,
                default_visible: None,
                description: Some("Contact directory".into()),
                tags: None,
            },
            ViewContributionDef {
                id: "crm:organizations".into(),
                label: "Organizations".into(),
                zone: ViewZone::Content,
                component_id: "OrgListView".into(),
                icon: None,
                default_visible: None,
                description: Some("Organization directory".into()),
                tags: None,
            },
            ViewContributionDef {
                id: "crm:pipeline".into(),
                label: "Deal Pipeline".into(),
                zone: ViewZone::Content,
                component_id: "PipelineView".into(),
                icon: None,
                default_visible: None,
                description: Some("Sales pipeline kanban".into()),
                tags: None,
            },
            ViewContributionDef {
                id: "crm:relationships".into(),
                label: "Relationships".into(),
                zone: ViewZone::Content,
                component_id: "RelationshipGraphView".into(),
                icon: None,
                default_visible: None,
                description: Some("Contact relationship graph".into()),
                tags: None,
            },
        ]),
        commands: Some(vec![
            CommandContributionDef {
                id: "crm:new-contact".into(),
                label: "New Contact".into(),
                category: "CRM".into(),
                shortcut: None,
                description: None,
                action: "crm.newContact".into(),
                payload: None,
                when: None,
            },
            CommandContributionDef {
                id: "crm:new-organization".into(),
                label: "New Organization".into(),
                category: "CRM".into(),
                shortcut: None,
                description: None,
                action: "crm.newOrganization".into(),
                payload: None,
                when: None,
            },
            CommandContributionDef {
                id: "crm:log-activity".into(),
                label: "Log Activity".into(),
                category: "CRM".into(),
                shortcut: None,
                description: None,
                action: "crm.logActivity".into(),
                payload: None,
                when: None,
            },
        ]),
        keybindings: Some(vec![KeybindingContributionDef {
            command: "crm:new-contact".into(),
            key: "ctrl+shift+c".into(),
            when: None,
        }]),
        activity_bar: Some(vec![ActivityBarContributionDef {
            id: "crm:activity".into(),
            label: "CRM".into(),
            icon: None,
            position: Some(ActivityBarPosition::Top),
            priority: Some(15),
        }]),
        context_menus: None,
        settings: None,
        toolbar: None,
        status_bar: None,
        weak_ref_providers: None,
        immersive: None,
    })
}

pub struct CrmBundle;

impl PluginBundle for CrmBundle {
    fn id(&self) -> &str {
        "prism.plugin.crm"
    }

    fn name(&self) -> &str {
        "CRM"
    }

    fn install(&self, ctx: &mut PluginInstallContext<'_>) {
        ctx.plugin_registry.register(build_plugin());
    }
}

pub fn create_crm_bundle() -> Box<dyn PluginBundle> {
    Box::new(CrmBundle)
}
