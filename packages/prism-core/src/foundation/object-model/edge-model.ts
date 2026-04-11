/**
 * EdgeModel — stateful, event-driven in-memory store of ObjectEdges.
 *
 * Companion to TreeModel: TreeModel handles the object hierarchy,
 * EdgeModel handles the graph edges between objects.
 */

import type { ObjectEdge } from "./types.js";
import { edgeId } from "./types.js";
import type { ObjectRegistry } from "./registry.js";
import { TreeModelError } from "./tree-model.js";

// ── Event types ──────────────────────────────────────────────────────────────

export type EdgeModelEvent =
  | { kind: "add"; edge: ObjectEdge }
  | { kind: "remove"; edge: ObjectEdge }
  | { kind: "update"; edge: ObjectEdge; previous: ObjectEdge }
  | { kind: "change" };

export type EdgeModelEventListener = (event: EdgeModelEvent) => void;

export interface EdgeModelHooks {
  beforeAdd?: (edge: Omit<ObjectEdge, "id" | "createdAt">) => void;
  afterAdd?: (edge: ObjectEdge) => void;
  beforeRemove?: (edge: ObjectEdge) => void;
  afterRemove?: (edge: ObjectEdge) => void;
  beforeUpdate?: (edge: ObjectEdge, changes: Partial<ObjectEdge>) => void;
  afterUpdate?: (edge: ObjectEdge, previous: ObjectEdge) => void;
}

// ── Constructor options ───────────────────────────────────────────────────────

export interface EdgeModelOptions {
  registry?: ObjectRegistry;
  edges?: ObjectEdge[];
  hooks?: EdgeModelHooks;
  generateId?: () => string;
}

// ── Implementation ────────────────────────────────────────────────────────────

export class EdgeModel {
  private readonly map = new Map<string, ObjectEdge>();
  private readonly listeners = new Set<EdgeModelEventListener>();
  private readonly hooks: EdgeModelHooks;
  private readonly registry: ObjectRegistry | undefined;
  private readonly generateId: () => string;

  constructor(options: EdgeModelOptions = {}) {
    this.hooks = options.hooks ?? {};
    this.registry = options.registry;
    this.generateId = options.generateId ?? defaultIdGenerator;

    if (options.edges) {
      for (const edge of options.edges) this.map.set(edge.id, edge);
    }
  }

  // ── Mutations ───────────────────────────────────────────────────────────────

  add(
    draft: Omit<ObjectEdge, "id" | "createdAt"> & { id?: string },
  ): ObjectEdge {
    if (!draft.sourceId || !draft.targetId || !draft.relation) {
      throw new Error(
        `[EdgeModel] Invalid edge: sourceId, targetId, and relation are required`,
      );
    }

    if (this.registry) {
      const edgeDef = this.registry.getEdgeType(draft.relation);
      if (edgeDef && !edgeDef.allowMultiple) {
        const existing = this.getBetween(
          draft.sourceId,
          draft.targetId,
          draft.relation,
        );
        if (existing.length > 0) {
          throw new TreeModelError(
            "CONTAINMENT_VIOLATION",
            `Relation '${draft.relation}' does not allow multiple edges between the same objects`,
          );
        }
        if (edgeDef.undirected) {
          const reverse = this.getBetween(
            draft.targetId,
            draft.sourceId,
            draft.relation,
          );
          if (reverse.length > 0) {
            throw new TreeModelError(
              "CONTAINMENT_VIOLATION",
              `Undirected relation '${draft.relation}' already exists between these objects`,
            );
          }
        }
      }
    }

    this.hooks.beforeAdd?.(draft);

    const edge: ObjectEdge = {
      ...draft,
      id: edgeId(draft.id ?? this.generateId()),
      createdAt: new Date().toISOString(),
    };

    this.map.set(edge.id, edge);
    this.hooks.afterAdd?.(edge);
    this.emit({ kind: "add", edge });
    this.emit({ kind: "change" });
    return edge;
  }

  remove(id: string): ObjectEdge | null {
    const edge = this.map.get(id);
    if (!edge) return null;

    this.hooks.beforeRemove?.(edge);
    this.map.delete(id);
    this.hooks.afterRemove?.(edge);
    this.emit({ kind: "remove", edge });
    this.emit({ kind: "change" });
    return edge;
  }

  update(
    id: string,
    changes: Partial<Omit<ObjectEdge, "id" | "createdAt">>,
  ): ObjectEdge {
    const edge = this.map.get(id);
    if (!edge) throw new TreeModelError("NOT_FOUND", `Edge '${id}' not found`);

    this.hooks.beforeUpdate?.(edge, changes);

    const safeChanges = { ...changes } as Record<string, unknown>;
    delete safeChanges.id;
    delete safeChanges.createdAt;
    const updated: ObjectEdge = { ...edge, ...(safeChanges as Partial<ObjectEdge>) };
    this.map.set(id, updated);

    this.hooks.afterUpdate?.(updated, edge);
    this.emit({ kind: "update", edge: updated, previous: edge });
    this.emit({ kind: "change" });
    return updated;
  }

  // ── Query ───────────────────────────────────────────────────────────────────

  get(id: string): ObjectEdge | undefined {
    return this.map.get(id);
  }

  has(id: string): boolean {
    return this.map.has(id);
  }

  get size(): number {
    return this.map.size;
  }

  getAll(): ObjectEdge[] {
    return [...this.map.values()];
  }

  getFrom(sourceId: string, relation?: string): ObjectEdge[] {
    return [...this.map.values()].filter(
      (e) =>
        e.sourceId === sourceId &&
        (relation === undefined || e.relation === relation),
    );
  }

  getTo(targetId: string, relation?: string): ObjectEdge[] {
    return [...this.map.values()].filter(
      (e) =>
        e.targetId === targetId &&
        (relation === undefined || e.relation === relation),
    );
  }

  getBetween(
    sourceId: string,
    targetId: string,
    relation?: string,
  ): ObjectEdge[] {
    return [...this.map.values()].filter(
      (e) =>
        e.sourceId === sourceId &&
        e.targetId === targetId &&
        (relation === undefined || e.relation === relation),
    );
  }

  getConnected(objectId: string, relation?: string): ObjectEdge[] {
    return [...this.map.values()].filter(
      (e) =>
        (e.sourceId === objectId || e.targetId === objectId) &&
        (relation === undefined || e.relation === relation),
    );
  }

  // ── Serialization ───────────────────────────────────────────────────────────

  toJSON(): ObjectEdge[] {
    return this.getAll();
  }

  static fromJSON(
    edges: ObjectEdge[],
    options: Omit<EdgeModelOptions, "edges"> = {},
  ): EdgeModel {
    return new EdgeModel({ ...options, edges });
  }

  // ── Events ──────────────────────────────────────────────────────────────────

  on(listener: EdgeModelEventListener): () => void {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }

  // ── Internal ────────────────────────────────────────────────────────────────

  private emit(event: EdgeModelEvent): void {
    for (const listener of this.listeners) listener(event);
  }
}

// ── Default ID generator ─────────────────────────────────────────────────────

function defaultIdGenerator(): string {
  if (
    typeof crypto !== "undefined" &&
    typeof crypto.randomUUID === "function"
  ) {
    return crypto.randomUUID();
  }
  return `${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 9)}`;
}
