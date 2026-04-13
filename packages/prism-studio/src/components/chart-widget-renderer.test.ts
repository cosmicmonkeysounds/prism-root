import { describe, it, expect } from "vitest";
import type { GraphObject, ObjectId } from "@prism/core/object-model";
import { aggregateObjects } from "./chart-data.js";

function obj(id: string, data: Record<string, unknown>): GraphObject {
  return {
    id: id as ObjectId,
    type: "sale",
    name: id,
    data,
    createdAt: 0,
    updatedAt: 0,
  } as unknown as GraphObject;
}

describe("aggregateObjects", () => {
  const sales = [
    obj("s1", { region: "east", amount: 100 }),
    obj("s2", { region: "east", amount: 200 }),
    obj("s3", { region: "west", amount: 50 }),
  ];

  it("counts objects per group", () => {
    const points = aggregateObjects(sales, "region", undefined, "count");
    const east = points.find((p) => p.label === "east");
    const west = points.find((p) => p.label === "west");
    expect(east?.value).toBe(2);
    expect(west?.value).toBe(1);
  });

  it("sums a numeric field per group", () => {
    const points = aggregateObjects(sales, "region", "amount", "sum");
    expect(points.find((p) => p.label === "east")?.value).toBe(300);
    expect(points.find((p) => p.label === "west")?.value).toBe(50);
  });

  it("averages a numeric field per group", () => {
    const points = aggregateObjects(sales, "region", "amount", "avg");
    expect(points.find((p) => p.label === "east")?.value).toBe(150);
  });

  it("computes min and max", () => {
    const minPoints = aggregateObjects(sales, "region", "amount", "min");
    const maxPoints = aggregateObjects(sales, "region", "amount", "max");
    expect(minPoints.find((p) => p.label === "east")?.value).toBe(100);
    expect(maxPoints.find((p) => p.label === "east")?.value).toBe(200);
  });

  it("ignores non-numeric values in sum aggregation", () => {
    const mixed = [
      obj("a", { region: "n", amount: 10 }),
      obj("b", { region: "n", amount: "not a number" }),
      obj("c", { region: "n", amount: 5 }),
    ];
    const points = aggregateObjects(mixed, "region", "amount", "sum");
    expect(points[0]?.value).toBe(15);
  });

  it("falls back to 0 when group has no numeric samples", () => {
    const bad = [obj("a", { region: "n", amount: "x" })];
    const points = aggregateObjects(bad, "region", "amount", "sum");
    expect(points[0]?.value).toBe(0);
  });
});
