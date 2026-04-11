/**
 * @prism/core — UndoRedoManager
 *
 * Framework-agnostic undo/redo stack.
 *
 * Design:
 *   Mutations reduce to snapshot diffs: before/after state.
 *   Undo = restore `before`. Redo = restore `after`.
 *   No complex command objects — just snapshots of what changed.
 *
 *   Batch mutations (e.g. "move to folder" = update parentId + reorder)
 *   are recorded as a single undoable entry.
 *
 * Usage:
 *   const manager = new UndoRedoManager(applier, { maxHistory: 50 });
 *   manager.push('Create task', [{ kind: 'object', before: null, after: obj }]);
 *   manager.undo();  // restores previous state
 *   manager.redo();  // re-applies the mutation
 */

import type {
  ObjectSnapshot,
  UndoEntry,
  UndoApplier,
  UndoListener,
} from "./undo-types.js";

export class UndoRedoManager {
  private _past: UndoEntry[] = [];
  private _future: UndoEntry[] = [];
  private _listeners = new Set<UndoListener>();
  private _maxHistory: number;

  constructor(
    private _applier: UndoApplier,
    options: { maxHistory?: number } = {},
  ) {
    this._maxHistory = options.maxHistory ?? 100;
  }

  // ── Record ──────────────────────────────────────────────────────────────────

  /**
   * Push a new undoable entry onto the stack.
   * Clears the redo stack (new mutation invalidates future).
   */
  push(description: string, snapshots: ObjectSnapshot[]): void {
    if (snapshots.length === 0) return;
    this._past.push({
      description,
      snapshots,
      timestamp: Date.now(),
    });
    if (this._past.length > this._maxHistory) this._past.shift();
    this._future = [];
    this._notify();
  }

  /**
   * Merge snapshots into the most recent entry (for rapid edits).
   * Useful for coalescing rapid keystrokes into a single undo step.
   */
  merge(snapshots: ObjectSnapshot[]): void {
    const last = this._past[this._past.length - 1];
    if (!last) return;
    last.snapshots.push(...snapshots);
    this._notify();
  }

  // ── Undo / Redo ─────────────────────────────────────────────────────────────

  get canUndo(): boolean {
    return this._past.length > 0;
  }

  get canRedo(): boolean {
    return this._future.length > 0;
  }

  get undoLabel(): string | null {
    return this._past[this._past.length - 1]?.description ?? null;
  }

  get redoLabel(): string | null {
    return this._future[this._future.length - 1]?.description ?? null;
  }

  undo(): void {
    const entry = this._past.pop();
    if (!entry) return;
    this._applier(entry.snapshots, "undo");
    this._future.push(entry);
    this._notify();
  }

  redo(): void {
    const entry = this._future.pop();
    if (!entry) return;
    this._applier(entry.snapshots, "redo");
    this._past.push(entry);
    this._notify();
  }

  clear(): void {
    this._past = [];
    this._future = [];
    this._notify();
  }

  get history(): readonly UndoEntry[] {
    return this._past;
  }

  get historySize(): number {
    return this._past.length;
  }

  get futureSize(): number {
    return this._future.length;
  }

  // ── Subscriptions ──────────────────────────────────────────────────────────

  /**
   * Subscribe to stack changes (canUndo/canRedo/labels changed).
   * Returns unsubscribe function.
   */
  subscribe(cb: UndoListener): () => void {
    this._listeners.add(cb);
    return () => this._listeners.delete(cb);
  }

  private _notify(): void {
    for (const cb of this._listeners) cb();
  }
}
