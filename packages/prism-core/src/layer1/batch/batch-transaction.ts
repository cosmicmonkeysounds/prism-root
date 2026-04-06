/**
 * @prism/core — BatchTransaction
 *
 * Collects multiple tree/edge mutations and executes them atomically.
 * On success, pushes a single undo entry to UndoRedoManager.
 * On failure, rolls back all mutations applied so far.
 *
 * Usage:
 *   const tx = createBatchTransaction({ tree, edges, undo });
 *   tx.add({ kind: 'create-object', draft: { type: 'task', name: 'A' } });
 *   tx.add({ kind: 'update-object', id: '1', changes: { status: 'done' } });
 *   const result = tx.execute({ description: 'Bulk update tasks' });
 */

import type { GraphObject, ObjectEdge } from "../object-model/types.js";
import type { TreeModel } from "../object-model/tree-model.js";
import type { EdgeModel } from "../object-model/edge-model.js";
import type { UndoRedoManager } from "../undo/undo-manager.js";
import type { ObjectSnapshot } from "../undo/undo-types.js";

import type {
  BatchOp,
  BatchResult,
  BatchValidationError,
  BatchValidationResult,
  BatchExecuteOptions,
} from "./batch-types.js";

// ── Options ───────────────────────────────────────────────────────────────────

export interface BatchTransactionOptions {
  tree: TreeModel;
  edges?: EdgeModel;
  undo?: UndoRedoManager;
}

// ── Interface ─────────────────────────────────────────────────────────────────

export interface BatchTransaction {
  /** Enqueue an operation. Does not execute it yet. */
  add(op: BatchOp): void;
  /** Enqueue multiple operations. */
  addAll(ops: BatchOp[]): void;
  /** Number of queued operations. */
  readonly size: number;
  /** Read-only view of queued operations. */
  readonly ops: readonly BatchOp[];
  /** Pre-flight validation — checks types exist, IDs present, etc. */
  validate(): BatchValidationResult;
  /** Execute all queued ops atomically. Returns result or throws on failure. */
  execute(options?: BatchExecuteOptions): BatchResult;
  /** Discard all queued operations. */
  clear(): void;
}

// ── Factory ───────────────────────────────────────────────────────────────────

export function createBatchTransaction(
  options: BatchTransactionOptions,
): BatchTransaction {
  const { tree, edges, undo } = options;
  let queue: BatchOp[] = [];

  function add(op: BatchOp): void {
    queue.push(op);
  }

  function addAll(ops: BatchOp[]): void {
    queue.push(...ops);
  }

  function validate(): BatchValidationResult {
    const errors: BatchValidationError[] = [];

    for (const [i, op] of queue.entries()) {
      switch (op.kind) {
        case "create-object":
          if (!op.draft.type) {
            errors.push({ index: i, op, reason: "Missing type in draft" });
          }
          if (!op.draft.name) {
            errors.push({ index: i, op, reason: "Missing name in draft" });
          }
          break;

        case "update-object":
        case "delete-object":
        case "move-object":
          if (!op.id) {
            errors.push({ index: i, op, reason: "Missing object id" });
          }
          break;

        case "create-edge":
          if (!edges) {
            errors.push({ index: i, op, reason: "No EdgeModel provided" });
          }
          if (!op.draft.sourceId || !op.draft.targetId) {
            errors.push({
              index: i,
              op,
              reason: "Missing sourceId or targetId in edge draft",
            });
          }
          break;

        case "update-edge":
        case "delete-edge":
          if (!edges) {
            errors.push({ index: i, op, reason: "No EdgeModel provided" });
          }
          if (!op.id) {
            errors.push({ index: i, op, reason: "Missing edge id" });
          }
          break;
      }
    }

    return { valid: errors.length === 0, errors };
  }

  function execute(execOptions: BatchExecuteOptions = {}): BatchResult {
    const {
      description = "Batch operation",
      onProgress,
    } = execOptions;

    const snapshots: ObjectSnapshot[] = [];
    const created: GraphObject[] = [];
    const createdEdges: ObjectEdge[] = [];
    let executed = 0;

    // Collect rollback actions in case of failure
    const rollbacks: Array<() => void> = [];

    try {
      for (const [i, op] of queue.entries()) {
        onProgress?.({ current: i, total: queue.length, op });

        switch (op.kind) {
          case "create-object": {
            const addOpts: { parentId?: string | null; position?: number } = {};
            if (op.parentId !== undefined) addOpts.parentId = op.parentId;
            if (op.position !== undefined) addOpts.position = op.position;
            const obj = tree.add(op.draft, addOpts);
            created.push(obj);
            snapshots.push({
              kind: "object",
              before: null,
              after: structuredClone(obj),
            });
            rollbacks.push(() => tree.remove(obj.id));
            break;
          }

          case "update-object": {
            const before = tree.get(op.id);
            if (!before) throw new Error(`Object '${op.id}' not found`);
            const beforeClone = structuredClone(before);
            const after = tree.update(op.id, op.changes);
            snapshots.push({
              kind: "object",
              before: beforeClone,
              after: structuredClone(after),
            });
            rollbacks.push(() => {
              tree.update(op.id, beforeClone);
            });
            break;
          }

          case "delete-object": {
            const target = tree.get(op.id);
            if (!target) throw new Error(`Object '${op.id}' not found`);
            const descendants = tree.getDescendants(op.id);
            const allDeleted = [target, ...descendants];
            for (const d of allDeleted) {
              snapshots.push({
                kind: "object",
                before: structuredClone(d),
                after: null,
              });
            }
            tree.remove(op.id);
            rollbacks.push(() => {
              for (const d of allDeleted) {
                tree.add(d, {
                  parentId: d.parentId,
                  position: d.position,
                });
              }
            });
            break;
          }

          case "move-object": {
            const obj = tree.get(op.id);
            if (!obj) throw new Error(`Object '${op.id}' not found`);
            const beforeState = structuredClone(obj);
            const moved = tree.move(op.id, op.toParentId, op.toPosition);
            snapshots.push({
              kind: "object",
              before: beforeState,
              after: structuredClone(moved),
            });
            rollbacks.push(() => {
              tree.move(op.id, beforeState.parentId, beforeState.position);
            });
            break;
          }

          case "create-edge": {
            if (!edges) throw new Error("No EdgeModel provided");
            const edge = edges.add(op.draft);
            createdEdges.push(edge);
            snapshots.push({
              kind: "edge",
              before: null,
              after: structuredClone(edge),
            });
            rollbacks.push(() => edges.remove(edge.id));
            break;
          }

          case "update-edge": {
            if (!edges) throw new Error("No EdgeModel provided");
            const beforeEdge = edges.get(op.id);
            if (!beforeEdge) throw new Error(`Edge '${op.id}' not found`);
            const beforeEdgeClone = structuredClone(beforeEdge);
            const afterEdge = edges.update(op.id, op.changes);
            snapshots.push({
              kind: "edge",
              before: beforeEdgeClone,
              after: structuredClone(afterEdge),
            });
            rollbacks.push(() => {
              edges.update(op.id, beforeEdgeClone);
            });
            break;
          }

          case "delete-edge": {
            if (!edges) throw new Error("No EdgeModel provided");
            const edgeToDelete = edges.get(op.id);
            if (!edgeToDelete) throw new Error(`Edge '${op.id}' not found`);
            const edgeClone = structuredClone(edgeToDelete);
            edges.remove(op.id);
            snapshots.push({
              kind: "edge",
              before: edgeClone,
              after: null,
            });
            rollbacks.push(() => edges.add(edgeClone));
            break;
          }
        }

        executed++;
      }
    } catch (err) {
      // Roll back all mutations in reverse order
      for (let r = rollbacks.length - 1; r >= 0; r--) {
        try {
          const fn = rollbacks[r];
          if (fn) fn();
        } catch {
          // Best-effort rollback — swallow nested errors
        }
      }
      throw err;
    }

    // Push a single undo entry for the whole batch
    if (undo && snapshots.length > 0) {
      undo.push(description, snapshots);
    }

    return { executed, created, createdEdges };
  }

  function clear(): void {
    queue = [];
  }

  return {
    add,
    addAll,
    get size() {
      return queue.length;
    },
    get ops() {
      return queue;
    },
    validate,
    execute,
    clear,
  };
}
