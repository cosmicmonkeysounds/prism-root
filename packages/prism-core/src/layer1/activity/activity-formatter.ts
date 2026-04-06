/**
 * Activity Formatter
 *
 * Pure functions that turn ActivityEvents into human-readable text.
 * No DOM, no React, no external deps.
 *
 * Exported functions:
 *   formatActivity(event, opts?)    → ActivityDescription
 *   formatFieldName(field)          → string
 *   formatFieldValue(value, field?) → string
 *   groupActivityByDate(events)     → ActivityGroup[]
 */

import type {
  ActivityEvent,
  ActivityDescription,
  ActivityGroup,
} from "./activity-log.js";

// ── Constants ──────────────────────────────────────────────────────────────────

const MAX_TEXT_LENGTH = 60;

// ── formatFieldName ────────────────────────────────────────────────────────────

/**
 * Converts a raw field path to a display label.
 *
 * @example
 *   formatFieldName('data.priority')  → 'priority'
 *   formatFieldName('parentId')       → 'parent'
 *   formatFieldName('endDate')        → 'end date'
 */
export function formatFieldName(field: string): string {
  const bare = field.startsWith("data.") ? field.slice(5) : field;

  const OVERRIDES: Record<string, string> = {
    parentId: "parent",
    endDate: "end date",
    createdAt: "created",
    deletedAt: "deleted",
    updatedAt: "updated",
  };
  const override = OVERRIDES[bare as keyof typeof OVERRIDES];
  if (override !== undefined) return override;

  return bare
    .replace(/([a-z])([A-Z])/g, "$1 $2")
    .replace(/_/g, " ")
    .replace(/-/g, " ")
    .toLowerCase()
    .trim();
}

// ── formatFieldValue ───────────────────────────────────────────────────────────

/**
 * Formats a field value for inline display in an activity summary.
 *
 * - null / undefined   → "(none)"
 * - boolean            → "yes" / "no"
 * - arrays             → comma-joined, max 3 items + "and N more"
 * - long strings       → truncated to MAX_TEXT_LENGTH chars
 * - numbers            → locale string
 */
export function formatFieldValue(value: unknown, _field?: string): string {
  if (value === null || value === undefined) return "(none)";

  if (typeof value === "boolean") return value ? "yes" : "no";

  if (typeof value === "number") return value.toLocaleString();

  if (Array.isArray(value)) {
    if (value.length === 0) return "(empty)";
    const shown = value.slice(0, 3).map((v) => formatFieldValue(v));
    const rest = value.length - 3;
    return rest > 0 ? `${shown.join(", ")} and ${rest} more` : shown.join(", ");
  }

  if (typeof value === "string") {
    if (/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}/.test(value)) {
      try {
        return new Date(value).toLocaleString("en-US", {
          year: "numeric",
          month: "short",
          day: "numeric",
          hour: "numeric",
          minute: "2-digit",
        });
      } catch {
        // fall through
      }
    }
    if (/^\d{4}-\d{2}-\d{2}$/.test(value)) {
      try {
        const parts = value.split("-").map(Number) as [number, number, number];
        const d = new Date(Date.UTC(parts[0], parts[1] - 1, parts[2]));
        return d.toLocaleDateString("en-US", {
          year: "numeric",
          month: "short",
          day: "numeric",
          timeZone: "UTC",
        });
      } catch {
        // fall through
      }
    }
    if (value.length > MAX_TEXT_LENGTH) {
      return value.slice(0, MAX_TEXT_LENGTH) + "\u2026";
    }
    return value || "(empty)";
  }

  if (typeof value === "object") {
    try {
      const s = JSON.stringify(value);
      return s.length > MAX_TEXT_LENGTH
        ? s.slice(0, MAX_TEXT_LENGTH) + "\u2026"
        : s;
    } catch {
      return "(object)";
    }
  }

  return String(value);
}

// ── formatActivity ─────────────────────────────────────────────────────────────

/**
 * Builds a human-readable ActivityDescription for one event.
 */
export function formatActivity(
  event: ActivityEvent,
  opts?: { actorName?: string; objectName?: string },
): ActivityDescription {
  const actor = opts?.actorName ?? event.actorName ?? "Someone";
  const object = opts?.objectName;

  let text: string;
  let html: string | undefined;

  const bold = (s: string): string => `<b>${s}</b>`;

  switch (event.verb) {
    case "created": {
      text = object
        ? `${actor} created "${object}"`
        : `${actor} created this`;
      html = object
        ? `${bold(actor)} created ${bold(object)}`
        : `${bold(actor)} created this`;
      break;
    }

    case "deleted": {
      text = `${actor} deleted this`;
      html = `${bold(actor)} deleted this`;
      break;
    }

    case "restored": {
      text = `${actor} restored this`;
      html = `${bold(actor)} restored this`;
      break;
    }

    case "renamed": {
      const nc = event.changes?.find((c) => c.field === "name");
      if (nc) {
        text = `${actor} renamed from "${formatFieldValue(nc.before)}" to "${formatFieldValue(nc.after)}"`;
        html = `${bold(actor)} renamed from ${bold(formatFieldValue(nc.before))} to ${bold(formatFieldValue(nc.after))}`;
      } else {
        text = `${actor} renamed this`;
        html = `${bold(actor)} renamed this`;
      }
      break;
    }

    case "status-changed": {
      const from = event.fromStatus
        ? formatFieldValue(event.fromStatus)
        : "(none)";
      const to = event.toStatus
        ? formatFieldValue(event.toStatus)
        : "(none)";
      text = `${actor} changed status from "${from}" to "${to}"`;
      html = `${bold(actor)} changed status from ${bold(from)} to ${bold(to)}`;
      break;
    }

    case "moved": {
      const fromId = event.fromParentId ?? null;
      const toId = event.toParentId ?? null;
      if (!fromId && toId) {
        text = `${actor} moved this into a container`;
        html = `${bold(actor)} moved this into a container`;
      } else if (fromId && !toId) {
        text = `${actor} moved this to root level`;
        html = `${bold(actor)} moved this to root level`;
      } else {
        text = `${actor} moved this to a new location`;
        html = `${bold(actor)} moved this to a new location`;
      }
      break;
    }

    case "updated": {
      const changes = event.changes ?? [];
      if (changes.length === 0) {
        text = `${actor} updated this`;
        html = `${bold(actor)} updated this`;
      } else if (changes.length === 1 && changes[0]) {
        const c = changes[0];
        const fieldLabel = formatFieldName(c.field);
        const fromVal = formatFieldValue(c.before, c.field);
        const toVal = formatFieldValue(c.after, c.field);
        text = `${actor} changed ${fieldLabel} from "${fromVal}" to "${toVal}"`;
        html = `${bold(actor)} changed ${fieldLabel} from ${bold(fromVal)} to ${bold(toVal)}`;
      } else {
        const fieldNames = changes
          .slice(0, 3)
          .map((c) => formatFieldName(c.field))
          .join(", ");
        const extra = changes.length - 3;
        const fieldSummary =
          extra > 0 ? `${fieldNames} and ${extra} more` : fieldNames;
        text = `${actor} updated ${fieldSummary}`;
        html = `${bold(actor)} updated ${fieldSummary}`;
      }
      break;
    }

    case "commented": {
      const comment = event.meta?.["comment"] as string | undefined;
      if (comment) {
        const preview =
          comment.length > MAX_TEXT_LENGTH
            ? comment.slice(0, MAX_TEXT_LENGTH) + "\u2026"
            : comment;
        text = `${actor} commented: "${preview}"`;
        html = `${bold(actor)} commented: "${preview}"`;
      } else {
        text = `${actor} left a comment`;
        html = `${bold(actor)} left a comment`;
      }
      break;
    }

    case "mentioned": {
      text = `${actor} mentioned this`;
      html = `${bold(actor)} mentioned this`;
      break;
    }

    case "assigned": {
      const assignee = event.meta?.["assigneeName"] as string | undefined;
      text = assignee
        ? `${actor} assigned this to ${assignee}`
        : `${actor} assigned this`;
      html = assignee
        ? `${bold(actor)} assigned this to ${bold(assignee)}`
        : `${bold(actor)} assigned this`;
      break;
    }

    case "unassigned": {
      const assignee = event.meta?.["assigneeName"] as string | undefined;
      text = assignee
        ? `${actor} unassigned ${assignee}`
        : `${actor} unassigned this`;
      html = assignee
        ? `${bold(actor)} unassigned ${bold(assignee)}`
        : `${bold(actor)} unassigned this`;
      break;
    }

    case "attached": {
      const name = event.meta?.["name"] as string | undefined;
      text = name
        ? `${actor} attached "${name}"`
        : `${actor} added an attachment`;
      html = name
        ? `${bold(actor)} attached ${bold(name)}`
        : `${bold(actor)} added an attachment`;
      break;
    }

    case "detached": {
      const name = event.meta?.["name"] as string | undefined;
      text = name
        ? `${actor} removed attachment "${name}"`
        : `${actor} removed an attachment`;
      html = name
        ? `${bold(actor)} removed attachment ${bold(name)}`
        : `${bold(actor)} removed an attachment`;
      break;
    }

    case "linked": {
      const target = event.meta?.["targetName"] as string | undefined;
      text = target
        ? `${actor} linked to "${target}"`
        : `${actor} added a link`;
      html = target
        ? `${bold(actor)} linked to ${bold(target)}`
        : `${bold(actor)} added a link`;
      break;
    }

    case "unlinked": {
      const target = event.meta?.["targetName"] as string | undefined;
      text = target
        ? `${actor} removed link to "${target}"`
        : `${actor} removed a link`;
      html = target
        ? `${bold(actor)} removed link to ${bold(target)}`
        : `${bold(actor)} removed a link`;
      break;
    }

    case "completed": {
      text = `${actor} completed this`;
      html = `${bold(actor)} completed this`;
      break;
    }

    case "reopened": {
      text = `${actor} reopened this`;
      html = `${bold(actor)} reopened this`;
      break;
    }

    case "blocked": {
      const reason = event.meta?.["reason"] as string | undefined;
      text = reason
        ? `${actor} blocked this: "${reason}"`
        : `${actor} blocked this`;
      html = reason
        ? `${bold(actor)} blocked this: "${reason}"`
        : `${bold(actor)} blocked this`;
      break;
    }

    case "unblocked": {
      text = `${actor} unblocked this`;
      html = `${bold(actor)} unblocked this`;
      break;
    }

    case "custom": {
      const label =
        (event.meta?.["verb"] as string | undefined) ?? "performed an action";
      text = `${actor} ${label}`;
      html = `${bold(actor)} ${label}`;
      break;
    }

    default: {
      text = `${actor} updated this`;
      html = `${bold(actor)} updated this`;
    }
  }

  return { text, html };
}

// ── groupActivityByDate ────────────────────────────────────────────────────────

function startOfDayUTC(isoDate: string): number {
  const d = new Date(isoDate);
  return Date.UTC(d.getUTCFullYear(), d.getUTCMonth(), d.getUTCDate());
}

function todayUTC(): number {
  const now = new Date();
  return Date.UTC(now.getUTCFullYear(), now.getUTCMonth(), now.getUTCDate());
}

const ONE_DAY = 86_400_000;

/**
 * Partitions events into labelled date buckets for timeline rendering.
 * Events can be in any order; each group is sorted newest-first.
 *
 * Buckets: Today | Yesterday | This week | Earlier
 */
export function groupActivityByDate(events: ActivityEvent[]): ActivityGroup[] {
  const today = todayUTC();
  const yesterday = today - ONE_DAY;
  const weekAgo = today - 7 * ONE_DAY;

  type BucketLabel = "Today" | "Yesterday" | "This week" | "Earlier";
  const buckets: Record<BucketLabel, ActivityEvent[]> = {
    Today: [],
    Yesterday: [],
    "This week": [],
    Earlier: [],
  };

  for (const event of events) {
    const day = startOfDayUTC(event.createdAt);
    if (day >= today) {
      buckets["Today"].push(event);
    } else if (day >= yesterday) {
      buckets["Yesterday"].push(event);
    } else if (day > weekAgo) {
      buckets["This week"].push(event);
    } else {
      buckets["Earlier"].push(event);
    }
  }

  const ORDER: readonly BucketLabel[] = ["Today", "Yesterday", "This week", "Earlier"];
  const groups: ActivityGroup[] = [];

  for (const label of ORDER) {
    const bucket = buckets[label];
    if (bucket.length === 0) continue;
    bucket.sort((a, b) => (a.createdAt > b.createdAt ? -1 : 1));
    groups.push({ label, events: bucket });
  }

  return groups;
}
