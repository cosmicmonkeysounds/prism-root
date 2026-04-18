//! `plugin_bundles::platform` — Platform services bundle.
//!
//! Port of `kernel/plugin-bundles/platform/platform.ts`: calendar
//! events, messages, reminders, and feeds. Cross-cutting integrations
//! rather than a single domain.

use serde_json::json;

use super::builders::{
    edge_def, entity_def, enum_options, owned_strings, ui_hidden, ui_multiline, ui_placeholder,
    ui_readonly, EdgeSpec, EntitySpec, Field,
};
use super::flux_types::{
    flux_types, FluxActionKind, FluxAutomationAction, FluxAutomationPreset, FluxTriggerKind,
};
use super::install::{PluginBundle, PluginInstallContext};
use crate::foundation::object_model::types::DefaultChildView;
use crate::foundation::object_model::{
    EdgeBehavior, EdgeTypeDef, EntityDef, EntityFieldDef, EntityFieldType, EnumOption,
};
use crate::kernel::plugin::{
    plugin_id, ActivityBarContributionDef, ActivityBarPosition, CommandContributionDef,
    KeybindingContributionDef, PluginContributions, PrismPlugin, ViewContributionDef, ViewZone,
};

// ── Domain constants ────────────────────────────────────────────────────────

pub mod platform_categories {
    pub const CALENDAR: &str = "platform:calendar";
    pub const MESSAGING: &str = "platform:messaging";
    pub const REMINDERS: &str = "platform:reminders";
    pub const FEEDS: &str = "platform:feeds";
}

pub mod platform_types {
    pub const CALENDAR_EVENT: &str = "platform:calendar-event";
    pub const MESSAGE: &str = "platform:message";
    pub const REMINDER: &str = "platform:reminder";
    pub const FEED: &str = "platform:feed";
    pub const FEED_ITEM: &str = "platform:feed-item";
}

pub mod platform_edges {
    pub const REMINDS_ABOUT: &str = "platform:reminds-about";
    pub const EVENT_FOR: &str = "platform:event-for";
    pub const REPLY_TO: &str = "platform:reply-to";
    pub const FEED_SOURCE: &str = "platform:feed-source";
}

// ── Option tables ───────────────────────────────────────────────────────────

fn event_recurrences() -> Vec<EnumOption> {
    enum_options(&[
        ("none", "None"),
        ("daily", "Daily"),
        ("weekly", "Weekly"),
        ("biweekly", "Bi-weekly"),
        ("monthly", "Monthly"),
        ("yearly", "Yearly"),
    ])
}

fn message_channels() -> Vec<EnumOption> {
    enum_options(&[
        ("internal", "Internal"),
        ("email", "Email"),
        ("sms", "SMS"),
        ("slack", "Slack"),
        ("discord", "Discord"),
    ])
}

fn reminder_priorities() -> Vec<EnumOption> {
    enum_options(&[
        ("low", "Low"),
        ("normal", "Normal"),
        ("high", "High"),
        ("urgent", "Urgent"),
    ])
}

// ── Fields ──────────────────────────────────────────────────────────────────

fn calendar_event_fields() -> Vec<EntityFieldDef> {
    vec![
        Field::new("startTime", EntityFieldType::Datetime)
            .label("Start")
            .required()
            .build(),
        Field::new("endTime", EntityFieldType::Datetime)
            .label("End")
            .required()
            .build(),
        Field::new("allDay", EntityFieldType::Bool)
            .label("All Day")
            .default(json!(false))
            .build(),
        Field::new("location", EntityFieldType::String)
            .label("Location")
            .build(),
        Field::new("locationUrl", EntityFieldType::Url)
            .label("Location URL")
            .build(),
        Field::new("description", EntityFieldType::Text)
            .label("Description")
            .ui(ui_multiline())
            .build(),
        Field::new("recurrence", EntityFieldType::Enum)
            .label("Recurrence")
            .enum_values(event_recurrences())
            .default(json!("none"))
            .build(),
        Field::new("recurrenceEnd", EntityFieldType::Date)
            .label("Recurrence End")
            .build(),
        Field::new("color", EntityFieldType::Color)
            .label("Color")
            .build(),
        Field::new("calendarId", EntityFieldType::String)
            .label("Calendar ID")
            .build(),
        Field::new("externalId", EntityFieldType::String)
            .label("External ID")
            .ui(ui_hidden())
            .build(),
        Field::new("attendees", EntityFieldType::String)
            .label("Attendees")
            .ui(ui_placeholder("comma-separated emails"))
            .build(),
        Field::new("remindMinutesBefore", EntityFieldType::Int)
            .label("Remind (min before)")
            .default(json!(15))
            .build(),
    ]
}

fn message_fields() -> Vec<EntityFieldDef> {
    vec![
        Field::new("channel", EntityFieldType::Enum)
            .label("Channel")
            .enum_values(message_channels())
            .default(json!("internal"))
            .build(),
        Field::new("from", EntityFieldType::ObjectRef)
            .label("From")
            .ref_types([flux_types::CONTACT])
            .build(),
        Field::new("to", EntityFieldType::String)
            .label("To")
            .ui(ui_placeholder("recipients"))
            .build(),
        Field::new("subject", EntityFieldType::String)
            .label("Subject")
            .build(),
        Field::new("body", EntityFieldType::Text)
            .label("Body")
            .ui(ui_multiline())
            .build(),
        Field::new("sentAt", EntityFieldType::Datetime)
            .label("Sent At")
            .build(),
        Field::new("receivedAt", EntityFieldType::Datetime)
            .label("Received At")
            .build(),
        Field::new("threadId", EntityFieldType::String)
            .label("Thread ID")
            .ui(ui_hidden())
            .build(),
        Field::new("externalId", EntityFieldType::String)
            .label("External ID")
            .ui(ui_hidden())
            .build(),
        Field::new("isRead", EntityFieldType::Bool)
            .label("Read")
            .default(json!(false))
            .build(),
        Field::new("isStarred", EntityFieldType::Bool)
            .label("Starred")
            .default(json!(false))
            .build(),
        Field::new("hasAttachments", EntityFieldType::Bool)
            .label("Has Attachments")
            .default(json!(false))
            .build(),
    ]
}

fn reminder_fields() -> Vec<EntityFieldDef> {
    vec![
        Field::new("dueAt", EntityFieldType::Datetime)
            .label("Due At")
            .required()
            .build(),
        Field::new("priority", EntityFieldType::Enum)
            .label("Priority")
            .enum_values(reminder_priorities())
            .default(json!("normal"))
            .build(),
        Field::new("snoozedUntil", EntityFieldType::Datetime)
            .label("Snoozed Until")
            .build(),
        Field::new("recurring", EntityFieldType::Enum)
            .label("Recurring")
            .enum_values(event_recurrences())
            .default(json!("none"))
            .build(),
        Field::new("description", EntityFieldType::Text)
            .label("Description")
            .ui(ui_multiline())
            .build(),
    ]
}

fn feed_fields() -> Vec<EntityFieldDef> {
    vec![
        Field::new("feedUrl", EntityFieldType::Url)
            .label("Feed URL")
            .required()
            .build(),
        Field::new("feedType", EntityFieldType::Enum)
            .label("Feed Type")
            .enum_values(enum_options(&[
                ("rss", "RSS"),
                ("atom", "Atom"),
                ("json", "JSON Feed"),
                ("webhook", "Webhook"),
            ]))
            .build(),
        Field::new("refreshIntervalMinutes", EntityFieldType::Int)
            .label("Refresh Interval (min)")
            .default(json!(60))
            .build(),
        Field::new("lastFetchedAt", EntityFieldType::Datetime)
            .label("Last Fetched")
            .ui(ui_readonly())
            .build(),
        Field::new("itemCount", EntityFieldType::Int)
            .label("Items")
            .default(json!(0))
            .ui(ui_readonly())
            .build(),
        Field::new("autoArchiveDays", EntityFieldType::Int)
            .label("Auto-archive After (days)")
            .default(json!(30))
            .build(),
    ]
}

fn feed_item_fields() -> Vec<EntityFieldDef> {
    vec![
        Field::new("title", EntityFieldType::String)
            .label("Title")
            .build(),
        Field::new("url", EntityFieldType::Url).label("URL").build(),
        Field::new("author", EntityFieldType::String)
            .label("Author")
            .build(),
        Field::new("publishedAt", EntityFieldType::Datetime)
            .label("Published At")
            .build(),
        Field::new("summary", EntityFieldType::Text)
            .label("Summary")
            .ui(ui_multiline())
            .build(),
        Field::new("content", EntityFieldType::Text)
            .label("Content")
            .ui(ui_multiline())
            .build(),
        Field::new("isRead", EntityFieldType::Bool)
            .label("Read")
            .default(json!(false))
            .build(),
        Field::new("isStarred", EntityFieldType::Bool)
            .label("Starred")
            .default(json!(false))
            .build(),
        Field::new("externalId", EntityFieldType::String)
            .label("External ID")
            .ui(ui_hidden())
            .build(),
    ]
}

// ── Entity + edge defs ──────────────────────────────────────────────────────

pub fn build_entity_defs() -> Vec<EntityDef> {
    vec![
        entity_def(EntitySpec {
            type_name: platform_types::CALENDAR_EVENT,
            nsid: "io.prismapp.platform.calendar-event",
            category: platform_categories::CALENDAR,
            label: "Event",
            plural_label: "Events",
            default_child_view: Some(DefaultChildView::Timeline),
            child_only: false,
            extra_child_types: None,
            fields: calendar_event_fields(),
        }),
        entity_def(EntitySpec {
            type_name: platform_types::MESSAGE,
            nsid: "io.prismapp.platform.message",
            category: platform_categories::MESSAGING,
            label: "Message",
            plural_label: "Messages",
            default_child_view: Some(DefaultChildView::List),
            child_only: false,
            extra_child_types: None,
            fields: message_fields(),
        }),
        entity_def(EntitySpec {
            type_name: platform_types::REMINDER,
            nsid: "io.prismapp.platform.reminder",
            category: platform_categories::REMINDERS,
            label: "Reminder",
            plural_label: "Reminders",
            default_child_view: Some(DefaultChildView::List),
            child_only: false,
            extra_child_types: None,
            fields: reminder_fields(),
        }),
        entity_def(EntitySpec {
            type_name: platform_types::FEED,
            nsid: "io.prismapp.platform.feed",
            category: platform_categories::FEEDS,
            label: "Feed",
            plural_label: "Feeds",
            default_child_view: Some(DefaultChildView::List),
            child_only: false,
            extra_child_types: Some(owned_strings([platform_types::FEED_ITEM])),
            fields: feed_fields(),
        }),
        entity_def(EntitySpec {
            type_name: platform_types::FEED_ITEM,
            nsid: "io.prismapp.platform.feed-item",
            category: platform_categories::FEEDS,
            label: "Feed Item",
            plural_label: "Feed Items",
            default_child_view: None,
            child_only: true,
            extra_child_types: None,
            fields: feed_item_fields(),
        }),
    ]
}

pub fn build_edge_defs() -> Vec<EdgeTypeDef> {
    vec![
        edge_def(EdgeSpec {
            relation: platform_edges::REMINDS_ABOUT,
            nsid: "io.prismapp.platform.reminds-about",
            label: "Reminds About",
            behavior: EdgeBehavior::Weak,
            source_types: owned_strings([platform_types::REMINDER]),
            target_types: None,
            description: Some("Links a reminder to any object"),
            suggest_inline: true,
            undirected: false,
        }),
        edge_def(EdgeSpec {
            relation: platform_edges::EVENT_FOR,
            nsid: "io.prismapp.platform.event-for",
            label: "Event For",
            behavior: EdgeBehavior::Weak,
            source_types: owned_strings([platform_types::CALENDAR_EVENT]),
            target_types: Some(owned_strings([
                flux_types::PROJECT,
                flux_types::CONTACT,
                flux_types::ORGANIZATION,
            ])),
            description: None,
            suggest_inline: true,
            undirected: false,
        }),
        edge_def(EdgeSpec {
            relation: platform_edges::REPLY_TO,
            nsid: "io.prismapp.platform.reply-to",
            label: "Reply To",
            behavior: EdgeBehavior::Weak,
            source_types: owned_strings([platform_types::MESSAGE]),
            target_types: Some(owned_strings([platform_types::MESSAGE])),
            description: None,
            suggest_inline: false,
            undirected: false,
        }),
        edge_def(EdgeSpec {
            relation: platform_edges::FEED_SOURCE,
            nsid: "io.prismapp.platform.feed-source",
            label: "Feed Source",
            behavior: EdgeBehavior::Membership,
            source_types: owned_strings([platform_types::FEED_ITEM]),
            target_types: Some(owned_strings([platform_types::FEED])),
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
            id: "platform:auto:reminder-due".into(),
            name: "Fire reminder notification".into(),
            entity_type: platform_types::REMINDER.into(),
            trigger: FluxTriggerKind::OnDueDate,
            condition: Some("status == 'pending'".into()),
            actions: vec![FluxAutomationAction {
                kind: FluxActionKind::SendNotification,
                target: "owner".into(),
                value: "Reminder: {{name}}".into(),
            }],
        },
        FluxAutomationPreset {
            id: "platform:auto:event-reminder".into(),
            name: "Pre-event reminder".into(),
            entity_type: platform_types::CALENDAR_EVENT.into(),
            trigger: FluxTriggerKind::OnSchedule,
            condition: Some("remindMinutesBefore > 0".into()),
            actions: vec![FluxAutomationAction {
                kind: FluxActionKind::SendNotification,
                target: "owner".into(),
                value: "Event '{{name}}' starting in {{remindMinutesBefore}} minutes".into(),
            }],
        },
        FluxAutomationPreset {
            id: "platform:auto:feed-archive".into(),
            name: "Auto-archive old feed items".into(),
            entity_type: platform_types::FEED_ITEM.into(),
            trigger: FluxTriggerKind::OnSchedule,
            condition: Some("daysOld > parent.autoArchiveDays".into()),
            actions: vec![FluxAutomationAction {
                kind: FluxActionKind::MoveToStatus,
                target: "status".into(),
                value: "archived".into(),
            }],
        },
    ]
}

// ── Plugin ──────────────────────────────────────────────────────────────────

fn view(id: &str, label: &str, component_id: &str, description: &str) -> ViewContributionDef {
    ViewContributionDef {
        id: id.into(),
        label: label.into(),
        zone: ViewZone::Content,
        component_id: component_id.into(),
        icon: None,
        default_visible: None,
        description: Some(description.into()),
        tags: None,
    }
}

fn command(id: &str, label: &str, action: &str) -> CommandContributionDef {
    CommandContributionDef {
        id: id.into(),
        label: label.into(),
        category: "Platform".into(),
        shortcut: None,
        description: None,
        action: action.into(),
        payload: None,
        when: None,
    }
}

pub fn build_plugin() -> PrismPlugin {
    PrismPlugin::new(plugin_id("prism.plugin.platform"), "Platform").with_contributes(
        PluginContributions {
            views: Some(vec![
                view(
                    "platform:calendar",
                    "Calendar",
                    "CalendarView",
                    "Calendar and events",
                ),
                view(
                    "platform:inbox",
                    "Inbox",
                    "InboxView",
                    "Unified messaging inbox",
                ),
                view(
                    "platform:reminders",
                    "Reminders",
                    "ReminderListView",
                    "Reminder manager",
                ),
                view(
                    "platform:feeds",
                    "Feeds",
                    "FeedReaderView",
                    "News & feed reader",
                ),
            ]),
            commands: Some(vec![
                command("platform:new-event", "New Event", "platform.newEvent"),
                command(
                    "platform:new-reminder",
                    "New Reminder",
                    "platform.newReminder",
                ),
                command(
                    "platform:compose-message",
                    "Compose Message",
                    "platform.composeMessage",
                ),
                command("platform:add-feed", "Add Feed", "platform.addFeed"),
            ]),
            keybindings: Some(vec![
                KeybindingContributionDef {
                    command: "platform:new-event".into(),
                    key: "ctrl+shift+e".into(),
                    when: None,
                },
                KeybindingContributionDef {
                    command: "platform:new-reminder".into(),
                    key: "ctrl+shift+r".into(),
                    when: None,
                },
            ]),
            activity_bar: Some(vec![ActivityBarContributionDef {
                id: "platform:activity".into(),
                label: "Platform".into(),
                icon: None,
                position: Some(ActivityBarPosition::Top),
                priority: Some(10),
            }]),
            context_menus: None,
            settings: None,
            toolbar: None,
            status_bar: None,
            weak_ref_providers: None,
            immersive: None,
        },
    )
}

pub struct PlatformBundle;

impl PluginBundle for PlatformBundle {
    fn id(&self) -> &str {
        "prism.plugin.platform"
    }

    fn name(&self) -> &str {
        "Platform"
    }

    fn install(&self, ctx: &mut PluginInstallContext<'_>) {
        ctx.object_registry.register_all(build_entity_defs());
        ctx.object_registry.register_edges(build_edge_defs());
        ctx.plugin_registry.register(build_plugin());
    }
}

pub fn create_platform_bundle() -> Box<dyn PluginBundle> {
    Box::new(PlatformBundle)
}
