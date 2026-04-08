import { describe, it, expect } from "vitest";
import type { GraphObject, ObjectId } from "@prism/core/object-model";
import { readListField } from "./list-widget-renderer.js";

function obj(id: string, overrides: Partial<GraphObject> = {}): GraphObject {
  return {
    id: id as ObjectId,
    type: "task",
    name: `Task ${id}`,
    parentId: null,
    position: 0,
    status: "todo",
    tags: ["a", "b"],
    date: null,
    endDate: null,
    description: "desc",
    color: null,
    image: null,
    pinned: false,
    data: { priority: 3, owner: "alice" },
    createdAt: "2026-04-08T00:00:00Z",
    updatedAt: "2026-04-08T12:00:00Z",
    ...overrides,
  } as GraphObject;
}

describe("readListField", () => {
  it("reads top-level GraphObject fields", () => {
    const o = obj("1");
    expect(readListField(o, "name")).toBe("Task 1");
    expect(readListField(o, "type")).toBe("task");
    expect(readListField(o, "status")).toBe("todo");
    expect(readListField(o, "description")).toBe("desc");
  });

  it("joins tags with comma", () => {
    expect(readListField(obj("1"), "tags")).toBe("a, b");
  });

  it("falls back to data payload", () => {
    expect(readListField(obj("1"), "priority")).toBe("3");
    expect(readListField(obj("1"), "owner")).toBe("alice");
  });

  it("returns empty string for unknown or nullish fields", () => {
    expect(readListField(obj("1"), "missing")).toBe("");
    expect(readListField(obj("1", { status: null }), "status")).toBe("");
    expect(readListField(obj("1"), "")).toBe("");
  });
});
