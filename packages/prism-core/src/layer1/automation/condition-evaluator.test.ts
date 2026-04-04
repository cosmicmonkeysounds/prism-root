import { describe, it, expect } from "vitest";
import {
  evaluateCondition,
  compare,
  getPath,
  interpolate,
  matchesObjectTrigger,
} from "./condition-evaluator.js";
import type { AutomationCondition, AutomationContext } from "./automation-types.js";

// ── getPath ─────────────────────────────────────────────────────────────────

describe("getPath", () => {
  it("resolves top-level key", () => {
    expect(getPath({ a: 1 }, "a")).toBe(1);
  });

  it("resolves nested path", () => {
    expect(getPath({ a: { b: { c: 42 } } }, "a.b.c")).toBe(42);
  });

  it("returns undefined for missing path", () => {
    expect(getPath({ a: 1 }, "b.c")).toBeUndefined();
  });

  it("returns undefined for null intermediate", () => {
    expect(getPath({ a: null }, "a.b")).toBeUndefined();
  });
});

// ── compare ─────────────────────────────────────────────────────────────────

describe("compare", () => {
  it("eq", () => {
    expect(compare(5, "eq", 5)).toBe(true);
    expect(compare(5, "eq", 6)).toBe(false);
  });

  it("neq", () => {
    expect(compare(5, "neq", 6)).toBe(true);
    expect(compare(5, "neq", 5)).toBe(false);
  });

  it("gt / gte / lt / lte", () => {
    expect(compare(10, "gt", 5)).toBe(true);
    expect(compare(5, "gte", 5)).toBe(true);
    expect(compare(3, "lt", 5)).toBe(true);
    expect(compare(5, "lte", 5)).toBe(true);
  });

  it("contains", () => {
    expect(compare("hello world", "contains", "world")).toBe(true);
    expect(compare("hello", "contains", "xyz")).toBe(false);
  });

  it("startsWith / endsWith", () => {
    expect(compare("hello", "startsWith", "hel")).toBe(true);
    expect(compare("hello", "endsWith", "llo")).toBe(true);
  });

  it("matches (regex)", () => {
    expect(compare("abc123", "matches", "^abc\\d+$")).toBe(true);
    expect(compare("xyz", "matches", "^abc")).toBe(false);
  });

  it("unknown operator returns false", () => {
    expect(compare(1, "unknown", 1)).toBe(false);
  });
});

// ── evaluateCondition ────────────────────────────────────────────────────────

describe("evaluateCondition", () => {
  const ctx: AutomationContext = {
    automationId: "a1",
    triggeredAt: "2026-01-01T00:00:00Z",
    triggerType: "object:created",
    object: { type: "task", tags: ["urgent", "backend"], status: "open" },
  };

  it("field condition — eq", () => {
    const cond: AutomationCondition = {
      type: "field",
      path: "object.status",
      operator: "eq",
      value: "open",
    };
    expect(evaluateCondition(cond, ctx)).toBe(true);
  });

  it("field condition — neq", () => {
    const cond: AutomationCondition = {
      type: "field",
      path: "object.status",
      operator: "neq",
      value: "closed",
    };
    expect(evaluateCondition(cond, ctx)).toBe(true);
  });

  it("type condition", () => {
    expect(
      evaluateCondition({ type: "type", objectType: "task" }, ctx),
    ).toBe(true);
    expect(
      evaluateCondition({ type: "type", objectType: "goal" }, ctx),
    ).toBe(false);
  });

  it("tags condition — all", () => {
    expect(
      evaluateCondition(
        { type: "tags", tags: ["urgent", "backend"], mode: "all" },
        ctx,
      ),
    ).toBe(true);
    expect(
      evaluateCondition(
        { type: "tags", tags: ["urgent", "frontend"], mode: "all" },
        ctx,
      ),
    ).toBe(false);
  });

  it("tags condition — any", () => {
    expect(
      evaluateCondition(
        { type: "tags", tags: ["frontend", "backend"], mode: "any" },
        ctx,
      ),
    ).toBe(true);
    expect(
      evaluateCondition(
        { type: "tags", tags: ["frontend", "design"], mode: "any" },
        ctx,
      ),
    ).toBe(false);
  });

  it("tags condition — missing tags on object", () => {
    const noTagsCtx: AutomationContext = {
      ...ctx,
      object: { type: "task" },
    };
    expect(
      evaluateCondition(
        { type: "tags", tags: ["urgent"], mode: "all" },
        noTagsCtx,
      ),
    ).toBe(false);
  });

  it("and combinator", () => {
    const cond: AutomationCondition = {
      type: "and",
      conditions: [
        { type: "type", objectType: "task" },
        { type: "field", path: "object.status", operator: "eq", value: "open" },
      ],
    };
    expect(evaluateCondition(cond, ctx)).toBe(true);
  });

  it("and combinator — one fails", () => {
    const cond: AutomationCondition = {
      type: "and",
      conditions: [
        { type: "type", objectType: "task" },
        { type: "field", path: "object.status", operator: "eq", value: "closed" },
      ],
    };
    expect(evaluateCondition(cond, ctx)).toBe(false);
  });

  it("or combinator", () => {
    const cond: AutomationCondition = {
      type: "or",
      conditions: [
        { type: "type", objectType: "goal" },
        { type: "type", objectType: "task" },
      ],
    };
    expect(evaluateCondition(cond, ctx)).toBe(true);
  });

  it("not combinator", () => {
    const cond: AutomationCondition = {
      type: "not",
      condition: { type: "type", objectType: "goal" },
    };
    expect(evaluateCondition(cond, ctx)).toBe(true);
  });

  it("nested combinators", () => {
    const cond: AutomationCondition = {
      type: "and",
      conditions: [
        {
          type: "or",
          conditions: [
            { type: "type", objectType: "task" },
            { type: "type", objectType: "goal" },
          ],
        },
        {
          type: "not",
          condition: {
            type: "field",
            path: "object.status",
            operator: "eq",
            value: "closed",
          },
        },
      ],
    };
    expect(evaluateCondition(cond, ctx)).toBe(true);
  });
});

// ── interpolate ─────────────────────────────────────────────────────────────

describe("interpolate", () => {
  const ctx: AutomationContext = {
    automationId: "a1",
    triggeredAt: "2026-01-01T00:00:00Z",
    triggerType: "object:created",
    object: { type: "task", name: "Fix bug" },
  };

  it("replaces {{path}} placeholders", () => {
    const result = interpolate(
      { title: "New: {{object.name}}", kind: "{{object.type}}" },
      ctx,
    );
    expect(result).toEqual({ title: "New: Fix bug", kind: "task" });
  });

  it("replaces missing paths with empty string", () => {
    const result = interpolate({ value: "{{object.missing}}" }, ctx);
    expect(result).toEqual({ value: "" });
  });

  it("handles nested objects", () => {
    const result = interpolate(
      { data: { ref: "{{automationId}}" } },
      ctx,
    );
    expect(result).toEqual({ data: { ref: "a1" } });
  });
});

// ── matchesObjectTrigger ────────────────────────────────────────────────────

describe("matchesObjectTrigger", () => {
  const event = {
    object: { type: "task", tags: ["urgent"], status: "open" },
  };

  it("matches when no filters", () => {
    expect(matchesObjectTrigger({}, event)).toBe(true);
  });

  it("filters by objectTypes", () => {
    expect(
      matchesObjectTrigger({ objectTypes: ["task", "goal"] }, event),
    ).toBe(true);
    expect(
      matchesObjectTrigger({ objectTypes: ["goal"] }, event),
    ).toBe(false);
  });

  it("filters by tags", () => {
    expect(matchesObjectTrigger({ tags: ["urgent"] }, event)).toBe(true);
    expect(matchesObjectTrigger({ tags: ["urgent", "other"] }, event)).toBe(
      false,
    );
  });

  it("filters by fieldMatch", () => {
    expect(
      matchesObjectTrigger({ fieldMatch: { status: "open" } }, event),
    ).toBe(true);
    expect(
      matchesObjectTrigger({ fieldMatch: { status: "closed" } }, event),
    ).toBe(false);
  });

  it("combines all filters", () => {
    expect(
      matchesObjectTrigger(
        { objectTypes: ["task"], tags: ["urgent"], fieldMatch: { status: "open" } },
        event,
      ),
    ).toBe(true);
    expect(
      matchesObjectTrigger(
        { objectTypes: ["task"], tags: ["missing"], fieldMatch: { status: "open" } },
        event,
      ),
    ).toBe(false);
  });
});
