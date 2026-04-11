/**
 * @prism/plugin-platform — Platform Services Domain Types (Layer 1)
 *
 * Calendar, messaging, reminders, AI integration, and feeds.
 * These are cross-cutting platform capabilities rather than
 * domain-specific data modules.
 */

import type { EntityDef, EdgeTypeDef } from "@prism/core/object-model";
import type { FluxAutomationPreset } from "@prism/core/flux";
import type { PrismPlugin } from "@prism/core/plugin";

// ── Categories ───────────��───────────────────────────────────────────────

export const PLATFORM_CATEGORIES = {
  CALENDAR: "platform:calendar",
  MESSAGING: "platform:messaging",
  REMINDERS: "platform:reminders",
  FEEDS: "platform:feeds",
} as const;

export type PlatformCategory = typeof PLATFORM_CATEGORIES[keyof typeof PLATFORM_CATEGORIES];

// ── Entity Type Strings ──────────────────────────────────────────────────

export const PLATFORM_TYPES = {
  CALENDAR_EVENT: "platform:calendar-event",
  MESSAGE: "platform:message",
  REMINDER: "platform:reminder",
  FEED: "platform:feed",
  FEED_ITEM: "platform:feed-item",
} as const;

export type PlatformEntityType = typeof PLATFORM_TYPES[keyof typeof PLATFORM_TYPES];

// ── Edge Relation Strings ────────────────────────────────────────────────

export const PLATFORM_EDGES = {
  REMINDS_ABOUT: "platform:reminds-about",
  EVENT_FOR: "platform:event-for",
  REPLY_TO: "platform:reply-to",
  FEED_SOURCE: "platform:feed-source",
} as const;

export type PlatformEdgeType = typeof PLATFORM_EDGES[keyof typeof PLATFORM_EDGES];

// ── Status Values ────────────────────────────────────────────────────────

export const EVENT_STATUSES = [
  { value: "confirmed", label: "Confirmed" },
  { value: "tentative", label: "Tentative" },
  { value: "cancelled", label: "Cancelled" },
] as const;

export const EVENT_RECURRENCES = [
  { value: "none", label: "None" },
  { value: "daily", label: "Daily" },
  { value: "weekly", label: "Weekly" },
  { value: "biweekly", label: "Bi-weekly" },
  { value: "monthly", label: "Monthly" },
  { value: "yearly", label: "Yearly" },
] as const;

export const MESSAGE_STATUSES = [
  { value: "draft", label: "Draft" },
  { value: "sent", label: "Sent" },
  { value: "received", label: "Received" },
  { value: "read", label: "Read" },
  { value: "archived", label: "Archived" },
] as const;

export const MESSAGE_CHANNELS = [
  { value: "internal", label: "Internal" },
  { value: "email", label: "Email" },
  { value: "sms", label: "SMS" },
  { value: "slack", label: "Slack" },
  { value: "discord", label: "Discord" },
] as const;

export const REMINDER_STATUSES = [
  { value: "pending", label: "Pending" },
  { value: "snoozed", label: "Snoozed" },
  { value: "done", label: "Done" },
  { value: "dismissed", label: "Dismissed" },
] as const;

export const REMINDER_PRIORITIES = [
  { value: "low", label: "Low" },
  { value: "normal", label: "Normal" },
  { value: "high", label: "High" },
  { value: "urgent", label: "Urgent" },
] as const;

// ── Registry ─────────────────────────────────────────────��───────────────

export interface PlatformRegistry {
  getEntityDefs(): EntityDef[];
  getEdgeDefs(): EdgeTypeDef[];
  getEntityDef(type: PlatformEntityType): EntityDef | undefined;
  getEdgeDef(relation: PlatformEdgeType): EdgeTypeDef | undefined;
  getAutomationPresets(): FluxAutomationPreset[];
  getPlugin(): PrismPlugin;
}
