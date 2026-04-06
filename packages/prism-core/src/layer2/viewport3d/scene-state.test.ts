import { describe, it, expect, vi } from "vitest";
import { createSceneState } from "./scene-state.js";
import type { Transform } from "./viewport3d-types.js";

describe("SceneState", () => {
  it("starts with an empty scene graph", () => {
    const ss = createSceneState();
    const g = ss.getGraph();
    expect(g.nodes.size).toBe(0);
    expect(g.materials.size).toBe(0);
    expect(g.rootIds.length).toBe(0);
  });

  // ---- Node CRUD ----

  it("adds a mesh node and returns its ID", () => {
    const ss = createSceneState();
    const id = ss.addNode({ kind: "mesh", name: "Cube" });
    expect(id).toBeTruthy();
    const g = ss.getGraph();
    expect(g.nodes.size).toBe(1);
    expect(g.nodes.get(id)?.name).toBe("Cube");
    expect(g.nodes.get(id)?.kind).toBe("mesh");
    expect(g.rootIds).toContain(id);
  });

  it("assigns default transform to new nodes", () => {
    const ss = createSceneState();
    const id = ss.addNode({ kind: "group" });
    const node = ss.getGraph().nodes.get(id);
    expect(node?.transform.position).toEqual([0, 0, 0]);
    expect(node?.transform.scale).toEqual([1, 1, 1]);
  });

  it("removes a node", () => {
    const ss = createSceneState();
    const id = ss.addNode({ kind: "mesh" });
    ss.removeNode(id);
    expect(ss.getGraph().nodes.size).toBe(0);
    expect(ss.getGraph().rootIds.length).toBe(0);
  });

  it("removes descendants when removing a parent", () => {
    const ss = createSceneState();
    const parent = ss.addNode({ kind: "group", name: "Parent" });
    const child = ss.addNode({ kind: "mesh", name: "Child", parentId: parent });
    ss.addNode({ kind: "mesh", name: "GC", parentId: child });

    ss.removeNode(parent);
    expect(ss.getGraph().nodes.size).toBe(0);
  });

  it("updates a node's properties", () => {
    const ss = createSceneState();
    const id = ss.addNode({ kind: "mesh", name: "A" });
    ss.updateNode(id, { name: "B", visible: false });
    const node = ss.getGraph().nodes.get(id);
    expect(node?.name).toBe("B");
    expect(node?.visible).toBe(false);
  });

  it("sets transform on a node", () => {
    const ss = createSceneState();
    const id = ss.addNode({ kind: "mesh" });
    const t: Transform = {
      position: [1, 2, 3],
      rotation: [0.1, 0.2, 0.3, "XYZ"],
      scale: [2, 2, 2],
    };
    ss.setTransform(id, t);
    const node = ss.getGraph().nodes.get(id);
    expect(node?.transform.position).toEqual([1, 2, 3]);
    expect(node?.transform.scale).toEqual([2, 2, 2]);
  });

  // ---- Hierarchy ----

  it("tracks parent-child relationships", () => {
    const ss = createSceneState();
    const parent = ss.addNode({ kind: "group" });
    const child = ss.addNode({ kind: "mesh", parentId: parent });
    expect(ss.getChildren(parent)).toContain(child);
    expect(ss.getGraph().nodes.get(child)?.parentId).toBe(parent);
  });

  it("reparents a node from root to child", () => {
    const ss = createSceneState();
    const a = ss.addNode({ kind: "group", name: "A" });
    const b = ss.addNode({ kind: "mesh", name: "B" });
    expect(ss.getGraph().rootIds).toContain(b);

    ss.reparent(b, a);
    const g = ss.getGraph();
    expect(g.nodes.get(b)?.parentId).toBe(a);
    expect(g.rootIds).not.toContain(b);
    expect(ss.getChildren(a)).toContain(b);
  });

  it("reparents a node to root", () => {
    const ss = createSceneState();
    const a = ss.addNode({ kind: "group" });
    const b = ss.addNode({ kind: "mesh", parentId: a });
    expect(ss.getGraph().rootIds).not.toContain(b);

    ss.reparent(b, null);
    expect(ss.getGraph().nodes.get(b)?.parentId).toBeNull();
    expect(ss.getGraph().rootIds).toContain(b);
  });

  it("getDescendants returns full subtree", () => {
    const ss = createSceneState();
    const root = ss.addNode({ kind: "group" });
    const c1 = ss.addNode({ kind: "mesh", parentId: root });
    const c2 = ss.addNode({ kind: "mesh", parentId: root });
    const gc = ss.addNode({ kind: "mesh", parentId: c1 });
    const descendants = ss.getDescendants(root);
    expect(descendants).toContain(c1);
    expect(descendants).toContain(c2);
    expect(descendants).toContain(gc);
    expect(descendants.length).toBe(3);
  });

  // ---- Materials ----

  it("adds and retrieves materials", () => {
    const ss = createSceneState();
    const id = ss.addMaterial({ color: "#ff0000" });
    const g = ss.getGraph();
    expect(g.materials.size).toBe(1);
    expect(g.materials.get(id)?.color).toBe("#ff0000");
    expect(g.materials.get(id)?.roughness).toBe(0.5);
  });

  it("updates a material", () => {
    const ss = createSceneState();
    const id = ss.addMaterial();
    ss.updateMaterial(id, { metalness: 1, roughness: 0.1 });
    const mat = ss.getGraph().materials.get(id);
    expect(mat?.metalness).toBe(1);
    expect(mat.roughness).toBe(0.1);
  });

  it("removes a material", () => {
    const ss = createSceneState();
    const id = ss.addMaterial();
    ss.removeMaterial(id);
    expect(ss.getGraph().materials.size).toBe(0);
  });

  // ---- Subscriptions ----

  it("notifies listeners on changes", () => {
    const ss = createSceneState();
    const listener = vi.fn();
    ss.subscribe(listener);
    ss.addNode({ kind: "mesh" });
    expect(listener).toHaveBeenCalledTimes(1);
    expect(listener.mock.calls[0]?.[0].nodes.size).toBe(1);
  });

  it("unsubscribe stops notifications", () => {
    const ss = createSceneState();
    const listener = vi.fn();
    const unsub = ss.subscribe(listener);
    unsub();
    ss.addNode({ kind: "mesh" });
    expect(listener).not.toHaveBeenCalled();
  });

  // ---- Loro sync ----

  it("exports and imports state between documents", () => {
    const ss1 = createSceneState();
    ss1.addNode({ kind: "mesh", name: "Synced" });
    ss1.addMaterial({ color: "#00ff00" });
    const data = ss1.exportState();

    const ss2 = createSceneState();
    ss2.importState(data);
    const g = ss2.getGraph();
    expect(g.nodes.size).toBe(1);
    expect([...g.nodes.values()][0]?.name).toBe("Synced");
    expect(g.materials.size).toBe(1);
  });

  // ---- Light & Camera nodes ----

  it("adds light nodes with params", () => {
    const ss = createSceneState();
    const id = ss.addNode({
      kind: "light-point",
      light: { color: "#ffffff", intensity: 2, castShadow: true },
    });
    const node = ss.getGraph().nodes.get(id);
    expect(node?.kind).toBe("light-point");
    expect(node?.light?.intensity).toBe(2);
  });

  it("adds camera nodes with params", () => {
    const ss = createSceneState();
    const id = ss.addNode({
      kind: "camera-perspective",
      camera: { fov: 75, near: 0.1, far: 1000 },
    });
    const node = ss.getGraph().nodes.get(id);
    expect(node?.camera?.fov).toBe(75);
  });
});
