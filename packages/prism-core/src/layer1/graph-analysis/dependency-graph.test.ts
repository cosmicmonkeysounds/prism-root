import { describe, it, expect } from "vitest";
import {
  buildDependencyGraph,
  buildPredecessorGraph,
  topologicalSort,
  detectCycles,
  findBlockingChain,
  findImpactedObjects,
  computeSlipImpact,
} from "./dependency-graph.js";
import type { GraphObject } from "../object-model/types.js";
import { objectId } from "../object-model/types.js";

function makeObj(
  id: string,
  data: Record<string, unknown> = {},
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
  };
}

// A → B → C (linear chain)
const LINEAR = [
  makeObj("A"),
  makeObj("B", { dependsOn: ["A"] }),
  makeObj("C", { dependsOn: ["B"] }),
];

// Diamond: A → B, A → C, B → D, C → D
const DIAMOND = [
  makeObj("A"),
  makeObj("B", { dependsOn: ["A"] }),
  makeObj("C", { dependsOn: ["A"] }),
  makeObj("D", { dependsOn: ["B", "C"] }),
];

// Cycle: A → B → C → A
const CYCLIC = [
  makeObj("A", { dependsOn: ["C"] }),
  makeObj("B", { dependsOn: ["A"] }),
  makeObj("C", { dependsOn: ["B"] }),
];

describe("buildDependencyGraph", () => {
  it("builds forward graph", () => {
    const graph = buildDependencyGraph(LINEAR);
    expect([...(graph.get("A") ?? [])]).toContain("B");
    expect([...(graph.get("B") ?? [])]).toContain("C");
    expect(graph.get("C")?.size).toBe(0);
  });

  it("handles objects with no dependencies", () => {
    const graph = buildDependencyGraph([makeObj("X")]);
    expect(graph.get("X")?.size).toBe(0);
  });
});

describe("buildPredecessorGraph", () => {
  it("builds inverse graph", () => {
    const graph = buildPredecessorGraph(LINEAR);
    expect(graph.get("A")?.size).toBe(0);
    expect([...(graph.get("B") ?? [])]).toContain("A");
    expect([...(graph.get("C") ?? [])]).toContain("B");
  });
});

describe("topologicalSort", () => {
  it("returns correct order for linear chain", () => {
    const order = topologicalSort(LINEAR);
    expect(order.indexOf("A")).toBeLessThan(order.indexOf("B"));
    expect(order.indexOf("B")).toBeLessThan(order.indexOf("C"));
  });

  it("returns correct order for diamond", () => {
    const order = topologicalSort(DIAMOND);
    expect(order.indexOf("A")).toBeLessThan(order.indexOf("B"));
    expect(order.indexOf("A")).toBeLessThan(order.indexOf("C"));
    expect(order.indexOf("B")).toBeLessThan(order.indexOf("D"));
    expect(order.indexOf("C")).toBeLessThan(order.indexOf("D"));
  });

  it("handles empty input", () => {
    expect(topologicalSort([])).toEqual([]);
  });

  it("appends cyclic nodes at end", () => {
    const order = topologicalSort(CYCLIC);
    expect(order).toHaveLength(3);
  });

  it("supports blockedBy field", () => {
    const objs = [
      makeObj("X"),
      makeObj("Y", { blockedBy: ["X"] }),
    ];
    const order = topologicalSort(objs);
    expect(order.indexOf("X")).toBeLessThan(order.indexOf("Y"));
  });
});

describe("detectCycles", () => {
  it("returns empty for acyclic graphs", () => {
    expect(detectCycles(LINEAR)).toEqual([]);
    expect(detectCycles(DIAMOND)).toEqual([]);
  });

  it("detects cycles", () => {
    const cycles = detectCycles(CYCLIC);
    expect(cycles.length).toBeGreaterThan(0);
    const cycle = cycles[0];
    expect(cycle[0]).toBe(cycle[cycle.length - 1]);
  });
});

describe("findBlockingChain", () => {
  it("returns upstream blockers in BFS order", () => {
    const chain = findBlockingChain("C", LINEAR);
    expect(chain).toEqual(["B", "A"]);
  });

  it("returns empty for root objects", () => {
    expect(findBlockingChain("A", LINEAR)).toEqual([]);
  });

  it("handles diamond convergence", () => {
    const chain = findBlockingChain("D", DIAMOND);
    expect(chain).toContain("B");
    expect(chain).toContain("C");
    expect(chain).toContain("A");
  });
});

describe("findImpactedObjects", () => {
  it("returns downstream objects in BFS order", () => {
    const impacted = findImpactedObjects("A", LINEAR);
    expect(impacted).toEqual(["B", "C"]);
  });

  it("returns empty for leaf objects", () => {
    expect(findImpactedObjects("C", LINEAR)).toEqual([]);
  });

  it("handles diamond fan-out", () => {
    const impacted = findImpactedObjects("A", DIAMOND);
    expect(impacted).toContain("B");
    expect(impacted).toContain("C");
    expect(impacted).toContain("D");
  });
});

describe("computeSlipImpact", () => {
  it("propagates slip through linear chain", () => {
    const impacts = computeSlipImpact("A", 3, LINEAR);
    expect(impacts).toHaveLength(2);
    expect(impacts[0].objectId).toBe("B");
    expect(impacts[0].slipDays).toBe(3);
    expect(impacts[0].depth).toBe(1);
    expect(impacts[1].objectId).toBe("C");
    expect(impacts[1].slipDays).toBe(3);
    expect(impacts[1].depth).toBe(2);
  });

  it("propagates max slip through diamond", () => {
    const impacts = computeSlipImpact("A", 5, DIAMOND);
    const d = impacts.find((i) => i.objectId === "D");
    expect(d).toBeDefined();
    expect(d?.slipDays).toBe(5);
  });

  it("returns empty when no downstream objects", () => {
    expect(computeSlipImpact("C", 2, LINEAR)).toEqual([]);
  });

  it("sorts by depth then name", () => {
    const impacts = computeSlipImpact("A", 1, DIAMOND);
    expect(impacts[0].depth).toBeLessThanOrEqual(impacts[1].depth);
  });
});
