/**
 * Gizmo control state management with undo integration.
 *
 * Manages translate/rotate/scale gizmo mode, selection, snapping,
 * and records transform changes as undoable operations via the
 * Layer 1 UndoRedoManager.
 */

import type {
  GizmoMode,
  GizmoSpace,
  GizmoState,
  GizmoTransformEvent,
  Transform,
  Vec3,
} from "./viewport3d-types.js";
import { DEFAULT_GIZMO_STATE } from "./viewport3d-types.js";
import type { SceneState } from "./scene-state.js";

// ---------------------------------------------------------------------------
// Snapping helpers
// ---------------------------------------------------------------------------

export function snapValue(value: number, step: number): number {
  if (step <= 0) return value;
  return Math.round(value / step) * step;
}

export function snapVec3(v: Vec3, step: number): Vec3 {
  return [snapValue(v[0], step), snapValue(v[1], step), snapValue(v[2], step)];
}

export function snapTransform(
  transform: Transform,
  state: GizmoState,
): Transform {
  if (!state.snapping) return transform;

  return {
    position: snapVec3(transform.position, state.snapTranslate),
    rotation: [
      snapValue(transform.rotation[0], state.snapRotate * (Math.PI / 180)),
      snapValue(transform.rotation[1], state.snapRotate * (Math.PI / 180)),
      snapValue(transform.rotation[2], state.snapRotate * (Math.PI / 180)),
      transform.rotation[3],
    ],
    scale: snapVec3(transform.scale, state.snapScale),
  };
}

// ---------------------------------------------------------------------------
// Undo adapter interface (duck-typed to avoid hard dep on UndoRedoManager)
// ---------------------------------------------------------------------------

export type GizmoUndoAdapter = {
  record(entry: {
    label: string;
    undo: () => void;
    redo: () => void;
  }): void;
};

// ---------------------------------------------------------------------------
// GizmoController
// ---------------------------------------------------------------------------

export type GizmoListener = (state: GizmoState) => void;

export type GizmoController = {
  getState(): GizmoState;
  setMode(mode: GizmoMode): void;
  setSpace(space: GizmoSpace): void;
  setSnapping(enabled: boolean): void;
  setSnapValues(translate?: number, rotate?: number, scale?: number): void;
  select(nodeIds: readonly string[]): void;
  addToSelection(nodeId: string): void;
  removeFromSelection(nodeId: string): void;
  clearSelection(): void;
  toggleMode(): void;

  /**
   * Begin a gizmo drag.  Call commitTransform() when the drag ends.
   * The before-state is captured here; the after-state at commit time.
   */
  beginTransform(): void;

  /**
   * Commit the current transform, recording an undo entry.
   * Returns the transform events for each selected node.
   */
  commitTransform(): readonly GizmoTransformEvent[];

  /** Cancel an in-progress transform — restores the before-state. */
  cancelTransform(): void;

  subscribe(listener: GizmoListener): () => void;
};

export function createGizmoController(
  sceneState: SceneState,
  undoAdapter?: GizmoUndoAdapter,
): GizmoController {
  let state: GizmoState = { ...DEFAULT_GIZMO_STATE };
  const listeners = new Set<GizmoListener>();
  let dragSnapshots: Map<string, Transform> | null = null;

  function notify(): void {
    for (const listener of listeners) listener(state);
  }

  function getSelectedTransforms(): Map<string, Transform> {
    const graph = sceneState.getGraph();
    const result = new Map<string, Transform>();
    for (const id of state.selectedNodeIds) {
      const node = graph.nodes.get(id);
      if (node) result.set(id, node.transform);
    }
    return result;
  }

  const MODE_ORDER: readonly GizmoMode[] = ["translate", "rotate", "scale"];

  return {
    getState() { return state; },

    setMode(mode) {
      state = { ...state, mode };
      notify();
    },

    setSpace(space) {
      state = { ...state, space };
      notify();
    },

    setSnapping(enabled) {
      state = { ...state, snapping: enabled };
      notify();
    },

    setSnapValues(translate, rotate, scale) {
      state = {
        ...state,
        snapTranslate: translate ?? state.snapTranslate,
        snapRotate: rotate ?? state.snapRotate,
        snapScale: scale ?? state.snapScale,
      };
      notify();
    },

    select(nodeIds) {
      state = { ...state, selectedNodeIds: [...nodeIds] };
      notify();
    },

    addToSelection(nodeId) {
      if (state.selectedNodeIds.includes(nodeId)) return;
      state = { ...state, selectedNodeIds: [...state.selectedNodeIds, nodeId] };
      notify();
    },

    removeFromSelection(nodeId) {
      state = {
        ...state,
        selectedNodeIds: state.selectedNodeIds.filter((id) => id !== nodeId),
      };
      notify();
    },

    clearSelection() {
      state = { ...state, selectedNodeIds: [] };
      notify();
    },

    toggleMode() {
      const idx = MODE_ORDER.indexOf(state.mode);
      const next = MODE_ORDER[(idx + 1) % MODE_ORDER.length] ?? "translate";
      state = { ...state, mode: next };
      notify();
    },

    beginTransform() {
      dragSnapshots = getSelectedTransforms();
    },

    commitTransform() {
      if (!dragSnapshots) return [];

      const before = dragSnapshots;
      const after = getSelectedTransforms();
      dragSnapshots = null;

      const events: GizmoTransformEvent[] = [];

      for (const [id, beforeT] of before) {
        const afterT = after.get(id);
        if (!afterT) continue;
        events.push({
          nodeId: id,
          axis: "xyz",
          before: beforeT,
          after: afterT,
        });
      }

      if (undoAdapter && events.length > 0) {
        const capturedEvents = events;
        undoAdapter.record({
          label: `${state.mode} ${events.length} object(s)`,
          undo() {
            for (const evt of capturedEvents) {
              sceneState.setTransform(evt.nodeId, evt.before);
            }
          },
          redo() {
            for (const evt of capturedEvents) {
              sceneState.setTransform(evt.nodeId, evt.after);
            }
          },
        });
      }

      return events;
    },

    cancelTransform() {
      if (!dragSnapshots) return;
      for (const [id, t] of dragSnapshots) {
        sceneState.setTransform(id, t);
      }
      dragSnapshots = null;
    },

    subscribe(listener) {
      listeners.add(listener);
      return () => { listeners.delete(listener); };
    },
  };
}
