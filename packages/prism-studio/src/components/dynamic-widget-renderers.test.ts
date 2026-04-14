/**
 * Pure-helper tests for dynamic-widget-renderers.
 *
 * These cover the filter / order / format functions that power the 10 new
 * dynamic widgets (tasks, reminders, contacts, events, notes, goals, habits,
 * bookmarks, timer sessions, captures). React rendering is out of scope here
 * — the helpers are the high-leverage correctness boundary.
 */

import { describe, it, expect } from "vitest";
import type { GraphObject, ObjectId } from "@prism/core/object-model";
import {
  parseObjectDate,
  formatRelativeDate,
  filterTasks,
  orderTasks,
  priorityColor,
  filterReminders,
  filterContacts,
  contactInitials,
  filterEvents,
  formatEventTime,
  filterNotes,
  notePreview,
  filterGoals,
  goalRatio,
  formatDuration,
  filterBookmarks,
  bookmarkHost,
  habitWeeklyRatio,
  filterCaptures,
} from "./dynamic-widget-renderers.js";

// Fixed "now" used by the relative-date tests. 2026-04-13T12:00 local.
const NOW = new Date(2026, 3, 13, 12, 0, 0).getTime();

function isoAt(year: number, month: number, day: number, hour = 12): string {
  return new Date(year, month - 1, day, hour).toISOString();
}

function obj(overrides: Partial<GraphObject> & { type: string; name?: string }): GraphObject {
  const base = {
    id: `id-${Math.random().toString(36).slice(2, 9)}` as ObjectId,
    name: "Untitled",
    parentId: null,
    position: 0,
    status: null,
    tags: [],
    date: null,
    endDate: null,
    description: "",
    color: null,
    image: null,
    pinned: false,
    data: {},
    createdAt: "2026-04-01T00:00:00Z",
    updatedAt: "2026-04-01T00:00:00Z",
  };
  return { ...base, ...overrides } as GraphObject;
}

// ─────────────────────────────────────────────────────────────────────────────
// parseObjectDate / formatRelativeDate
// ─────────────────────────────────────────────────────────────────────────────

describe("parseObjectDate", () => {
  it("returns null for null / undefined / empty", () => {
    expect(parseObjectDate(null)).toBeNull();
    expect(parseObjectDate(undefined)).toBeNull();
    expect(parseObjectDate("")).toBeNull();
  });

  it("returns null for malformed input", () => {
    expect(parseObjectDate("not a date")).toBeNull();
  });

  it("parses valid ISO strings", () => {
    const ms = parseObjectDate("2026-04-13T12:00:00Z");
    expect(typeof ms).toBe("number");
    expect(ms).toBeGreaterThan(0);
  });
});

describe("formatRelativeDate", () => {
  it("returns empty label for missing date", () => {
    expect(formatRelativeDate(null, NOW).label).toBe("");
    expect(formatRelativeDate(null, NOW).tone).toBe("none");
  });

  it("labels overdue items with day count", () => {
    const r = formatRelativeDate(isoAt(2026, 4, 10), NOW);
    expect(r.tone).toBe("overdue");
    expect(r.label).toMatch(/Overdue \d+d/);
  });

  it("labels today/tomorrow in plain words", () => {
    expect(formatRelativeDate(isoAt(2026, 4, 13), NOW).label).toBe("Today");
    expect(formatRelativeDate(isoAt(2026, 4, 14), NOW).label).toBe("Tomorrow");
  });

  it("labels 2–7 days ahead as In Nd", () => {
    expect(formatRelativeDate(isoAt(2026, 4, 15), NOW).label).toBe("In 2d");
    expect(formatRelativeDate(isoAt(2026, 4, 20), NOW).label).toBe("In 7d");
  });

  it("falls back to calendar date beyond a week", () => {
    const r = formatRelativeDate(isoAt(2026, 5, 10), NOW);
    expect(r.tone).toBe("later");
    expect(r.label).not.toMatch(/In \d+d/);
  });
});

// ─────────────────────────────────────────────────────────────────────────────
// filterTasks / orderTasks
// ─────────────────────────────────────────────────────────────────────────────

describe("filterTasks", () => {
  const mkTask = (name: string, status: string | null, date: string | null, project?: string) =>
    obj({ type: "task", name, status, date, data: project ? { project } : {} });

  const tasks: GraphObject[] = [
    mkTask("A done", "done", isoAt(2026, 4, 10)),
    mkTask("B todo overdue", "todo", isoAt(2026, 4, 10)),
    mkTask("C todo today", "todo", isoAt(2026, 4, 13)),
    mkTask("D doing future", "doing", isoAt(2026, 4, 20)),
    mkTask("E todo no date", "todo", null, "Backlog"),
  ];

  it("returns everything on filter=all", () => {
    expect(filterTasks(tasks, "all", "", NOW)).toHaveLength(5);
  });

  it("excludes done/archived on filter=open", () => {
    const open = filterTasks(tasks, "open", "", NOW);
    expect(open.find((t) => t.status === "done")).toBeUndefined();
    expect(open).toHaveLength(4);
  });

  it("only includes done on filter=done", () => {
    const done = filterTasks(tasks, "done", "", NOW);
    expect(done).toHaveLength(1);
    expect(done[0]?.name).toBe("A done");
  });

  it("filter=today returns tasks due today", () => {
    const today = filterTasks(tasks, "today", "", NOW);
    expect(today).toHaveLength(1);
    expect(today[0]?.name).toBe("C todo today");
  });

  it("filter=overdue excludes today + future + done", () => {
    const overdue = filterTasks(tasks, "overdue", "", NOW);
    expect(overdue).toHaveLength(1);
    expect(overdue[0]?.name).toBe("B todo overdue");
  });

  it("applies project filter on top of status filter", () => {
    const backlog = filterTasks(tasks, "all", "Backlog", NOW);
    expect(backlog).toHaveLength(1);
    expect(backlog[0]?.name).toBe("E todo no date");
  });
});

describe("orderTasks", () => {
  it("sorts by date ascending with nulls last", () => {
    const a = obj({ type: "task", name: "A", date: isoAt(2026, 4, 15) });
    const b = obj({ type: "task", name: "B", date: isoAt(2026, 4, 10) });
    const c = obj({ type: "task", name: "C", date: null });
    const sorted = orderTasks([a, c, b]);
    expect(sorted.map((t) => t.name)).toEqual(["B", "A", "C"]);
  });

  it("breaks ties on priority with urgent first", () => {
    const a = obj({ type: "task", name: "normal", date: null, data: { priority: "normal" } });
    const b = obj({ type: "task", name: "urgent", date: null, data: { priority: "urgent" } });
    const sorted = orderTasks([a, b]);
    expect(sorted[0]?.name).toBe("urgent");
  });
});

describe("priorityColor", () => {
  it("returns distinct colors per priority", () => {
    expect(priorityColor("urgent")).toBe("#dc2626");
    expect(priorityColor("high")).toBe("#f59e0b");
    expect(priorityColor("low")).toBe("#64748b");
    expect(priorityColor("normal")).toBe("#6366f1");
    expect(priorityColor(undefined)).toBe("#6366f1");
  });
});

// ─────────────────────────────────────────────────────────────────────────────
// filterReminders
// ─────────────────────────────────────────────────────────────────────────────

describe("filterReminders", () => {
  const mk = (name: string, status: string | null, date: string | null) =>
    obj({ type: "reminder", name, status, date });

  const rems: GraphObject[] = [
    mk("A done", "done", isoAt(2026, 4, 12)),
    mk("B open overdue", "todo", isoAt(2026, 4, 10)),
    mk("C open today", "todo", isoAt(2026, 4, 13)),
    mk("D open future", "todo", isoAt(2026, 4, 20)),
  ];

  it("returns open items (not done) on upcoming", () => {
    const r = filterReminders(rems, "upcoming", NOW);
    expect(r).toHaveLength(3);
  });

  it("returns only today open", () => {
    const r = filterReminders(rems, "today", NOW);
    expect(r.map((x) => x.name)).toEqual(["C open today"]);
  });

  it("returns only overdue open", () => {
    const r = filterReminders(rems, "overdue", NOW);
    expect(r.map((x) => x.name)).toEqual(["B open overdue"]);
  });
});

// ─────────────────────────────────────────────────────────────────────────────
// filterContacts / contactInitials
// ─────────────────────────────────────────────────────────────────────────────

describe("filterContacts", () => {
  const a = obj({ type: "contact", name: "Alex", pinned: true, data: { lastContactedAt: isoAt(2026, 4, 10) } });
  const b = obj({ type: "contact", name: "Bea", data: { lastContactedAt: isoAt(2026, 4, 12) } });
  const c = obj({ type: "contact", name: "Cid", data: {} });

  it("returns all on filter=all", () => {
    expect(filterContacts([a, b, c], "all")).toHaveLength(3);
  });

  it("returns only pinned on filter=favorites", () => {
    const fav = filterContacts([a, b, c], "favorites");
    expect(fav).toHaveLength(1);
    expect(fav[0]?.name).toBe("Alex");
  });

  it("sorts by lastContactedAt desc on filter=recent, nulls last", () => {
    const recent = filterContacts([c, a, b], "recent");
    expect(recent.map((x) => x.name)).toEqual(["Bea", "Alex", "Cid"]);
  });
});

describe("contactInitials", () => {
  it("returns first letter for single-word names", () => {
    expect(contactInitials("Alex")).toBe("A");
  });

  it("returns first + last initial for multi-word names", () => {
    expect(contactInitials("Alex Chen")).toBe("AC");
    expect(contactInitials("Mary Jane Watson")).toBe("MW");
  });

  it("returns ? on empty", () => {
    expect(contactInitials("")).toBe("?");
  });
});

// ─────────────────────────────────────────────────────────────────────────────
// filterEvents / formatEventTime
// ─────────────────────────────────────────────────────────────────────────────

describe("filterEvents", () => {
  const mk = (month: number, day: number, name: string) =>
    obj({ type: "event", name, date: isoAt(2026, month, day) });

  const events: GraphObject[] = [
    mk(4, 10, "Past"),
    mk(4, 13, "Today"),
    mk(4, 16, "In3"),
    mk(4, 23, "In10"),
    mk(5, 23, "In40"),
  ];

  it("today range excludes past and future", () => {
    const r = filterEvents(events, "today", NOW);
    expect(r.map((e) => e.name)).toEqual(["Today"]);
  });

  it("week range includes 7 days ahead inclusive start", () => {
    const r = filterEvents(events, "week", NOW);
    expect(r.map((e) => e.name)).toContain("Today");
    expect(r.map((e) => e.name)).toContain("In3");
  });

  it("month range catches items within 30 days", () => {
    const r = filterEvents(events, "month", NOW);
    expect(r.map((e) => e.name)).toContain("In10");
  });

  it("all range returns everything from today forward sorted by date", () => {
    const r = filterEvents(events, "all", NOW);
    const dates = r.map((e) => parseObjectDate(e.date) ?? 0);
    for (let i = 1; i < dates.length; i++) {
      expect(dates[i]).toBeGreaterThanOrEqual(dates[i - 1] ?? 0);
    }
  });
});

describe("formatEventTime", () => {
  it("returns empty for missing/invalid", () => {
    expect(formatEventTime(null)).toBe("");
    expect(formatEventTime("not a date")).toBe("");
  });

  it("returns a hh:mm string for a valid date", () => {
    expect(formatEventTime(isoAt(2026, 4, 13, 15))).toMatch(/\d{1,2}:\d{2}/);
  });
});

// ─────────────────────────────────────────────────────────────────────────────
// filterNotes / notePreview
// ─────────────────────────────────────────────────────────────────────────────

describe("filterNotes", () => {
  const a = obj({
    type: "note",
    name: "pinned-ideas",
    pinned: true,
    tags: ["ideas"],
    updatedAt: "2026-04-10T00:00:00Z",
  });
  const b = obj({
    type: "note",
    name: "recent-planning",
    tags: ["planning"],
    updatedAt: "2026-04-12T00:00:00Z",
  });
  const c = obj({
    type: "note",
    name: "mixed",
    tags: ["ideas", "planning"],
    updatedAt: "2026-04-11T00:00:00Z",
  });

  it("all + no tag returns everything", () => {
    expect(filterNotes([a, b, c], "all", "")).toHaveLength(3);
  });

  it("pinned filter drops unpinned", () => {
    expect(filterNotes([a, b, c], "pinned", "")).toEqual([a]);
  });

  it("recent sorts by updatedAt desc", () => {
    const r = filterNotes([a, b, c], "recent", "");
    expect(r.map((n) => n.name)).toEqual(["recent-planning", "mixed", "pinned-ideas"]);
  });

  it("tag filter narrows by tag match", () => {
    expect(filterNotes([a, b, c], "all", "planning").map((n) => n.name)).toEqual([
      "recent-planning",
      "mixed",
    ]);
  });

  it("tag filter is case-insensitive", () => {
    expect(filterNotes([a, b, c], "all", "IDEAS").map((n) => n.name)).toEqual([
      "pinned-ideas",
      "mixed",
    ]);
  });
});

describe("notePreview", () => {
  it("reads body from data first", () => {
    const n = obj({ type: "note", data: { body: "hello world" }, description: "fallback" });
    expect(notePreview(n, 20)).toBe("hello world");
  });

  it("falls back to description if no body", () => {
    const n = obj({ type: "note", description: "fallback" });
    expect(notePreview(n, 20)).toBe("fallback");
  });

  it("collapses whitespace runs", () => {
    const n = obj({ type: "note", data: { body: "a   b\n\nc" } });
    expect(notePreview(n, 20)).toBe("a b c");
  });

  it("truncates with ellipsis beyond the limit", () => {
    const body = "abcdefghij";
    const n = obj({ type: "note", data: { body } });
    expect(notePreview(n, 5)).toBe("abcd\u2026");
  });
});

// ─────────────────────────────────────────────────────────────────────────────
// filterGoals / goalRatio
// ─────────────────────────────────────────────────────────────────────────────

describe("filterGoals", () => {
  const a = obj({ type: "goal", name: "A", status: "doing" });
  const b = obj({ type: "goal", name: "B", status: "done" });
  const c = obj({ type: "goal", name: "C", status: "todo" });

  it("active excludes done + archived", () => {
    expect(filterGoals([a, b, c], "active").map((g) => g.name)).toEqual(["A", "C"]);
  });

  it("completed returns only done", () => {
    expect(filterGoals([a, b, c], "completed").map((g) => g.name)).toEqual(["B"]);
  });
});

describe("goalRatio", () => {
  it("clamps to [0,1]", () => {
    expect(goalRatio(obj({ type: "goal", data: { currentValue: 200, targetValue: 100 } }))).toBe(1);
    expect(goalRatio(obj({ type: "goal", data: { currentValue: -50, targetValue: 100 } }))).toBe(0);
  });

  it("returns 0 if target is missing or zero", () => {
    expect(goalRatio(obj({ type: "goal", data: {} }))).toBe(0);
    expect(goalRatio(obj({ type: "goal", data: { currentValue: 10, targetValue: 0 } }))).toBe(0);
  });

  it("computes 0.5 for half-complete goal", () => {
    expect(goalRatio(obj({ type: "goal", data: { currentValue: 50, targetValue: 100 } }))).toBe(0.5);
  });
});

// ─────────────────────────────────────────────────────────────────────────────
// formatDuration
// ─────────────────────────────────────────────────────────────────────────────

describe("formatDuration", () => {
  it("returns 0m for non-positive", () => {
    expect(formatDuration(0)).toBe("0m");
    expect(formatDuration(-10)).toBe("0m");
  });

  it("formats seconds for sub-minute", () => {
    expect(formatDuration(30_000)).toBe("30s");
  });

  it("formats minutes without seconds beyond 10m", () => {
    expect(formatDuration(20 * 60_000)).toBe("20m");
  });

  it("formats Hm for sessions over 1h", () => {
    expect(formatDuration(75 * 60_000)).toBe("1h 15m");
  });
});

// ─────────────────────────────────────────────────────────────────────────────
// filterBookmarks / bookmarkHost
// ─────────────────────────────────────────────────────────────────────────────

describe("filterBookmarks", () => {
  const a = obj({ type: "bookmark", name: "A", data: { folder: "work" } });
  const b = obj({ type: "bookmark", name: "B", data: { folder: "fun" } });
  const c = obj({ type: "bookmark", name: "C", data: {} });

  it("blank folder returns all", () => {
    expect(filterBookmarks([a, b, c], "")).toHaveLength(3);
  });

  it("exact folder match filters the list", () => {
    expect(filterBookmarks([a, b, c], "work").map((x) => x.name)).toEqual(["A"]);
  });
});

describe("bookmarkHost", () => {
  it("strips protocol and leading www.", () => {
    expect(bookmarkHost("https://www.example.com/path")).toBe("example.com");
  });

  it("returns original string on parse failure", () => {
    expect(bookmarkHost("not a url")).toBe("not a url");
  });
});

// ─────────────────────────────────────────────────────────────────────────────
// habitWeeklyRatio
// ─────────────────────────────────────────────────────────────────────────────

describe("habitWeeklyRatio", () => {
  it("computes streak/target within 0..1", () => {
    expect(habitWeeklyRatio(obj({ type: "habit", data: { streak: 3, targetPerWeek: 7 } }))).toBeCloseTo(3 / 7);
  });

  it("clamps ratio at 1 if streak exceeds target", () => {
    expect(habitWeeklyRatio(obj({ type: "habit", data: { streak: 20, targetPerWeek: 7 } }))).toBe(1);
  });

  it("returns 0 for zero / missing target", () => {
    expect(habitWeeklyRatio(obj({ type: "habit", data: { streak: 3, targetPerWeek: 0 } }))).toBe(0);
  });
});

// ─────────────────────────────────────────────────────────────────────────────
// filterCaptures
// ─────────────────────────────────────────────────────────────────────────────

describe("filterCaptures", () => {
  const pending = obj({ type: "capture", name: "P", data: {} });
  const processed = obj({
    type: "capture",
    name: "X",
    data: { processedAt: "2026-04-12T00:00:00Z" },
  });

  it("hides processed by default", () => {
    expect(filterCaptures([pending, processed], false).map((c) => c.name)).toEqual(["P"]);
  });

  it("shows processed when toggled on", () => {
    expect(filterCaptures([pending, processed], true)).toHaveLength(2);
  });
});
