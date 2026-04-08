/**
 * @prism/plugin-platform — Platform Services Domain Registry (Layer 1)
 *
 * Registers calendar events, messages, reminders, and feeds.
 */

import type { EntityDef, EntityFieldDef, EdgeTypeDef } from "../../object-model/types.js";
import type { FluxAutomationPreset } from "../../flux/flux-types.js";
import { FLUX_TYPES } from "../../flux/flux-types.js";
import type { PrismPlugin } from "../../plugin/plugin-types.js";
import { pluginId } from "../../plugin/plugin-types.js";
import type { PlatformRegistry, PlatformEntityType, PlatformEdgeType } from "./platform-types.js";
import {
  PLATFORM_CATEGORIES, PLATFORM_TYPES, PLATFORM_EDGES,
  EVENT_RECURRENCES, MESSAGE_CHANNELS, REMINDER_PRIORITIES,
} from "./platform-types.js";

// ── Field Definitions ────────────────────────────────────────────────────

function enumOptions(values: ReadonlyArray<{ value: string; label: string }>): Array<{ value: string; label: string }> {
  return values.map(v => ({ value: v.value, label: v.label }));
}

const CALENDAR_EVENT_FIELDS: EntityFieldDef[] = [
  { id: "startTime", type: "datetime", label: "Start", required: true },
  { id: "endTime", type: "datetime", label: "End", required: true },
  { id: "allDay", type: "bool", label: "All Day", default: false },
  { id: "location", type: "string", label: "Location" },
  { id: "locationUrl", type: "url", label: "Location URL" },
  { id: "description", type: "text", label: "Description", ui: { multiline: true } },
  { id: "recurrence", type: "enum", label: "Recurrence", enumOptions: enumOptions(EVENT_RECURRENCES), default: "none" },
  { id: "recurrenceEnd", type: "date", label: "Recurrence End" },
  { id: "color", type: "color", label: "Color" },
  { id: "calendarId", type: "string", label: "Calendar ID" },
  { id: "externalId", type: "string", label: "External ID", ui: { hidden: true } },
  { id: "attendees", type: "string", label: "Attendees", ui: { placeholder: "comma-separated emails" } },
  { id: "remindMinutesBefore", type: "int", label: "Remind (min before)", default: 15 },
];

const MESSAGE_FIELDS: EntityFieldDef[] = [
  { id: "channel", type: "enum", label: "Channel", enumOptions: enumOptions(MESSAGE_CHANNELS), default: "internal" },
  { id: "from", type: "object_ref", label: "From", refTypes: [FLUX_TYPES.CONTACT] },
  { id: "to", type: "string", label: "To", ui: { placeholder: "recipients" } },
  { id: "subject", type: "string", label: "Subject" },
  { id: "body", type: "text", label: "Body", ui: { multiline: true } },
  { id: "sentAt", type: "datetime", label: "Sent At" },
  { id: "receivedAt", type: "datetime", label: "Received At" },
  { id: "threadId", type: "string", label: "Thread ID", ui: { hidden: true } },
  { id: "externalId", type: "string", label: "External ID", ui: { hidden: true } },
  { id: "isRead", type: "bool", label: "Read", default: false },
  { id: "isStarred", type: "bool", label: "Starred", default: false },
  { id: "hasAttachments", type: "bool", label: "Has Attachments", default: false },
];

const REMINDER_FIELDS: EntityFieldDef[] = [
  { id: "dueAt", type: "datetime", label: "Due At", required: true },
  { id: "priority", type: "enum", label: "Priority", enumOptions: enumOptions(REMINDER_PRIORITIES), default: "normal" },
  { id: "snoozedUntil", type: "datetime", label: "Snoozed Until" },
  { id: "recurring", type: "enum", label: "Recurring", enumOptions: enumOptions(EVENT_RECURRENCES), default: "none" },
  { id: "description", type: "text", label: "Description", ui: { multiline: true } },
];

const FEED_FIELDS: EntityFieldDef[] = [
  { id: "feedUrl", type: "url", label: "Feed URL", required: true },
  { id: "feedType", type: "enum", label: "Feed Type", enumOptions: [
    { value: "rss", label: "RSS" },
    { value: "atom", label: "Atom" },
    { value: "json", label: "JSON Feed" },
    { value: "webhook", label: "Webhook" },
  ] },
  { id: "refreshIntervalMinutes", type: "int", label: "Refresh Interval (min)", default: 60 },
  { id: "lastFetchedAt", type: "datetime", label: "Last Fetched", ui: { readonly: true } },
  { id: "itemCount", type: "int", label: "Items", default: 0, ui: { readonly: true } },
  { id: "autoArchiveDays", type: "int", label: "Auto-archive After (days)", default: 30 },
];

const FEED_ITEM_FIELDS: EntityFieldDef[] = [
  { id: "title", type: "string", label: "Title" },
  { id: "url", type: "url", label: "URL" },
  { id: "author", type: "string", label: "Author" },
  { id: "publishedAt", type: "datetime", label: "Published At" },
  { id: "summary", type: "text", label: "Summary", ui: { multiline: true } },
  { id: "content", type: "text", label: "Content", ui: { multiline: true } },
  { id: "isRead", type: "bool", label: "Read", default: false },
  { id: "isStarred", type: "bool", label: "Starred", default: false },
  { id: "externalId", type: "string", label: "External ID", ui: { hidden: true } },
];

// ── Entity Definitions ───────────────────────────────────────────────────

function buildEntityDefs(): EntityDef[] {
  return [
    {
      type: PLATFORM_TYPES.CALENDAR_EVENT,
      nsid: "io.prismapp.platform.calendar-event",
      category: PLATFORM_CATEGORIES.CALENDAR,
      label: "Event",
      pluralLabel: "Events",
      defaultChildView: "timeline",
      fields: CALENDAR_EVENT_FIELDS,
    },
    {
      type: PLATFORM_TYPES.MESSAGE,
      nsid: "io.prismapp.platform.message",
      category: PLATFORM_CATEGORIES.MESSAGING,
      label: "Message",
      pluralLabel: "Messages",
      defaultChildView: "list",
      fields: MESSAGE_FIELDS,
    },
    {
      type: PLATFORM_TYPES.REMINDER,
      nsid: "io.prismapp.platform.reminder",
      category: PLATFORM_CATEGORIES.REMINDERS,
      label: "Reminder",
      pluralLabel: "Reminders",
      defaultChildView: "list",
      fields: REMINDER_FIELDS,
    },
    {
      type: PLATFORM_TYPES.FEED,
      nsid: "io.prismapp.platform.feed",
      category: PLATFORM_CATEGORIES.FEEDS,
      label: "Feed",
      pluralLabel: "Feeds",
      defaultChildView: "list",
      fields: FEED_FIELDS,
      extraChildTypes: [PLATFORM_TYPES.FEED_ITEM],
    },
    {
      type: PLATFORM_TYPES.FEED_ITEM,
      nsid: "io.prismapp.platform.feed-item",
      category: PLATFORM_CATEGORIES.FEEDS,
      label: "Feed Item",
      pluralLabel: "Feed Items",
      childOnly: true,
      fields: FEED_ITEM_FIELDS,
    },
  ];
}

// ── Edge Definitions ─────────────────────────────────────────────────────

function buildEdgeDefs(): EdgeTypeDef[] {
  return [
    {
      relation: PLATFORM_EDGES.REMINDS_ABOUT,
      nsid: "io.prismapp.platform.reminds-about",
      label: "Reminds About",
      behavior: "weak",
      sourceTypes: [PLATFORM_TYPES.REMINDER],
      description: "Links a reminder to any object",
      suggestInline: true,
    },
    {
      relation: PLATFORM_EDGES.EVENT_FOR,
      nsid: "io.prismapp.platform.event-for",
      label: "Event For",
      behavior: "weak",
      sourceTypes: [PLATFORM_TYPES.CALENDAR_EVENT],
      targetTypes: [FLUX_TYPES.PROJECT, FLUX_TYPES.CONTACT, FLUX_TYPES.ORGANIZATION],
      suggestInline: true,
    },
    {
      relation: PLATFORM_EDGES.REPLY_TO,
      nsid: "io.prismapp.platform.reply-to",
      label: "Reply To",
      behavior: "weak",
      sourceTypes: [PLATFORM_TYPES.MESSAGE],
      targetTypes: [PLATFORM_TYPES.MESSAGE],
    },
    {
      relation: PLATFORM_EDGES.FEED_SOURCE,
      nsid: "io.prismapp.platform.feed-source",
      label: "Feed Source",
      behavior: "membership",
      sourceTypes: [PLATFORM_TYPES.FEED_ITEM],
      targetTypes: [PLATFORM_TYPES.FEED],
    },
  ];
}

// ── Automation Presets ────────────────────────────────────────────────────

function buildAutomationPresets(): FluxAutomationPreset[] {
  return [
    {
      id: "platform:auto:reminder-due",
      name: "Fire reminder notification",
      entityType: PLATFORM_TYPES.REMINDER,
      trigger: "on_due_date",
      condition: "status == 'pending'",
      actions: [
        { kind: "send_notification", target: "owner", value: "Reminder: {{name}}" },
      ],
    },
    {
      id: "platform:auto:event-reminder",
      name: "Pre-event reminder",
      entityType: PLATFORM_TYPES.CALENDAR_EVENT,
      trigger: "on_schedule",
      condition: "remindMinutesBefore > 0",
      actions: [
        { kind: "send_notification", target: "owner", value: "Event '{{name}}' starting in {{remindMinutesBefore}} minutes" },
      ],
    },
    {
      id: "platform:auto:feed-archive",
      name: "Auto-archive old feed items",
      entityType: PLATFORM_TYPES.FEED_ITEM,
      trigger: "on_schedule",
      condition: "daysOld > parent.autoArchiveDays",
      actions: [
        { kind: "move_to_status", target: "status", value: "archived" },
      ],
    },
  ];
}

// ── Plugin ───────────────────────────────────────────────────────────────

function buildPlugin(): PrismPlugin {
  return {
    id: pluginId("prism.plugin.platform"),
    name: "Platform",
    contributes: {
      views: [
        { id: "platform:calendar", label: "Calendar", zone: "content", componentId: "CalendarView", description: "Calendar and events" },
        { id: "platform:inbox", label: "Inbox", zone: "content", componentId: "InboxView", description: "Unified messaging inbox" },
        { id: "platform:reminders", label: "Reminders", zone: "content", componentId: "ReminderListView", description: "Reminder manager" },
        { id: "platform:feeds", label: "Feeds", zone: "content", componentId: "FeedReaderView", description: "News & feed reader" },
      ],
      commands: [
        { id: "platform:new-event", label: "New Event", category: "Platform", action: "platform.newEvent" },
        { id: "platform:new-reminder", label: "New Reminder", category: "Platform", action: "platform.newReminder" },
        { id: "platform:compose-message", label: "Compose Message", category: "Platform", action: "platform.composeMessage" },
        { id: "platform:add-feed", label: "Add Feed", category: "Platform", action: "platform.addFeed" },
      ],
      keybindings: [
        { command: "platform:new-event", key: "ctrl+shift+e" },
        { command: "platform:new-reminder", key: "ctrl+shift+r" },
      ],
      activityBar: [
        { id: "platform:activity", label: "Platform", position: "top", priority: 10 },
      ],
    },
  };
}

// ── Factory ──────────────────────────────────────────────────────────────

export function createPlatformRegistry(): PlatformRegistry {
  const entityDefs = buildEntityDefs();
  const edgeDefs = buildEdgeDefs();
  const presets = buildAutomationPresets();
  const plugin = buildPlugin();

  return {
    getEntityDefs: () => entityDefs,
    getEdgeDefs: () => edgeDefs,
    getEntityDef: (type: PlatformEntityType) => entityDefs.find(d => d.type === type),
    getEdgeDef: (relation: PlatformEdgeType) => edgeDefs.find(d => d.relation === relation),
    getAutomationPresets: () => presets,
    getPlugin: () => plugin,
  };
}

// ── Self-Registering Bundle ──────────────────────────────────────────────

import type { PluginBundle, PluginInstallContext } from "../plugin-install.js";

export function createPlatformBundle(): PluginBundle {
  return {
    id: "prism.plugin.platform",
    name: "Platform",
    install(ctx: PluginInstallContext) {
      const reg = createPlatformRegistry();
      ctx.objectRegistry.registerAll(reg.getEntityDefs());
      ctx.objectRegistry.registerEdges(reg.getEdgeDefs());
      return ctx.pluginRegistry.register(reg.getPlugin());
    },
  };
}
