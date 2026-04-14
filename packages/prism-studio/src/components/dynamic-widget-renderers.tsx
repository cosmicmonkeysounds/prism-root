/**
 * Dynamic widget renderers — Puck-draggable views over runtime record data.
 *
 * Each widget pulls objects from the kernel store (`kernel.store.allObjects()`)
 * filtered by record type, then renders a specialised affordance: task rows
 * with checkboxes, reminder rows with relative date chips, contact cards,
 * event timelines, etc.
 *
 * Pure helpers (filtering, bucketing, formatting) are exported at the top of
 * the module for vitest — no React needed to cover them.
 */

import { useState, type CSSProperties } from "react";
import type { GraphObject } from "@prism/core/object-model";

// ─────────────────────────────────────────────────────────────────────────────
// Shared pure helpers
// ─────────────────────────────────────────────────────────────────────────────

export type TaskFilter = "all" | "open" | "today" | "overdue" | "done";
export type ReminderFilter = "all" | "upcoming" | "overdue" | "today" | "done";
export type ContactFilter = "all" | "favorites" | "recent";
export type EventRange = "today" | "week" | "month" | "all";
export type NoteFilter = "all" | "pinned" | "recent";
export type GoalFilter = "all" | "active" | "completed";

export interface RelativeDateResult {
  label: string;
  tone: "overdue" | "today" | "soon" | "later" | "none";
}

/** Parse a date/endDate into a millisecond timestamp, or null if missing. */
export function parseObjectDate(value: string | null | undefined): number | null {
  if (!value) return null;
  const ms = Date.parse(value);
  return Number.isNaN(ms) ? null : ms;
}

/** Get "now" in ms. Separate fn so tests can stub it. */
export function nowMs(): number {
  return Date.now();
}

/**
 * Convert an absolute ISO date into a relative, user-facing chip label.
 * Returns "Overdue Nd", "Today", "Tomorrow", "In Nd", or weekday/date strings.
 */
export function formatRelativeDate(
  iso: string | null | undefined,
  now: number = nowMs(),
): RelativeDateResult {
  const ms = parseObjectDate(iso);
  if (ms === null) return { label: "", tone: "none" };
  const startOfToday = new Date(now);
  startOfToday.setHours(0, 0, 0, 0);
  const startOfTarget = new Date(ms);
  startOfTarget.setHours(0, 0, 0, 0);
  const dayDiff = Math.round(
    (startOfTarget.getTime() - startOfToday.getTime()) / 86_400_000,
  );
  if (dayDiff < 0) return { label: `Overdue ${Math.abs(dayDiff)}d`, tone: "overdue" };
  if (dayDiff === 0) return { label: "Today", tone: "today" };
  if (dayDiff === 1) return { label: "Tomorrow", tone: "soon" };
  if (dayDiff <= 7) return { label: `In ${dayDiff}d`, tone: "soon" };
  const d = new Date(ms);
  return {
    label: d.toLocaleDateString(undefined, { month: "short", day: "numeric" }),
    tone: "later",
  };
}

/**
 * Apply a task filter to a list of tasks. "open" = not done/archived;
 * "today" = due today and not done; "overdue" = due before today and not done.
 */
export function filterTasks(
  tasks: GraphObject[],
  filter: TaskFilter,
  project: string,
  now: number = nowMs(),
): GraphObject[] {
  const startOfToday = new Date(now);
  startOfToday.setHours(0, 0, 0, 0);
  const endOfToday = startOfToday.getTime() + 86_400_000;
  return tasks.filter((t) => {
    if (project) {
      const p = (t.data as Record<string, unknown>)["project"];
      if (typeof p !== "string" || p.trim() !== project.trim()) return false;
    }
    const isDone = t.status === "done" || t.status === "archived";
    const due = parseObjectDate(t.date);
    switch (filter) {
      case "all":
        return true;
      case "open":
        return !isDone;
      case "done":
        return isDone;
      case "today":
        return !isDone && due !== null && due >= startOfToday.getTime() && due < endOfToday;
      case "overdue":
        return !isDone && due !== null && due < startOfToday.getTime();
    }
  });
}

/** Order tasks: overdue first, then by date asc (nulls last), then priority. */
export function orderTasks(tasks: GraphObject[]): GraphObject[] {
  const priorityRank: Record<string, number> = { urgent: 0, high: 1, normal: 2, low: 3 };
  return [...tasks].sort((a, b) => {
    const ad = parseObjectDate(a.date);
    const bd = parseObjectDate(b.date);
    if (ad !== null && bd !== null && ad !== bd) return ad - bd;
    if (ad !== null && bd === null) return -1;
    if (ad === null && bd !== null) return 1;
    const ap = priorityRank[String((a.data as Record<string, unknown>)["priority"] ?? "normal")] ?? 2;
    const bp = priorityRank[String((b.data as Record<string, unknown>)["priority"] ?? "normal")] ?? 2;
    return ap - bp;
  });
}

export function priorityColor(priority: string | undefined): string {
  switch (priority) {
    case "urgent":
      return "#dc2626";
    case "high":
      return "#f59e0b";
    case "low":
      return "#64748b";
    case "normal":
    default:
      return "#6366f1";
  }
}

/** Filter reminders by status/date window. */
export function filterReminders(
  reminders: GraphObject[],
  filter: ReminderFilter,
  now: number = nowMs(),
): GraphObject[] {
  const startOfToday = new Date(now);
  startOfToday.setHours(0, 0, 0, 0);
  const endOfToday = startOfToday.getTime() + 86_400_000;
  return reminders.filter((r) => {
    const isDone = r.status === "done" || r.status === "archived";
    const due = parseObjectDate(r.date);
    switch (filter) {
      case "all":
        return true;
      case "upcoming":
        return !isDone;
      case "done":
        return isDone;
      case "today":
        return !isDone && due !== null && due >= startOfToday.getTime() && due < endOfToday;
      case "overdue":
        return !isDone && due !== null && due < startOfToday.getTime();
    }
  });
}

export function filterContacts(
  contacts: GraphObject[],
  filter: ContactFilter,
): GraphObject[] {
  switch (filter) {
    case "all":
      return contacts;
    case "favorites":
      return contacts.filter((c) => c.pinned);
    case "recent":
      return [...contacts].sort((a, b) => {
        const ad = parseObjectDate(
          String((a.data as Record<string, unknown>)["lastContactedAt"] ?? "") || null,
        );
        const bd = parseObjectDate(
          String((b.data as Record<string, unknown>)["lastContactedAt"] ?? "") || null,
        );
        if (ad === null && bd === null) return 0;
        if (ad === null) return 1;
        if (bd === null) return -1;
        return bd - ad;
      });
  }
}

/** Get initials for a contact avatar fallback. */
export function contactInitials(name: string): string {
  const parts = name.trim().split(/\s+/).filter(Boolean);
  if (parts.length === 0) return "?";
  if (parts.length === 1) {
    const first = parts[0] ?? "";
    return (first[0] ?? "?").toUpperCase();
  }
  const a = parts[0] ?? "";
  const b = parts[parts.length - 1] ?? "";
  return ((a[0] ?? "") + (b[0] ?? "")).toUpperCase() || "?";
}

/** Filter events to upcoming items inside a fixed range. */
export function filterEvents(
  events: GraphObject[],
  range: EventRange,
  now: number = nowMs(),
): GraphObject[] {
  const startOfToday = new Date(now);
  startOfToday.setHours(0, 0, 0, 0);
  const start = startOfToday.getTime();
  const end =
    range === "today"
      ? start + 86_400_000
      : range === "week"
      ? start + 7 * 86_400_000
      : range === "month"
      ? start + 30 * 86_400_000
      : Number.POSITIVE_INFINITY;
  return events
    .filter((e) => {
      const d = parseObjectDate(e.date);
      if (d === null) return false;
      return d >= start && d < end;
    })
    .sort((a, b) => (parseObjectDate(a.date) ?? 0) - (parseObjectDate(b.date) ?? 0));
}

export function formatEventTime(iso: string | null | undefined): string {
  if (!iso) return "";
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return "";
  return d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
}

export function filterNotes(
  notes: GraphObject[],
  filter: NoteFilter,
  tag: string,
): GraphObject[] {
  let result = notes;
  if (tag.trim()) {
    const t = tag.trim().toLowerCase();
    result = result.filter((n) => n.tags.some((tg) => tg.toLowerCase() === t));
  }
  switch (filter) {
    case "all":
      return result;
    case "pinned":
      return result.filter((n) => n.pinned);
    case "recent":
      return [...result].sort(
        (a, b) => (parseObjectDate(b.updatedAt) ?? 0) - (parseObjectDate(a.updatedAt) ?? 0),
      );
  }
}

/** Extract a preview string from a note object, truncating cleanly. */
export function notePreview(note: GraphObject, length: number): string {
  const body = String((note.data as Record<string, unknown>)["body"] ?? note.description ?? "");
  const clean = body.replace(/\s+/g, " ").trim();
  if (clean.length <= length) return clean;
  return clean.slice(0, Math.max(0, length - 1)) + "\u2026";
}

export function filterGoals(goals: GraphObject[], filter: GoalFilter): GraphObject[] {
  switch (filter) {
    case "all":
      return goals;
    case "active":
      return goals.filter((g) => g.status !== "done" && g.status !== "archived");
    case "completed":
      return goals.filter((g) => g.status === "done");
  }
}

/** 0..1 ratio of currentValue / targetValue, clamped. */
export function goalRatio(goal: GraphObject): number {
  const d = goal.data as Record<string, unknown>;
  const current = Number(d["currentValue"] ?? 0);
  const target = Number(d["targetValue"] ?? 0);
  if (!Number.isFinite(current) || !Number.isFinite(target) || target <= 0) return 0;
  return Math.max(0, Math.min(1, current / target));
}

/** Format duration in ms as "Xh Ym" or "Ym Xs" for short sessions. */
export function formatDuration(ms: number): string {
  if (!Number.isFinite(ms) || ms <= 0) return "0m";
  const s = Math.floor(ms / 1000);
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const sec = s % 60;
  if (h > 0) return `${h}h ${m}m`;
  if (m > 0) return `${m}m ${sec > 0 && m < 10 ? `${sec}s` : ""}`.trim();
  return `${sec}s`;
}

export function filterBookmarks(bookmarks: GraphObject[], folder: string): GraphObject[] {
  if (!folder.trim()) return bookmarks;
  const f = folder.trim();
  return bookmarks.filter((b) => {
    const bf = (b.data as Record<string, unknown>)["folder"];
    return typeof bf === "string" && bf === f;
  });
}

/** Pull a safe hostname for display from a bookmark URL. */
export function bookmarkHost(url: string): string {
  try {
    const u = new URL(url);
    return u.hostname.replace(/^www\./, "");
  } catch {
    return url;
  }
}

/** Habit: ratio of completed days this week (0..1) against target. */
export function habitWeeklyRatio(habit: GraphObject): number {
  const d = habit.data as Record<string, unknown>;
  const target = Number(d["targetPerWeek"] ?? 7);
  const streak = Number(d["streak"] ?? 0);
  if (!Number.isFinite(target) || target <= 0) return 0;
  const progress = Math.min(target, Math.max(0, streak));
  return progress / target;
}

export function filterCaptures(
  captures: GraphObject[],
  showProcessed: boolean,
): GraphObject[] {
  return captures.filter((c) => {
    const processed = !!(c.data as Record<string, unknown>)["processedAt"];
    return showProcessed || !processed;
  });
}

// ─────────────────────────────────────────────────────────────────────────────
// Shared styles / primitives
// ─────────────────────────────────────────────────────────────────────────────

const cardBase: CSSProperties = {
  border: "1px solid #334155",
  borderRadius: 6,
  background: "#0f172a",
  color: "#e2e8f0",
  overflow: "hidden",
};

const headingRow: CSSProperties = {
  display: "flex",
  alignItems: "center",
  justifyContent: "space-between",
  padding: "10px 12px",
  borderBottom: "1px solid #1e293b",
  fontSize: 13,
  fontWeight: 600,
  color: "#e2e8f0",
  background: "#111827",
};

const emptyState: CSSProperties = {
  padding: 20,
  fontSize: 12,
  fontStyle: "italic",
  color: "#64748b",
  textAlign: "center",
};

function relativeToneStyle(tone: RelativeDateResult["tone"]): CSSProperties {
  switch (tone) {
    case "overdue":
      return { background: "#451a1a", color: "#fca5a5" };
    case "today":
      return { background: "#1a4731", color: "#34d399" };
    case "soon":
      return { background: "#1e3a8a", color: "#93c5fd" };
    case "later":
      return { background: "#1e293b", color: "#94a3b8" };
    case "none":
    default:
      return { background: "transparent", color: "transparent" };
  }
}

function DateChip({ iso }: { iso: string | null | undefined }) {
  const { label, tone } = formatRelativeDate(iso);
  if (!label) return null;
  return (
    <span
      data-testid="date-chip"
      style={{
        fontSize: 10,
        padding: "2px 6px",
        borderRadius: 3,
        whiteSpace: "nowrap",
        ...relativeToneStyle(tone),
      }}
    >
      {label}
    </span>
  );
}

// ─────────────────────────────────────────────────────────────────────────────
// TasksWidgetRenderer
// ─────────────────────────────────────────────────────────────────────────────

export interface TasksWidgetProps {
  objects: GraphObject[];
  title?: string;
  filter?: TaskFilter;
  project?: string;
  maxItems?: number;
  showPriority?: boolean;
  showDueDate?: boolean;
  onToggleDone?: (id: string, newStatus: string) => void;
  onSelectObject?: (id: string) => void;
  selectedId?: string | null;
}

export function TasksWidgetRenderer(props: TasksWidgetProps) {
  const {
    objects,
    title = "Tasks",
    filter = "open",
    project = "",
    maxItems = 10,
    showPriority = true,
    showDueDate = true,
    onToggleDone,
    onSelectObject,
    selectedId = null,
  } = props;

  const filtered = orderTasks(filterTasks(objects, filter, project)).slice(0, maxItems);

  return (
    <div data-testid="tasks-widget" style={cardBase}>
      <div style={headingRow}>
        <span>{title}</span>
        <span style={{ fontSize: 11, color: "#94a3b8", fontWeight: 400 }}>
          {filtered.length} shown
        </span>
      </div>
      {filtered.length === 0 ? (
        <div style={emptyState}>No tasks match this filter.</div>
      ) : (
        filtered.map((t) => {
          const isDone = t.status === "done" || t.status === "archived";
          const priority = String((t.data as Record<string, unknown>)["priority"] ?? "normal");
          const isSelected = selectedId === t.id;
          return (
            <div
              key={t.id}
              data-testid={`tasks-widget-row-${t.id}`}
              onClick={() => onSelectObject?.(t.id)}
              style={{
                display: "flex",
                alignItems: "center",
                gap: 10,
                padding: "8px 12px",
                borderBottom: "1px solid #1e293b",
                cursor: "pointer",
                background: isSelected ? "#1a3350" : "transparent",
              }}
            >
              <button
                type="button"
                aria-label={isDone ? "Mark not done" : "Mark done"}
                onClick={(e) => {
                  e.stopPropagation();
                  onToggleDone?.(t.id, isDone ? "todo" : "done");
                }}
                style={{
                  width: 16,
                  height: 16,
                  borderRadius: 3,
                  border: `1px solid ${isDone ? "#22c55e" : "#475569"}`,
                  background: isDone ? "#22c55e" : "transparent",
                  color: "#0f172a",
                  fontSize: 10,
                  lineHeight: "14px",
                  cursor: "pointer",
                  flexShrink: 0,
                }}
              >
                {isDone ? "\u2713" : ""}
              </button>
              {showPriority ? (
                <span
                  aria-label={`Priority ${priority}`}
                  style={{
                    width: 6,
                    height: 6,
                    borderRadius: 999,
                    background: priorityColor(priority),
                    flexShrink: 0,
                  }}
                />
              ) : null}
              <div
                style={{
                  flex: 1,
                  minWidth: 0,
                  fontSize: 13,
                  color: isDone ? "#64748b" : "#e2e8f0",
                  textDecoration: isDone ? "line-through" : "none",
                  overflow: "hidden",
                  textOverflow: "ellipsis",
                  whiteSpace: "nowrap",
                }}
              >
                {t.name}
              </div>
              {showDueDate ? <DateChip iso={t.date} /> : null}
            </div>
          );
        })
      )}
    </div>
  );
}

// ─────────────────────────────────────────────────────────────────────────────
// RemindersWidgetRenderer
// ─────────────────────────────────────────────────────────────────────────────

export interface RemindersWidgetProps {
  objects: GraphObject[];
  title?: string;
  filter?: ReminderFilter;
  maxItems?: number;
  onToggleDone?: (id: string, newStatus: string) => void;
  onSelectObject?: (id: string) => void;
  selectedId?: string | null;
}

export function RemindersWidgetRenderer(props: RemindersWidgetProps) {
  const {
    objects,
    title = "Reminders",
    filter = "upcoming",
    maxItems = 8,
    onToggleDone,
    onSelectObject,
    selectedId = null,
  } = props;

  const filtered = filterReminders(objects, filter)
    .sort((a, b) => (parseObjectDate(a.date) ?? 0) - (parseObjectDate(b.date) ?? 0))
    .slice(0, maxItems);

  return (
    <div data-testid="reminders-widget" style={cardBase}>
      <div style={headingRow}>
        <span>{title}</span>
        <span style={{ fontSize: 11, color: "#94a3b8", fontWeight: 400 }}>
          {filtered.length} shown
        </span>
      </div>
      {filtered.length === 0 ? (
        <div style={emptyState}>Nothing on the list.</div>
      ) : (
        filtered.map((r) => {
          const isDone = r.status === "done" || r.status === "archived";
          const isSelected = selectedId === r.id;
          const repeat = String((r.data as Record<string, unknown>)["repeat"] ?? "none");
          return (
            <div
              key={r.id}
              data-testid={`reminders-widget-row-${r.id}`}
              onClick={() => onSelectObject?.(r.id)}
              style={{
                display: "flex",
                alignItems: "center",
                gap: 10,
                padding: "8px 12px",
                borderBottom: "1px solid #1e293b",
                cursor: "pointer",
                background: isSelected ? "#1a3350" : "transparent",
              }}
            >
              <button
                type="button"
                aria-label={isDone ? "Mark pending" : "Mark done"}
                onClick={(e) => {
                  e.stopPropagation();
                  onToggleDone?.(r.id, isDone ? "todo" : "done");
                }}
                style={{
                  width: 16,
                  height: 16,
                  borderRadius: 999,
                  border: `1px solid ${isDone ? "#f59e0b" : "#475569"}`,
                  background: isDone ? "#f59e0b" : "transparent",
                  color: "#0f172a",
                  fontSize: 10,
                  lineHeight: "14px",
                  cursor: "pointer",
                  flexShrink: 0,
                }}
              >
                {isDone ? "\u2713" : ""}
              </button>
              <div
                style={{
                  flex: 1,
                  minWidth: 0,
                  fontSize: 13,
                  color: isDone ? "#64748b" : "#e2e8f0",
                  overflow: "hidden",
                  textOverflow: "ellipsis",
                  whiteSpace: "nowrap",
                }}
              >
                {r.name}
                {repeat !== "none" ? (
                  <span style={{ marginLeft: 6, fontSize: 10, color: "#94a3b8" }}>
                    {"\u21BB "}
                    {repeat}
                  </span>
                ) : null}
              </div>
              <DateChip iso={r.date} />
            </div>
          );
        })
      )}
    </div>
  );
}

// ─────────────────────────────────────────────────────────────────────────────
// ContactsWidgetRenderer
// ─────────────────────────────────────────────────────────────────────────────

export interface ContactsWidgetProps {
  objects: GraphObject[];
  title?: string;
  filter?: ContactFilter;
  display?: "cards" | "list";
  maxItems?: number;
  showOrg?: boolean;
  showActions?: boolean;
  onSelectObject?: (id: string) => void;
  selectedId?: string | null;
}

export function ContactsWidgetRenderer(props: ContactsWidgetProps) {
  const {
    objects,
    title = "Contacts",
    filter = "favorites",
    display = "cards",
    maxItems = 12,
    showOrg = true,
    showActions = true,
    onSelectObject,
    selectedId = null,
  } = props;

  const filtered = filterContacts(objects, filter).slice(0, maxItems);

  const renderCard = (c: GraphObject) => {
    const d = c.data as Record<string, unknown>;
    const email = typeof d["email"] === "string" ? (d["email"] as string) : "";
    const phone = typeof d["phone"] === "string" ? (d["phone"] as string) : "";
    const org = typeof d["org"] === "string" ? (d["org"] as string) : "";
    const role = typeof d["role"] === "string" ? (d["role"] as string) : "";
    const avatar = typeof d["avatarUrl"] === "string" ? (d["avatarUrl"] as string) : "";
    const isSelected = selectedId === c.id;
    return (
      <div
        key={c.id}
        data-testid={`contacts-widget-card-${c.id}`}
        onClick={() => onSelectObject?.(c.id)}
        style={{
          display: "flex",
          alignItems: "center",
          gap: 10,
          padding: "10px 12px",
          borderBottom: display === "list" ? "1px solid #1e293b" : "none",
          cursor: "pointer",
          background: isSelected ? "#1a3350" : "transparent",
        }}
      >
        <div
          style={{
            width: 32,
            height: 32,
            borderRadius: 999,
            background: avatar
              ? `url(${avatar}) center/cover`
              : "linear-gradient(135deg, #0ea5e9, #6366f1)",
            color: "#f8fafc",
            fontSize: 12,
            fontWeight: 600,
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            flexShrink: 0,
          }}
        >
          {avatar ? "" : contactInitials(c.name)}
        </div>
        <div style={{ flex: 1, minWidth: 0 }}>
          <div style={{ fontSize: 13, fontWeight: 500, color: "#e2e8f0" }}>{c.name}</div>
          {showOrg && (org || role) ? (
            <div style={{ fontSize: 11, color: "#94a3b8" }}>
              {role}
              {role && org ? " \u00B7 " : ""}
              {org}
            </div>
          ) : null}
        </div>
        {showActions ? (
          <div style={{ display: "flex", gap: 6 }}>
            {email ? (
              <a
                href={`mailto:${email}`}
                onClick={(e) => e.stopPropagation()}
                title={email}
                style={{
                  fontSize: 11,
                  padding: "2px 6px",
                  borderRadius: 3,
                  background: "#1e293b",
                  color: "#93c5fd",
                  textDecoration: "none",
                }}
              >
                Email
              </a>
            ) : null}
            {phone ? (
              <a
                href={`tel:${phone}`}
                onClick={(e) => e.stopPropagation()}
                title={phone}
                style={{
                  fontSize: 11,
                  padding: "2px 6px",
                  borderRadius: 3,
                  background: "#1e293b",
                  color: "#86efac",
                  textDecoration: "none",
                }}
              >
                Call
              </a>
            ) : null}
          </div>
        ) : null}
      </div>
    );
  };

  return (
    <div data-testid="contacts-widget" style={cardBase}>
      <div style={headingRow}>
        <span>{title}</span>
        <span style={{ fontSize: 11, color: "#94a3b8", fontWeight: 400 }}>
          {filtered.length} shown
        </span>
      </div>
      {filtered.length === 0 ? (
        <div style={emptyState}>No contacts match this filter.</div>
      ) : display === "cards" ? (
        <div
          style={{
            display: "grid",
            gridTemplateColumns: "repeat(auto-fill, minmax(220px, 1fr))",
            gap: 1,
            background: "#1e293b",
          }}
        >
          {filtered.map((c) => (
            <div key={c.id} style={{ background: "#0f172a" }}>
              {renderCard(c)}
            </div>
          ))}
        </div>
      ) : (
        filtered.map((c) => renderCard(c))
      )}
    </div>
  );
}

// ─────────────────────────────────────────────────────────────────────────────
// EventsWidgetRenderer
// ─────────────────────────────────────────────────────────────────────────────

export interface EventsWidgetProps {
  objects: GraphObject[];
  title?: string;
  range?: EventRange;
  maxItems?: number;
  showLocation?: boolean;
  onSelectObject?: (id: string) => void;
  selectedId?: string | null;
}

export function EventsWidgetRenderer(props: EventsWidgetProps) {
  const {
    objects,
    title = "Upcoming events",
    range = "week",
    maxItems = 8,
    showLocation = true,
    onSelectObject,
    selectedId = null,
  } = props;

  const filtered = filterEvents(objects, range).slice(0, maxItems);

  return (
    <div data-testid="events-widget" style={cardBase}>
      <div style={headingRow}>
        <span>{title}</span>
        <span style={{ fontSize: 11, color: "#94a3b8", fontWeight: 400 }}>
          {range === "today" ? "today" : range === "week" ? "next 7 days" : range === "month" ? "next 30 days" : "all"}
        </span>
      </div>
      {filtered.length === 0 ? (
        <div style={emptyState}>Nothing scheduled.</div>
      ) : (
        filtered.map((e) => {
          const d = e.data as Record<string, unknown>;
          const location = typeof d["location"] === "string" ? (d["location"] as string) : "";
          const isSelected = selectedId === e.id;
          return (
            <div
              key={e.id}
              data-testid={`events-widget-row-${e.id}`}
              onClick={() => onSelectObject?.(e.id)}
              style={{
                display: "flex",
                gap: 12,
                padding: "10px 12px",
                borderBottom: "1px solid #1e293b",
                cursor: "pointer",
                background: isSelected ? "#1a3350" : "transparent",
              }}
            >
              <div
                style={{
                  width: 48,
                  textAlign: "center",
                  flexShrink: 0,
                  color: "#93c5fd",
                  fontSize: 11,
                }}
              >
                <DateChip iso={e.date} />
                <div style={{ marginTop: 4, color: "#94a3b8" }}>
                  {formatEventTime(e.date)}
                </div>
              </div>
              <div style={{ flex: 1, minWidth: 0 }}>
                <div style={{ fontSize: 13, fontWeight: 500, color: "#e2e8f0" }}>{e.name}</div>
                {showLocation && location ? (
                  <div style={{ fontSize: 11, color: "#94a3b8" }}>
                    {"\u{1F4CD} "}
                    {location}
                  </div>
                ) : null}
              </div>
            </div>
          );
        })
      )}
    </div>
  );
}

// ─────────────────────────────────────────────────────────────────────────────
// NotesWidgetRenderer
// ─────────────────────────────────────────────────────────────────────────────

export interface NotesWidgetProps {
  objects: GraphObject[];
  title?: string;
  filter?: NoteFilter;
  tag?: string;
  maxItems?: number;
  previewLength?: number;
  onSelectObject?: (id: string) => void;
  selectedId?: string | null;
}

export function NotesWidgetRenderer(props: NotesWidgetProps) {
  const {
    objects,
    title = "Notes",
    filter = "pinned",
    tag = "",
    maxItems = 8,
    previewLength = 120,
    onSelectObject,
    selectedId = null,
  } = props;

  const filtered = filterNotes(objects, filter, tag).slice(0, maxItems);

  return (
    <div data-testid="notes-widget" style={cardBase}>
      <div style={headingRow}>
        <span>{title}</span>
        <span style={{ fontSize: 11, color: "#94a3b8", fontWeight: 400 }}>
          {filtered.length} shown
        </span>
      </div>
      {filtered.length === 0 ? (
        <div style={emptyState}>No notes yet.</div>
      ) : (
        filtered.map((n) => {
          const isSelected = selectedId === n.id;
          return (
            <div
              key={n.id}
              data-testid={`notes-widget-row-${n.id}`}
              onClick={() => onSelectObject?.(n.id)}
              style={{
                padding: "10px 12px",
                borderBottom: "1px solid #1e293b",
                cursor: "pointer",
                background: isSelected ? "#1a3350" : "transparent",
              }}
            >
              <div
                style={{
                  display: "flex",
                  alignItems: "center",
                  gap: 6,
                  fontSize: 13,
                  fontWeight: 500,
                  color: "#e2e8f0",
                }}
              >
                {n.pinned ? <span style={{ color: "#facc15" }}>{"\u2605"}</span> : null}
                {n.name}
              </div>
              <div
                style={{
                  marginTop: 4,
                  fontSize: 11,
                  color: "#94a3b8",
                  lineHeight: 1.4,
                }}
              >
                {notePreview(n, previewLength)}
              </div>
              {n.tags.length > 0 ? (
                <div style={{ marginTop: 6, display: "flex", gap: 4, flexWrap: "wrap" }}>
                  {n.tags.slice(0, 5).map((t) => (
                    <span
                      key={t}
                      style={{
                        fontSize: 10,
                        padding: "1px 5px",
                        borderRadius: 3,
                        background: "#1e293b",
                        color: "#94a3b8",
                      }}
                    >
                      #{t}
                    </span>
                  ))}
                </div>
              ) : null}
            </div>
          );
        })
      )}
    </div>
  );
}

// ─────────────────────────────────────────────────────────────────────────────
// GoalsWidgetRenderer
// ─────────────────────────────────────────────────────────────────────────────

export interface GoalsWidgetProps {
  objects: GraphObject[];
  title?: string;
  filter?: GoalFilter;
  maxItems?: number;
  onSelectObject?: (id: string) => void;
  selectedId?: string | null;
}

export function GoalsWidgetRenderer(props: GoalsWidgetProps) {
  const {
    objects,
    title = "Goals",
    filter = "active",
    maxItems = 6,
    onSelectObject,
    selectedId = null,
  } = props;

  const filtered = filterGoals(objects, filter).slice(0, maxItems);

  return (
    <div data-testid="goals-widget" style={cardBase}>
      <div style={headingRow}>
        <span>{title}</span>
        <span style={{ fontSize: 11, color: "#94a3b8", fontWeight: 400 }}>
          {filtered.length} shown
        </span>
      </div>
      {filtered.length === 0 ? (
        <div style={emptyState}>No goals set.</div>
      ) : (
        filtered.map((g) => {
          const ratio = goalRatio(g);
          const d = g.data as Record<string, unknown>;
          const unit = typeof d["unit"] === "string" ? (d["unit"] as string) : "";
          const current = Number(d["currentValue"] ?? 0);
          const target = Number(d["targetValue"] ?? 0);
          const isSelected = selectedId === g.id;
          return (
            <div
              key={g.id}
              data-testid={`goals-widget-row-${g.id}`}
              onClick={() => onSelectObject?.(g.id)}
              style={{
                padding: "10px 12px",
                borderBottom: "1px solid #1e293b",
                cursor: "pointer",
                background: isSelected ? "#1a3350" : "transparent",
              }}
            >
              <div
                style={{
                  display: "flex",
                  justifyContent: "space-between",
                  alignItems: "center",
                  fontSize: 13,
                  fontWeight: 500,
                  color: "#e2e8f0",
                }}
              >
                <span>{g.name}</span>
                <span style={{ fontSize: 11, color: "#94a3b8" }}>
                  {current}
                  {unit ? ` ${unit}` : ""} / {target}
                  {unit ? ` ${unit}` : ""}
                </span>
              </div>
              <div
                role="progressbar"
                aria-valuenow={Math.round(ratio * 100)}
                style={{
                  marginTop: 6,
                  height: 6,
                  borderRadius: 999,
                  background: "#1e293b",
                  overflow: "hidden",
                }}
              >
                <div
                  style={{
                    width: `${Math.round(ratio * 100)}%`,
                    height: "100%",
                    background: "linear-gradient(90deg, #ec4899, #f472b6)",
                  }}
                />
              </div>
            </div>
          );
        })
      )}
    </div>
  );
}

// ─────────────────────────────────────────────────────────────────────────────
// HabitTrackerWidgetRenderer
// ─────────────────────────────────────────────────────────────────────────────

export interface HabitTrackerWidgetProps {
  objects: GraphObject[];
  title?: string;
  maxItems?: number;
  showStreak?: boolean;
  onSelectObject?: (id: string) => void;
  selectedId?: string | null;
}

export function HabitTrackerWidgetRenderer(props: HabitTrackerWidgetProps) {
  const {
    objects,
    title = "Habits",
    maxItems = 8,
    showStreak = true,
    onSelectObject,
    selectedId = null,
  } = props;

  const habits = objects.slice(0, maxItems);

  return (
    <div data-testid="habit-tracker-widget" style={cardBase}>
      <div style={headingRow}>
        <span>{title}</span>
        <span style={{ fontSize: 11, color: "#94a3b8", fontWeight: 400 }}>
          {habits.length} habits
        </span>
      </div>
      {habits.length === 0 ? (
        <div style={emptyState}>No habits tracked.</div>
      ) : (
        habits.map((h) => {
          const ratio = habitWeeklyRatio(h);
          const d = h.data as Record<string, unknown>;
          const streak = Number(d["streak"] ?? 0);
          const target = Number(d["targetPerWeek"] ?? 7);
          const isSelected = selectedId === h.id;
          return (
            <div
              key={h.id}
              data-testid={`habit-tracker-row-${h.id}`}
              onClick={() => onSelectObject?.(h.id)}
              style={{
                display: "flex",
                alignItems: "center",
                gap: 10,
                padding: "10px 12px",
                borderBottom: "1px solid #1e293b",
                cursor: "pointer",
                background: isSelected ? "#1a3350" : "transparent",
              }}
            >
              <div style={{ flex: 1, minWidth: 0 }}>
                <div style={{ fontSize: 13, fontWeight: 500, color: "#e2e8f0" }}>{h.name}</div>
                <div
                  style={{
                    marginTop: 4,
                    height: 4,
                    borderRadius: 999,
                    background: "#1e293b",
                    overflow: "hidden",
                  }}
                >
                  <div
                    style={{
                      width: `${Math.round(ratio * 100)}%`,
                      height: "100%",
                      background: "#14b8a6",
                    }}
                  />
                </div>
              </div>
              {showStreak ? (
                <div
                  style={{
                    fontSize: 11,
                    color: streak > 0 ? "#fb923c" : "#64748b",
                    fontWeight: 600,
                    flexShrink: 0,
                  }}
                  title={`Target ${target}/week`}
                >
                  {"\u{1F525} "}
                  {streak}
                </div>
              ) : null}
            </div>
          );
        })
      )}
    </div>
  );
}

// ─────────────────────────────────────────────────────────────────────────────
// BookmarksWidgetRenderer
// ─────────────────────────────────────────────────────────────────────────────

export interface BookmarksWidgetProps {
  objects: GraphObject[];
  title?: string;
  folder?: string;
  maxItems?: number;
  display?: "grid" | "list";
  onSelectObject?: (id: string) => void;
  selectedId?: string | null;
}

export function BookmarksWidgetRenderer(props: BookmarksWidgetProps) {
  const {
    objects,
    title = "Bookmarks",
    folder = "",
    maxItems = 12,
    display = "grid",
    onSelectObject,
    selectedId = null,
  } = props;

  const filtered = filterBookmarks(objects, folder).slice(0, maxItems);

  return (
    <div data-testid="bookmarks-widget" style={cardBase}>
      <div style={headingRow}>
        <span>{title}</span>
        <span style={{ fontSize: 11, color: "#94a3b8", fontWeight: 400 }}>
          {filtered.length} shown
        </span>
      </div>
      {filtered.length === 0 ? (
        <div style={emptyState}>No bookmarks.</div>
      ) : display === "grid" ? (
        <div
          style={{
            display: "grid",
            gridTemplateColumns: "repeat(auto-fill, minmax(140px, 1fr))",
            gap: 8,
            padding: 10,
          }}
        >
          {filtered.map((b) => {
            const d = b.data as Record<string, unknown>;
            const url = typeof d["url"] === "string" ? (d["url"] as string) : "";
            const favicon = typeof d["faviconUrl"] === "string" ? (d["faviconUrl"] as string) : "";
            return (
              <a
                key={b.id}
                data-testid={`bookmark-card-${b.id}`}
                href={url || "#"}
                target="_blank"
                rel="noopener noreferrer"
                onClick={(e) => {
                  if (!url) e.preventDefault();
                  onSelectObject?.(b.id);
                }}
                style={{
                  display: "flex",
                  flexDirection: "column",
                  alignItems: "center",
                  gap: 4,
                  padding: 10,
                  borderRadius: 6,
                  background: "#1e293b",
                  textDecoration: "none",
                  color: "#e2e8f0",
                }}
              >
                {favicon ? (
                  <img
                    src={favicon}
                    alt=""
                    width={24}
                    height={24}
                    style={{ borderRadius: 4 }}
                  />
                ) : (
                  <div
                    style={{
                      width: 24,
                      height: 24,
                      borderRadius: 4,
                      background: "#334155",
                      display: "flex",
                      alignItems: "center",
                      justifyContent: "center",
                      fontSize: 12,
                      color: "#94a3b8",
                    }}
                  >
                    {"\u{1F517}"}
                  </div>
                )}
                <div
                  style={{
                    fontSize: 11,
                    textAlign: "center",
                    overflow: "hidden",
                    textOverflow: "ellipsis",
                    whiteSpace: "nowrap",
                    width: "100%",
                  }}
                >
                  {b.name}
                </div>
                <div style={{ fontSize: 9, color: "#64748b" }}>{bookmarkHost(url)}</div>
              </a>
            );
          })}
        </div>
      ) : (
        filtered.map((b) => {
          const d = b.data as Record<string, unknown>;
          const url = typeof d["url"] === "string" ? (d["url"] as string) : "";
          const isSelected = selectedId === b.id;
          return (
            <a
              key={b.id}
              href={url || "#"}
              target="_blank"
              rel="noopener noreferrer"
              onClick={(e) => {
                if (!url) e.preventDefault();
                onSelectObject?.(b.id);
              }}
              style={{
                display: "flex",
                justifyContent: "space-between",
                alignItems: "center",
                padding: "8px 12px",
                borderBottom: "1px solid #1e293b",
                textDecoration: "none",
                color: "#e2e8f0",
                background: isSelected ? "#1a3350" : "transparent",
              }}
            >
              <span style={{ fontSize: 13 }}>{b.name}</span>
              <span style={{ fontSize: 11, color: "#64748b" }}>{bookmarkHost(url)}</span>
            </a>
          );
        })
      )}
    </div>
  );
}

// ─────────────────────────────────────────────────────────────────────────────
// TimerWidgetRenderer — local interactive focus timer
// ─────────────────────────────────────────────────────────────────────────────

export interface TimerWidgetProps {
  objects: GraphObject[];
  title?: string;
  defaultMinutes?: number;
  maxRecent?: number;
  onCreateSession?: (durationMs: number) => void;
  onSelectObject?: (id: string) => void;
  selectedId?: string | null;
}

export function TimerWidgetRenderer(props: TimerWidgetProps) {
  const {
    objects,
    title = "Focus timer",
    defaultMinutes = 25,
    maxRecent = 5,
    onCreateSession,
    onSelectObject,
    selectedId = null,
  } = props;

  const [minutes, setMinutes] = useState(defaultMinutes);
  const recent = [...objects]
    .sort((a, b) => (parseObjectDate(b.createdAt) ?? 0) - (parseObjectDate(a.createdAt) ?? 0))
    .slice(0, maxRecent);

  return (
    <div data-testid="timer-widget" style={cardBase}>
      <div style={headingRow}>
        <span>{title}</span>
      </div>
      <div
        style={{
          padding: 14,
          display: "flex",
          flexDirection: "column",
          alignItems: "center",
          gap: 10,
        }}
      >
        <div style={{ fontSize: 32, fontWeight: 600, color: "#f8fafc", fontVariantNumeric: "tabular-nums" }}>
          {String(minutes).padStart(2, "0")}:00
        </div>
        <input
          type="range"
          min={5}
          max={90}
          step={5}
          value={minutes}
          onChange={(e) => setMinutes(Number(e.target.value))}
          style={{ width: "100%" }}
        />
        <button
          type="button"
          onClick={() => onCreateSession?.(minutes * 60_000)}
          style={{
            padding: "6px 14px",
            borderRadius: 6,
            border: "1px solid #ef4444",
            background: "#7f1d1d",
            color: "#fecaca",
            fontSize: 12,
            cursor: "pointer",
          }}
        >
          Log {minutes}m session
        </button>
      </div>
      {recent.length > 0 ? (
        <div style={{ borderTop: "1px solid #1e293b" }}>
          {recent.map((s) => {
            const duration = Number((s.data as Record<string, unknown>)["durationMs"] ?? 0);
            const isSelected = selectedId === s.id;
            return (
              <div
                key={s.id}
                data-testid={`timer-session-row-${s.id}`}
                onClick={() => onSelectObject?.(s.id)}
                style={{
                  display: "flex",
                  justifyContent: "space-between",
                  padding: "6px 12px",
                  borderBottom: "1px solid #1e293b",
                  fontSize: 11,
                  cursor: "pointer",
                  color: "#94a3b8",
                  background: isSelected ? "#1a3350" : "transparent",
                }}
              >
                <span>{s.name}</span>
                <span>{formatDuration(duration)}</span>
              </div>
            );
          })}
        </div>
      ) : null}
    </div>
  );
}

// ─────────────────────────────────────────────────────────────────────────────
// CaptureInboxWidgetRenderer — quick capture inbox
// ─────────────────────────────────────────────────────────────────────────────

export interface CaptureInboxWidgetProps {
  objects: GraphObject[];
  title?: string;
  maxItems?: number;
  showProcessed?: boolean;
  onCaptureSubmit?: (text: string) => void;
  onMarkProcessed?: (id: string) => void;
  onSelectObject?: (id: string) => void;
  selectedId?: string | null;
}

export function CaptureInboxWidgetRenderer(props: CaptureInboxWidgetProps) {
  const {
    objects,
    title = "Inbox",
    maxItems = 10,
    showProcessed = false,
    onCaptureSubmit,
    onMarkProcessed,
    onSelectObject,
    selectedId = null,
  } = props;

  const [draft, setDraft] = useState("");
  const filtered = filterCaptures(objects, showProcessed)
    .sort((a, b) => (parseObjectDate(b.createdAt) ?? 0) - (parseObjectDate(a.createdAt) ?? 0))
    .slice(0, maxItems);

  const submit = () => {
    const text = draft.trim();
    if (!text) return;
    onCaptureSubmit?.(text);
    setDraft("");
  };

  return (
    <div data-testid="capture-inbox-widget" style={cardBase}>
      <div style={headingRow}>
        <span>{title}</span>
        <span style={{ fontSize: 11, color: "#94a3b8", fontWeight: 400 }}>
          {filtered.length} pending
        </span>
      </div>
      <div
        style={{
          display: "flex",
          gap: 6,
          padding: 10,
          borderBottom: "1px solid #1e293b",
        }}
      >
        <input
          type="text"
          value={draft}
          onChange={(e) => setDraft(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") {
              e.preventDefault();
              submit();
            }
          }}
          placeholder={"Capture a thought\u2026"}
          style={{
            flex: 1,
            padding: "6px 10px",
            borderRadius: 4,
            border: "1px solid #334155",
            background: "#1e293b",
            color: "#e2e8f0",
            fontSize: 12,
          }}
        />
        <button
          type="button"
          onClick={submit}
          disabled={!draft.trim()}
          style={{
            padding: "6px 12px",
            borderRadius: 4,
            border: "1px solid #475569",
            background: draft.trim() ? "#6366f1" : "#1e293b",
            color: draft.trim() ? "#f8fafc" : "#64748b",
            fontSize: 12,
            cursor: draft.trim() ? "pointer" : "not-allowed",
          }}
        >
          Add
        </button>
      </div>
      {filtered.length === 0 ? (
        <div style={emptyState}>{"Inbox zero \u{1F389}"}</div>
      ) : (
        filtered.map((c) => {
          const isSelected = selectedId === c.id;
          const processed = !!(c.data as Record<string, unknown>)["processedAt"];
          return (
            <div
              key={c.id}
              data-testid={`capture-inbox-row-${c.id}`}
              onClick={() => onSelectObject?.(c.id)}
              style={{
                display: "flex",
                alignItems: "center",
                gap: 10,
                padding: "8px 12px",
                borderBottom: "1px solid #1e293b",
                cursor: "pointer",
                background: isSelected ? "#1a3350" : "transparent",
                opacity: processed ? 0.5 : 1,
              }}
            >
              <div
                style={{
                  flex: 1,
                  minWidth: 0,
                  fontSize: 12,
                  color: "#e2e8f0",
                  overflow: "hidden",
                  textOverflow: "ellipsis",
                  whiteSpace: "nowrap",
                }}
              >
                {c.name}
              </div>
              {!processed ? (
                <button
                  type="button"
                  onClick={(e) => {
                    e.stopPropagation();
                    onMarkProcessed?.(c.id);
                  }}
                  style={{
                    fontSize: 10,
                    padding: "2px 6px",
                    borderRadius: 3,
                    border: "1px solid #475569",
                    background: "#1e293b",
                    color: "#94a3b8",
                    cursor: "pointer",
                  }}
                >
                  Done
                </button>
              ) : (
                <span style={{ fontSize: 10, color: "#22c55e" }}>{"\u2713"}</span>
              )}
            </div>
          );
        })
      )}
    </div>
  );
}
