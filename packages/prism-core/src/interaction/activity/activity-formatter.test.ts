import { describe, it, expect } from "vitest";
import type { ActivityEvent } from "./activity-log.js";
import {
  formatActivity,
  formatFieldName,
  formatFieldValue,
  groupActivityByDate,
} from "./activity-formatter.js";

// ── Helpers ───────────────────────────────────────────────────────────────────

function makeEvent(overrides: Partial<ActivityEvent> = {}): ActivityEvent {
  return {
    id: "evt-1",
    objectId: "obj-1",
    verb: "updated",
    createdAt: new Date().toISOString(),
    ...overrides,
  };
}

// ── formatFieldName ──────────────────────────────────────────────────────────

describe("formatFieldName", () => {
  it("strips data. prefix", () => {
    expect(formatFieldName("data.priority")).toBe("priority");
  });

  it("uses override for parentId", () => {
    expect(formatFieldName("parentId")).toBe("parent");
  });

  it("uses override for endDate", () => {
    expect(formatFieldName("endDate")).toBe("end date");
  });

  it("converts camelCase to words", () => {
    expect(formatFieldName("myField")).toBe("my field");
    expect(formatFieldName("backgroundColor")).toBe("background color");
  });

  it("converts snake_case to words", () => {
    expect(formatFieldName("some_field")).toBe("some field");
  });

  it("handles data. + camelCase", () => {
    expect(formatFieldName("data.dueDate")).toBe("due date");
  });
});

// ── formatFieldValue ─────────────────────────────────────────────────────────

describe("formatFieldValue", () => {
  it("returns (none) for null/undefined", () => {
    expect(formatFieldValue(null)).toBe("(none)");
    expect(formatFieldValue(undefined)).toBe("(none)");
  });

  it("returns yes/no for booleans", () => {
    expect(formatFieldValue(true)).toBe("yes");
    expect(formatFieldValue(false)).toBe("no");
  });

  it("formats numbers with locale", () => {
    expect(formatFieldValue(1234)).toBeTruthy();
  });

  it("truncates long strings", () => {
    const long = "a".repeat(100);
    const result = formatFieldValue(long);
    expect(result.length).toBeLessThan(100);
    expect(result).toContain("\u2026");
  });

  it("returns (empty) for empty strings", () => {
    expect(formatFieldValue("")).toBe("(empty)");
  });

  it("formats arrays with max 3 items", () => {
    expect(formatFieldValue(["a", "b"])).toBe("a, b");
    expect(formatFieldValue(["a", "b", "c", "d", "e"])).toBe(
      "a, b, c and 2 more",
    );
  });

  it("returns (empty) for empty arrays", () => {
    expect(formatFieldValue([])).toBe("(empty)");
  });

  it("formats ISO dates", () => {
    const result = formatFieldValue("2026-03-15");
    expect(result).toContain("Mar");
    expect(result).toContain("15");
    expect(result).toContain("2026");
  });

  it("formats ISO datetimes", () => {
    const result = formatFieldValue("2026-03-15T14:34:00.000Z");
    expect(result).toContain("Mar");
    expect(result).toContain("15");
  });

  it("formats plain objects as JSON", () => {
    expect(formatFieldValue({ x: 1 })).toBe('{"x":1}');
  });
});

// ── formatActivity ───────────────────────────────────────────────────────────

describe("formatActivity", () => {
  it("formats created", () => {
    const result = formatActivity(
      makeEvent({ verb: "created", actorName: "Alice" }),
    );
    expect(result.text).toBe("Alice created this");
    expect(result.html).toContain("<b>Alice</b>");
  });

  it("formats created with object name", () => {
    const result = formatActivity(
      makeEvent({ verb: "created", actorName: "Alice" }),
      { objectName: "My Task" },
    );
    expect(result.text).toContain('"My Task"');
  });

  it("formats deleted", () => {
    const result = formatActivity(
      makeEvent({ verb: "deleted", actorName: "Bob" }),
    );
    expect(result.text).toBe("Bob deleted this");
  });

  it("formats restored", () => {
    const result = formatActivity(
      makeEvent({ verb: "restored", actorName: "Bob" }),
    );
    expect(result.text).toBe("Bob restored this");
  });

  it("formats renamed with changes", () => {
    const result = formatActivity(
      makeEvent({
        verb: "renamed",
        actorName: "Alice",
        changes: [{ field: "name", before: "Old", after: "New" }],
      }),
    );
    expect(result.text).toContain("renamed from");
    expect(result.text).toContain("Old");
    expect(result.text).toContain("New");
  });

  it("formats renamed without changes", () => {
    const result = formatActivity(
      makeEvent({ verb: "renamed", actorName: "Alice" }),
    );
    expect(result.text).toBe("Alice renamed this");
  });

  it("formats status-changed", () => {
    const result = formatActivity(
      makeEvent({
        verb: "status-changed",
        actorName: "Alice",
        fromStatus: "todo",
        toStatus: "done",
      }),
    );
    expect(result.text).toContain("changed status");
    expect(result.text).toContain("todo");
    expect(result.text).toContain("done");
  });

  it("formats moved into container", () => {
    const result = formatActivity(
      makeEvent({
        verb: "moved",
        actorName: "Alice",
        fromParentId: null,
        toParentId: "p1",
      }),
    );
    expect(result.text).toContain("into a container");
  });

  it("formats moved to root", () => {
    const result = formatActivity(
      makeEvent({
        verb: "moved",
        actorName: "Alice",
        fromParentId: "p1",
        toParentId: null,
      }),
    );
    expect(result.text).toContain("root level");
  });

  it("formats moved between containers", () => {
    const result = formatActivity(
      makeEvent({
        verb: "moved",
        actorName: "Alice",
        fromParentId: "p1",
        toParentId: "p2",
      }),
    );
    expect(result.text).toContain("new location");
  });

  it("formats updated with single change", () => {
    const result = formatActivity(
      makeEvent({
        verb: "updated",
        actorName: "Alice",
        changes: [{ field: "description", before: "Old", after: "New" }],
      }),
    );
    expect(result.text).toContain("changed description");
  });

  it("formats updated with multiple changes", () => {
    const result = formatActivity(
      makeEvent({
        verb: "updated",
        actorName: "Alice",
        changes: [
          { field: "description", before: "A", after: "B" },
          { field: "color", before: null, after: "red" },
          { field: "pinned", before: false, after: true },
          { field: "data.priority", before: "low", after: "high" },
        ],
      }),
    );
    expect(result.text).toContain("updated");
    expect(result.text).toContain("and 1 more");
  });

  it("formats updated with no changes", () => {
    const result = formatActivity(
      makeEvent({ verb: "updated", actorName: "Alice" }),
    );
    expect(result.text).toBe("Alice updated this");
  });

  it("formats commented with text", () => {
    const result = formatActivity(
      makeEvent({
        verb: "commented",
        actorName: "Alice",
        meta: { comment: "Great work!" },
      }),
    );
    expect(result.text).toContain("commented");
    expect(result.text).toContain("Great work!");
  });

  it("formats commented without text", () => {
    const result = formatActivity(
      makeEvent({ verb: "commented", actorName: "Alice" }),
    );
    expect(result.text).toBe("Alice left a comment");
  });

  it("formats mentioned", () => {
    const result = formatActivity(
      makeEvent({ verb: "mentioned", actorName: "Alice" }),
    );
    expect(result.text).toBe("Alice mentioned this");
  });

  it("formats assigned with assignee", () => {
    const result = formatActivity(
      makeEvent({
        verb: "assigned",
        actorName: "Alice",
        meta: { assigneeName: "Bob" },
      }),
    );
    expect(result.text).toContain("assigned this to Bob");
  });

  it("formats unassigned with assignee", () => {
    const result = formatActivity(
      makeEvent({
        verb: "unassigned",
        actorName: "Alice",
        meta: { assigneeName: "Bob" },
      }),
    );
    expect(result.text).toContain("unassigned Bob");
  });

  it("formats attached with name", () => {
    const result = formatActivity(
      makeEvent({
        verb: "attached",
        actorName: "Alice",
        meta: { name: "doc.pdf" },
      }),
    );
    expect(result.text).toContain('attached "doc.pdf"');
  });

  it("formats detached with name", () => {
    const result = formatActivity(
      makeEvent({
        verb: "detached",
        actorName: "Alice",
        meta: { name: "doc.pdf" },
      }),
    );
    expect(result.text).toContain('removed attachment "doc.pdf"');
  });

  it("formats linked with target", () => {
    const result = formatActivity(
      makeEvent({
        verb: "linked",
        actorName: "Alice",
        meta: { targetName: "Task B" },
      }),
    );
    expect(result.text).toContain('linked to "Task B"');
  });

  it("formats unlinked with target", () => {
    const result = formatActivity(
      makeEvent({
        verb: "unlinked",
        actorName: "Alice",
        meta: { targetName: "Task B" },
      }),
    );
    expect(result.text).toContain('removed link to "Task B"');
  });

  it("formats completed", () => {
    const result = formatActivity(
      makeEvent({ verb: "completed", actorName: "Alice" }),
    );
    expect(result.text).toBe("Alice completed this");
  });

  it("formats reopened", () => {
    const result = formatActivity(
      makeEvent({ verb: "reopened", actorName: "Alice" }),
    );
    expect(result.text).toBe("Alice reopened this");
  });

  it("formats blocked with reason", () => {
    const result = formatActivity(
      makeEvent({
        verb: "blocked",
        actorName: "Alice",
        meta: { reason: "dependency" },
      }),
    );
    expect(result.text).toContain("blocked this");
    expect(result.text).toContain("dependency");
  });

  it("formats unblocked", () => {
    const result = formatActivity(
      makeEvent({ verb: "unblocked", actorName: "Alice" }),
    );
    expect(result.text).toBe("Alice unblocked this");
  });

  it("formats custom verb", () => {
    const result = formatActivity(
      makeEvent({
        verb: "custom",
        actorName: "Alice",
        meta: { verb: "archived the project" },
      }),
    );
    expect(result.text).toBe("Alice archived the project");
  });

  it("uses Someone when no actor", () => {
    const result = formatActivity(makeEvent({ verb: "updated" }));
    expect(result.text).toContain("Someone");
  });

  it("uses actorName override from opts", () => {
    const result = formatActivity(
      makeEvent({ verb: "updated", actorName: "Alice" }),
      { actorName: "System" },
    );
    expect(result.text).toContain("System");
  });
});

// ── groupActivityByDate ──────────────────────────────────────────────────────

describe("groupActivityByDate", () => {
  it("returns empty array for no events", () => {
    expect(groupActivityByDate([])).toEqual([]);
  });

  it("groups today's events", () => {
    const events = [
      makeEvent({ id: "a", createdAt: new Date().toISOString() }),
      makeEvent({ id: "b", createdAt: new Date().toISOString() }),
    ];

    const groups = groupActivityByDate(events);
    expect(groups).toHaveLength(1);
    expect(groups.at(0)?.label).toBe("Today");
    expect(groups.at(0)?.events).toHaveLength(2);
  });

  it("groups yesterday's events", () => {
    const yesterday = new Date(Date.now() - 86_400_000).toISOString();
    const events = [makeEvent({ id: "a", createdAt: yesterday })];

    const groups = groupActivityByDate(events);
    expect(groups).toHaveLength(1);
    expect(groups.at(0)?.label).toBe("Yesterday");
  });

  it("groups older events as Earlier", () => {
    const old = new Date("2020-01-01T00:00:00.000Z").toISOString();
    const events = [makeEvent({ id: "a", createdAt: old })];

    const groups = groupActivityByDate(events);
    expect(groups).toHaveLength(1);
    expect(groups.at(0)?.label).toBe("Earlier");
  });

  it("creates multiple groups in order", () => {
    const now = new Date().toISOString();
    const old = new Date("2020-01-01T00:00:00.000Z").toISOString();

    const events = [
      makeEvent({ id: "a", createdAt: now }),
      makeEvent({ id: "b", createdAt: old }),
    ];

    const groups = groupActivityByDate(events);
    expect(groups).toHaveLength(2);
    expect(groups.at(0)?.label).toBe("Today");
    expect(groups.at(1)?.label).toBe("Earlier");
  });

  it("sorts events within each group newest-first", () => {
    const now1 = new Date();
    const now2 = new Date(now1.getTime() + 1000);

    const events = [
      makeEvent({ id: "early", createdAt: now1.toISOString() }),
      makeEvent({ id: "late", createdAt: now2.toISOString() }),
    ];

    const groups = groupActivityByDate(events);
    const todayEvents = groups.at(0)?.events ?? [];
    expect(todayEvents.at(0)?.id).toBe("late");
    expect(todayEvents.at(1)?.id).toBe("early");
  });
});
