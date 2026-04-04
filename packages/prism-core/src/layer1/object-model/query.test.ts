import { describe, it, expect } from "vitest";
import {
  queryToParams,
  paramsToQuery,
  matchesQuery,
  sortObjects,
} from "./query.js";
import { objectId } from "./types.js";
import type { GraphObject, ObjectQuery } from "./types.js";

function makeObj(overrides: Partial<GraphObject> = {}): GraphObject {
  return {
    id: objectId("test"),
    type: "task",
    name: "Test Task",
    parentId: null,
    position: 0,
    status: "active",
    tags: ["important"],
    date: "2026-01-15",
    endDate: null,
    description: "A test task",
    color: null,
    image: null,
    pinned: false,
    data: {},
    createdAt: "2026-01-01T00:00:00Z",
    updatedAt: "2026-01-15T00:00:00Z",
    ...overrides,
  };
}

describe("queryToParams / paramsToQuery round-trip", () => {
  it("round-trips simple query", () => {
    const q: ObjectQuery = { type: "task", status: "active", limit: 10 };
    const params = queryToParams(q);
    const result = paramsToQuery(params);
    expect(result.type).toBe("task");
    expect(result.status).toBe("active");
    expect(result.limit).toBe(10);
  });

  it("round-trips array values", () => {
    const q: ObjectQuery = { type: ["task", "note"], tags: ["a", "b"] };
    const params = queryToParams(q);
    const result = paramsToQuery(params);
    expect(result.type).toEqual(["task", "note"]);
    expect(result.tags).toEqual(["a", "b"]);
  });

  it("round-trips null parentId", () => {
    const q: ObjectQuery = { parentId: null };
    const params = queryToParams(q);
    const result = paramsToQuery(params);
    expect(result.parentId).toBeNull();
  });
});

describe("matchesQuery", () => {
  it("matches by type", () => {
    const obj = makeObj({ type: "task" });
    expect(matchesQuery(obj, { type: "task" })).toBe(true);
    expect(matchesQuery(obj, { type: "note" })).toBe(false);
  });

  it("matches by type array", () => {
    const obj = makeObj({ type: "task" });
    expect(matchesQuery(obj, { type: ["task", "note"] })).toBe(true);
  });

  it("matches by status", () => {
    const obj = makeObj({ status: "done" });
    expect(matchesQuery(obj, { status: "done" })).toBe(true);
    expect(matchesQuery(obj, { status: "active" })).toBe(false);
  });

  it("matches by tags (all required)", () => {
    const obj = makeObj({ tags: ["a", "b", "c"] });
    expect(matchesQuery(obj, { tags: ["a", "b"] })).toBe(true);
    expect(matchesQuery(obj, { tags: ["a", "d"] })).toBe(false);
  });

  it("matches by search", () => {
    const obj = makeObj({ name: "Hello World", description: "test" });
    expect(matchesQuery(obj, { search: "hello" })).toBe(true);
    expect(matchesQuery(obj, { search: "test" })).toBe(true);
    expect(matchesQuery(obj, { search: "nope" })).toBe(false);
  });

  it("matches by parentId null (root objects)", () => {
    const obj = makeObj({ parentId: null });
    expect(matchesQuery(obj, { parentId: null })).toBe(true);
    expect(matchesQuery(obj, { parentId: "some-id" })).toBe(false);
  });

  it("filters deleted by default", () => {
    const obj = makeObj({ deletedAt: "2026-01-01" });
    expect(matchesQuery(obj, {})).toBe(false);
    expect(matchesQuery(obj, { includeDeleted: true })).toBe(true);
  });

  it("matches by date range", () => {
    const obj = makeObj({ date: "2026-06-15" });
    expect(matchesQuery(obj, { dateAfter: "2026-06-01" })).toBe(true);
    expect(matchesQuery(obj, { dateAfter: "2026-07-01" })).toBe(false);
    expect(matchesQuery(obj, { dateBefore: "2026-07-01" })).toBe(true);
    expect(matchesQuery(obj, { dateBefore: "2026-06-01" })).toBe(false);
  });

  it("matches pinned", () => {
    expect(matchesQuery(makeObj({ pinned: true }), { pinned: true })).toBe(true);
    expect(matchesQuery(makeObj({ pinned: false }), { pinned: true })).toBe(false);
  });
});

describe("sortObjects", () => {
  it("sorts by name ascending", () => {
    const objects = [
      makeObj({ id: objectId("c"), name: "Charlie" }),
      makeObj({ id: objectId("a"), name: "Alice" }),
      makeObj({ id: objectId("b"), name: "Bob" }),
    ];
    sortObjects(objects, { sortBy: "name", sortDir: "asc" });
    expect(objects.map((o) => o.name)).toEqual(["Alice", "Bob", "Charlie"]);
  });

  it("sorts by name descending", () => {
    const objects = [
      makeObj({ id: objectId("a"), name: "Alice" }),
      makeObj({ id: objectId("b"), name: "Bob" }),
    ];
    sortObjects(objects, { sortBy: "name", sortDir: "desc" });
    expect(objects.map((o) => o.name)).toEqual(["Bob", "Alice"]);
  });

  it("defaults to position ascending", () => {
    const objects = [
      makeObj({ id: objectId("b"), position: 2 }),
      makeObj({ id: objectId("a"), position: 0 }),
      makeObj({ id: objectId("c"), position: 1 }),
    ];
    sortObjects(objects, {});
    expect(objects.map((o) => o.position)).toEqual([0, 1, 2]);
  });
});
