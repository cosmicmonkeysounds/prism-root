import { describe, it, expect, beforeEach } from "vitest";
import { TreeModel } from "../object-model/tree-model.js";
import { EdgeModel } from "../object-model/edge-model.js";
import { UndoRedoManager } from "../undo/undo-manager.js";
import { objectId } from "../object-model/types.js";
import { createBatchTransaction } from "./batch-transaction.js";
import type { BatchTransaction } from "./batch-transaction.js";

// ── Helpers ───────────────────────────────────────────────────────────────────

let idCounter = 0;
const genId = () => `id-${++idCounter}`;
const noop = () => {};

function makeTree() {
  return new TreeModel({ generateId: genId });
}

function makeEdges() {
  return new EdgeModel({ generateId: genId });
}

function makeUndo() {
  return new UndoRedoManager(noop, { maxHistory: 50 });
}

// ── Tests ────────────────────────────────────────────────────────────────────

describe("BatchTransaction", () => {
  let tree: TreeModel;
  let edges: EdgeModel;
  let undo: UndoRedoManager;
  let tx: BatchTransaction;

  beforeEach(() => {
    idCounter = 0;
    tree = makeTree();
    edges = makeEdges();
    undo = makeUndo();
    tx = createBatchTransaction({ tree, edges, undo });
  });

  // ── add / size / ops ─��────────────────────────────────────────────────────

  describe("queuing", () => {
    it("starts empty", () => {
      expect(tx.size).toBe(0);
      expect(tx.ops).toEqual([]);
    });

    it("add enqueues one op", () => {
      tx.add({ kind: "create-object", draft: { type: "task", name: "A" } });
      expect(tx.size).toBe(1);
    });

    it("addAll enqueues multiple ops", () => {
      tx.addAll([
        { kind: "create-object", draft: { type: "task", name: "A" } },
        { kind: "create-object", draft: { type: "task", name: "B" } },
      ]);
      expect(tx.size).toBe(2);
    });

    it("clear resets queue", () => {
      tx.add({ kind: "create-object", draft: { type: "task", name: "A" } });
      tx.clear();
      expect(tx.size).toBe(0);
    });
  });

  // ── validate ──────────────────────────────────────────────────────────────

  describe("validate", () => {
    it("returns valid for well-formed ops", () => {
      tx.add({ kind: "create-object", draft: { type: "task", name: "A" } });
      const result = tx.validate();
      expect(result.valid).toBe(true);
      expect(result.errors).toHaveLength(0);
    });

    it("catches missing type in create-object", () => {
      tx.add({ kind: "create-object", draft: { type: "", name: "A" } });
      const result = tx.validate();
      expect(result.valid).toBe(false);
      expect(result.errors[0].reason).toContain("type");
    });

    it("catches missing name in create-object", () => {
      tx.add({ kind: "create-object", draft: { type: "task", name: "" } });
      const result = tx.validate();
      expect(result.valid).toBe(false);
    });

    it("catches missing id in update-object", () => {
      tx.add({ kind: "update-object", id: "", changes: { name: "X" } });
      const result = tx.validate();
      expect(result.valid).toBe(false);
    });

    it("catches edge op without EdgeModel", () => {
      const txNoEdge = createBatchTransaction({ tree });
      txNoEdge.add({
        kind: "create-edge",
        draft: {
          sourceId: objectId("a"),
          targetId: objectId("b"),
          relation: "dep",
          data: {},
        },
      });
      const result = txNoEdge.validate();
      expect(result.valid).toBe(false);
      expect(result.errors[0].reason).toContain("EdgeModel");
    });

    it("catches missing sourceId in create-edge", () => {
      tx.add({
        kind: "create-edge",
        draft: {
          sourceId: objectId(""),
          targetId: objectId("b"),
          relation: "dep",
          data: {},
        },
      });
      const result = tx.validate();
      expect(result.valid).toBe(false);
    });
  });

  // ── execute: create ───────────────────────────────────────────────────────

  describe("execute create-object", () => {
    it("creates objects in the tree", () => {
      tx.add({ kind: "create-object", draft: { type: "task", name: "A" } });
      tx.add({ kind: "create-object", draft: { type: "task", name: "B" } });
      const result = tx.execute();
      expect(result.executed).toBe(2);
      expect(result.created).toHaveLength(2);
      expect(tree.size).toBe(2);
    });

    it("creates objects with parentId and position", () => {
      const parent = tree.add({ type: "folder", name: "F" });
      tx.add({
        kind: "create-object",
        draft: { type: "task", name: "A" },
        parentId: parent.id,
        position: 0,
      });
      const result = tx.execute();
      expect(result.created[0].parentId).toBe(parent.id);
    });
  });

  // ── execute: update ───────────────────────────────────────────────────────

  describe("execute update-object", () => {
    it("updates an existing object", () => {
      const obj = tree.add({ type: "task", name: "A" });
      tx.add({ kind: "update-object", id: obj.id, changes: { name: "B" } });
      tx.execute();
      expect(tree.get(obj.id)?.name).toBe("B");
    });

    it("throws for missing object", () => {
      tx.add({
        kind: "update-object",
        id: "nonexistent",
        changes: { name: "X" },
      });
      expect(() => tx.execute()).toThrow("not found");
    });
  });

  // ── execute: delete ───────────────────────────────────────────────────────

  describe("execute delete-object", () => {
    it("removes object and descendants", () => {
      const parent = tree.add({ type: "folder", name: "F" });
      tree.add({ type: "task", name: "A" }, { parentId: parent.id });
      tx.add({ kind: "delete-object", id: parent.id });
      tx.execute();
      expect(tree.size).toBe(0);
    });
  });

  // ── execute: move ─────────────────────────────────────────────────────────

  describe("execute move-object", () => {
    it("moves object to new parent", () => {
      const folder = tree.add({ type: "folder", name: "F" });
      const task = tree.add({ type: "task", name: "A" });
      tx.add({ kind: "move-object", id: task.id, toParentId: folder.id });
      tx.execute();
      expect(tree.get(task.id)?.parentId).toBe(folder.id);
    });
  });

  // ── execute: edges ─���──────────────────────────────────────────────────────

  describe("execute edge operations", () => {
    it("creates an edge", () => {
      const a = tree.add({ type: "task", name: "A" });
      const b = tree.add({ type: "task", name: "B" });
      tx.add({
        kind: "create-edge",
        draft: {
          sourceId: a.id,
          targetId: b.id,
          relation: "depends-on",
          data: {},
        },
      });
      const result = tx.execute();
      expect(result.createdEdges).toHaveLength(1);
      expect(edges.getAll()).toHaveLength(1);
    });

    it("updates an edge", () => {
      const a = tree.add({ type: "task", name: "A" });
      const b = tree.add({ type: "task", name: "B" });
      const edge = edges.add({
        sourceId: a.id,
        targetId: b.id,
        relation: "depends-on",
        data: {},
      });
      tx.add({
        kind: "update-edge",
        id: edge.id,
        changes: { data: { weight: 5 } },
      });
      tx.execute();
      expect(edges.get(edge.id)?.data["weight"]).toBe(5);
    });

    it("deletes an edge", () => {
      const a = tree.add({ type: "task", name: "A" });
      const b = tree.add({ type: "task", name: "B" });
      const edge = edges.add({
        sourceId: a.id,
        targetId: b.id,
        relation: "dep",
        data: {},
      });
      tx.add({ kind: "delete-edge", id: edge.id });
      tx.execute();
      expect(edges.getAll()).toHaveLength(0);
    });
  });

  // ── undo integration ─────────────────────────────────────────────────────

  describe("undo integration", () => {
    it("pushes single undo entry for entire batch", () => {
      tx.add({ kind: "create-object", draft: { type: "task", name: "A" } });
      tx.add({ kind: "create-object", draft: { type: "task", name: "B" } });
      tx.execute({ description: "Bulk create" });
      expect(undo.history).toHaveLength(1);
      expect(undo.history[0].description).toBe("Bulk create");
      expect(undo.history[0].snapshots).toHaveLength(2);
    });

    it("does not push undo entry when no undo manager", () => {
      const txNoUndo = createBatchTransaction({ tree, edges });
      txNoUndo.add({ kind: "create-object", draft: { type: "task", name: "A" } });
      txNoUndo.execute();
      // No crash, just works without undo
      expect(tree.size).toBe(1);
    });
  });

  // ── progress callback ─────────────────────────────────────────────────────

  describe("progress callback", () => {
    it("calls onProgress for each op", () => {
      const calls: number[] = [];
      tx.add({ kind: "create-object", draft: { type: "task", name: "A" } });
      tx.add({ kind: "create-object", draft: { type: "task", name: "B" } });
      tx.execute({
        onProgress: (p) => calls.push(p.current),
      });
      expect(calls).toEqual([0, 1]);
    });
  });

  // ── rollback on failure ─���─────────────────────────────────────────────────

  describe("rollback on failure", () => {
    it("rolls back all mutations when a later op fails", () => {
      tx.add({ kind: "create-object", draft: { type: "task", name: "A" } });
      tx.add({
        kind: "update-object",
        id: "nonexistent",
        changes: { name: "X" },
      });
      expect(() => tx.execute()).toThrow();
      // The created object should have been rolled back
      expect(tree.size).toBe(0);
    });
  });

  // ── mixed operations ──────────────────────────────────────────────────────

  describe("mixed operations", () => {
    it("handles create + update + move in one batch", () => {
      const folder = tree.add({ type: "folder", name: "F" });
      tx.add({ kind: "create-object", draft: { type: "task", name: "A" } });
      // We execute in two batches since we need the ID
      const result1 = tx.execute({ description: "Create" });
      const taskId = result1.created[0].id;

      const tx2 = createBatchTransaction({ tree, edges, undo });
      tx2.add({ kind: "update-object", id: taskId, changes: { status: "done" } });
      tx2.add({ kind: "move-object", id: taskId, toParentId: folder.id });
      tx2.execute({ description: "Update and move" });

      expect(tree.get(taskId)?.status).toBe("done");
      expect(tree.get(taskId)?.parentId).toBe(folder.id);
      expect(undo.history).toHaveLength(2);
    });
  });
});
