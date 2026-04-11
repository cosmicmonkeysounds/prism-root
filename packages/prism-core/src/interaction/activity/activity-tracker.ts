/**
 * Activity Tracker
 *
 * Watches GraphObjects via a store-like subscription interface and
 * automatically derives ActivityEvents from structural diffs.
 *
 * - Duck-typed via TrackableStore (no compile-time dep on CollectionStore).
 * - Per-object subscriptions: call track(id, store) to start watching.
 * - Compares previous snapshot against new value on each emission.
 * - Derives the most-specific verb (moved, renamed, status-changed,
 *   deleted, restored, updated) before falling back to 'updated'.
 * - ignoredFields list filters out noise (updatedAt by default).
 */

import type { GraphObject } from "@prism/core/object-model";
import type { ActivityStore, ActivityVerb, ActivityEventInput, FieldChange } from "./activity-log.js";

// ── Trackable store interface ──────────────────────────────────────────────────

/**
 * Minimal interface required by ActivityTracker.
 * Structurally compatible with CollectionStore or any object subscription source.
 */
export interface TrackableStore {
  /** Subscribe to changes for a single object. Returns an unsubscribe function. */
  subscribeObject(id: string, cb: (obj: unknown) => void): () => void;

  /** Returns the current value for an object ID, or undefined if not present. */
  get(id: string): unknown;
}

// ── Options ────────────────────────────────────────────────────────────────────

export interface ActivityTrackerOptions {
  /** The activity store to record events into. */
  activityStore: ActivityStore;

  /**
   * Top-level field names to skip when building an 'updated' diff.
   * @default ['updatedAt']
   */
  ignoredFields?: string[];

  /** Actor to stamp on all events produced by this tracker. */
  actorId?: string;
  actorName?: string;
}

// ── Helpers ────────────────────────────────────────────────────────────────────

const DEFAULT_IGNORED = new Set(["updatedAt"]);

/** Shallow-equal check for primitives; JSON-equal for objects/arrays. */
function valuesEqual(a: unknown, b: unknown): boolean {
  if (a === b) return true;
  if (a === null || b === null) return false;
  if (typeof a !== "object" || typeof b !== "object") return false;
  try {
    return JSON.stringify(a) === JSON.stringify(b);
  } catch {
    return false;
  }
}

const SHELL_FIELDS: Array<keyof GraphObject> = [
  "name",
  "type",
  "parentId",
  "position",
  "status",
  "tags",
  "date",
  "endDate",
  "description",
  "color",
  "image",
  "pinned",
  "createdAt",
  "deletedAt",
];

/**
 * Computes a flat diff between two GraphObject snapshots.
 * Returns one FieldChange per top-level field that changed.
 * Data payload is diffed one level deep as "data.<key>".
 */
function diffObjects(
  prev: GraphObject,
  next: GraphObject,
  ignored: Set<string>,
): FieldChange[] {
  const changes: FieldChange[] = [];

  for (const field of SHELL_FIELDS) {
    if (ignored.has(field)) continue;
    if (!valuesEqual(prev[field], next[field])) {
      changes.push({ field, before: prev[field], after: next[field] });
    }
  }

  const prevData = prev.data ?? {};
  const nextData = next.data ?? {};
  const dataKeys = new Set([
    ...Object.keys(prevData),
    ...Object.keys(nextData),
  ]);

  for (const key of dataKeys) {
    const fieldPath = `data.${key}`;
    if (ignored.has(fieldPath) || ignored.has(key)) continue;
    const pv = prevData[key];
    const nv = nextData[key];
    if (!valuesEqual(pv, nv)) {
      changes.push({ field: fieldPath, before: pv, after: nv });
    }
  }

  return changes;
}

/**
 * Infers the most-specific ActivityVerb from the diff results and the
 * structural delta between prev and next.
 */
function inferVerb(
  prev: GraphObject,
  next: GraphObject,
  changes: FieldChange[],
): ActivityVerb {
  if (!prev.deletedAt && next.deletedAt) return "deleted";
  if (prev.deletedAt && !next.deletedAt) return "restored";

  if (prev.parentId !== next.parentId) return "moved";

  if (
    changes.length === 1 &&
    changes[0] !== undefined &&
    changes[0].field === "name"
  ) {
    return "renamed";
  }

  const statusChange = changes.find((c) => c.field === "status");
  if (statusChange) return "status-changed";

  return "updated";
}

// ── ActivityTracker ────────────────────────────────────────────────────────────

export interface ActivityTracker {
  /**
   * Begin watching an object. On every emission the tracker diffs against
   * the previous snapshot and records the appropriate ActivityEvent.
   * Calling track() for an already-tracked id replaces the subscription.
   * Returns an unsubscribe function.
   */
  track(objectId: string, store: TrackableStore): () => void;

  /** Stops watching all tracked objects and clears all snapshots. */
  untrackAll(): void;

  /** Returns the set of currently tracked object IDs. */
  trackedIds(): ReadonlySet<string>;
}

export function createActivityTracker(
  options: ActivityTrackerOptions,
): ActivityTracker {
  const activityStore = options.activityStore;
  const ignored = new Set([
    ...DEFAULT_IGNORED,
    ...(options.ignoredFields ?? []),
  ]);
  const actorId = options.actorId;
  const actorName = options.actorName;

  /** objectId → unsubscribe function */
  const unsubs = new Map<string, () => void>();

  /** objectId → last-seen GraphObject snapshot */
  const snapshots = new Map<string, GraphObject>();

  // ── Internal ─────────────────────────────────────────────────────────────

  function handleUpdate(
    objectId: string,
    next: GraphObject | null | undefined,
  ): void {
    const prev = snapshots.get(objectId);

    if (!prev && !next) return;

    // Object appeared (created or first-seen)
    if (!prev && next) {
      snapshots.set(objectId, structuredClone(next));
      const age = Date.now() - new Date(next.createdAt).getTime();
      if (age < 5_000) {
        activityStore.record({
          objectId,
          verb: "created",
          ...(actorId != null && { actorId }),
          ...(actorName != null && { actorName }),
        });
      }
      return;
    }

    // Object disappeared (hard delete)
    if (prev && !next) {
      snapshots.delete(objectId);
      if (!prev.deletedAt) {
        activityStore.record({
          objectId,
          verb: "deleted",
          ...(actorId != null && { actorId }),
          ...(actorName != null && { actorName }),
        });
      }
      return;
    }

    // Both exist — diff
    const p = prev as GraphObject;
    const n = next as GraphObject;
    const changes = diffObjects(p, n, ignored);

    if (changes.length === 0 && p.deletedAt === n.deletedAt) {
      snapshots.set(objectId, structuredClone(n));
      return;
    }

    const verb = inferVerb(p, n, changes);

    const eventBase: ActivityEventInput = {
      objectId,
      verb,
      ...(actorId != null && { actorId }),
      ...(actorName != null && { actorName }),
    };

    if (verb === "updated" || verb === "renamed") {
      eventBase.changes = changes;
    } else if (verb === "status-changed") {
      const sc = changes.find((c) => c.field === "status");
      if (sc?.before != null) eventBase.fromStatus = sc.before as string;
      if (sc?.after != null) eventBase.toStatus = sc.after as string;
      if (changes.length > 1) {
        eventBase.changes = changes;
      }
    } else if (verb === "moved") {
      eventBase.fromParentId = p.parentId as string | null;
      eventBase.toParentId = n.parentId as string | null;
      const nameChange = changes.find((c) => c.field === "name");
      if (nameChange) {
        eventBase.changes = [nameChange];
      }
    }

    activityStore.record(eventBase);
    snapshots.set(objectId, structuredClone(n));
  }

  // ── Public API ───────────────────────────────────────────────────────────

  function track(objectId: string, store: TrackableStore): () => void {
    unsubs.get(objectId)?.();

    const initial = store.get(objectId) as GraphObject | undefined;
    if (initial) {
      snapshots.set(objectId, structuredClone(initial));
    }

    const unsub = store.subscribeObject(objectId, (raw) => {
      handleUpdate(objectId, raw as GraphObject | null | undefined);
    });

    unsubs.set(objectId, unsub);

    return () => {
      unsub();
      unsubs.delete(objectId);
      snapshots.delete(objectId);
    };
  }

  function untrackAll(): void {
    for (const unsub of unsubs.values()) {
      unsub();
    }
    unsubs.clear();
    snapshots.clear();
  }

  function trackedIds(): ReadonlySet<string> {
    return new Set(unsubs.keys());
  }

  return { track, untrackAll, trackedIds };
}
