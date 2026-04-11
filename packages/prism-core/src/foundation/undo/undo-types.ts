/**
 * @prism/core — Undo/Redo Types
 *
 * Snapshot-based undo: every mutation records before/after state.
 * Undo = restore `before`. Redo = restore `after`.
 * Batched mutations (e.g. "move to folder") = single undoable entry.
 */

import type {
  GraphObject,
  ObjectEdge,
} from "@prism/core/object-model";

// ── Snapshots ───────────────────────────────────────────────────────────────────

export type ObjectSnapshot =
  | { kind: "object"; before: GraphObject | null; after: GraphObject | null }
  | { kind: "edge"; before: ObjectEdge | null; after: ObjectEdge | null };

export interface UndoEntry {
  /** Human-readable description (e.g. "Move to folder", "Delete task"). */
  description: string;
  /** One or more snapshots — always applied/reverted together. */
  snapshots: ObjectSnapshot[];
  timestamp: number;
}

// ── Applier ─────────────────────────────────────────────────────────────────────

/**
 * Function that applies snapshot diffs.
 * Called by UndoRedoManager during undo/redo.
 * The direction tells the applier which side of the snapshot to restore.
 */
export type UndoApplier = (
  snapshots: ObjectSnapshot[],
  direction: "undo" | "redo",
) => void;

// ── Listener ────────────────────────────────────────────────────────────────────

/**
 * Listener called when the undo/redo stack changes
 * (push, undo, redo, clear, merge).
 */
export type UndoListener = () => void;
