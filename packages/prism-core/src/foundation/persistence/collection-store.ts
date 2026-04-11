/**
 * CollectionStore — Loro CRDT-backed storage for GraphObjects and ObjectEdges.
 *
 * Each CollectionStore wraps a single LoroDoc that holds two top-level maps:
 *   - "objects" — LoroMap<ObjectId, JSON-serialized GraphObject>
 *   - "edges"   — LoroMap<EdgeId, JSON-serialized ObjectEdge>
 *
 * This is the persistence-side counterpart to TreeModel/EdgeModel (which are
 * in-memory projection caches). A CollectionStore is the durable truth;
 * TreeModel/EdgeModel are ephemeral views that can be rebuilt from it.
 *
 * Supports:
 *   - CRUD for objects and edges
 *   - Filtering/querying objects by type, tags, status
 *   - Snapshot export/import for disk persistence and peer sync
 *   - Change subscriptions for reactive projections
 */

import { LoroDoc } from "loro-crdt";
import type { VersionVector } from "loro-crdt";
import type { CrdtSnapshot, CrdtUpdate } from "@prism/shared/types";
import type {
  GraphObject,
  ObjectEdge,
  ObjectId,
  EdgeId,
} from "@prism/core/object-model";

// ── Types ─────────────────────────────────────────────────────────────────────

export type CollectionChangeType =
  | "object-put"
  | "object-remove"
  | "edge-put"
  | "edge-remove";

export interface CollectionChange {
  type: CollectionChangeType;
  id: string;
}

export type CollectionChangeHandler = (changes: CollectionChange[]) => void;

export interface CollectionStoreOptions {
  /** Loro peer ID for multi-peer CRDT sync. */
  peerId?: bigint;
}

export interface ObjectFilter {
  /** Filter by object type(s). */
  types?: string[];
  /** Filter by tag(s) — object must have ALL listed tags. */
  tags?: string[];
  /** Filter by status value(s). */
  statuses?: string[];
  /** Filter by parent ID. */
  parentId?: ObjectId | null;
  /** Only include non-deleted objects. Default: true. */
  excludeDeleted?: boolean;
}

// ── CollectionStore ───────────────────────────────────────────────────────────

export interface CollectionStore {
  /** The underlying LoroDoc. */
  readonly doc: LoroDoc;

  // ── Object CRUD ──────────────────────────────────────────────────────────

  /** Store or update a GraphObject. */
  putObject(obj: GraphObject): void;
  /** Retrieve a GraphObject by ID, or undefined if not found. */
  getObject(id: ObjectId): GraphObject | undefined;
  /** Remove a GraphObject by ID. Returns true if it existed. */
  removeObject(id: ObjectId): boolean;
  /** List all objects, optionally filtered. */
  listObjects(filter?: ObjectFilter): GraphObject[];
  /** Count of stored objects. */
  objectCount(): number;

  // ── Edge CRUD ────────────────────────────────────────────────────────────

  /** Store or update an ObjectEdge. */
  putEdge(edge: ObjectEdge): void;
  /** Retrieve an ObjectEdge by ID, or undefined if not found. */
  getEdge(id: EdgeId): ObjectEdge | undefined;
  /** Remove an ObjectEdge by ID. Returns true if it existed. */
  removeEdge(id: EdgeId): boolean;
  /** List all edges, optionally filtered by source/target/relation. */
  listEdges(filter?: {
    sourceId?: ObjectId;
    targetId?: ObjectId;
    relation?: string;
  }): ObjectEdge[];
  /** Count of stored edges. */
  edgeCount(): number;

  // ── Snapshot / Sync ──────────────────────────────────────────────────────

  /** Export the full document state as a binary snapshot. */
  exportSnapshot(): CrdtSnapshot;
  /** Export incremental updates since a version vector. */
  exportUpdate(since?: VersionVector): CrdtUpdate;
  /** Import a snapshot or update from another peer. */
  import(data: CrdtSnapshot | CrdtUpdate): void;

  // ── Change subscription ──────────────────────────────────────────────────

  /** Subscribe to changes. Returns unsubscribe function. */
  onChange(handler: CollectionChangeHandler): () => void;

  // ── Bulk ──────────────────────────────────────────────────────────────────

  /** Get all objects as a plain array. */
  allObjects(): GraphObject[];
  /** Get all edges as a plain array. */
  allEdges(): ObjectEdge[];
  /** Export entire store as JSON (for debugging/inspection). */
  toJSON(): { objects: Record<string, GraphObject>; edges: Record<string, ObjectEdge> };
}

// ── Factory ───────────────────────────────────────────────────────────────────

export function createCollectionStore(
  options?: CollectionStoreOptions,
): CollectionStore {
  const doc = new LoroDoc();
  if (options?.peerId !== undefined) {
    doc.setPeerId(options.peerId);
  }

  const objectsMap = doc.getMap("objects");
  const edgesMap = doc.getMap("edges");

  const listeners = new Set<CollectionChangeHandler>();

  // Subscribe to doc-level changes and translate to CollectionChange events
  doc.subscribe((event) => {
    if (listeners.size === 0) return;
    const changes: CollectionChange[] = [];
    for (const e of event.events) {
      if (e.diff.type === "map") {
        for (const [key, val] of Object.entries(e.diff.updated)) {
          // Determine which map this change belongs to by checking the path
          const path = e.path;
          const containerKey = path.length > 0 ? String(path[0]) : "";
          if (containerKey === "objects") {
            changes.push({
              type: val === null || val === undefined ? "object-remove" : "object-put",
              id: key,
            });
          } else if (containerKey === "edges") {
            changes.push({
              type: val === null || val === undefined ? "edge-remove" : "edge-put",
              id: key,
            });
          }
        }
      }
    }
    if (changes.length > 0) {
      for (const handler of listeners) {
        handler(changes);
      }
    }
  });

  function putObject(obj: GraphObject): void {
    objectsMap.set(obj.id, JSON.stringify(obj));
    doc.commit();
  }

  function getObject(id: ObjectId): GraphObject | undefined {
    const raw = objectsMap.get(id);
    if (raw === undefined || raw === null) return undefined;
    return JSON.parse(raw as string) as GraphObject;
  }

  function removeObject(id: ObjectId): boolean {
    const exists = objectsMap.get(id) !== undefined && objectsMap.get(id) !== null;
    if (exists) {
      objectsMap.delete(id);
      doc.commit();
    }
    return exists;
  }

  function allObjects(): GraphObject[] {
    const json = objectsMap.toJSON() as Record<string, string>;
    return Object.values(json).map((raw) => JSON.parse(raw) as GraphObject);
  }

  function listObjects(filter?: ObjectFilter): GraphObject[] {
    let objects = allObjects();

    if (!filter) return objects;

    const excludeDeleted = filter.excludeDeleted ?? true;

    if (excludeDeleted) {
      objects = objects.filter((o) => !o.deletedAt);
    }
    if (filter.types && filter.types.length > 0) {
      const types = new Set(filter.types);
      objects = objects.filter((o) => types.has(o.type));
    }
    if (filter.tags && filter.tags.length > 0) {
      const tags = filter.tags;
      objects = objects.filter((o) =>
        tags.every((tag) => o.tags.includes(tag)),
      );
    }
    if (filter.statuses && filter.statuses.length > 0) {
      const statuses = new Set(filter.statuses);
      objects = objects.filter((o) => o.status !== null && statuses.has(o.status));
    }
    if (filter.parentId !== undefined) {
      objects = objects.filter((o) => o.parentId === filter.parentId);
    }

    return objects;
  }

  function objectCount(): number {
    return Object.keys(objectsMap.toJSON() as Record<string, unknown>).length;
  }

  function putEdge(edge: ObjectEdge): void {
    edgesMap.set(edge.id, JSON.stringify(edge));
    doc.commit();
  }

  function getEdge(id: EdgeId): ObjectEdge | undefined {
    const raw = edgesMap.get(id);
    if (raw === undefined || raw === null) return undefined;
    return JSON.parse(raw as string) as ObjectEdge;
  }

  function removeEdge(id: EdgeId): boolean {
    const exists = edgesMap.get(id) !== undefined && edgesMap.get(id) !== null;
    if (exists) {
      edgesMap.delete(id);
      doc.commit();
    }
    return exists;
  }

  function allEdges(): ObjectEdge[] {
    const json = edgesMap.toJSON() as Record<string, string>;
    return Object.values(json).map((raw) => JSON.parse(raw) as ObjectEdge);
  }

  function listEdges(filter?: {
    sourceId?: ObjectId;
    targetId?: ObjectId;
    relation?: string;
  }): ObjectEdge[] {
    let edges = allEdges();

    if (!filter) return edges;

    if (filter.sourceId) {
      edges = edges.filter((e) => e.sourceId === filter.sourceId);
    }
    if (filter.targetId) {
      edges = edges.filter((e) => e.targetId === filter.targetId);
    }
    if (filter.relation) {
      edges = edges.filter((e) => e.relation === filter.relation);
    }

    return edges;
  }

  function edgeCount(): number {
    return Object.keys(edgesMap.toJSON() as Record<string, unknown>).length;
  }

  function exportSnapshot(): CrdtSnapshot {
    return doc.export({ mode: "snapshot" });
  }

  function exportUpdate(since?: VersionVector): CrdtUpdate {
    if (since !== undefined) {
      return doc.export({ mode: "update", from: since });
    }
    return doc.export({ mode: "update" });
  }

  function importData(data: CrdtSnapshot | CrdtUpdate): void {
    doc.import(data);
  }

  function toJSON(): {
    objects: Record<string, GraphObject>;
    edges: Record<string, ObjectEdge>;
  } {
    const rawObjects = objectsMap.toJSON() as Record<string, string>;
    const rawEdges = edgesMap.toJSON() as Record<string, string>;
    const objects: Record<string, GraphObject> = {};
    const edges: Record<string, ObjectEdge> = {};
    for (const [k, v] of Object.entries(rawObjects)) {
      objects[k] = JSON.parse(v) as GraphObject;
    }
    for (const [k, v] of Object.entries(rawEdges)) {
      edges[k] = JSON.parse(v) as ObjectEdge;
    }
    return { objects, edges };
  }

  return {
    doc,
    putObject,
    getObject,
    removeObject,
    listObjects,
    objectCount,
    putEdge,
    getEdge,
    removeEdge,
    listEdges,
    edgeCount,
    exportSnapshot,
    exportUpdate,
    import: importData,
    onChange(handler: CollectionChangeHandler): () => void {
      listeners.add(handler);
      return () => { listeners.delete(handler); };
    },
    allObjects,
    allEdges,
    toJSON,
  };
}
