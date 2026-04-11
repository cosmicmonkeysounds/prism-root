import { describe, it, expect } from "vitest";
import { applyElkLayout } from "./auto-layout.js";
import type { Node, Edge } from "@xyflow/react";

describe("applyElkLayout", () => {
  const makeNodes = (): Node[] => [
    {
      id: "a",
      position: { x: 0, y: 0 },
      data: { label: "A" },
      type: "default",
    },
    {
      id: "b",
      position: { x: 0, y: 0 },
      data: { label: "B" },
      type: "default",
    },
    {
      id: "c",
      position: { x: 0, y: 0 },
      data: { label: "C" },
      type: "default",
    },
  ];

  const makeEdges = (): Edge[] => [
    { id: "e1", source: "a", target: "b" },
    { id: "e2", source: "b", target: "c" },
  ];

  it("assigns distinct positions to each node", async () => {
    const result = await applyElkLayout(makeNodes(), makeEdges());
    expect(result).toHaveLength(3);

    const positions = result.map((n) => `${n.position.x},${n.position.y}`);
    const unique = new Set(positions);
    expect(unique.size).toBe(3);
  });

  it("preserves node ids and data", async () => {
    const result = await applyElkLayout(makeNodes(), makeEdges());
    expect(result.map((n) => n.id)).toEqual(["a", "b", "c"]);
    expect(result[0]?.data?.label).toBe("A");
  });

  it("does not mutate input nodes", async () => {
    const nodes = makeNodes();
    const origPositions = nodes.map((n) => ({ ...n.position }));
    await applyElkLayout(nodes, makeEdges());
    nodes.forEach((n, i) => {
      expect(n.position).toEqual(origPositions[i]);
    });
  });

  it("handles empty input", async () => {
    const result = await applyElkLayout([], []);
    expect(result).toEqual([]);
  });

  it("respects direction option", async () => {
    const downResult = await applyElkLayout(makeNodes(), makeEdges(), {
      direction: "DOWN",
    });
    const rightResult = await applyElkLayout(makeNodes(), makeEdges(), {
      direction: "RIGHT",
    });

    // In DOWN layout, y-spread should be larger than x-spread
    // In RIGHT layout, x-spread should be larger than y-spread
    const downYSpread = Math.max(...downResult.map((n) => n.position.y)) -
      Math.min(...downResult.map((n) => n.position.y));
    const rightXSpread = Math.max(...rightResult.map((n) => n.position.x)) -
      Math.min(...rightResult.map((n) => n.position.x));

    expect(downYSpread).toBeGreaterThan(0);
    expect(rightXSpread).toBeGreaterThan(0);
  });
});
