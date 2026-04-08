import { describe, it, expect } from "vitest";
import type { GraphObject, ObjectId } from "@prism/core/object-model";
import {
  parseTableColumns,
  readCellValue,
  sortObjects,
} from "./table-widget-renderer.js";

function obj(id: string, overrides: Partial<GraphObject> = {}): GraphObject {
  return {
    id: id as ObjectId,
    type: "task",
    name: `Task ${id}`,
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
    ...overrides,
  } as GraphObject;
}

describe("parseTableColumns", () => {
  it("returns default columns for empty spec", () => {
    expect(parseTableColumns("").map((c) => c.id)).toEqual([
      "name",
      "type",
      "status",
      "updatedAt",
    ]);
  });

  it("parses id:Label pairs", () => {
    const cols = parseTableColumns("name:Name, status:Status");
    expect(cols).toEqual([
      { id: "name", label: "Name" },
      { id: "status", label: "Status" },
    ]);
  });

  it("falls back to id when label is missing", () => {
    const cols = parseTableColumns("priority, owner");
    expect(cols).toEqual([
      { id: "priority", label: "priority" },
      { id: "owner", label: "owner" },
    ]);
  });

  it("skips blank entries", () => {
    expect(parseTableColumns("name:Name, ,status:Status")).toHaveLength(2);
  });
});

describe("readCellValue", () => {
  it("reads shell fields", () => {
    const o = obj("1", { status: "done", tags: ["x", "y"] });
    expect(readCellValue(o, "name")).toBe("Task 1");
    expect(readCellValue(o, "type")).toBe("task");
    expect(readCellValue(o, "status")).toBe("done");
    expect(readCellValue(o, "tags")).toBe("x, y");
  });

  it("reads data payload keys", () => {
    const o = obj("1", { data: { priority: 5 } });
    expect(readCellValue(o, "priority")).toBe("5");
  });

  it("returns empty for missing values", () => {
    expect(readCellValue(obj("1"), "nope")).toBe("");
    expect(readCellValue(obj("1"), "")).toBe("");
  });
});

describe("sortObjects", () => {
  it("sorts ascending by shell field", () => {
    const sorted = sortObjects(
      [obj("b", { name: "Bravo" }), obj("a", { name: "Alpha" })],
      "name",
      "asc",
    );
    expect(sorted.map((o) => o.name)).toEqual(["Alpha", "Bravo"]);
  });

  it("sorts descending", () => {
    const sorted = sortObjects(
      [obj("a", { name: "Alpha" }), obj("b", { name: "Bravo" })],
      "name",
      "desc",
    );
    expect(sorted.map((o) => o.name)).toEqual(["Bravo", "Alpha"]);
  });

  it("treats numeric strings naturally", () => {
    const sorted = sortObjects(
      [
        obj("1", { data: { priority: "10" } }),
        obj("2", { data: { priority: "2" } }),
      ],
      "priority",
      "asc",
    );
    expect(sorted.map((o) => o.id)).toEqual(["2", "1"]);
  });

  it("returns input unchanged when sortField is empty", () => {
    const input = [obj("a"), obj("b")];
    const sorted = sortObjects(input, "", "asc");
    expect(sorted.map((o) => o.id)).toEqual(["a", "b"]);
    // should be a new array, not same reference
    expect(sorted).toBe(input);
  });
});
