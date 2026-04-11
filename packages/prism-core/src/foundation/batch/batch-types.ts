/**
 * @prism/core — Batch Operation Types
 *
 * Typed mutation descriptors for collecting multiple tree/edge operations
 * into a single atomic batch. Each op maps to a TreeModel or EdgeModel method.
 */

import type { GraphObject, ObjectEdge } from "@prism/core/object-model";

// ── Object Operations ─────────────────────────────────────────────────────────

export interface CreateObjectOp {
  kind: "create-object";
  draft: Partial<GraphObject> & { type: string; name: string };
  parentId?: string | null;
  position?: number;
}

export interface UpdateObjectOp {
  kind: "update-object";
  id: string;
  changes: Partial<Omit<GraphObject, "id" | "type" | "createdAt">>;
}

export interface DeleteObjectOp {
  kind: "delete-object";
  id: string;
}

export interface MoveObjectOp {
  kind: "move-object";
  id: string;
  toParentId: string | null;
  toPosition?: number;
}

// ── Edge Operations ───────────────────────────────────────────────────────────

export interface CreateEdgeOp {
  kind: "create-edge";
  draft: Omit<ObjectEdge, "id" | "createdAt"> & { id?: string };
}

export interface UpdateEdgeOp {
  kind: "update-edge";
  id: string;
  changes: Partial<Omit<ObjectEdge, "id" | "createdAt">>;
}

export interface DeleteEdgeOp {
  kind: "delete-edge";
  id: string;
}

// ── Union ─────────────────────────────────────────────────────────────────────

export type BatchOp =
  | CreateObjectOp
  | UpdateObjectOp
  | DeleteObjectOp
  | MoveObjectOp
  | CreateEdgeOp
  | UpdateEdgeOp
  | DeleteEdgeOp;

// ── Result ────────────────────────────────────────────────────────────────────

export interface BatchResult {
  /** Total number of operations executed. */
  executed: number;
  /** Objects created by create-object ops, in order. */
  created: GraphObject[];
  /** Edge IDs created by create-edge ops, in order. */
  createdEdges: ObjectEdge[];
}

// ── Progress ──────────────────────────────────────────────────────────────────

export interface BatchProgress {
  /** Index of the operation currently executing (0-based). */
  current: number;
  /** Total number of operations. */
  total: number;
  /** The operation about to execute. */
  op: BatchOp;
}

export type BatchProgressCallback = (progress: BatchProgress) => void;

// ── Validation ────────────────────────────────────────────────────────────────

export interface BatchValidationError {
  /** Index of the failing operation in the ops array. */
  index: number;
  /** The operation that failed. */
  op: BatchOp;
  /** Human-readable reason. */
  reason: string;
}

export interface BatchValidationResult {
  valid: boolean;
  errors: BatchValidationError[];
}

// ── Options ───────────────────────────────────────────────────────────────────

export interface BatchExecuteOptions {
  /** Human-readable description for the undo entry. Default: "Batch operation". */
  description?: string;
  /** Called before each operation executes. */
  onProgress?: BatchProgressCallback;
}
