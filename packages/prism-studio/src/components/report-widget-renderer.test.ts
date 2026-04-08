import { describe, it, expect } from "vitest";
import type { GraphObject, ObjectId } from "@prism/core/object-model";
import {
  buildReportGroups,
  computeAggregate,
  formatAggregate,
} from "./report-widget-renderer.js";

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

describe("buildReportGroups", () => {
  it("groups by a shell field and counts", () => {
    const groups = buildReportGroups(
      [
        obj("1", { type: "task" }),
        obj("2", { type: "note" }),
        obj("3", { type: "task" }),
      ],
      "type",
    );
    expect(groups).toHaveLength(2);
    expect(groups.find((g) => g.key === "task")?.aggregate).toBe(2);
    expect(groups.find((g) => g.key === "note")?.aggregate).toBe(1);
  });

  it("falls back to — for missing group values", () => {
    const groups = buildReportGroups([obj("1", { status: null })], "status");
    expect(groups[0]?.key).toBe("—");
  });

  it("sorts groups alphabetically", () => {
    const groups = buildReportGroups(
      [obj("1", { type: "zeta" }), obj("2", { type: "alpha" })],
      "type",
    );
    expect(groups.map((g) => g.key)).toEqual(["alpha", "zeta"]);
  });

  it("computes sum aggregation over data field", () => {
    const groups = buildReportGroups(
      [
        obj("1", { type: "sale", data: { amount: "10" } }),
        obj("2", { type: "sale", data: { amount: "25" } }),
      ],
      "type",
      "sum",
      "amount",
    );
    expect(groups[0]?.aggregate).toBe(35);
  });
});

describe("computeAggregate", () => {
  const list = [
    obj("1", { data: { amount: 10 } }),
    obj("2", { data: { amount: 20 } }),
    obj("3", { data: { amount: 30 } }),
  ];

  it("count ignores value field", () => {
    expect(computeAggregate(list, "count")).toBe(3);
  });

  it("sum adds numeric values", () => {
    expect(computeAggregate(list, "sum", "amount")).toBe(60);
  });

  it("avg divides by count", () => {
    expect(computeAggregate(list, "avg", "amount")).toBe(20);
  });

  it("min and max find extremes", () => {
    expect(computeAggregate(list, "min", "amount")).toBe(10);
    expect(computeAggregate(list, "max", "amount")).toBe(30);
  });

  it("returns 0 when value field is missing", () => {
    expect(computeAggregate(list, "sum")).toBe(0);
    expect(computeAggregate([], "sum", "amount")).toBe(0);
  });

  it("ignores non-numeric values", () => {
    const mixed = [
      obj("1", { data: { x: 10 } }),
      obj("2", { data: { x: "oops" } }),
      obj("3", { data: { x: 4 } }),
    ];
    expect(computeAggregate(mixed, "sum", "x")).toBe(14);
  });
});

describe("formatAggregate", () => {
  it("rounds count", () => {
    expect(formatAggregate(5, "count")).toBe("5");
    expect(formatAggregate(5.7, "count")).toBe("6");
  });

  it("keeps integers integer", () => {
    expect(formatAggregate(60, "sum")).toBe("60");
  });

  it("formats floats to 2 decimals", () => {
    expect(formatAggregate(20.333, "avg")).toBe("20.33");
  });

  it("renders non-finite as dash", () => {
    expect(formatAggregate(Number.NaN, "sum")).toBe("—");
  });
});
