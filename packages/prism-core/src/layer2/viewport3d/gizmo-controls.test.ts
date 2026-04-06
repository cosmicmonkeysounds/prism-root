import { describe, it, expect, vi } from "vitest";
import { createGizmoController, snapValue, snapVec3, snapTransform } from "./gizmo-controls.js";
import { createSceneState } from "./scene-state.js";
import type { GizmoUndoAdapter } from "./gizmo-controls.js";
import type { GizmoState, Transform } from "./viewport3d-types.js";

describe("snapValue", () => {
  it("snaps to nearest step", () => {
    expect(snapValue(1.3, 0.5)).toBeCloseTo(1.5);
    expect(snapValue(1.2, 0.5)).toBeCloseTo(1.0);
    expect(snapValue(2.75, 1)).toBeCloseTo(3);
  });

  it("returns original for step <= 0", () => {
    expect(snapValue(1.3, 0)).toBe(1.3);
    expect(snapValue(1.3, -1)).toBe(1.3);
  });
});

describe("snapVec3", () => {
  it("snaps each component", () => {
    const result = snapVec3([1.3, 2.7, 3.1], 1);
    expect(result).toEqual([1, 3, 3]);
  });
});

describe("snapTransform", () => {
  it("passes through when snapping disabled", () => {
    const t: Transform = {
      position: [1.3, 2.7, 3.1],
      rotation: [0.1, 0.2, 0.3, "XYZ"],
      scale: [1.5, 1.5, 1.5],
    };
    const state: GizmoState = {
      mode: "translate",
      space: "world",
      selectedNodeIds: [],
      snapping: false,
      snapTranslate: 1,
      snapRotate: 15,
      snapScale: 0.1,
    };
    expect(snapTransform(t, state)).toBe(t);
  });

  it("snaps position, rotation, scale when enabled", () => {
    const t: Transform = {
      position: [1.3, 2.7, 3.1],
      rotation: [0.1, 0.2, 0.3, "XYZ"],
      scale: [1.55, 1.55, 1.55],
    };
    const state: GizmoState = {
      mode: "translate",
      space: "world",
      selectedNodeIds: [],
      snapping: true,
      snapTranslate: 1,
      snapRotate: 90,
      snapScale: 0.5,
    };
    const result = snapTransform(t, state);
    expect(result.position).toEqual([1, 3, 3]);
    expect(result.scale[0]).toBeCloseTo(1.5);
  });
});

describe("GizmoController", () => {
  function setup() {
    const ss = createSceneState();
    const undoEntries: Array<{ label: string; undo: () => void; redo: () => void }> = [];
    const undo: GizmoUndoAdapter = {
      record(entry) { undoEntries.push(entry); },
    };
    const ctrl = createGizmoController(ss, undo);
    return { ss, ctrl, undoEntries };
  }

  it("starts with default state", () => {
    const { ctrl } = setup();
    const s = ctrl.getState();
    expect(s.mode).toBe("translate");
    expect(s.space).toBe("world");
    expect(s.selectedNodeIds).toEqual([]);
  });

  it("changes mode", () => {
    const { ctrl } = setup();
    ctrl.setMode("rotate");
    expect(ctrl.getState().mode).toBe("rotate");
  });

  it("toggles mode cyclically", () => {
    const { ctrl } = setup();
    expect(ctrl.getState().mode).toBe("translate");
    ctrl.toggleMode();
    expect(ctrl.getState().mode).toBe("rotate");
    ctrl.toggleMode();
    expect(ctrl.getState().mode).toBe("scale");
    ctrl.toggleMode();
    expect(ctrl.getState().mode).toBe("translate");
  });

  it("changes space", () => {
    const { ctrl } = setup();
    ctrl.setSpace("local");
    expect(ctrl.getState().space).toBe("local");
  });

  it("manages selection", () => {
    const { ctrl } = setup();
    ctrl.select(["a", "b"]);
    expect(ctrl.getState().selectedNodeIds).toEqual(["a", "b"]);

    ctrl.addToSelection("c");
    expect(ctrl.getState().selectedNodeIds).toEqual(["a", "b", "c"]);

    ctrl.removeFromSelection("b");
    expect(ctrl.getState().selectedNodeIds).toEqual(["a", "c"]);

    ctrl.clearSelection();
    expect(ctrl.getState().selectedNodeIds).toEqual([]);
  });

  it("does not duplicate on addToSelection", () => {
    const { ctrl } = setup();
    ctrl.select(["a"]);
    ctrl.addToSelection("a");
    expect(ctrl.getState().selectedNodeIds).toEqual(["a"]);
  });

  it("sets snapping", () => {
    const { ctrl } = setup();
    ctrl.setSnapping(true);
    expect(ctrl.getState().snapping).toBe(true);
    ctrl.setSnapValues(2, 45, 0.25);
    expect(ctrl.getState().snapTranslate).toBe(2);
    expect(ctrl.getState().snapRotate).toBe(45);
    expect(ctrl.getState().snapScale).toBe(0.25);
  });

  it("records undo on commitTransform", () => {
    const { ss, ctrl, undoEntries } = setup();
    const id = ss.addNode({ kind: "mesh" });
    ctrl.select([id]);

    ctrl.beginTransform();
    ss.setTransform(id, {
      position: [5, 0, 0],
      rotation: [0, 0, 0, "XYZ"],
      scale: [1, 1, 1],
    });
    const events = ctrl.commitTransform();

    expect(events.length).toBe(1);
    expect(events[0]?.nodeId).toBe(id);
    expect(events[0]?.before.position).toEqual([0, 0, 0]);
    expect(events[0]?.after.position).toEqual([5, 0, 0]);

    expect(undoEntries.length).toBe(1);
    expect(undoEntries[0]?.label).toContain("translate");
  });

  it("undo restores previous transform", () => {
    const { ss, ctrl, undoEntries } = setup();
    const id = ss.addNode({ kind: "mesh" });
    ctrl.select([id]);

    ctrl.beginTransform();
    ss.setTransform(id, {
      position: [10, 0, 0],
      rotation: [0, 0, 0, "XYZ"],
      scale: [1, 1, 1],
    });
    ctrl.commitTransform();

    // Undo
    undoEntries[0]?.undo();
    const node = ss.getGraph().nodes.get(id);
    expect(node?.transform.position).toEqual([0, 0, 0]);
  });

  it("redo reapplies transform", () => {
    const { ss, ctrl, undoEntries } = setup();
    const id = ss.addNode({ kind: "mesh" });
    ctrl.select([id]);

    ctrl.beginTransform();
    ss.setTransform(id, {
      position: [10, 0, 0],
      rotation: [0, 0, 0, "XYZ"],
      scale: [1, 1, 1],
    });
    ctrl.commitTransform();

    undoEntries[0]?.undo();
    undoEntries[0]?.redo();
    const node = ss.getGraph().nodes.get(id);
    expect(node?.transform.position).toEqual([10, 0, 0]);
  });

  it("cancelTransform restores original", () => {
    const { ss, ctrl, undoEntries } = setup();
    const id = ss.addNode({ kind: "mesh" });
    ctrl.select([id]);

    ctrl.beginTransform();
    ss.setTransform(id, {
      position: [99, 99, 99],
      rotation: [0, 0, 0, "XYZ"],
      scale: [1, 1, 1],
    });
    ctrl.cancelTransform();

    const node = ss.getGraph().nodes.get(id);
    expect(node?.transform.position).toEqual([0, 0, 0]);
    expect(undoEntries.length).toBe(0);
  });

  it("notifies listeners on state change", () => {
    const { ctrl } = setup();
    const listener = vi.fn();
    ctrl.subscribe(listener);
    ctrl.setMode("scale");
    expect(listener).toHaveBeenCalledTimes(1);
    expect(listener.mock.calls[0]?.[0].mode).toBe("scale");
  });

  it("unsubscribe stops notifications", () => {
    const { ctrl } = setup();
    const listener = vi.fn();
    const unsub = ctrl.subscribe(listener);
    unsub();
    ctrl.setMode("rotate");
    expect(listener).not.toHaveBeenCalled();
  });
});
