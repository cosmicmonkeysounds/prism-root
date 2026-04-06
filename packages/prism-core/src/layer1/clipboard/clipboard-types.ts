/**
 * @prism/core — Clipboard Types
 *
 * Serializable subtree representation for cut/copy/paste operations.
 * Objects and edges are captured as snapshots with original IDs preserved,
 * then remapped to fresh IDs on paste.
 */

import type { GraphObject, ObjectEdge } from "../object-model/types.js";

// ── Serialized Subtree ────────────────────────────────────────────────────────

export interface SerializedSubtree {
  /** The root object (top of the copied subtree). */
  root: GraphObject;
  /** All descendants in depth-first order. */
  descendants: GraphObject[];
  /** Edges whose source AND target are both within this subtree. */
  internalEdges: ObjectEdge[];
}

// ── Clipboard Entry ───────────────────────────────────────────────────────────

export type ClipboardMode = "copy" | "cut";

export interface ClipboardEntry {
  mode: ClipboardMode;
  subtrees: SerializedSubtree[];
  /** When mode is "cut", these are the original IDs to delete after paste. */
  sourceIds: string[];
  timestamp: number;
}

// ── Paste Options ─────────────────────────────────────────────────────────────

export interface PasteOptions {
  /** Target parent for pasted objects. null = root level. */
  parentId?: string | null;
  /** Position among siblings. Appends at end if omitted. */
  position?: number;
}

// ── Paste Result ──────────────────────────────────────────────────────────────

export interface PasteResult {
  /** All newly created objects (root + descendants), in order. */
  created: GraphObject[];
  /** All newly created internal edges. */
  createdEdges: ObjectEdge[];
  /** Map of original ID -> new ID for all remapped objects. */
  idMap: Map<string, string>;
}
