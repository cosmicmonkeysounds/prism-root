//! `plugin_bundles::work` — Work domain bundle.
//!
//! Port of `kernel/plugin-bundles/work/work.ts`: freelance gigs, time
//! entries, focus blocks. Extends Flux with three new entity types,
//! three edges, and three automation presets.

use serde_json::json;

use super::builders::{
    edge_def, entity_def, enum_options, owned_strings, ui_multiline, ui_multiline_group,
    ui_placeholder, ui_readonly, EdgeSpec, EntitySpec, Field,
};
use super::flux_types::{
    flux_types, FluxActionKind, FluxAutomationAction, FluxAutomationPreset, FluxTriggerKind,
};
use super::install::{PluginBundle, PluginInstallContext};
use crate::foundation::object_model::types::DefaultChildView;
use crate::foundation::object_model::{
    EdgeBehavior, EdgeTypeDef, EntityDef, EntityFieldDef, EntityFieldType,
};
use crate::kernel::plugin::{
    plugin_id, ActivityBarContributionDef, ActivityBarPosition, CommandContributionDef,
    KeybindingContributionDef, PluginContributions, PrismPlugin, ViewContributionDef, ViewZone,
};

// ── Domain constants ────────────────────────────────────────────────────────

pub mod work_categories {
    pub const FREELANCE: &str = "work:freelance";
    pub const TIME: &str = "work:time";
    pub const FOCUS: &str = "work:focus";
}

pub mod work_types {
    pub const GIG: &str = "work:gig";
    pub const TIME_ENTRY: &str = "work:time-entry";
    pub const FOCUS_BLOCK: &str = "work:focus-block";
}

pub mod work_edges {
    pub const TRACKED_FOR: &str = "work:tracked-for";
    pub const BILLED_TO: &str = "work:billed-to";
    pub const FOCUS_ON: &str = "work:focus-on";
}

// ── Fields ──────────────────────────────────────────────────────────────────

fn gig_fields() -> Vec<EntityFieldDef> {
    vec![
        Field::new("client", EntityFieldType::ObjectRef)
            .label("Client")
            .ref_types([flux_types::CONTACT, flux_types::ORGANIZATION])
            .build(),
        Field::new("rate", EntityFieldType::Float)
            .label("Rate")
            .build(),
        Field::new("rateUnit", EntityFieldType::Enum)
            .label("Rate Unit")
            .enum_values(enum_options(&[
                ("hourly", "Hourly"),
                ("daily", "Daily"),
                ("weekly", "Weekly"),
                ("fixed", "Fixed Price"),
                ("retainer", "Retainer"),
            ]))
            .default(json!("hourly"))
            .build(),
        Field::new("currency", EntityFieldType::Enum)
            .label("Currency")
            .enum_values(enum_options(&[
                ("USD", "USD"),
                ("EUR", "EUR"),
                ("GBP", "GBP"),
                ("CAD", "CAD"),
                ("AUD", "AUD"),
            ]))
            .default(json!("USD"))
            .build(),
        Field::new("estimatedHours", EntityFieldType::Float)
            .label("Estimated Hours")
            .build(),
        Field::new("actualHours", EntityFieldType::Float)
            .label("Actual Hours")
            .default(json!(0))
            .ui(ui_readonly())
            .build(),
        Field::new("startDate", EntityFieldType::Date)
            .label("Start Date")
            .build(),
        Field::new("endDate", EntityFieldType::Date)
            .label("End Date")
            .build(),
        Field::new("contractUrl", EntityFieldType::Url)
            .label("Contract URL")
            .build(),
        Field::new("totalBilled", EntityFieldType::Float)
            .label("Total Billed")
            .expression("actualHours * rate")
            .ui(ui_readonly())
            .build(),
        Field::new("scope", EntityFieldType::Text)
            .label("Scope of Work")
            .ui(ui_multiline_group("Details"))
            .build(),
    ]
}

fn time_entry_fields() -> Vec<EntityFieldDef> {
    vec![
        Field::new("startTime", EntityFieldType::Datetime)
            .label("Start Time")
            .required()
            .build(),
        Field::new("endTime", EntityFieldType::Datetime)
            .label("End Time")
            .build(),
        Field::new("durationMinutes", EntityFieldType::Int)
            .label("Duration (min)")
            .ui(ui_readonly())
            .build(),
        Field::new("billable", EntityFieldType::Bool)
            .label("Billable")
            .default(json!(true))
            .build(),
        Field::new("rate", EntityFieldType::Float)
            .label("Rate Override")
            .build(),
        Field::new("description", EntityFieldType::Text)
            .label("Description")
            .ui(ui_multiline())
            .build(),
        Field::new("tags", EntityFieldType::String)
            .label("Tags")
            .ui(ui_placeholder("comma-separated"))
            .build(),
    ]
}

fn focus_block_fields() -> Vec<EntityFieldDef> {
    vec![
        Field::new("scheduledStart", EntityFieldType::Datetime)
            .label("Scheduled Start")
            .required()
            .build(),
        Field::new("scheduledEnd", EntityFieldType::Datetime)
            .label("Scheduled End")
            .required()
            .build(),
        Field::new("durationMinutes", EntityFieldType::Int)
            .label("Duration (min)")
            .build(),
        Field::new("focusType", EntityFieldType::Enum)
            .label("Focus Type")
            .enum_values(enum_options(&[
                ("deep_work", "Deep Work"),
                ("shallow_work", "Shallow Work"),
                ("creative", "Creative"),
                ("admin", "Admin"),
                ("learning", "Learning"),
                ("break", "Break"),
            ]))
            .default(json!("deep_work"))
            .build(),
        Field::new("energyLevel", EntityFieldType::Enum)
            .label("Energy Level")
            .enum_values(enum_options(&[
                ("high", "High"),
                ("medium", "Medium"),
                ("low", "Low"),
            ]))
            .build(),
        Field::new("cognitiveLoad", EntityFieldType::Enum)
            .label("Cognitive Load")
            .enum_values(enum_options(&[
                ("heavy", "Heavy"),
                ("moderate", "Moderate"),
                ("light", "Light"),
            ]))
            .build(),
        Field::new("completionNote", EntityFieldType::Text)
            .label("Completion Note")
            .ui(ui_multiline())
            .build(),
    ]
}

// ── Entity + edge defs ──────────────────────────────────────────────────────

pub fn build_entity_defs() -> Vec<EntityDef> {
    vec![
        entity_def(EntitySpec {
            type_name: work_types::GIG,
            nsid: "io.prismapp.work.gig",
            category: work_categories::FREELANCE,
            label: "Gig",
            plural_label: "Gigs",
            default_child_view: Some(DefaultChildView::Kanban),
            child_only: false,
            extra_child_types: Some(owned_strings([flux_types::TASK, work_types::TIME_ENTRY])),
            fields: gig_fields(),
        }),
        entity_def(EntitySpec {
            type_name: work_types::TIME_ENTRY,
            nsid: "io.prismapp.work.time-entry",
            category: work_categories::TIME,
            label: "Time Entry",
            plural_label: "Time Entries",
            default_child_view: Some(DefaultChildView::List),
            child_only: true,
            extra_child_types: None,
            fields: time_entry_fields(),
        }),
        entity_def(EntitySpec {
            type_name: work_types::FOCUS_BLOCK,
            nsid: "io.prismapp.work.focus-block",
            category: work_categories::FOCUS,
            label: "Focus Block",
            plural_label: "Focus Blocks",
            default_child_view: Some(DefaultChildView::Timeline),
            child_only: false,
            extra_child_types: None,
            fields: focus_block_fields(),
        }),
    ]
}

pub fn build_edge_defs() -> Vec<EdgeTypeDef> {
    vec![
        edge_def(EdgeSpec {
            relation: work_edges::TRACKED_FOR,
            nsid: "io.prismapp.work.tracked-for",
            label: "Tracked For",
            behavior: EdgeBehavior::Membership,
            source_types: owned_strings([work_types::TIME_ENTRY]),
            target_types: Some(owned_strings([
                flux_types::TASK,
                flux_types::PROJECT,
                work_types::GIG,
            ])),
            description: None,
            suggest_inline: false,
            undirected: false,
        }),
        edge_def(EdgeSpec {
            relation: work_edges::BILLED_TO,
            nsid: "io.prismapp.work.billed-to",
            label: "Billed To",
            behavior: EdgeBehavior::Assignment,
            source_types: owned_strings([work_types::TIME_ENTRY]),
            target_types: Some(owned_strings([flux_types::INVOICE])),
            description: None,
            suggest_inline: false,
            undirected: false,
        }),
        edge_def(EdgeSpec {
            relation: work_edges::FOCUS_ON,
            nsid: "io.prismapp.work.focus-on",
            label: "Focus On",
            behavior: EdgeBehavior::Weak,
            source_types: owned_strings([work_types::FOCUS_BLOCK]),
            target_types: Some(owned_strings([
                flux_types::TASK,
                flux_types::PROJECT,
                work_types::GIG,
            ])),
            description: None,
            suggest_inline: false,
            undirected: false,
        }),
    ]
}

// ── Automation presets ──────────────────────────────────────────────────────

pub fn build_automation_presets() -> Vec<FluxAutomationPreset> {
    vec![
        FluxAutomationPreset {
            id: "work:auto:gig-hours-rollup".into(),
            name: "Roll up tracked hours to gig".into(),
            entity_type: work_types::TIME_ENTRY.into(),
            trigger: FluxTriggerKind::OnUpdate,
            condition: Some("durationMinutes > 0".into()),
            actions: vec![FluxAutomationAction {
                kind: FluxActionKind::SetField,
                target: "actualHours".into(),
                value: "{{sum(children.durationMinutes) / 60}}".into(),
            }],
        },
        FluxAutomationPreset {
            id: "work:auto:time-entry-stop".into(),
            name: "Calculate duration on stop".into(),
            entity_type: work_types::TIME_ENTRY.into(),
            trigger: FluxTriggerKind::OnStatusChange,
            condition: Some("status == 'stopped'".into()),
            actions: vec![FluxAutomationAction {
                kind: FluxActionKind::SetField,
                target: "durationMinutes".into(),
                value: "{{diff(endTime, startTime, 'minutes')}}".into(),
            }],
        },
        FluxAutomationPreset {
            id: "work:auto:focus-complete".into(),
            name: "Mark focus block completed".into(),
            entity_type: work_types::FOCUS_BLOCK.into(),
            trigger: FluxTriggerKind::OnUpdate,
            condition: Some("now() >= scheduledEnd".into()),
            actions: vec![FluxAutomationAction {
                kind: FluxActionKind::MoveToStatus,
                target: "status".into(),
                value: "completed".into(),
            }],
        },
    ]
}

// ── Plugin ──────────────────────────────────────────────────────────────────

pub fn build_plugin() -> PrismPlugin {
    PrismPlugin::new(plugin_id("prism.plugin.work"), "Work").with_contributes(PluginContributions {
        views: Some(vec![
            ViewContributionDef {
                id: "work:gigs".into(),
                label: "Gigs".into(),
                zone: ViewZone::Content,
                component_id: "GigBoardView".into(),
                icon: None,
                default_visible: None,
                description: Some("Freelance gig board".into()),
                tags: None,
            },
            ViewContributionDef {
                id: "work:timesheet".into(),
                label: "Timesheet".into(),
                zone: ViewZone::Content,
                component_id: "TimesheetView".into(),
                icon: None,
                default_visible: None,
                description: Some("Time tracking table".into()),
                tags: None,
            },
            ViewContributionDef {
                id: "work:focus".into(),
                label: "Focus Planner".into(),
                zone: ViewZone::Content,
                component_id: "FocusPlannerView".into(),
                icon: None,
                default_visible: None,
                description: Some("Daily focus block scheduler".into()),
                tags: None,
            },
        ]),
        commands: Some(vec![
            CommandContributionDef {
                id: "work:start-timer".into(),
                label: "Start Timer".into(),
                category: "Work".into(),
                shortcut: None,
                description: None,
                action: "work.startTimer".into(),
                payload: None,
                when: None,
            },
            CommandContributionDef {
                id: "work:stop-timer".into(),
                label: "Stop Timer".into(),
                category: "Work".into(),
                shortcut: None,
                description: None,
                action: "work.stopTimer".into(),
                payload: None,
                when: None,
            },
            CommandContributionDef {
                id: "work:new-gig".into(),
                label: "New Gig".into(),
                category: "Work".into(),
                shortcut: None,
                description: None,
                action: "work.newGig".into(),
                payload: None,
                when: None,
            },
            CommandContributionDef {
                id: "work:new-focus-block".into(),
                label: "New Focus Block".into(),
                category: "Work".into(),
                shortcut: None,
                description: None,
                action: "work.newFocusBlock".into(),
                payload: None,
                when: None,
            },
        ]),
        keybindings: Some(vec![
            KeybindingContributionDef {
                command: "work:start-timer".into(),
                key: "ctrl+shift+t".into(),
                when: None,
            },
            KeybindingContributionDef {
                command: "work:stop-timer".into(),
                key: "ctrl+shift+s".into(),
                when: None,
            },
        ]),
        activity_bar: Some(vec![ActivityBarContributionDef {
            id: "work:activity".into(),
            label: "Work".into(),
            icon: None,
            position: Some(ActivityBarPosition::Top),
            priority: Some(20),
        }]),
        context_menus: None,
        settings: None,
        toolbar: None,
        status_bar: None,
        weak_ref_providers: None,
        immersive: None,
    })
}

// ── Bundle ──────────────────────────────────────────────────────────────────

pub struct WorkBundle;

impl PluginBundle for WorkBundle {
    fn id(&self) -> &str {
        "prism.plugin.work"
    }

    fn name(&self) -> &str {
        "Work"
    }

    fn install(&self, ctx: &mut PluginInstallContext<'_>) {
        ctx.object_registry.register_all(build_entity_defs());
        ctx.object_registry.register_edges(build_edge_defs());
        ctx.plugin_registry.register(build_plugin());
    }
}

pub fn create_work_bundle() -> Box<dyn PluginBundle> {
    Box::new(WorkBundle)
}
