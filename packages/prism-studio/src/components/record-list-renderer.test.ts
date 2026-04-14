import { describe, expect, it } from "vitest";
import { objectId, type GraphObject } from "@prism/core/object-model";
import {
  resolveTemplateField,
  applyRecordListView,
} from "./record-list-renderer.js";

function makeObj(partial: {
  id?: string;
  type?: string;
  name?: string;
  status?: string;
  tags?: string[];
  description?: string;
  updatedAt?: string;
  deletedAt?: string | null;
  data?: Record<string, unknown>;
}): GraphObject {
  return {
    id: objectId(partial.id ?? "o1"),
    type: partial.type ?? "task",
    name: partial.name ?? "Untitled",
    parentId: null,
    position: 0,
    status: partial.status ?? "draft",
    tags: partial.tags ?? [],
    date: null,
    endDate: null,
    description: partial.description ?? "",
    color: null,
    image: null,
    pinned: false,
    data: partial.data ?? {},
    createdAt: "2026-01-01T00:00:00Z",
    updatedAt: partial.updatedAt ?? "2026-01-01T00:00:00Z",
    deletedAt: partial.deletedAt ?? null,
  } as unknown as GraphObject;
}

describe("resolveTemplateField", () => {
  it("reads shell fields", () => {
    const obj = makeObj({ name: "Buy milk" });
    expect(resolveTemplateField(obj, { field: "name" })).toBe("Buy milk");
  });

  it("reads data payload fields", () => {
    const obj = makeObj({ data: { priority: "high" } });
    expect(resolveTemplateField(obj, { field: "priority" })).toBe("high");
  });

  it("joins tags into a comma-separated string", () => {
    const obj = makeObj({ tags: ["work", "urgent"] });
    expect(resolveTemplateField(obj, { field: "tags", kind: "tags" })).toBe(
      "work, urgent",
    );
  });

  it("formats dates when kind is 'date'", () => {
    const obj = makeObj({ updatedAt: "2026-01-15T10:30:00Z" });
    const result = resolveTemplateField(obj, {
      field: "updatedAt",
      kind: "date",
    });
    expect(result).toMatch(/2026/);
  });

  it("returns empty string when the field is missing", () => {
    const obj = makeObj({});
    expect(resolveTemplateField(obj, { field: "nonexistent" })).toBe("");
  });

  it("treats 'badge'/'status'/'text' kinds as identity for the value", () => {
    const obj = makeObj({ status: "done" });
    expect(resolveTemplateField(obj, { field: "status", kind: "status" })).toBe(
      "done",
    );
    expect(resolveTemplateField(obj, { field: "status", kind: "badge" })).toBe(
      "done",
    );
    expect(resolveTemplateField(obj, { field: "status", kind: "text" })).toBe(
      "done",
    );
  });
});

describe("applyRecordListView", () => {
  it("returns non-deleted objects when no viewConfig is supplied", () => {
    const objects = [
      makeObj({ id: "a" }),
      makeObj({ id: "b", deletedAt: "2026-01-02T00:00:00Z" }),
      makeObj({ id: "c" }),
    ];
    const result = applyRecordListView(objects);
    expect(result.map((o) => o.id)).toEqual(["a", "c"]);
  });

  it("applies filters from viewConfig", () => {
    const objects = [
      makeObj({ id: "a", status: "open" }),
      makeObj({ id: "b", status: "done" }),
      makeObj({ id: "c", status: "open" }),
    ];
    const result = applyRecordListView(objects, {
      filters: [{ field: "status", op: "eq", value: "open" }],
    });
    expect(result.map((o) => o.id)).toEqual(["a", "c"]);
  });

  it("applies sorts from viewConfig", () => {
    const objects = [
      makeObj({ id: "a", name: "Charlie" }),
      makeObj({ id: "b", name: "Alpha" }),
      makeObj({ id: "c", name: "Bravo" }),
    ];
    const result = applyRecordListView(objects, {
      sorts: [{ field: "name", dir: "asc" }],
    });
    expect(result.map((o) => o.name)).toEqual(["Alpha", "Bravo", "Charlie"]);
  });

  it("applies limit from viewConfig", () => {
    const objects = [
      makeObj({ id: "a" }),
      makeObj({ id: "b" }),
      makeObj({ id: "c" }),
      makeObj({ id: "d" }),
    ];
    const result = applyRecordListView(objects, { limit: 2 });
    expect(result).toHaveLength(2);
  });

  it("composes filter + sort + limit in one pipeline", () => {
    const objects = [
      makeObj({ id: "a", status: "open", name: "Zebra" }),
      makeObj({ id: "b", status: "done", name: "Alpha" }),
      makeObj({ id: "c", status: "open", name: "Delta" }),
      makeObj({ id: "d", status: "open", name: "Bravo" }),
    ];
    const result = applyRecordListView(objects, {
      filters: [{ field: "status", op: "eq", value: "open" }],
      sorts: [{ field: "name", dir: "asc" }],
      limit: 2,
    });
    expect(result.map((o) => o.name)).toEqual(["Bravo", "Delta"]);
  });

  it("excludes deleted objects by default when viewConfig is provided", () => {
    const objects = [
      makeObj({ id: "a" }),
      makeObj({ id: "b", deletedAt: "2026-01-02T00:00:00Z" }),
    ];
    const result = applyRecordListView(objects, {
      sorts: [{ field: "id", dir: "asc" }],
    });
    expect(result.map((o) => o.id)).toEqual(["a"]);
  });
});
