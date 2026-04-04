import { describe, it, expect, beforeEach } from "vitest";
import { TreeModel, TreeModelError } from "./tree-model.js";
import { ObjectRegistry } from "./registry.js";
import type { CategoryRule, EntityDef } from "./types.js";

const RULES: CategoryRule[] = [
  { category: "container", canParent: ["container", "leaf"], canBeRoot: true },
  { category: "leaf", canParent: [], canBeRoot: true },
];

const DEFS: EntityDef[] = [
  { type: "folder", category: "container", label: "Folder" },
  { type: "doc", category: "leaf", label: "Document" },
];

describe("TreeModel", () => {
  let counter: number;
  let registry: ObjectRegistry;
  let tree: TreeModel;

  beforeEach(() => {
    counter = 0;
    registry = new ObjectRegistry(RULES);
    registry.registerAll(DEFS);
    tree = new TreeModel({
      registry,
      generateId: () => `id-${counter++}`,
    });
  });

  // ── Add ──────────────────────────────────────────────────────────────────────

  it("adds root-level objects", () => {
    const obj = tree.add({ type: "folder", name: "Root" });
    expect(obj.id).toBe("id-0");
    expect(obj.parentId).toBeNull();
    expect(obj.position).toBe(0);
    expect(tree.size).toBe(1);
  });

  it("adds child objects with correct position", () => {
    const parent = tree.add({ type: "folder", name: "Parent" });
    const child1 = tree.add({ type: "doc", name: "A" }, { parentId: parent.id });
    const child2 = tree.add({ type: "doc", name: "B" }, { parentId: parent.id });

    expect(child1.position).toBe(0);
    expect(child2.position).toBe(1);
  });

  it("validates containment on add", () => {
    const doc = tree.add({ type: "doc", name: "Doc" });
    expect(() =>
      tree.add({ type: "folder", name: "Nested" }, { parentId: doc.id }),
    ).toThrow(TreeModelError);
  });

  it("throws NOT_FOUND for missing parent", () => {
    expect(() =>
      tree.add({ type: "doc", name: "Orphan" }, { parentId: "nope" }),
    ).toThrow(TreeModelError);
  });

  // ── Remove ───────────────────────────────────────────────────────────────────

  it("removes object and descendants", () => {
    const parent = tree.add({ type: "folder", name: "Parent" });
    tree.add({ type: "doc", name: "Child" }, { parentId: parent.id });

    const result = tree.remove(parent.id);
    expect(result).not.toBeNull();
    expect(result?.removed.id).toBe(parent.id);
    expect(result?.descendants.length).toBe(1);
    expect(tree.size).toBe(0);
  });

  it("returns null for missing id", () => {
    expect(tree.remove("nope")).toBeNull();
  });

  it("compacts sibling positions after removal", () => {
    tree.add({ type: "folder", name: "A" });
    const b = tree.add({ type: "folder", name: "B" });
    tree.add({ type: "folder", name: "C" });

    tree.remove(b.id);

    const roots = tree.getChildren(null);
    expect(roots.map((r) => r.name)).toEqual(["A", "C"]);
    expect(roots[0]?.position).toBe(0);
    expect(roots[1]?.position).toBe(1);
  });

  // ── Move ─────────────────────────────────────────────────────────────────────

  it("moves object to a new parent", () => {
    const f1 = tree.add({ type: "folder", name: "F1" });
    const f2 = tree.add({ type: "folder", name: "F2" });
    const doc = tree.add({ type: "doc", name: "Doc" }, { parentId: f1.id });

    const moved = tree.move(doc.id, f2.id);
    expect(moved.parentId).toBe(f2.id);
    expect(tree.getChildren(f1.id).length).toBe(0);
    expect(tree.getChildren(f2.id).length).toBe(1);
  });

  it("prevents circular references", () => {
    const parent = tree.add({ type: "folder", name: "Parent" });
    const child = tree.add(
      { type: "folder", name: "Child" },
      { parentId: parent.id },
    );

    expect(() => tree.move(parent.id, child.id)).toThrow(TreeModelError);
  });

  it("prevents moving into self", () => {
    const obj = tree.add({ type: "folder", name: "Self" });
    expect(() => tree.move(obj.id, obj.id)).toThrow(TreeModelError);
  });

  it("validates containment on move", () => {
    const doc = tree.add({ type: "doc", name: "Doc" });
    const folder = tree.add({ type: "folder", name: "F" });

    // doc (leaf) can't parent folder (container)
    expect(() => tree.move(folder.id, doc.id)).toThrow(TreeModelError);
  });

  // ── Reorder ──────────────────────────────────────────────────────────────────

  it("reorders within same parent", () => {
    tree.add({ type: "folder", name: "A" });
    const b = tree.add({ type: "folder", name: "B" });
    tree.add({ type: "folder", name: "C" });

    const result = tree.reorder(b.id, 0);
    expect(result.map((r) => r.name)).toEqual(["B", "A", "C"]);
  });

  // ── Duplicate ────────────────────────────────────────────────────────────────

  it("duplicates a single object", () => {
    const obj = tree.add({ type: "folder", name: "Original" });
    const copies = tree.duplicate(obj.id);

    expect(copies.length).toBe(1);
    expect(copies[0]?.name).toBe("Original");
    expect(copies[0]?.id).not.toBe(obj.id);
    expect(tree.size).toBe(2);
  });

  it("deep duplicates with descendants", () => {
    const parent = tree.add({ type: "folder", name: "Parent" });
    tree.add({ type: "doc", name: "Child" }, { parentId: parent.id });

    const copies = tree.duplicate(parent.id, { deep: true });
    expect(copies.length).toBe(2);
    expect(tree.size).toBe(4);
  });

  // ── Update ───────────────────────────────────────────────────────────────────

  it("updates shell fields", () => {
    const obj = tree.add({ type: "folder", name: "Old" });
    const updated = tree.update(obj.id, { name: "New" });

    expect(updated.name).toBe("New");
    expect(updated.id).toBe(obj.id);
    expect(updated.type).toBe("folder"); // immutable
  });

  // ── Query ────────────────────────────────────────────────────────────────────

  it("getChildren returns sorted by position", () => {
    const parent = tree.add({ type: "folder", name: "P" });
    tree.add({ type: "doc", name: "B" }, { parentId: parent.id });
    tree.add({ type: "doc", name: "A" }, { parentId: parent.id, position: 0 });

    const children = tree.getChildren(parent.id);
    expect(children.map((c) => c.name)).toEqual(["A", "B"]);
  });

  it("getDescendants returns depth-first", () => {
    const root = tree.add({ type: "folder", name: "Root" });
    const sub = tree.add(
      { type: "folder", name: "Sub" },
      { parentId: root.id },
    );
    tree.add({ type: "doc", name: "Leaf" }, { parentId: sub.id });

    const descs = tree.getDescendants(root.id);
    expect(descs.map((d) => d.name)).toEqual(["Sub", "Leaf"]);
  });

  it("getAncestors returns closest first", () => {
    const root = tree.add({ type: "folder", name: "Root" });
    const sub = tree.add(
      { type: "folder", name: "Sub" },
      { parentId: root.id },
    );
    const leaf = tree.add({ type: "doc", name: "Leaf" }, { parentId: sub.id });

    const ancestors = tree.getAncestors(leaf.id);
    expect(ancestors.map((a) => a.name)).toEqual(["Sub", "Root"]);
  });

  it("buildTree creates hierarchy", () => {
    const root = tree.add({ type: "folder", name: "Root" });
    tree.add({ type: "doc", name: "Child" }, { parentId: root.id });

    const nodes = tree.buildTree();
    expect(nodes.length).toBe(1);
    expect(nodes[0]?.children.length).toBe(1);
  });

  // ── Events ───────────────────────────────────────────────────────────────────

  it("fires events on mutations", () => {
    const events: string[] = [];
    tree.on((e) => events.push(e.kind));

    tree.add({ type: "folder", name: "Test" });
    expect(events).toEqual(["add", "change"]);
  });

  it("unsubscribe works", () => {
    const events: string[] = [];
    const unsub = tree.on((e) => events.push(e.kind));
    unsub();

    tree.add({ type: "folder", name: "Test" });
    expect(events.length).toBe(0);
  });

  // ── Serialization ────────────────────────────────────────────────────────────

  it("round-trips through JSON", () => {
    tree.add({ type: "folder", name: "A" });
    tree.add({ type: "doc", name: "B" });

    const json = tree.toJSON();
    const restored = TreeModel.fromJSON(json, { registry });
    expect(restored.size).toBe(2);
    expect(json[0] && restored.get(json[0].id)?.name).toBe("A");
  });
});
