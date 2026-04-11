import { describe, it, expect } from "vitest";
import type { GraphObject } from "@prism/core/object-model";
import { objectId } from "@prism/core/object-model";
import {
  getFieldValue,
  applyFilters,
  applySorts,
  applyGroups,
  applyViewConfig,
} from "./view-config.js";
import type { FilterConfig, SortConfig } from "./view-config.js";

// ── Helpers ───────────────────────────────────────────────────────────────────

function makeObject(overrides: Partial<GraphObject> = {}): GraphObject {
  return {
    id: objectId("obj-1"),
    type: "task",
    name: "Test Task",
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
    createdAt: "2026-01-01T00:00:00Z",
    updatedAt: "2026-01-01T00:00:00Z",
    ...overrides,
  };
}

// ── getFieldValue ─────────────────────────────────────────────────────────────

describe("getFieldValue", () => {
  it("reads shell fields", () => {
    const obj = makeObject({ name: "Hello", status: "active" });
    expect(getFieldValue(obj, "name")).toBe("Hello");
    expect(getFieldValue(obj, "status")).toBe("active");
  });

  it("reads data payload fields", () => {
    const obj = makeObject({ data: { priority: 5, note: "important" } });
    expect(getFieldValue(obj, "priority")).toBe(5);
    expect(getFieldValue(obj, "note")).toBe("important");
  });

  it("returns undefined for missing fields", () => {
    const obj = makeObject();
    expect(getFieldValue(obj, "nonexistent")).toBeUndefined();
  });
});

// ── applyFilters ─────────────────────────────────────────────────────────────

describe("applyFilters", () => {
  const objects = [
    makeObject({ id: objectId("a"), name: "Alpha", status: "active", tags: ["urgent"], data: { priority: 1 } }),
    makeObject({ id: objectId("b"), name: "Beta", status: "done", tags: ["backend"], data: { priority: 3 } }),
    makeObject({ id: objectId("c"), name: "Gamma", status: "active", tags: ["urgent", "backend"], data: { priority: 2 } }),
    makeObject({ id: objectId("d"), name: "Delta", status: null, tags: [], data: {} }),
  ];

  it("returns all when no filters", () => {
    expect(applyFilters(objects, [])).toHaveLength(4);
  });

  describe("eq", () => {
    it("filters by exact match", () => {
      const result = applyFilters(objects, [{ field: "status", op: "eq", value: "active" }]);
      expect(result).toHaveLength(2);
      expect(result.map((o) => o.id)).toEqual(["a", "c"]);
    });
  });

  describe("neq", () => {
    it("filters by not equal", () => {
      const result = applyFilters(objects, [{ field: "status", op: "neq", value: "active" }]);
      expect(result).toHaveLength(2);
    });
  });

  describe("contains", () => {
    it("string contains (case-insensitive)", () => {
      const result = applyFilters(objects, [{ field: "name", op: "contains", value: "alpha" }]);
      expect(result).toHaveLength(1);
      expect(result[0]?.id).toBe("a");
    });

    it("array contains", () => {
      const result = applyFilters(objects, [{ field: "tags", op: "contains", value: "urgent" }]);
      expect(result).toHaveLength(2);
    });
  });

  describe("starts", () => {
    it("string starts with (case-insensitive)", () => {
      const result = applyFilters(objects, [{ field: "name", op: "starts", value: "ga" }]);
      expect(result).toHaveLength(1);
      expect(result[0]?.id).toBe("c");
    });
  });

  describe("gt / gte / lt / lte", () => {
    it("gt filters data payload numbers", () => {
      const result = applyFilters(objects, [{ field: "priority", op: "gt", value: 1 }]);
      expect(result).toHaveLength(2);
    });

    it("gte includes equal", () => {
      const result = applyFilters(objects, [{ field: "priority", op: "gte", value: 2 }]);
      expect(result).toHaveLength(2);
    });

    it("lt filters", () => {
      const result = applyFilters(objects, [{ field: "priority", op: "lt", value: 3 }]);
      expect(result).toHaveLength(2);
    });

    it("lte includes equal", () => {
      const result = applyFilters(objects, [{ field: "priority", op: "lte", value: 2 }]);
      expect(result).toHaveLength(2);
    });
  });

  describe("in / nin", () => {
    it("in matches against array of values", () => {
      const result = applyFilters(objects, [{ field: "status", op: "in", value: ["active", "done"] }]);
      expect(result).toHaveLength(3);
    });

    it("nin excludes array of values", () => {
      const result = applyFilters(objects, [{ field: "status", op: "nin", value: ["active"] }]);
      expect(result).toHaveLength(2); // done + null
    });
  });

  describe("empty / notempty", () => {
    it("empty matches null, undefined, empty string, empty array", () => {
      const result = applyFilters(objects, [{ field: "status", op: "empty" }]);
      expect(result).toHaveLength(1);
      expect(result[0]?.id).toBe("d");
    });

    it("notempty excludes empty values", () => {
      const result = applyFilters(objects, [{ field: "status", op: "notempty" }]);
      expect(result).toHaveLength(3);
    });

    it("empty on tags checks array length", () => {
      const result = applyFilters(objects, [{ field: "tags", op: "empty" }]);
      expect(result).toHaveLength(1);
      expect(result[0]?.id).toBe("d");
    });
  });

  describe("AND combination", () => {
    it("combines multiple filters with AND", () => {
      const filters: FilterConfig[] = [
        { field: "status", op: "eq", value: "active" },
        { field: "tags", op: "contains", value: "backend" },
      ];
      const result = applyFilters(objects, filters);
      expect(result).toHaveLength(1);
      expect(result[0]?.id).toBe("c");
    });
  });
});

// ── applySorts ───────────────────────────────────────────────────────────────

describe("applySorts", () => {
  const objects = [
    makeObject({ id: objectId("c"), name: "Gamma", data: { priority: 2 } }),
    makeObject({ id: objectId("a"), name: "Alpha", data: { priority: 1 } }),
    makeObject({ id: objectId("b"), name: "Beta", data: { priority: 3 } }),
  ];

  it("returns same array when no sorts", () => {
    expect(applySorts(objects, [])).toEqual(objects);
  });

  it("sorts by name asc", () => {
    const result = applySorts(objects, [{ field: "name", dir: "asc" }]);
    expect(result.map((o) => o.id)).toEqual(["a", "b", "c"]);
  });

  it("sorts by name desc", () => {
    const result = applySorts(objects, [{ field: "name", dir: "desc" }]);
    expect(result.map((o) => o.id)).toEqual(["c", "b", "a"]);
  });

  it("sorts by data payload field", () => {
    const result = applySorts(objects, [{ field: "priority", dir: "asc" }]);
    expect(result.map((o) => o.id)).toEqual(["a", "c", "b"]);
  });

  it("does not mutate original array", () => {
    const original = [...objects];
    applySorts(objects, [{ field: "name", dir: "asc" }]);
    expect(objects.map((o) => o.id)).toEqual(original.map((o) => o.id));
  });

  it("supports multi-level sort", () => {
    const objs = [
      makeObject({ id: objectId("a"), status: "active", name: "Zebra" }),
      makeObject({ id: objectId("b"), status: "active", name: "Apple" }),
      makeObject({ id: objectId("c"), status: "done", name: "Mango" }),
    ];
    const sorts: SortConfig[] = [
      { field: "status", dir: "asc" },
      { field: "name", dir: "asc" },
    ];
    const result = applySorts(objs, sorts);
    expect(result.map((o) => o.id)).toEqual(["b", "a", "c"]);
  });
});

// ── applyGroups ──────────────────────────────────────────────────────────────

describe("applyGroups", () => {
  const objects = [
    makeObject({ id: objectId("a"), status: "active" }),
    makeObject({ id: objectId("b"), status: "done" }),
    makeObject({ id: objectId("c"), status: "active" }),
    makeObject({ id: objectId("d"), status: null }),
  ];

  it("returns single 'All' group when no groups", () => {
    const groups = applyGroups(objects, []);
    expect(groups).toHaveLength(1);
    expect(groups[0]?.key).toBe("__all__");
    expect(groups[0]?.objects).toHaveLength(4);
  });

  it("groups by status", () => {
    const groups = applyGroups(objects, [{ field: "status" }]);
    expect(groups).toHaveLength(3);

    const keys = groups.map((g) => g.key);
    expect(keys).toEqual(["active", "done", "__none__"]);

    expect(groups[0]?.objects).toHaveLength(2);
    expect(groups[1]?.objects).toHaveLength(1);
    expect(groups[2]?.objects).toHaveLength(1);
  });

  it("labels __none__ as 'None'", () => {
    const groups = applyGroups(objects, [{ field: "status" }]);
    const noneGroup = groups.find((g) => g.key === "__none__");
    expect(noneGroup?.label).toBe("None");
  });

  it("preserves insertion order of groups", () => {
    const objs = [
      makeObject({ id: objectId("x"), type: "note" }),
      makeObject({ id: objectId("y"), type: "task" }),
      makeObject({ id: objectId("z"), type: "note" }),
    ];
    const groups = applyGroups(objs, [{ field: "type" }]);
    expect(groups.map((g) => g.key)).toEqual(["note", "task"]);
  });

  it("respects collapsed config", () => {
    const groups = applyGroups(objects, [{ field: "status", collapsed: true }]);
    expect(groups.every((g) => g.collapsed)).toBe(true);
  });

  it("groups by data payload field", () => {
    const objs = [
      makeObject({ id: objectId("a"), data: { priority: "high" } }),
      makeObject({ id: objectId("b"), data: { priority: "low" } }),
      makeObject({ id: objectId("c"), data: { priority: "high" } }),
    ];
    const groups = applyGroups(objs, [{ field: "priority" }]);
    expect(groups).toHaveLength(2);
    expect(groups[0]?.key).toBe("high");
    expect(groups[0]?.objects).toHaveLength(2);
  });
});

// ── applyViewConfig ──────────────────────────────────────────────────────────

describe("applyViewConfig", () => {
  const objects = [
    makeObject({ id: objectId("a"), name: "Alpha", status: "active" }),
    makeObject({ id: objectId("b"), name: "Beta", status: "done" }),
    makeObject({ id: objectId("c"), name: "Gamma", status: "active", deletedAt: "2026-02-01T00:00:00Z" }),
  ];

  it("excludes deleted by default", () => {
    const result = applyViewConfig(objects, {});
    expect(result).toHaveLength(2);
  });

  it("includes deleted when excludeDeleted is false", () => {
    const result = applyViewConfig(objects, { excludeDeleted: false });
    expect(result).toHaveLength(3);
  });

  it("applies filters + sorts + limit", () => {
    const result = applyViewConfig(objects, {
      filters: [{ field: "status", op: "eq", value: "active" }],
      sorts: [{ field: "name", dir: "desc" }],
      limit: 1,
    });
    expect(result).toHaveLength(1);
    expect(result[0]?.id).toBe("a"); // Alpha is first active after desc sort... wait
  });

  it("applies full pipeline in order", () => {
    const objs = [
      makeObject({ id: objectId("a"), name: "Zebra", status: "active" }),
      makeObject({ id: objectId("b"), name: "Apple", status: "active" }),
      makeObject({ id: objectId("c"), name: "Mango", status: "done" }),
    ];
    const result = applyViewConfig(objs, {
      filters: [{ field: "status", op: "eq", value: "active" }],
      sorts: [{ field: "name", dir: "asc" }],
      limit: 1,
    });
    expect(result).toHaveLength(1);
    expect(result[0]?.id).toBe("b"); // Apple (sorted first, then limited)
  });
});
