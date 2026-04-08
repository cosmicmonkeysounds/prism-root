import { describe, it, expect } from "vitest";
import type { GraphObject, ObjectId } from "@prism/core/object-model";
import { buildMonthGrid, toIsoDate, readDateField } from "./calendar-widget-renderer.js";

function obj(id: string, data: Record<string, unknown>): GraphObject {
  return {
    id: id as ObjectId,
    type: "event",
    name: id,
    data,
    createdAt: 0,
    updatedAt: 0,
  } as unknown as GraphObject;
}

describe("toIsoDate", () => {
  it("formats a Date as YYYY-MM-DD with zero padding", () => {
    expect(toIsoDate(new Date(2025, 0, 5))).toBe("2025-01-05");
    expect(toIsoDate(new Date(2025, 11, 31))).toBe("2025-12-31");
  });
});

describe("readDateField", () => {
  it("extracts date portion from ISO timestamps", () => {
    expect(readDateField(obj("a", { date: "2025-03-14T08:00:00Z" }), "date")).toBe("2025-03-14");
  });
  it("accepts bare date strings", () => {
    expect(readDateField(obj("a", { date: "2025-03-14" }), "date")).toBe("2025-03-14");
  });
  it("returns null when missing or non-string", () => {
    expect(readDateField(obj("a", {}), "date")).toBe(null);
    expect(readDateField(obj("a", { date: 123 }), "date")).toBe(null);
  });
});

describe("buildMonthGrid", () => {
  it("produces 42 cells (6 weeks)", () => {
    const cells = buildMonthGrid(new Date(2025, 2, 15), [], "date");
    expect(cells).toHaveLength(42);
  });

  it("marks cells outside the anchor month as inMonth=false", () => {
    const cells = buildMonthGrid(new Date(2025, 2, 15), [], "date");
    const inMonth = cells.filter((c) => c.inMonth);
    expect(inMonth.length).toBe(31); // March 2025 has 31 days
  });

  it("places events on matching cells", () => {
    const events = [
      obj("e1", { date: "2025-03-14" }),
      obj("e2", { date: "2025-03-14" }),
      obj("e3", { date: "2025-03-01" }),
    ];
    const cells = buildMonthGrid(new Date(2025, 2, 15), events, "date");
    const mar14 = cells.find((c) => c.isoDate === "2025-03-14");
    const mar01 = cells.find((c) => c.isoDate === "2025-03-01");
    expect(mar14?.events).toHaveLength(2);
    expect(mar01?.events).toHaveLength(1);
  });
});
