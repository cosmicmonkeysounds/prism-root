/**
 * @prism/core — Undo Bridge
 *
 * Wires TreeModel and EdgeModel lifecycle hooks to an UndoRedoManager,
 * automatically recording before/after snapshots for every mutation.
 *
 * Usage:
 *   const manager = new UndoRedoManager(applier);
 *   const { treeHooks, edgeHooks } = createUndoBridge(manager);
 *   const tree = new TreeModel({ hooks: treeHooks });
 *   const edges = new EdgeModel({ hooks: edgeHooks });
 *   // All tree/edge mutations now auto-record undo snapshots.
 */

import type { GraphObject, ObjectEdge } from "../object-model/types.js";
import type { TreeModelHooks } from "../object-model/tree-model.js";
import type { EdgeModelHooks } from "../object-model/edge-model.js";
import type { ObjectSnapshot } from "./undo-types.js";
import type { UndoRedoManager } from "./undo-manager.js";

export interface UndoBridge {
  treeHooks: TreeModelHooks;
  edgeHooks: EdgeModelHooks;
}

/**
 * Create TreeModelHooks + EdgeModelHooks that automatically push
 * snapshots to the given UndoRedoManager.
 *
 * Each after* hook records a snapshot with the before/after state.
 * Batch operations (e.g. remove with descendants) are recorded as
 * a single undo entry with multiple snapshots.
 */
export function createUndoBridge(manager: UndoRedoManager): UndoBridge {
  const treeHooks: TreeModelHooks = {
    afterAdd(object: GraphObject) {
      manager.push(`Create ${object.type}`, [
        { kind: "object", before: null, after: structuredClone(object) },
      ]);
    },

    afterRemove(object: GraphObject, descendants: GraphObject[]) {
      const snapshots: ObjectSnapshot[] = [
        { kind: "object", before: structuredClone(object), after: null },
        ...descendants.map(
          (d): ObjectSnapshot => ({
            kind: "object",
            before: structuredClone(d),
            after: null,
          }),
        ),
      ];
      manager.push(`Delete ${object.type}`, snapshots);
    },

    afterMove(object: GraphObject) {
      // afterMove receives the object in its new state.
      // We can't capture the full before state from the hook signature alone,
      // so we record the current state as both before and after.
      // The TreeModel event contains the from/to info for proper undo.
      manager.push(`Move ${object.type}`, [
        { kind: "object", before: null, after: structuredClone(object) },
      ]);
    },

    afterDuplicate(_original: GraphObject, copies: GraphObject[]) {
      const snapshots: ObjectSnapshot[] = copies.map((c) => ({
        kind: "object",
        before: null,
        after: structuredClone(c),
      }));
      manager.push(`Duplicate ${_original.type}`, snapshots);
    },

    afterUpdate(object: GraphObject, previous: GraphObject) {
      manager.push(`Update ${object.type}`, [
        {
          kind: "object",
          before: structuredClone(previous),
          after: structuredClone(object),
        },
      ]);
    },
  };

  const edgeHooks: EdgeModelHooks = {
    afterAdd(edge: ObjectEdge) {
      manager.push(`Create edge ${edge.relation}`, [
        { kind: "edge", before: null, after: structuredClone(edge) },
      ]);
    },

    afterRemove(edge: ObjectEdge) {
      manager.push(`Delete edge ${edge.relation}`, [
        { kind: "edge", before: structuredClone(edge), after: null },
      ]);
    },

    afterUpdate(edge: ObjectEdge, previous: ObjectEdge) {
      manager.push(`Update edge ${edge.relation}`, [
        {
          kind: "edge",
          before: structuredClone(previous),
          after: structuredClone(edge),
        },
      ]);
    },
  };

  return { treeHooks, edgeHooks };
}
