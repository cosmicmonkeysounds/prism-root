import { describe, it, expect } from "vitest";
import type { GraphObject, ObjectId } from "@prism/core/object-model";
import { groupByField } from "./kanban-widget-renderer.js";

function obj(id: string, data: Record<string, unknown>): GraphObject {
  return {
    id: id as ObjectId,
    type: "task",
    name: id,
    data,
    createdAt: 0,
    updatedAt: 0,
  } as unknown as GraphObject;
}

describe("groupByField", () => {
  it("buckets objects by a string field", () => {
    const cols = groupByField(
      [
        obj("1", { status: "todo" }),
        obj("2", { status: "done" }),
        obj("3", { status: "todo" }),
      ],
      "status",
    );
    const todo = cols.find((c) => c.value === "todo");
    const done = cols.find((c) => c.value === "done");
    expect(todo?.cards).toHaveLength(2);
    expect(done?.cards).toHaveLength(1);
  });

  it("groups missing or empty values under '—'", () => {
    const cols = groupByField(
      [
        obj("1", { status: "" }),
        obj("2", {}),
        obj("3", { status: "active" }),
      ],
      "status",
    );
    const blank = cols.find((c) => c.value === "—");
    expect(blank?.cards).toHaveLength(2);
  });

  it("coerces non-string values to string", () => {
    const cols = groupByField(
      [obj("1", { priority: 1 }), obj("2", { priority: 2 })],
      "priority",
    );
    expect(cols.map((c) => c.value).sort()).toEqual(["1", "2"]);
  });
});
