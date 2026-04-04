/**
 * TreeModel — stateful, event-driven in-memory tree of GraphObjects.
 *
 * Responsibilities:
 *   - Owns a mutable flat map of objects (id -> GraphObject)
 *   - Provides atomic tree operations: add, remove, move, reparent,
 *     reorder, duplicate, update
 *   - Validates containment via an optional ObjectRegistry
 *   - Fires typed events after every mutation (for reactive views)
 *   - Calls lifecycle hooks before mutations (hooks can throw to cancel)
 *
 * Design:
 *   - Pure in-memory — no HTTP, no async (operations are synchronous)
 *   - Framework-agnostic — wrap with React hooks, Signals, etc.
 *   - Pluggable ID generation (default: crypto.randomUUID or fallback)
 */

import type { GraphObject, ObjectId } from "./types.js";
import { objectId } from "./types.js";
import type { ObjectRegistry, TreeNode } from "./registry.js";

// ── Event types ───────────────────────────────────────────────────────────────

export type TreeModelEvent =
  | { kind: "add"; object: GraphObject }
  | { kind: "remove"; object: GraphObject; descendants: GraphObject[] }
  | {
      kind: "move";
      object: GraphObject;
      from: { parentId: string | null; position: number };
      to: { parentId: string | null; position: number };
    }
  | { kind: "reorder"; parentId: string | null; children: GraphObject[] }
  | { kind: "duplicate"; original: GraphObject; copies: GraphObject[] }
  | { kind: "update"; object: GraphObject; previous: GraphObject }
  | { kind: "change" };

export type TreeModelEventListener = (event: TreeModelEvent) => void;

// ── Lifecycle hooks ───────────────────────────────────────────────────────────

export interface TreeModelHooks {
  beforeAdd?: (
    draft: Partial<GraphObject> & { type: string; name: string },
    parentId: string | null,
  ) => void;
  afterAdd?: (object: GraphObject) => void;
  beforeRemove?: (object: GraphObject) => void;
  afterRemove?: (object: GraphObject, descendants: GraphObject[]) => void;
  beforeMove?: (
    object: GraphObject,
    toParentId: string | null,
    toPosition: number,
  ) => void;
  afterMove?: (object: GraphObject) => void;
  beforeDuplicate?: (object: GraphObject) => void;
  afterDuplicate?: (original: GraphObject, copies: GraphObject[]) => void;
  beforeUpdate?: (object: GraphObject, changes: Partial<GraphObject>) => void;
  afterUpdate?: (object: GraphObject, previous: GraphObject) => void;
}

// ── Operation options ─────────────────────────────────────────────────────────

export interface AddOptions {
  parentId?: string | null;
  position?: number;
}

export interface DuplicateOptions {
  deep?: boolean;
  targetParentId?: string | null | undefined;
  position?: number;
}

// ── Error ─────────────────────────────────────────────────────────────────────

export type TreeModelErrorCode =
  | "NOT_FOUND"
  | "CIRCULAR_REF"
  | "CONTAINMENT_VIOLATION"
  | "CANCELLED";

export class TreeModelError extends Error {
  constructor(
    public readonly code: TreeModelErrorCode,
    message: string,
  ) {
    super(message);
    this.name = "TreeModelError";
  }
}

// ── Constructor options ───────────────────────────────────────────────────────

export interface TreeModelOptions {
  registry?: ObjectRegistry;
  objects?: GraphObject[];
  hooks?: TreeModelHooks;
  generateId?: () => string;
}

// ── Implementation ────────────────────────────────────────────────────────────

export class TreeModel {
  private readonly map = new Map<string, GraphObject>();
  private readonly listeners = new Set<TreeModelEventListener>();
  private readonly hooks: TreeModelHooks;
  private readonly registry: ObjectRegistry | undefined;
  private readonly generateId: () => string;

  constructor(options: TreeModelOptions = {}) {
    this.hooks = options.hooks ?? {};
    this.registry = options.registry;
    this.generateId = options.generateId ?? defaultIdGenerator;

    if (options.objects) {
      for (const obj of options.objects) this.map.set(obj.id, obj);
    }
  }

  // ── Mutations ───────────────────────────────────────────────────────────────

  add(
    draft: Partial<GraphObject> & { type: string; name: string },
    { parentId = null, position }: AddOptions = {},
  ): GraphObject {
    if (parentId !== null && !this.map.has(parentId)) {
      throw new TreeModelError("NOT_FOUND", `Parent '${parentId}' not found`);
    }

    if (this.registry && parentId !== null) {
      const parentType = this.map.get(parentId)!.type;
      if (!this.registry.canBeChildOf(draft.type, parentType)) {
        throw new TreeModelError(
          "CONTAINMENT_VIOLATION",
          `'${draft.type}' cannot be a child of '${parentType}'`,
        );
      }
    }

    this.hooks.beforeAdd?.(draft, parentId);

    const siblings = this.getChildren(parentId);
    const pos =
      position !== undefined
        ? Math.max(0, Math.min(position, siblings.length))
        : siblings.length;

    for (const sib of siblings) {
      if (sib.position >= pos) {
        this.map.set(sib.id, { ...sib, position: sib.position + 1 });
      }
    }

    const now = new Date().toISOString();
    const obj: GraphObject = {
      status: null,
      description: "",
      color: null,
      tags: [],
      date: null,
      endDate: null,
      image: null,
      pinned: false,
      data: {},
      ...draft,
      id: objectId(draft.id ?? this.generateId()),
      parentId: parentId as ObjectId | null,
      position: pos,
      createdAt: draft.createdAt ?? now,
      updatedAt: now,
    };

    this.map.set(obj.id, obj);
    this.hooks.afterAdd?.(obj);
    this.emit({ kind: "add", object: obj });
    this.emit({ kind: "change" });
    return obj;
  }

  remove(
    id: string,
  ): { removed: GraphObject; descendants: GraphObject[] } | null {
    const obj = this.map.get(id);
    if (!obj) return null;

    this.hooks.beforeRemove?.(obj);

    const descendants: GraphObject[] = [];
    const collectDescendants = (nodeId: string) => {
      for (const child of this.getChildren(nodeId)) {
        descendants.push(child);
        collectDescendants(child.id);
      }
    };
    collectDescendants(id);

    this.map.delete(id);
    for (const desc of descendants) this.map.delete(desc.id);

    this.compactPositions(obj.parentId);

    this.hooks.afterRemove?.(obj, descendants);
    this.emit({ kind: "remove", object: obj, descendants });
    this.emit({ kind: "change" });
    return { removed: obj, descendants };
  }

  move(
    id: string,
    toParentId: string | null,
    toPosition?: number,
  ): GraphObject {
    const obj = this.map.get(id);
    if (!obj) throw new TreeModelError("NOT_FOUND", `Object '${id}' not found`);

    if (toParentId !== null) {
      if (toParentId === id) {
        throw new TreeModelError(
          "CIRCULAR_REF",
          `Cannot move '${id}' inside itself`,
        );
      }
      const descIds = new Set(
        this.getDescendants(id).map((d) => d.id as string),
      );
      if (descIds.has(toParentId)) {
        throw new TreeModelError(
          "CIRCULAR_REF",
          `Cannot move '${id}' inside its own descendant`,
        );
      }
      if (!this.map.has(toParentId)) {
        throw new TreeModelError(
          "NOT_FOUND",
          `Target parent '${toParentId}' not found`,
        );
      }
    }

    if (this.registry && toParentId !== null) {
      const parentType = this.map.get(toParentId)!.type;
      if (!this.registry.canBeChildOf(obj.type, parentType)) {
        throw new TreeModelError(
          "CONTAINMENT_VIOLATION",
          `'${obj.type}' cannot be a child of '${parentType}'`,
        );
      }
    }

    const newSiblings = this.getChildren(toParentId).filter(
      (s) => s.id !== id,
    );
    const pos =
      toPosition !== undefined
        ? Math.max(0, Math.min(toPosition, newSiblings.length))
        : newSiblings.length;

    this.hooks.beforeMove?.(obj, toParentId, pos);

    const from = { parentId: obj.parentId, position: obj.position };
    const changingParent = obj.parentId !== toParentId;

    this.map.set(id, {
      ...obj,
      parentId: toParentId as ObjectId | null,
      position: -1,
    });

    if (changingParent) this.compactPositions(obj.parentId);

    const freshSiblings = this.getChildren(toParentId).filter(
      (s) => s.id !== id,
    );
    for (const sib of freshSiblings) {
      if (sib.position >= pos) {
        this.map.set(sib.id, { ...sib, position: sib.position + 1 });
      }
    }

    const updated: GraphObject = {
      ...this.map.get(id)!,
      parentId: toParentId as ObjectId | null,
      position: pos,
      updatedAt: new Date().toISOString(),
    };
    this.map.set(id, updated);

    this.hooks.afterMove?.(updated);
    this.emit({
      kind: "move",
      object: updated,
      from,
      to: { parentId: toParentId, position: pos },
    });
    this.emit({ kind: "change" });
    return updated;
  }

  reparent(id: string, toParentId: string | null): GraphObject {
    return this.move(id, toParentId);
  }

  reorder(id: string, toPosition: number): GraphObject[] {
    const obj = this.map.get(id);
    if (!obj)
      throw new TreeModelError("NOT_FOUND", `Object '${id}' not found`);

    const siblings = this.getChildren(obj.parentId);
    const fromPos = siblings.findIndex((s) => s.id === id);
    if (fromPos === -1)
      throw new TreeModelError(
        "NOT_FOUND",
        `Object '${id}' not in sibling list`,
      );

    const clampedPos = Math.max(0, Math.min(toPosition, siblings.length));
    if (clampedPos === fromPos) return siblings;

    const reordered = [...siblings];
    const [moving] = reordered.splice(fromPos, 1);
    reordered.splice(clampedPos, 0, moving!);

    const updated: GraphObject[] = reordered.map((s, i) => {
      const u = { ...s, position: i, updatedAt: new Date().toISOString() };
      this.map.set(s.id, u);
      return u;
    });

    this.emit({
      kind: "reorder",
      parentId: obj.parentId,
      children: updated,
    });
    this.emit({ kind: "change" });
    return updated;
  }

  duplicate(id: string, options: DuplicateOptions = {}): GraphObject[] {
    const obj = this.map.get(id);
    if (!obj)
      throw new TreeModelError("NOT_FOUND", `Object '${id}' not found`);

    this.hooks.beforeDuplicate?.(obj);

    const { deep = false } = options;
    const targetParentId =
      "targetParentId" in options
        ? (options.targetParentId ?? null)
        : obj.parentId;

    const idMap = new Map<string, string>();
    const copies: GraphObject[] = [];
    const now = new Date().toISOString();

    const copyNode = (
      sourceId: string,
      copyParentId: string | null,
    ): GraphObject => {
      const source = this.map.get(sourceId)!;
      const newId = this.generateId();
      idMap.set(sourceId, newId);

      const siblings = this.getChildren(copyParentId);
      const refPos = sourceId === id ? source.position + 1 : source.position;

      for (const sib of siblings) {
        if (sib.position >= refPos) {
          this.map.set(sib.id, { ...sib, position: sib.position + 1 });
        }
      }

      const copy: GraphObject = {
        ...source,
        id: objectId(newId),
        parentId: copyParentId as ObjectId | null,
        position: refPos,
        createdAt: now,
        updatedAt: now,
      };
      this.map.set(newId, copy);
      copies.push(copy);

      if (deep) {
        for (const child of this.getChildren(sourceId)) {
          copyNode(child.id, newId);
        }
      }

      return copy;
    };

    copyNode(id, targetParentId);

    this.hooks.afterDuplicate?.(obj, copies);
    this.emit({ kind: "duplicate", original: obj, copies });
    this.emit({ kind: "change" });
    return copies;
  }

  update(
    id: string,
    changes: Partial<Omit<GraphObject, "id" | "type" | "createdAt">>,
  ): GraphObject {
    const obj = this.map.get(id);
    if (!obj)
      throw new TreeModelError("NOT_FOUND", `Object '${id}' not found`);

    this.hooks.beforeUpdate?.(obj, changes);

    const {
      id: _id,
      type: _type,
      createdAt: _created,
      ...safeChanges
    } = changes as GraphObject;
    const updated: GraphObject = {
      ...obj,
      ...safeChanges,
      updatedAt: new Date().toISOString(),
    };
    this.map.set(id, updated);

    this.hooks.afterUpdate?.(updated, obj);
    this.emit({ kind: "update", object: updated, previous: obj });
    this.emit({ kind: "change" });
    return updated;
  }

  // ── Query ───────────────────────────────────────────────────────────────────

  get(id: string): GraphObject | undefined {
    return this.map.get(id);
  }

  has(id: string): boolean {
    return this.map.has(id);
  }

  get size(): number {
    return this.map.size;
  }

  getChildren(parentId: string | null): GraphObject[] {
    return [...this.map.values()]
      .filter((o) => o.parentId === parentId)
      .sort((a, b) => a.position - b.position);
  }

  getDescendants(id: string): GraphObject[] {
    const result: GraphObject[] = [];
    const walk = (nodeId: string) => {
      for (const child of this.getChildren(nodeId)) {
        result.push(child);
        walk(child.id);
      }
    };
    walk(id);
    return result;
  }

  getAncestors(id: string): GraphObject[] {
    const ancestors: GraphObject[] = [];
    let current = this.map.get(id);
    while (current?.parentId) {
      const parent = this.map.get(current.parentId);
      if (!parent) break;
      ancestors.push(parent);
      current = parent;
    }
    return ancestors;
  }

  buildTree(): TreeNode[] {
    const nodeMap = new Map<string, TreeNode>();
    for (const obj of this.map.values()) {
      nodeMap.set(obj.id, { object: obj, children: [] });
    }
    const roots: TreeNode[] = [];
    for (const node of nodeMap.values()) {
      const parent = node.object.parentId
        ? nodeMap.get(node.object.parentId)
        : null;
      if (parent) parent.children.push(node);
      else roots.push(node);
    }
    const sort = (nodes: TreeNode[]) => {
      nodes.sort((a, b) => a.object.position - b.object.position);
      for (const n of nodes) sort(n.children);
    };
    sort(roots);
    return roots;
  }

  toArray(): GraphObject[] {
    return [...this.map.values()];
  }

  // ── Serialization ───────────────────────────────────────────────────────────

  toJSON(): GraphObject[] {
    return this.toArray();
  }

  static fromJSON(
    objects: GraphObject[],
    options: Omit<TreeModelOptions, "objects"> = {},
  ): TreeModel {
    return new TreeModel({ ...options, objects });
  }

  // ── Events ──────────────────────────────────────────────────────────────────

  on(listener: TreeModelEventListener): () => void {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }

  // ── Internal ────────────────────────────────────────────────────────────────

  private emit(event: TreeModelEvent): void {
    for (const listener of this.listeners) listener(event);
  }

  private compactPositions(parentId: string | null): void {
    const children = this.getChildren(parentId);
    children.forEach((child, i) => {
      if (child.position !== i) {
        this.map.set(child.id, { ...child, position: i });
      }
    });
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
