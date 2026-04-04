import { describe, it, expect, beforeEach } from "vitest";
import { EdgeModel } from "./edge-model.js";
import { ObjectRegistry } from "./registry.js";
import { objectId } from "./types.js";
import type { EdgeTypeDef } from "./types.js";

describe("EdgeModel", () => {
  let counter: number;
  let edges: EdgeModel;

  beforeEach(() => {
    counter = 0;
    edges = new EdgeModel({
      generateId: () => `edge-${counter++}`,
    });
  });

  // ── Add ──────────────────────────────────────────────────────────────────────

  it("adds edges", () => {
    const edge = edges.add({
      sourceId: objectId("a"),
      targetId: objectId("b"),
      relation: "blocks",
      data: {},
    });

    expect(edge.id).toBe("edge-0");
    expect(edge.sourceId).toBe("a");
    expect(edge.targetId).toBe("b");
    expect(edge.relation).toBe("blocks");
    expect(edges.size).toBe(1);
  });

  it("throws on missing required fields", () => {
    expect(() =>
      edges.add({ sourceId: objectId("a"), targetId: objectId(""), relation: "x", data: {} }),
    ).toThrow();
  });

  // ── Remove ───────────────────────────────────────────────────────────────────

  it("removes edges", () => {
    const edge = edges.add({
      sourceId: objectId("a"),
      targetId: objectId("b"),
      relation: "blocks",
      data: {},
    });

    const removed = edges.remove(edge.id);
    expect(removed).not.toBeNull();
    expect(edges.size).toBe(0);
  });

  it("returns null for missing edge", () => {
    expect(edges.remove("nope")).toBeNull();
  });

  // ── Update ───────────────────────────────────────────────────────────────────

  it("updates edge data", () => {
    const edge = edges.add({
      sourceId: objectId("a"),
      targetId: objectId("b"),
      relation: "blocks",
      data: { weight: 1 },
    });

    const updated = edges.update(edge.id, { data: { weight: 5 } });
    expect(updated.data).toEqual({ weight: 5 });
    expect(updated.id).toBe(edge.id); // immutable
  });

  // ── Query ────────────────────────────────────────────────────────────────────

  it("queries by source", () => {
    edges.add({ sourceId: objectId("a"), targetId: objectId("b"), relation: "blocks", data: {} });
    edges.add({ sourceId: objectId("a"), targetId: objectId("c"), relation: "refs", data: {} });
    edges.add({ sourceId: objectId("b"), targetId: objectId("c"), relation: "blocks", data: {} });

    expect(edges.getFrom("a").length).toBe(2);
    expect(edges.getFrom("a", "blocks").length).toBe(1);
  });

  it("queries by target", () => {
    edges.add({ sourceId: objectId("a"), targetId: objectId("c"), relation: "x", data: {} });
    edges.add({ sourceId: objectId("b"), targetId: objectId("c"), relation: "y", data: {} });

    expect(edges.getTo("c").length).toBe(2);
    expect(edges.getTo("c", "x").length).toBe(1);
  });

  it("queries between pair", () => {
    edges.add({ sourceId: objectId("a"), targetId: objectId("b"), relation: "x", data: {} });
    edges.add({ sourceId: objectId("a"), targetId: objectId("b"), relation: "y", data: {} });

    expect(edges.getBetween("a", "b").length).toBe(2);
    expect(edges.getBetween("a", "b", "x").length).toBe(1);
  });

  it("getConnected returns all edges touching an object", () => {
    edges.add({ sourceId: objectId("a"), targetId: objectId("b"), relation: "x", data: {} });
    edges.add({ sourceId: objectId("c"), targetId: objectId("a"), relation: "y", data: {} });

    expect(edges.getConnected("a").length).toBe(2);
  });

  // ── Registry validation ────────────────────────────────────────────────────

  it("enforces allowMultiple: false", () => {
    const registry = new ObjectRegistry();
    registry.registerEdge({
      relation: "assigned-to",
      label: "Assigned To",
      allowMultiple: false,
    });

    const model = new EdgeModel({
      registry,
      generateId: () => `e-${counter++}`,
    });

    model.add({
      sourceId: objectId("a"),
      targetId: objectId("b"),
      relation: "assigned-to",
      data: {},
    });

    expect(() =>
      model.add({
        sourceId: objectId("a"),
        targetId: objectId("b"),
        relation: "assigned-to",
        data: {},
      }),
    ).toThrow(/does not allow multiple/);
  });

  it("enforces undirected + allowMultiple: false", () => {
    const registry = new ObjectRegistry();
    registry.registerEdge({
      relation: "friends-with",
      label: "Friends",
      allowMultiple: false,
      undirected: true,
    });

    const model = new EdgeModel({
      registry,
      generateId: () => `e-${counter++}`,
    });

    model.add({
      sourceId: objectId("a"),
      targetId: objectId("b"),
      relation: "friends-with",
      data: {},
    });

    // Reverse direction should also be blocked
    expect(() =>
      model.add({
        sourceId: objectId("b"),
        targetId: objectId("a"),
        relation: "friends-with",
        data: {},
      }),
    ).toThrow(/already exists/);
  });

  // ── Events ───────────────────────────────────────────────────────────────────

  it("fires events on mutations", () => {
    const kinds: string[] = [];
    edges.on((e) => kinds.push(e.kind));

    const edge = edges.add({
      sourceId: objectId("a"),
      targetId: objectId("b"),
      relation: "x",
      data: {},
    });
    edges.update(edge.id, { data: { updated: true } });
    edges.remove(edge.id);

    expect(kinds).toEqual([
      "add", "change",
      "update", "change",
      "remove", "change",
    ]);
  });

  // ── Serialization ────────────────────────────────────────────────────────────

  it("round-trips through JSON", () => {
    edges.add({ sourceId: objectId("a"), targetId: objectId("b"), relation: "x", data: {} });
    edges.add({ sourceId: objectId("c"), targetId: objectId("d"), relation: "y", data: {} });

    const json = edges.toJSON();
    const restored = EdgeModel.fromJSON(json);
    expect(restored.size).toBe(2);
  });
});
