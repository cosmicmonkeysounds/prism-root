/**
 * Unit tests for Schema Designer pure helpers.
 *
 * Covers the layout/build functions that drive the canvas — render-free
 * logic that can be exercised without mounting React or xyflow.
 */

import { describe, it, expect } from "vitest";
import type { EntityDef, EdgeTypeDef } from "@prism/core/object-model";
import {
  layoutEntities,
  buildSchemaNodes,
  buildSchemaEdges,
} from "./schema-designer-panel.js";

function makeEntity(type: string, overrides: Partial<EntityDef<string>> = {}): EntityDef<string> {
  return {
    type,
    category: "custom",
    label: type,
    pluralLabel: `${type}s`,
    icon: "\u{1F4E6}",
    color: "#888888",
    fields: [],
    ...overrides,
  };
}

describe("layoutEntities", () => {
  it("places the first entity at (40, 40)", () => {
    const out = layoutEntities([makeEntity("a")], new Map());
    expect(out.get("a")).toEqual({ x: 40, y: 40 });
  });

  it("wraps to a new row after the column count is reached", () => {
    const defs = [
      makeEntity("a"),
      makeEntity("b"),
      makeEntity("c"),
      makeEntity("d"),
      makeEntity("e"),
    ];
    const out = layoutEntities(defs, new Map(), 220, 140, 4);
    expect(out.get("a")).toEqual({ x: 40, y: 40 });
    expect(out.get("d")).toEqual({ x: 3 * 220 + 40, y: 40 });
    expect(out.get("e")).toEqual({ x: 40, y: 140 + 40 });
  });

  it("preserves pre-existing positions and only assigns missing ones", () => {
    const existing = new Map([["a", { x: 500, y: 600 }]]);
    const out = layoutEntities([makeEntity("a"), makeEntity("b")], existing);
    expect(out.get("a")).toEqual({ x: 500, y: 600 });
    expect(out.get("b")).toBeDefined();
  });
});

describe("buildSchemaNodes", () => {
  it("returns one xyflow node per EntityDef with the schemaEntity custom type", () => {
    const nodes = buildSchemaNodes(
      [makeEntity("task"), makeEntity("project")],
      new Map([
        ["task", { x: 10, y: 20 }],
        ["project", { x: 30, y: 40 }],
      ]),
    );
    expect(nodes).toHaveLength(2);
    expect(nodes[0]?.id).toBe("task");
    expect(nodes[0]?.type).toBe("schemaEntity");
    expect(nodes[0]?.position).toEqual({ x: 10, y: 20 });
  });

  it("includes field count in the node subtitle", () => {
    const def = makeEntity("task", {
      fields: [
        { id: "title", type: "string", required: true },
        { id: "done", type: "bool", required: false },
      ],
    });
    const nodes = buildSchemaNodes([def], new Map([["task", { x: 0, y: 0 }]]));
    const data = nodes[0]?.data as { sub: string };
    expect(data.sub).toContain("2 fields");
  });

  it("uses the entity color for the border", () => {
    const def = makeEntity("task", { color: "#ff0000" });
    const nodes = buildSchemaNodes([def], new Map([["task", { x: 0, y: 0 }]]));
    const style = nodes[0]?.style as { border: string };
    expect(style.border).toContain("#ff0000");
  });

  it("falls back to (0,0) when a position is missing", () => {
    const nodes = buildSchemaNodes([makeEntity("a")], new Map());
    expect(nodes[0]?.position).toEqual({ x: 0, y: 0 });
  });
});

describe("buildSchemaEdges", () => {
  it("emits one edge per (source, target) pair declared on an EdgeTypeDef", () => {
    const edges: EdgeTypeDef[] = [
      {
        relation: "depends-on",
        label: "depends on",
        sourceTypes: ["task"],
        targetTypes: ["task", "milestone"],
      },
    ];
    const out = buildSchemaEdges(edges);
    expect(out).toHaveLength(2);
    expect(out[0]?.source).toBe("task");
    expect(out[0]?.target).toBe("task");
    expect(out[1]?.target).toBe("milestone");
  });

  it("skips edge defs that have no sourceTypes or targetTypes", () => {
    const out = buildSchemaEdges([
      { relation: "unscoped", label: "U" },
      { relation: "src-only", label: "S", sourceTypes: ["a"] },
      { relation: "tgt-only", label: "T", targetTypes: ["b"] },
    ]);
    expect(out).toEqual([]);
  });

  it("carries the edge color into the xyflow stroke style", () => {
    const out = buildSchemaEdges([
      {
        relation: "r",
        label: "R",
        color: "#abc123",
        sourceTypes: ["a"],
        targetTypes: ["b"],
      },
    ]);
    const style = out[0]?.style as { stroke: string };
    expect(style.stroke).toBe("#abc123");
  });

  it("assigns a unique id per emitted edge", () => {
    const out = buildSchemaEdges([
      {
        relation: "linked",
        label: "L",
        sourceTypes: ["a"],
        targetTypes: ["b", "c"],
      },
    ]);
    const ids = out.map((e) => e.id);
    expect(new Set(ids).size).toBe(ids.length);
    expect(ids).toContain("linked:a->b");
    expect(ids).toContain("linked:a->c");
  });
});
