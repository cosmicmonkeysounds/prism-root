import { describe, it, expect } from "vitest";
import { LoroDoc } from "loro-crdt";
import { createGraphStore } from "@prism/core/stores";
import { buildGraphFromKernel, reconcileGraph } from "./graph-panel-data.js";
import type { GraphObject, ObjectId } from "@prism/core/object-model";

const id = (s: string) => s as ObjectId;

function obj(partial: {
  id: string;
  type: string;
  name: string;
  parentId?: ObjectId | null;
  position?: number;
  data?: Record<string, unknown>;
  deletedAt?: number | null;
}): GraphObject {
  return {
    id: id(partial.id),
    type: partial.type,
    name: partial.name,
    parentId: partial.parentId ?? null,
    position: partial.position ?? 0,
    status: null,
    tags: [],
    date: null,
    endDate: null,
    description: "",
    color: null,
    icon: null,
    data: partial.data ?? {},
    createdAt: 0,
    updatedAt: 0,
    deletedAt: partial.deletedAt ?? null,
  } as unknown as GraphObject;
}

const registry = {
  get: (_t: string) => ({ icon: "●" }),
};

describe("buildGraphFromKernel", () => {
  it("emits one node per live object", () => {
    const out = buildGraphFromKernel(
      [
        obj({ id: "a", type: "page", name: "Home" }),
        obj({ id: "b", type: "section", name: "Hero", parentId: id("a") }),
      ],
      [],
      registry,
    );
    expect(out.nodes.map((n) => n.id).sort()).toEqual(["a", "b"]);
    expect(out.nodes[0]?.data?.label).toContain("Home");
  });

  it("skips soft-deleted objects", () => {
    const out = buildGraphFromKernel(
      [
        obj({ id: "a", type: "page", name: "Home" }),
        obj({ id: "b", type: "page", name: "Gone", deletedAt: 123 }),
      ],
      [],
      registry,
    );
    expect(out.nodes.map((n) => n.id)).toEqual(["a"]);
  });

  it("emits containment hard-ref edges for parent-child pairs", () => {
    const out = buildGraphFromKernel(
      [
        obj({ id: "a", type: "page", name: "Home" }),
        obj({ id: "b", type: "section", name: "Hero", parentId: id("a") }),
      ],
      [],
      registry,
    );
    expect(out.edges).toHaveLength(1);
    expect(out.edges[0]).toMatchObject({
      source: "a",
      target: "b",
      wireType: "hard",
    });
  });

  it("drops containment edges pointing at dead parents", () => {
    const out = buildGraphFromKernel(
      [obj({ id: "b", type: "section", name: "Orphan", parentId: id("missing") })],
      [],
      registry,
    );
    expect(out.edges).toHaveLength(0);
  });

  it("emits weak-ref edges for ObjectEdges", () => {
    const out = buildGraphFromKernel(
      [
        obj({ id: "a", type: "task", name: "A" }),
        obj({ id: "b", type: "task", name: "B" }),
      ],
      [
        {
          id: "edge-1",
          sourceId: id("a"),
          targetId: id("b"),
          relation: "depends-on",
        },
      ],
      registry,
    );
    expect(out.edges.find((e) => e.id === "edge-1")).toMatchObject({
      source: "a",
      target: "b",
      wireType: "weak",
      label: "depends-on",
    });
  });

  it("drops ObjectEdges that reference missing endpoints", () => {
    const out = buildGraphFromKernel(
      [obj({ id: "a", type: "task", name: "A" })],
      [
        {
          id: "edge-1",
          sourceId: id("a"),
          targetId: id("missing"),
          relation: "depends-on",
        },
      ],
      registry,
    );
    expect(out.edges.find((e) => e.id === "edge-1")).toBeUndefined();
  });
});

describe("reconcileGraph", () => {
  function fresh() {
    return createGraphStore(new LoroDoc());
  }

  it("adds new nodes and edges into an empty store", () => {
    const store = fresh();
    const desired = buildGraphFromKernel(
      [
        obj({ id: "a", type: "page", name: "Home" }),
        obj({ id: "b", type: "section", name: "Hero", parentId: id("a") }),
      ],
      [],
      registry,
    );
    const ids = reconcileGraph(store.getState(), desired);
    expect(ids).toEqual(new Set(["a", "b"]));
    expect(store.getState().nodes.map((n) => n.id).sort()).toEqual(["a", "b"]);
    expect(store.getState().edges).toHaveLength(1);
  });

  it("removes nodes that disappeared from the desired set", () => {
    const store = fresh();
    const a = buildGraphFromKernel(
      [
        obj({ id: "a", type: "page", name: "A" }),
        obj({ id: "b", type: "page", name: "B" }),
      ],
      [],
      registry,
    );
    reconcileGraph(store.getState(), a);
    expect(store.getState().nodes).toHaveLength(2);

    const b = buildGraphFromKernel(
      [obj({ id: "a", type: "page", name: "A" })],
      [],
      registry,
    );
    reconcileGraph(store.getState(), b);
    expect(store.getState().nodes.map((n) => n.id)).toEqual(["a"]);
  });

  it("preserves x/y of existing nodes when desired builds seed them at 0,0", () => {
    const store = fresh();
    const desired = buildGraphFromKernel(
      [obj({ id: "a", type: "page", name: "A" })],
      [],
      registry,
    );
    reconcileGraph(store.getState(), desired);

    // User drags the node.
    store.getState().moveNode("a", 500, 250);
    expect(store.getState().nodes[0]?.x).toBe(500);
    expect(store.getState().nodes[0]?.y).toBe(250);

    // Reconcile with a fresh build (still at 0,0).
    reconcileGraph(store.getState(), desired);
    expect(store.getState().nodes[0]?.x).toBe(500);
    expect(store.getState().nodes[0]?.y).toBe(250);
  });

  it("adds new edges and removes stale ones", () => {
    const store = fresh();
    const first = buildGraphFromKernel(
      [
        obj({ id: "a", type: "task", name: "A" }),
        obj({ id: "b", type: "task", name: "B" }),
      ],
      [
        {
          id: "e1",
          sourceId: id("a"),
          targetId: id("b"),
          relation: "depends-on",
        },
      ],
      registry,
    );
    reconcileGraph(store.getState(), first);
    expect(store.getState().edges.map((e) => e.id)).toEqual(["e1"]);

    const second = buildGraphFromKernel(
      [
        obj({ id: "a", type: "task", name: "A" }),
        obj({ id: "b", type: "task", name: "B" }),
      ],
      [
        {
          id: "e2",
          sourceId: id("b"),
          targetId: id("a"),
          relation: "blocks",
        },
      ],
      registry,
    );
    reconcileGraph(store.getState(), second);
    expect(store.getState().edges.map((e) => e.id)).toEqual(["e2"]);
  });
});
