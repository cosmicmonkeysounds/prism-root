import { describe, it, expect, beforeEach, vi } from "vitest";
import { UndoRedoManager } from "./undo-manager.js";
import type { ObjectSnapshot, UndoApplier } from "./undo-types.js";
import type { GraphObject, ObjectEdge, ObjectId, EdgeId } from "@prism/core/object-model";

// ── Helpers ─────────────────────────────────────────────────────────────────────

function makeObject(overrides: Partial<GraphObject> = {}): GraphObject {
  return {
    id: "obj-1" as ObjectId,
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
    id: "edge-1" as EdgeId,
    sourceId: "obj-1" as ObjectId,
    targetId: "obj-2" as ObjectId,
    relation: "depends-on",
    createdAt: "2026-01-01T00:00:00Z",
    data: {},
    ...overrides,
  };
}

describe("UndoRedoManager", () => {
  let applier: UndoApplier;
  let appliedCalls: Array<{ snapshots: ObjectSnapshot[]; direction: "undo" | "redo" }>;
  let manager: UndoRedoManager;

  beforeEach(() => {
    appliedCalls = [];
    applier = (snapshots, direction) => {
      appliedCalls.push({ snapshots, direction });
    };
    manager = new UndoRedoManager(applier);
  });

  // ── push ──────────────────────────────────────────────────────────────────

  it("push adds to undo stack", () => {
    const obj = makeObject();
    manager.push("Create task", [{ kind: "object", before: null, after: obj }]);
    expect(manager.canUndo).toBe(true);
    expect(manager.undoLabel).toBe("Create task");
    expect(manager.historySize).toBe(1);
  });

  it("push ignores empty snapshots", () => {
    manager.push("Empty", []);
    expect(manager.canUndo).toBe(false);
    expect(manager.historySize).toBe(0);
  });

  it("push clears redo stack", () => {
    const obj = makeObject();
    manager.push("A", [{ kind: "object", before: null, after: obj }]);
    manager.undo();
    expect(manager.canRedo).toBe(true);

    manager.push("B", [{ kind: "object", before: null, after: obj }]);
    expect(manager.canRedo).toBe(false);
  });

  // ── undo ──────────────────────────────────────────────────────────────────

  it("undo calls applier with 'undo' direction", () => {
    const obj = makeObject();
    const snap: ObjectSnapshot = { kind: "object", before: null, after: obj };
    manager.push("Create", [snap]);
    manager.undo();

    expect(appliedCalls.length).toBe(1);
    expect(appliedCalls[0].direction).toBe("undo");
    expect(appliedCalls[0].snapshots).toEqual([snap]);
  });

  it("undo moves entry to redo stack", () => {
    const obj = makeObject();
    manager.push("Create", [{ kind: "object", before: null, after: obj }]);
    manager.undo();
    expect(manager.canUndo).toBe(false);
    expect(manager.canRedo).toBe(true);
    expect(manager.redoLabel).toBe("Create");
  });

  it("undo with empty stack is a no-op", () => {
    manager.undo();
    expect(appliedCalls.length).toBe(0);
  });

  // ── redo ──────────────────────────────────────────────────────────────────

  it("redo calls applier with 'redo' direction", () => {
    const obj = makeObject();
    const snap: ObjectSnapshot = { kind: "object", before: null, after: obj };
    manager.push("Create", [snap]);
    manager.undo();
    manager.redo();

    expect(appliedCalls.length).toBe(2);
    expect(appliedCalls[1].direction).toBe("redo");
  });

  it("redo moves entry back to undo stack", () => {
    const obj = makeObject();
    manager.push("Create", [{ kind: "object", before: null, after: obj }]);
    manager.undo();
    manager.redo();
    expect(manager.canUndo).toBe(true);
    expect(manager.canRedo).toBe(false);
  });

  it("redo with empty stack is a no-op", () => {
    manager.redo();
    expect(appliedCalls.length).toBe(0);
  });

  // ── multiple undo/redo ────────────────────────────────────────────────────

  it("multiple undo/redo preserves order", () => {
    const obj1 = makeObject({ id: "obj-1" as ObjectId, name: "Task 1" });
    const obj2 = makeObject({ id: "obj-2" as ObjectId, name: "Task 2" });

    manager.push("Create 1", [{ kind: "object", before: null, after: obj1 }]);
    manager.push("Create 2", [{ kind: "object", before: null, after: obj2 }]);

    expect(manager.undoLabel).toBe("Create 2");
    manager.undo();
    expect(manager.undoLabel).toBe("Create 1");
    manager.undo();
    expect(manager.canUndo).toBe(false);

    manager.redo();
    expect(manager.undoLabel).toBe("Create 1");
    manager.redo();
    expect(manager.undoLabel).toBe("Create 2");
  });

  // ── merge ─────────────────────────────────────────────────────────────────

  it("merge appends snapshots to last entry", () => {
    const obj = makeObject();
    const edge = makeEdge();
    manager.push("Create", [{ kind: "object", before: null, after: obj }]);
    manager.merge([{ kind: "edge", before: null, after: edge }]);

    expect(manager.historySize).toBe(1);
    expect(manager.history[0].snapshots.length).toBe(2);
  });

  it("merge is a no-op when stack is empty", () => {
    manager.merge([{ kind: "object", before: null, after: makeObject() }]);
    expect(manager.historySize).toBe(0);
  });

  // ── clear ─────────────────────────────────────────────────────────────────

  it("clear empties both stacks", () => {
    const obj = makeObject();
    manager.push("A", [{ kind: "object", before: null, after: obj }]);
    manager.push("B", [{ kind: "object", before: null, after: obj }]);
    manager.undo();

    manager.clear();
    expect(manager.canUndo).toBe(false);
    expect(manager.canRedo).toBe(false);
    expect(manager.historySize).toBe(0);
    expect(manager.futureSize).toBe(0);
  });

  // ── maxHistory ────────────────────────────────────────────────────────────

  it("respects maxHistory limit", () => {
    const limited = new UndoRedoManager(applier, { maxHistory: 3 });
    const obj = makeObject();
    for (let i = 0; i < 5; i++) {
      limited.push(`Action ${i}`, [{ kind: "object", before: null, after: obj }]);
    }
    expect(limited.historySize).toBe(3);
    expect(limited.undoLabel).toBe("Action 4");
  });

  // ── labels ────────────────────────────────────────────────────────────────

  it("returns null labels when stacks are empty", () => {
    expect(manager.undoLabel).toBeNull();
    expect(manager.redoLabel).toBeNull();
  });

  // ── subscribe ─────────────────────────────────────────────────────────────

  it("subscribe notifies on push", () => {
    const fn = vi.fn();
    manager.subscribe(fn);
    manager.push("A", [{ kind: "object", before: null, after: makeObject() }]);
    expect(fn).toHaveBeenCalledTimes(1);
  });

  it("subscribe notifies on undo/redo", () => {
    const fn = vi.fn();
    manager.push("A", [{ kind: "object", before: null, after: makeObject() }]);
    manager.subscribe(fn);
    manager.undo();
    manager.redo();
    expect(fn).toHaveBeenCalledTimes(2);
  });

  it("subscribe notifies on clear", () => {
    const fn = vi.fn();
    manager.subscribe(fn);
    manager.clear();
    expect(fn).toHaveBeenCalledTimes(1);
  });

  it("subscribe notifies on merge", () => {
    const fn = vi.fn();
    manager.push("A", [{ kind: "object", before: null, after: makeObject() }]);
    manager.subscribe(fn);
    manager.merge([{ kind: "edge", before: null, after: makeEdge() }]);
    expect(fn).toHaveBeenCalledTimes(1);
  });

  it("unsubscribe stops notifications", () => {
    const fn = vi.fn();
    const unsub = manager.subscribe(fn);
    unsub();
    manager.push("A", [{ kind: "object", before: null, after: makeObject() }]);
    expect(fn).not.toHaveBeenCalled();
  });

  // ── edge snapshots ────────────────────────────────────────────────────────

  it("handles edge snapshots", () => {
    const edge = makeEdge();
    manager.push("Create edge", [{ kind: "edge", before: null, after: edge }]);
    manager.undo();

    expect(appliedCalls[0]?.snapshots[0]?.kind).toBe("edge");
  });

  // ── batch (multiple snapshots per entry) ──────────────────────────────────

  it("batch entry undoes all snapshots together", () => {
    const obj = makeObject();
    const edge = makeEdge();
    manager.push("Move to folder", [
      { kind: "object", before: obj, after: { ...obj, parentId: "folder" as ObjectId } },
      { kind: "edge", before: null, after: edge },
    ]);
    manager.undo();

    expect(appliedCalls[0].snapshots.length).toBe(2);
  });
});
