import { describe, it, expect } from "vitest";
import { computePlan } from "./planning-engine.js";
import type { PlanNode } from "./planning-engine.js";
import type { GraphObject } from "@prism/core/object-model";
import { objectId } from "@prism/core/object-model";

function makeObj(
  id: string,
  data: Record<string, unknown> = {},
  overrides: Partial<GraphObject> = {},
): GraphObject {
  return {
    id: objectId(id),
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
    data,
    createdAt: "2024-01-01T00:00:00Z",
    updatedAt: "2024-01-01T00:00:00Z",
    ...overrides,
  };
}

describe("computePlan", () => {
  it("returns empty result for empty input", () => {
    const result = computePlan([]);
    expect(result.totalDurationDays).toBe(0);
    expect(result.criticalPath).toEqual([]);
    expect(result.nodes.size).toBe(0);
  });

  it("single task has duration 1 by default", () => {
    const result = computePlan([makeObj("A")]);
    const node = result.nodes.get("A") as PlanNode;
    expect(node.durationDays).toBe(1);
    expect(node.earlyStart).toBe(0);
    expect(node.earlyFinish).toBe(1);
    expect(node.isCritical).toBe(true);
    expect(result.totalDurationDays).toBe(1);
    expect(result.criticalPath).toEqual(["A"]);
  });

  it("uses explicit durationDays", () => {
    const result = computePlan([makeObj("A", { durationDays: 5 })]);
    expect(result.nodes.get("A")?.durationDays).toBe(5);
    expect(result.totalDurationDays).toBe(5);
  });

  it("derives duration from estimateMs", () => {
    const ms = 3 * 24 * 60 * 60 * 1000; // 3 days
    const result = computePlan([makeObj("A", { estimateMs: ms })]);
    expect(result.nodes.get("A")?.durationDays).toBe(3);
  });

  it("derives duration from date span", () => {
    const result = computePlan([
      makeObj("A", {}, { date: "2024-01-01", endDate: "2024-01-04" }),
    ]);
    expect(result.nodes.get("A")?.durationDays).toBe(3);
  });

  it("computes linear chain correctly", () => {
    // A(2d) → B(3d) → C(1d) = 6 days total
    const objects = [
      makeObj("A", { durationDays: 2 }),
      makeObj("B", { durationDays: 3, dependsOn: ["A"] }),
      makeObj("C", { durationDays: 1, dependsOn: ["B"] }),
    ];
    const result = computePlan(objects);

    expect(result.totalDurationDays).toBe(6);

    const a = result.nodes.get("A") as PlanNode;
    expect(a.earlyStart).toBe(0);
    expect(a.earlyFinish).toBe(2);

    const b = result.nodes.get("B") as PlanNode;
    expect(b.earlyStart).toBe(2);
    expect(b.earlyFinish).toBe(5);

    const c = result.nodes.get("C") as PlanNode;
    expect(c.earlyStart).toBe(5);
    expect(c.earlyFinish).toBe(6);

    expect(result.criticalPath).toEqual(["A", "B", "C"]);
  });

  it("computes diamond with float correctly", () => {
    // A(1d) → B(3d) → D(1d)  = critical: A→B→D = 5d
    // A(1d) → C(1d) → D(1d)  = non-critical: A→C→D = 3d, float=2
    const objects = [
      makeObj("A", { durationDays: 1 }),
      makeObj("B", { durationDays: 3, dependsOn: ["A"] }),
      makeObj("C", { durationDays: 1, dependsOn: ["A"] }),
      makeObj("D", { durationDays: 1, dependsOn: ["B", "C"] }),
    ];
    const result = computePlan(objects);

    expect(result.totalDurationDays).toBe(5);

    const c = result.nodes.get("C") as PlanNode;
    expect(c.totalFloat).toBe(2);
    expect(c.isCritical).toBe(false);

    const b = result.nodes.get("B") as PlanNode;
    expect(b.totalFloat).toBe(0);
    expect(b.isCritical).toBe(true);

    expect(result.criticalPath).toContain("A");
    expect(result.criticalPath).toContain("B");
    expect(result.criticalPath).toContain("D");
    expect(result.criticalPath).not.toContain("C");
  });

  it("parallel independent tasks have float", () => {
    // X(5d), Y(2d) — no deps between them
    const objects = [
      makeObj("X", { durationDays: 5 }),
      makeObj("Y", { durationDays: 2 }),
    ];
    const result = computePlan(objects);

    expect(result.totalDurationDays).toBe(5);

    const x = result.nodes.get("X") as PlanNode;
    expect(x.isCritical).toBe(true);

    const y = result.nodes.get("Y") as PlanNode;
    expect(y.totalFloat).toBe(3);
    expect(y.isCritical).toBe(false);
  });

  it("supports blockedBy field", () => {
    const objects = [
      makeObj("A", { durationDays: 2 }),
      makeObj("B", { durationDays: 1, blockedBy: ["A"] }),
    ];
    const result = computePlan(objects);
    expect((result.nodes.get("B") as PlanNode).earlyStart).toBe(2);
  });

  it("records predecessors in plan nodes", () => {
    const objects = [
      makeObj("A"),
      makeObj("B", { dependsOn: ["A"] }),
    ];
    const result = computePlan(objects);
    expect((result.nodes.get("B") as PlanNode).predecessors).toEqual(["A"]);
    expect((result.nodes.get("A") as PlanNode).predecessors).toEqual([]);
  });
});
