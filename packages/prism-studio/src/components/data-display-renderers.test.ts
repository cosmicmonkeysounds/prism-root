/**
 * Pure-function tests for data display helpers.
 */

import { describe, it, expect } from "vitest";
import type { GraphObject } from "@prism/core/object-model";
import { objectId } from "@prism/core/object-model";
import { computeStat, formatStatValue, progressRatio } from "./data-display-renderers.js";

function obj(id: string, data: Record<string, unknown>): GraphObject {
  return {
    id: objectId(id),
    type: "sale",
    name: id,
    parentId: null,
    position: 0,
    status: "active",
    tags: [],
    date: null,
    endDate: null,
    description: "",
    color: null,
    image: null,
    pinned: false,
    data,
    createdAt: 0,
    updatedAt: 0,
    deletedAt: null,
  } as unknown as GraphObject;
}

describe("computeStat", () => {
  const items = [
    obj("a", { amount: 10, priority: 1 }),
    obj("b", { amount: 20, priority: 3 }),
    obj("c", { amount: 30, priority: 2 }),
    obj("d", { amount: "not a number" }),
  ];

  it("returns 0 for empty input", () => {
    expect(computeStat([], "count", undefined)).toBe(0);
    expect(computeStat([], "sum", "amount")).toBe(0);
  });

  it("counts objects regardless of field", () => {
    expect(computeStat(items, "count", undefined)).toBe(4);
    expect(computeStat(items, "count", "amount")).toBe(4);
  });

  it("sums numeric field values, skipping non-numbers", () => {
    expect(computeStat(items, "sum", "amount")).toBe(60);
  });

  it("averages numeric field values", () => {
    expect(computeStat(items, "avg", "amount")).toBe(20);
  });

  it("returns min/max of numeric field values", () => {
    expect(computeStat(items, "min", "amount")).toBe(10);
    expect(computeStat(items, "max", "amount")).toBe(30);
  });

  it("returns 0 when no numeric values are present", () => {
    const only = [obj("x", { amount: "nope" })];
    expect(computeStat(only, "sum", "amount")).toBe(0);
    expect(computeStat(only, "avg", "amount")).toBe(0);
  });

  it("falls back to count when valueField is missing", () => {
    expect(computeStat(items, "sum", undefined)).toBe(items.length);
  });
});

describe("formatStatValue", () => {
  it("rounds when decimals = 0", () => {
    expect(formatStatValue(12.7, false, 0)).toBe("13");
  });

  it("respects decimal places", () => {
    expect(formatStatValue(3.14159, false, 2)).toBe("3.14");
  });

  it("adds thousands separators", () => {
    expect(formatStatValue(1234567, true, 0)).toBe("1,234,567");
  });

  it("combines thousands + decimals", () => {
    expect(formatStatValue(1234567.89, true, 2)).toBe("1,234,567.89");
  });

  it("handles small numbers unchanged", () => {
    expect(formatStatValue(7, true, 0)).toBe("7");
    expect(formatStatValue(-42, false, 0)).toBe("-42");
  });
});

describe("progressRatio", () => {
  it("clamps to [0,1]", () => {
    expect(progressRatio(50, 100)).toBe(0.5);
    expect(progressRatio(-5, 100)).toBe(0);
    expect(progressRatio(150, 100)).toBe(1);
  });

  it("handles edge cases safely", () => {
    expect(progressRatio(5, 0)).toBe(0);
    expect(progressRatio(NaN, 100)).toBe(0);
    expect(progressRatio(5, NaN)).toBe(0);
  });
});
