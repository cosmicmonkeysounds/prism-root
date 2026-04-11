import { describe, it, expect, beforeEach } from "vitest";
import type { GraphObject, ObjectEdge } from "@prism/core/object-model";
import { objectId, edgeId } from "@prism/core/object-model";
import type { ObjectSnapshot } from "./undo-types.js";
import { UndoRedoManager } from "./undo-manager.js";
import { createUndoBridge } from "./undo-bridge.js";

// ── Helpers ───────────────────────────────────────────────────────────────────

function makeObject(overrides: Partial<GraphObject> = {}): GraphObject {
  return {
    id: objectId("obj-1"),
    type: "task",
    name: "Test Task",
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
    data: {},
    createdAt: "2026-01-01T00:00:00Z",
    updatedAt: "2026-01-01T00:00:00Z",
    ...overrides,
  };
}

function makeEdge(overrides: Partial<ObjectEdge> = {}): ObjectEdge {
  return {
    id: edgeId("edge-1"),
    sourceId: objectId("obj-1"),
    targetId: objectId("obj-2"),
    relation: "depends-on",
    createdAt: "2026-01-01T00:00:00Z",
    data: {},
    ...overrides,
  };
}

// ── Tests ─────────────────────────────────────────────────────────────────────

describe("createUndoBridge", () => {
  let applied: Array<{ snapshots: ObjectSnapshot[]; direction: "undo" | "redo" }>;
  let manager: UndoRedoManager;

  beforeEach(() => {
    applied = [];
    manager = new UndoRedoManager((snapshots, direction) => {
      applied.push({ snapshots, direction });
    });
  });

  it("returns treeHooks and edgeHooks", () => {
    const bridge = createUndoBridge(manager);
    expect(bridge.treeHooks).toBeDefined();
    expect(bridge.edgeHooks).toBeDefined();
  });

  // ── Tree hooks ──────────────────────────────────────────────────────────

  describe("treeHooks.afterAdd", () => {
    it("pushes a create snapshot", () => {
      const { treeHooks } = createUndoBridge(manager);
      const obj = makeObject();
      treeHooks.afterAdd?.(obj);

      expect(manager.historySize).toBe(1);
      expect(manager.undoLabel).toBe("Create task");

      manager.undo();
      expect(applied).toHaveLength(1);
      const snap = applied[0]?.snapshots[0];
      expect(snap?.kind).toBe("object");
      if (snap?.kind === "object") {
        expect(snap.before).toBeNull();
        expect(snap.after?.id).toBe("obj-1");
      }
    });
  });

  describe("treeHooks.afterRemove", () => {
    it("pushes delete snapshots for object and descendants", () => {
      const { treeHooks } = createUndoBridge(manager);
      const parent = makeObject({ id: objectId("p-1"), name: "Parent" });
      const child1 = makeObject({ id: objectId("c-1"), name: "Child 1", parentId: objectId("p-1") });
      const child2 = makeObject({ id: objectId("c-2"), name: "Child 2", parentId: objectId("p-1") });

      treeHooks.afterRemove?.(parent, [child1, child2]);

      expect(manager.historySize).toBe(1);
      manager.undo();
      const snaps = applied[0]?.snapshots ?? [];
      expect(snaps).toHaveLength(3); // parent + 2 children
      expect(snaps.every((s) => s.kind === "object")).toBe(true);
      for (const s of snaps) {
        if (s.kind === "object") {
          expect(s.before).not.toBeNull();
          expect(s.after).toBeNull();
        }
      }
    });
  });

  describe("treeHooks.afterMove", () => {
    it("pushes a move snapshot", () => {
      const { treeHooks } = createUndoBridge(manager);
      const obj = makeObject({ parentId: objectId("new-parent") });
      treeHooks.afterMove?.(obj);

      expect(manager.historySize).toBe(1);
      expect(manager.undoLabel).toBe("Move task");
    });
  });

  describe("treeHooks.afterDuplicate", () => {
    it("pushes snapshots for each copy", () => {
      const { treeHooks } = createUndoBridge(manager);
      const original = makeObject({ id: objectId("orig") });
      const copy1 = makeObject({ id: objectId("copy-1"), name: "Copy 1" });
      const copy2 = makeObject({ id: objectId("copy-2"), name: "Copy 2" });

      treeHooks.afterDuplicate?.(original, [copy1, copy2]);

      expect(manager.historySize).toBe(1);
      expect(manager.undoLabel).toBe("Duplicate task");
      manager.undo();
      const snaps = applied[0]?.snapshots ?? [];
      expect(snaps).toHaveLength(2);
      for (const s of snaps) {
        if (s.kind === "object") {
          expect(s.before).toBeNull();
          expect(s.after).not.toBeNull();
        }
      }
    });
  });

  describe("treeHooks.afterUpdate", () => {
    it("pushes before/after snapshot", () => {
      const { treeHooks } = createUndoBridge(manager);
      const previous = makeObject({ name: "Old Name" });
      const updated = makeObject({ name: "New Name" });

      treeHooks.afterUpdate?.(updated, previous);

      expect(manager.historySize).toBe(1);
      expect(manager.undoLabel).toBe("Update task");
      manager.undo();
      const snap = applied[0]?.snapshots[0];
      if (snap?.kind === "object") {
        expect(snap.before?.name).toBe("Old Name");
        expect(snap.after?.name).toBe("New Name");
      }
    });
  });

  // ── Edge hooks ──────────────────────────────────────────────────────────

  describe("edgeHooks.afterAdd", () => {
    it("pushes a create edge snapshot", () => {
      const { edgeHooks } = createUndoBridge(manager);
      const edge = makeEdge();
      edgeHooks.afterAdd?.(edge);

      expect(manager.historySize).toBe(1);
      expect(manager.undoLabel).toBe("Create edge depends-on");
      manager.undo();
      const snap = applied[0]?.snapshots[0];
      expect(snap?.kind).toBe("edge");
      if (snap?.kind === "edge") {
        expect(snap.before).toBeNull();
        expect(snap.after?.id).toBe("edge-1");
      }
    });
  });

  describe("edgeHooks.afterRemove", () => {
    it("pushes a delete edge snapshot", () => {
      const { edgeHooks } = createUndoBridge(manager);
      const edge = makeEdge();
      edgeHooks.afterRemove?.(edge);

      expect(manager.historySize).toBe(1);
      expect(manager.undoLabel).toBe("Delete edge depends-on");
      manager.undo();
      const snap = applied[0]?.snapshots[0];
      if (snap?.kind === "edge") {
        expect(snap.before?.id).toBe("edge-1");
        expect(snap.after).toBeNull();
      }
    });
  });

  describe("edgeHooks.afterUpdate", () => {
    it("pushes before/after edge snapshot", () => {
      const { edgeHooks } = createUndoBridge(manager);
      const previous = makeEdge({ data: { weight: 1 } });
      const updated = makeEdge({ data: { weight: 5 } });

      edgeHooks.afterUpdate?.(updated, previous);

      expect(manager.historySize).toBe(1);
      expect(manager.undoLabel).toBe("Update edge depends-on");
      manager.undo();
      const snap = applied[0]?.snapshots[0];
      if (snap?.kind === "edge") {
        expect((snap.before?.data as Record<string, number>)["weight"]).toBe(1);
        expect((snap.after?.data as Record<string, number>)["weight"]).toBe(5);
      }
    });
  });

  // ── Snapshot isolation ────────────────────────────────────────────────────

  it("snapshots are deep copies (not references)", () => {
    const { treeHooks } = createUndoBridge(manager);
    const obj = makeObject({ data: { count: 0 } });
    treeHooks.afterAdd?.(obj);

    // Mutate the original — snapshot should be unaffected
    obj.data["count"] = 99;

    manager.undo();
    const snap = applied[0]?.snapshots[0];
    if (snap?.kind === "object") {
      expect((snap.after?.data as Record<string, number>)["count"]).toBe(0);
    }
  });

  it("undo+redo round-trip preserves all entries", () => {
    const { treeHooks, edgeHooks } = createUndoBridge(manager);
    treeHooks.afterAdd?.(makeObject());
    edgeHooks.afterAdd?.(makeEdge());

    expect(manager.historySize).toBe(2);
    expect(manager.canUndo).toBe(true);

    manager.undo();
    manager.undo();
    expect(manager.canUndo).toBe(false);
    expect(manager.canRedo).toBe(true);

    manager.redo();
    manager.redo();
    expect(manager.historySize).toBe(2);
    expect(manager.canRedo).toBe(false);
  });
});
