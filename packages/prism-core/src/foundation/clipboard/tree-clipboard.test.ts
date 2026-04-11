import { describe, it, expect, beforeEach } from "vitest";
import { TreeModel } from "@prism/core/object-model";
import { EdgeModel } from "@prism/core/object-model";
import { UndoRedoManager } from "@prism/core/undo";
import { createTreeClipboard } from "./tree-clipboard.js";
import type { TreeClipboard } from "./tree-clipboard.js";

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

describe("TreeClipboard", () => {
  let tree: TreeModel;
  let edges: EdgeModel;
  let undo: UndoRedoManager;
  let clipboard: TreeClipboard;

  beforeEach(() => {
    idCounter = 0;
    tree = makeTree();
    edges = makeEdges();
    undo = makeUndo();
    clipboard = createTreeClipboard({ tree, edges, undo, generateId: genId });
  });

  // ── empty state ───────────────────────────────────────────────────────────

  describe("empty state", () => {
    it("starts with no content", () => {
      expect(clipboard.hasContent).toBe(false);
      expect(clipboard.entry).toBeNull();
    });

    it("paste returns null when empty", () => {
      expect(clipboard.paste()).toBeNull();
    });
  });

  // ── copy ──────────────────────────────────────────────────────────────────

  describe("copy", () => {
    it("copies a single object", () => {
      tree.add({ type: "task", name: "A" });
      clipboard.copy(["id-1"]);
      expect(clipboard.hasContent).toBe(true);
      const entry = clipboard.entry;
      expect(entry).not.toBeNull();
      expect(entry?.mode).toBe("copy");
      expect(entry?.subtrees).toHaveLength(1);
    });

    it("copies multiple objects", () => {
      tree.add({ type: "task", name: "A" });
      tree.add({ type: "task", name: "B" });
      clipboard.copy(["id-1", "id-2"]);
      expect(clipboard.entry?.subtrees).toHaveLength(2);
    });

    it("copies object with descendants", () => {
      const parent = tree.add({ type: "folder", name: "F" });
      tree.add({ type: "task", name: "A" }, { parentId: parent.id });
      tree.add({ type: "task", name: "B" }, { parentId: parent.id });
      clipboard.copy([parent.id]);
      expect(clipboard.entry?.subtrees[0].descendants).toHaveLength(2);
    });

    it("skips nonexistent objects", () => {
      tree.add({ type: "task", name: "A" });
      clipboard.copy(["id-1", "nonexistent"]);
      expect(clipboard.entry?.subtrees).toHaveLength(1);
    });

    it("captures internal edges", () => {
      const a = tree.add({ type: "task", name: "A" });
      const b = tree.add({ type: "task", name: "B" });
      const parent = tree.add({ type: "folder", name: "F" });
      tree.move(a.id, parent.id);
      tree.move(b.id, parent.id);
      edges.add({
        sourceId: a.id,
        targetId: b.id,
        relation: "depends-on",
        data: {},
      });
      clipboard.copy([parent.id]);
      expect(clipboard.entry?.subtrees[0].internalEdges).toHaveLength(1);
    });
  });

  // ── paste (copy mode) ─────────────────────────────────────────────────────

  describe("paste (copy)", () => {
    it("creates new objects with fresh IDs", () => {
      tree.add({ type: "task", name: "A" });
      clipboard.copy(["id-1"]);
      const result = clipboard.paste();
      expect(result).not.toBeNull();
      expect(result?.created).toHaveLength(1);
      expect(result?.created[0].id).not.toBe("id-1");
      expect(result?.created[0].name).toBe("A");
      expect(tree.size).toBe(2); // original + copy
    });

    it("pastes under specified parent", () => {
      const folder = tree.add({ type: "folder", name: "F" });
      tree.add({ type: "task", name: "A" });
      clipboard.copy(["id-2"]);
      const result = clipboard.paste({ parentId: folder.id });
      expect(result?.created[0].parentId).toBe(folder.id);
    });

    it("deep copies subtrees preserving hierarchy", () => {
      const parent = tree.add({ type: "folder", name: "F" });
      tree.add({ type: "task", name: "A" }, { parentId: parent.id });
      tree.add({ type: "task", name: "B" }, { parentId: parent.id });
      clipboard.copy([parent.id]);
      const result = clipboard.paste();
      expect(result).not.toBeNull();
      expect(result?.created).toHaveLength(3); // folder + 2 children
      // The children should be under the new folder
      const newFolder = result?.created[0];
      expect(newFolder).toBeDefined();
      const newChildren = tree.getChildren(newFolder?.id ?? "");
      expect(newChildren).toHaveLength(2);
    });

    it("remaps internal edges", () => {
      const a = tree.add({ type: "task", name: "A" });
      const b = tree.add({ type: "task", name: "B" });
      const parent = tree.add({ type: "folder", name: "F" });
      tree.move(a.id, parent.id);
      tree.move(b.id, parent.id);
      edges.add({
        sourceId: a.id,
        targetId: b.id,
        relation: "dep",
        data: {},
      });
      clipboard.copy([parent.id]);
      const result = clipboard.paste();
      expect(result).not.toBeNull();
      expect(result?.createdEdges).toHaveLength(1);
      const newEdge = result?.createdEdges[0];
      expect(newEdge).toBeDefined();
      expect(newEdge?.sourceId).not.toBe(a.id);
      expect(newEdge?.targetId).not.toBe(b.id);
      // Both endpoints should be in the new subtree
      const newIds = new Set(result?.created.map((o) => o.id as string));
      expect(newIds.has(newEdge?.sourceId as string)).toBe(true);
      expect(newIds.has(newEdge?.targetId as string)).toBe(true);
    });

    it("provides idMap from old to new IDs", () => {
      tree.add({ type: "task", name: "A" });
      clipboard.copy(["id-1"]);
      const result = clipboard.paste();
      expect(result).not.toBeNull();
      expect(result?.idMap.size).toBe(1);
      expect(result?.idMap.has("id-1")).toBe(true);
      expect(result?.idMap.get("id-1")).toBe(result?.created[0].id);
    });

    it("allows multiple pastes from same copy", () => {
      tree.add({ type: "task", name: "A" });
      clipboard.copy(["id-1"]);
      clipboard.paste();
      clipboard.paste();
      expect(tree.size).toBe(3); // original + 2 copies
    });

    it("pastes at specified position", () => {
      tree.add({ type: "task", name: "Existing" });
      tree.add({ type: "task", name: "Source" });
      clipboard.copy(["id-2"]);
      clipboard.paste({ position: 0 });
      const roots = tree.getChildren(null);
      expect(roots[0].name).toBe("Source");
    });
  });

  // ── cut ───────────────────────────────────────────────────────────────────

  describe("cut", () => {
    it("sets mode to cut", () => {
      tree.add({ type: "task", name: "A" });
      clipboard.cut(["id-1"]);
      expect(clipboard.entry?.mode).toBe("cut");
    });

    it("deletes source objects on paste", () => {
      tree.add({ type: "task", name: "A" });
      clipboard.cut(["id-1"]);
      const result = clipboard.paste();
      expect(result?.created).toHaveLength(1);
      expect(tree.has("id-1")).toBe(false); // original deleted
      expect(tree.size).toBe(1); // only the paste remains
    });

    it("clears clipboard after cut-paste (one-time)", () => {
      tree.add({ type: "task", name: "A" });
      clipboard.cut(["id-1"]);
      clipboard.paste();
      expect(clipboard.hasContent).toBe(false);
      expect(clipboard.paste()).toBeNull();
    });
  });

  // ── undo integration ─────────────────────────────────────────────────────

  describe("undo integration", () => {
    it("pushes single undo entry for paste", () => {
      tree.add({ type: "task", name: "A" });
      clipboard.copy(["id-1"]);
      clipboard.paste();
      expect(undo.history).toHaveLength(1);
      expect(undo.history[0].description).toBe("Paste");
    });

    it("includes cut deletions in the same undo entry", () => {
      tree.add({ type: "task", name: "A" });
      clipboard.cut(["id-1"]);
      clipboard.paste();
      expect(undo.history).toHaveLength(1);
      // Should have create + delete snapshots
      expect(undo.history[0].snapshots.length).toBeGreaterThanOrEqual(2);
    });
  });

  // ── clear ─────────────────────────────────────────────────────────────────

  describe("clear", () => {
    it("empties the clipboard", () => {
      tree.add({ type: "task", name: "A" });
      clipboard.copy(["id-1"]);
      clipboard.clear();
      expect(clipboard.hasContent).toBe(false);
      expect(clipboard.paste()).toBeNull();
    });
  });

  // ── clipboard without edges ───────────────────────────────────────────────

  describe("without EdgeModel", () => {
    it("works without edges", () => {
      const cb = createTreeClipboard({ tree, generateId: genId });
      tree.add({ type: "task", name: "A" });
      cb.copy(["id-1"]);
      const result = cb.paste();
      expect(result?.created).toHaveLength(1);
      expect(result?.createdEdges).toHaveLength(0);
    });
  });
});
