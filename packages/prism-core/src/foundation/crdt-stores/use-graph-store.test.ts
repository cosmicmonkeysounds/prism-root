import { describe, it, expect, beforeEach } from "vitest";
import { LoroDoc } from "loro-crdt";
import { createGraphStore } from "./use-graph-store.js";

describe("createGraphStore", () => {
  let doc: LoroDoc;
  let store: ReturnType<typeof createGraphStore>;

  beforeEach(() => {
    doc = new LoroDoc();
    store = createGraphStore(doc);
  });

  it("starts with empty nodes and edges", () => {
    const state = store.getState();
    expect(state.nodes).toEqual([]);
    expect(state.edges).toEqual([]);
  });

  it("adds a node", () => {
    store.getState().addNode({
      id: "n1",
      type: "default",
      x: 10,
      y: 20,
      width: 200,
      height: 100,
      data: { label: "Test" },
    });
    const nodes = store.getState().nodes;
    expect(nodes).toHaveLength(1);
    expect(nodes[0]?.id).toBe("n1");
    expect(nodes[0]?.x).toBe(10);
    expect(nodes[0]?.y).toBe(20);
  });

  it("moves a node", () => {
    store.getState().addNode({
      id: "n1",
      type: "default",
      x: 0,
      y: 0,
      width: 200,
      height: 100,
      data: {},
    });
    store.getState().moveNode("n1", 50, 75);
    const nodes = store.getState().nodes;
    expect(nodes[0]?.x).toBe(50);
    expect(nodes[0]?.y).toBe(75);
  });

  it("updates node data", () => {
    store.getState().addNode({
      id: "n1",
      type: "default",
      x: 0,
      y: 0,
      width: 200,
      height: 100,
      data: { foo: "bar" },
    });
    store.getState().updateNodeData("n1", { foo: "baz", extra: true });
    const data = store.getState().nodes[0]?.data;
    expect(data).toBeDefined();
  });

  it("adds an edge", () => {
    store.getState().addNode({
      id: "n1",
      type: "default",
      x: 0,
      y: 0,
      width: 100,
      height: 100,
      data: {},
    });
    store.getState().addNode({
      id: "n2",
      type: "default",
      x: 200,
      y: 0,
      width: 100,
      height: 100,
      data: {},
    });
    store.getState().addEdge({
      id: "e1",
      source: "n1",
      target: "n2",
      wireType: "hard",
    });
    const edges = store.getState().edges;
    expect(edges).toHaveLength(1);
    expect(edges[0]?.source).toBe("n1");
    expect(edges[0]?.target).toBe("n2");
    expect(edges[0]?.wireType).toBe("hard");
  });

  it("removes a node and cascades edge deletion", () => {
    store.getState().addNode({
      id: "n1",
      type: "default",
      x: 0,
      y: 0,
      width: 100,
      height: 100,
      data: {},
    });
    store.getState().addNode({
      id: "n2",
      type: "default",
      x: 200,
      y: 0,
      width: 100,
      height: 100,
      data: {},
    });
    store.getState().addEdge({
      id: "e1",
      source: "n1",
      target: "n2",
      wireType: "hard",
    });
    store.getState().removeNode("n1");
    expect(store.getState().nodes).toHaveLength(1);
    expect(store.getState().edges).toHaveLength(0);
  });

  it("removes an edge", () => {
    store.getState().addNode({
      id: "n1",
      type: "default",
      x: 0,
      y: 0,
      width: 100,
      height: 100,
      data: {},
    });
    store.getState().addNode({
      id: "n2",
      type: "default",
      x: 200,
      y: 0,
      width: 100,
      height: 100,
      data: {},
    });
    store.getState().addEdge({
      id: "e1",
      source: "n1",
      target: "n2",
      wireType: "weak",
      label: "refs",
    });
    store.getState().removeEdge("e1");
    expect(store.getState().edges).toHaveLength(0);
  });

  it("syncs between two stores via Loro", () => {
    store.getState().addNode({
      id: "n1",
      type: "default",
      x: 10,
      y: 20,
      width: 200,
      height: 100,
      data: { label: "Sync Test" },
    });

    // Export and import into a second doc/store
    const exported = doc.export({ mode: "snapshot" });
    const doc2 = new LoroDoc();
    doc2.import(exported);
    const store2 = createGraphStore(doc2);

    expect(store2.getState().nodes).toHaveLength(1);
    expect(store2.getState().nodes[0]?.id).toBe("n1");
  });

  it("handles moveNode on non-existent id gracefully", () => {
    store.getState().moveNode("nonexistent", 50, 50);
    expect(store.getState().nodes).toHaveLength(0);
  });

  it("handles removeNode on non-existent id gracefully", () => {
    store.getState().removeNode("nonexistent");
    expect(store.getState().nodes).toHaveLength(0);
  });
});
