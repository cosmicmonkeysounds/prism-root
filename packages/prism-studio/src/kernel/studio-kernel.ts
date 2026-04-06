/**
 * Studio Kernel — central nervous system wiring all Layer 1 systems.
 *
 * Creates and connects:
 *   - ObjectRegistry (entity/edge type definitions)
 *   - CollectionStore (Loro CRDT-backed object/edge storage)
 *   - PrismBus (typed event bus for cross-panel communication)
 *   - AtomStore (UI state atoms: selection, active panel, search)
 *   - ObjectAtomStore (in-memory object/edge cache with selectors)
 *   - UndoRedoManager (snapshot-based undo/redo)
 *   - NotificationStore (toast/alert queue)
 *   - SearchEngine (full-text TF-IDF search)
 *   - ActivityStore + ActivityTracker (audit trail)
 *   - LiveView (materialized filtered/sorted/grouped projection)
 *
 * The kernel is instantiated once at app startup and provided via React
 * context to all components.
 */

import { ObjectRegistry } from "@prism/core/object-model";
import type { GraphObject, ObjectEdge, ObjectId, EdgeId } from "@prism/core/object-model";
import { objectId, edgeId } from "@prism/core/object-model";
import { createCollectionStore } from "@prism/core/persistence";
import type { CollectionStore } from "@prism/core/persistence";
import {
  createPrismBus,
  PrismEvents,
  createAtomStore,
  createObjectAtomStore,
  connectBusToAtoms,
  connectBusToObjectAtoms,
} from "@prism/core/atom";
import type { PrismBus, AtomStore, ObjectAtomStore } from "@prism/core/atom";
import { UndoRedoManager } from "@prism/core/undo";
import type { ObjectSnapshot } from "@prism/core/undo";
import { createNotificationStore } from "@prism/core/notification";
import type { NotificationStore } from "@prism/core/notification";
import { createSearchEngine } from "@prism/core/search";
import type { SearchEngine } from "@prism/core/search";
import { createActivityStore } from "@prism/core/activity";
import type { ActivityStore } from "@prism/core/activity";
import { createActivityTracker } from "@prism/core/activity";
import type { ActivityTracker } from "@prism/core/activity";
import { createLiveView } from "@prism/core/view";
import type { LiveView, LiveViewOptions } from "@prism/core/view";
import type { ObjectTemplate, TemplateNode, InstantiateResult } from "@prism/core/template";
import { createPageBuilderRegistry } from "./entities.js";
import { createRelayManager } from "./relay-manager.js";
import type { RelayManager } from "./relay-manager.js";

// ── Clipboard Types ────────────────────────────────────────────────────────

export interface ClipboardEntry {
  mode: "copy" | "cut";
  objects: GraphObject[];
  edges: ObjectEdge[];
}

export interface PasteResult {
  created: GraphObject[];
  idMap: Map<string, string>;
}

// ── Kernel Interface ────────────────────────────────────────────────────────

export interface StudioKernel {
  readonly registry: ObjectRegistry<string>;
  readonly store: CollectionStore;
  readonly bus: PrismBus;
  readonly atoms: AtomStore;
  readonly objectAtoms: ObjectAtomStore;
  readonly undo: UndoRedoManager;
  readonly notifications: NotificationStore;
  readonly search: SearchEngine;
  readonly activity: ActivityStore;
  readonly activityTracker: ActivityTracker;
  readonly relay: RelayManager;

  /** Create a new object in the collection, emit bus event, push undo. */
  createObject(obj: Omit<GraphObject, "id" | "createdAt" | "updatedAt">): GraphObject;

  /** Update an existing object, emit bus event, push undo. */
  updateObject(id: ObjectId, patch: Partial<GraphObject>): GraphObject | undefined;

  /** Delete an object (soft-delete), emit bus event, push undo. */
  deleteObject(id: ObjectId): boolean;

  /** Create an edge, emit bus event. */
  createEdge(edge: Omit<ObjectEdge, "id" | "createdAt">): ObjectEdge;

  /** Delete an edge, emit bus event. */
  deleteEdge(id: EdgeId): boolean;

  /** Select an object (updates atoms + emits bus event). */
  select(id: ObjectId | null): void;

  // ── Clipboard ────────────────────────────────────────────────────────────

  /** Copy object subtrees to clipboard. */
  clipboardCopy(ids: ObjectId[]): void;

  /** Cut object subtrees to clipboard (deleted on paste). */
  clipboardCut(ids: ObjectId[]): void;

  /** Paste clipboard contents under a target parent. */
  clipboardPaste(parentId: ObjectId | null, position?: number): PasteResult | null;

  /** Whether the clipboard has content. */
  readonly clipboardHasContent: boolean;

  // ── Batch Operations ─────────────────────────────────────────────────────

  /** Execute multiple operations atomically with a single undo entry. */
  batch(
    label: string,
    operations: Array<
      | { kind: "create"; draft: Omit<GraphObject, "id" | "createdAt" | "updatedAt"> }
      | { kind: "update"; id: ObjectId; patch: Partial<GraphObject> }
      | { kind: "delete"; id: ObjectId }
    >,
  ): GraphObject[];

  // ── Templates ────────────────────────────────────────────────────────────

  /** Register a reusable template. */
  registerTemplate(template: ObjectTemplate): void;

  /** List all registered templates, optionally filtered by category. */
  listTemplates(category?: string): ObjectTemplate[];

  /** Instantiate a template under a parent. */
  instantiateTemplate(
    templateId: string,
    options?: { parentId?: ObjectId | null; position?: number; variables?: Record<string, string> },
  ): InstantiateResult | null;

  // ── LiveView ─────────────────────────────────────────────────────────────

  /** Create a live materialized view of the collection. */
  createLiveView(options?: LiveViewOptions): LiveView;

  /** Dispose all subscriptions. */
  dispose(): void;
}

// ── Helpers ─────────────────────────────────────────────────────────────────

let counter = 0;
function genId(): string {
  return `obj_${Date.now().toString(36)}_${(counter++).toString(36)}`;
}

const VARIABLE_RE = /\{\{(\w+)\}\}/g;

function interpolate(str: string, vars: Record<string, string>): string {
  return str.replace(VARIABLE_RE, (_match, name: string) => vars[name] ?? `{{${name}}}`);
}

// ── TrackableStore adapter for CollectionStore ─────────────────────────────

function createTrackableAdapter(store: CollectionStore) {
  return {
    get(id: string): unknown {
      return store.getObject(objectId(id));
    },
    subscribeObject(id: string, cb: (obj: unknown) => void): () => void {
      return store.onChange((changes) => {
        for (const change of changes) {
          if (change.id === id && (change.type === "object-put" || change.type === "object-remove")) {
            cb(store.getObject(objectId(id)));
          }
        }
      });
    },
  };
}

// ── Factory ─────────────────────────────────────────────────────────────────

export function createStudioKernel(): StudioKernel {
  const registry = createPageBuilderRegistry();
  const store = createCollectionStore();
  const bus = createPrismBus();
  const atoms = createAtomStore();
  const objectAtoms = createObjectAtomStore();
  const notifications = createNotificationStore({ maxItems: 100 });
  const search = createSearchEngine();
  const activityStore = createActivityStore();
  const tracker = createActivityTracker({ activityStore });
  const trackableAdapter = createTrackableAdapter(store);

  const relay = createRelayManager();

  // Index the default collection so search covers all kernel objects
  search.indexCollection("default", store);

  // Wire bus → atom stores (selection, navigation, object cache)
  const disconnectAtoms = connectBusToAtoms(bus, atoms);
  const disconnectObjectAtoms = connectBusToObjectAtoms(bus, objectAtoms);

  // Undo applier — restores snapshots to CollectionStore
  const undoApplier = (snapshots: ObjectSnapshot[], direction: "undo" | "redo") => {
    for (const snap of snapshots) {
      if (snap.kind === "object") {
        const target = direction === "undo" ? snap.before : snap.after;
        if (target) {
          store.putObject(target);
          objectAtoms.getState().setObject(target);
          bus.emit(PrismEvents.ObjectUpdated, { object: target });
        } else {
          const source = direction === "undo" ? snap.after : snap.before;
          if (source) {
            store.removeObject(source.id);
            objectAtoms.getState().removeObject(source.id);
            bus.emit(PrismEvents.ObjectDeleted, { id: source.id });
          }
        }
      } else {
        const target = direction === "undo" ? snap.before : snap.after;
        if (target) {
          store.putEdge(target);
          objectAtoms.getState().setEdge(target);
          bus.emit(PrismEvents.EdgeCreated, { edge: target });
        } else {
          const source = direction === "undo" ? snap.after : snap.before;
          if (source) {
            store.removeEdge(source.id);
            objectAtoms.getState().removeEdge(source.id);
            bus.emit(PrismEvents.EdgeDeleted, { id: source.id });
          }
        }
      }
    }
  };

  const undo = new UndoRedoManager(undoApplier, { maxHistory: 200 });

  // Sync CollectionStore changes → ObjectAtomStore (keeps cache in sync)
  const disconnectStoreSync = store.onChange((changes) => {
    for (const change of changes) {
      if (change.type === "object-put") {
        const obj = store.getObject(objectId(change.id));
        if (obj) objectAtoms.getState().setObject(obj);
      } else if (change.type === "object-remove") {
        objectAtoms.getState().removeObject(change.id);
      } else if (change.type === "edge-put") {
        const edge = store.getEdge(edgeId(change.id));
        if (edge) objectAtoms.getState().setEdge(edge);
      } else if (change.type === "edge-remove") {
        objectAtoms.getState().removeEdge(change.id);
      }
    }
  });

  // Template storage
  const templates = new Map<string, ObjectTemplate>();

  // Clipboard storage
  let clipboardEntry: ClipboardEntry | null = null;

  // ── Internal helpers ─────────────────────────────────────────────────────

  function getDescendants(id: ObjectId): GraphObject[] {
    const result: GraphObject[] = [];
    const queue = store.listObjects({ parentId: id }).filter((o) => !o.deletedAt);
    while (queue.length > 0) {
      const obj = queue.shift() as GraphObject;
      result.push(obj);
      queue.push(...store.listObjects({ parentId: obj.id }).filter((o) => !o.deletedAt));
    }
    return result;
  }

  function collectSubtree(id: ObjectId): { objects: GraphObject[]; edges: ObjectEdge[] } {
    const root = store.getObject(id);
    if (!root) return { objects: [], edges: [] };

    const descendants = getDescendants(id);
    const all = [root, ...descendants];
    const allIds = new Set(all.map((o) => o.id as string));

    // Collect internal edges (both endpoints in subtree)
    const internalEdges: ObjectEdge[] = [];
    for (const edge of store.allEdges()) {
      if (allIds.has(edge.sourceId as string) && allIds.has(edge.targetId as string)) {
        internalEdges.push(edge);
      }
    }

    return { objects: all, edges: internalEdges };
  }

  // ── CRUD with undo + bus + activity tracking ───────────────────────────

  function createObject(
    partial: Omit<GraphObject, "id" | "createdAt" | "updatedAt">,
  ): GraphObject {
    const now = new Date().toISOString();
    const obj: GraphObject = {
      ...partial,
      id: objectId(genId()),
      createdAt: now,
      updatedAt: now,
    };

    store.putObject(obj);
    bus.emit(PrismEvents.ObjectCreated, { object: obj });

    undo.push(`Create ${obj.type} "${obj.name}"`, [
      { kind: "object", before: null, after: structuredClone(obj) },
    ]);

    // Track activity
    activityStore.record({
      objectId: obj.id,
      verb: "created",
      meta: { objectType: obj.type, objectName: obj.name },
    });

    // Start tracking future changes
    tracker.track(obj.id, trackableAdapter);

    return obj;
  }

  function updateObject(
    id: ObjectId,
    patch: Partial<GraphObject>,
  ): GraphObject | undefined {
    const before = store.getObject(id);
    if (!before) return undefined;

    const after: GraphObject = {
      ...before,
      ...patch,
      id: before.id, // never overwrite identity
      updatedAt: new Date().toISOString(),
    };

    store.putObject(after);
    bus.emit(PrismEvents.ObjectUpdated, { object: after });

    undo.push(`Update ${after.type} "${after.name}"`, [
      { kind: "object", before: structuredClone(before), after: structuredClone(after) },
    ]);

    return after;
  }

  function deleteObject(id: ObjectId): boolean {
    const before = store.getObject(id);
    if (!before) return false;

    const after: GraphObject = {
      ...before,
      deletedAt: new Date().toISOString(),
      updatedAt: new Date().toISOString(),
    };

    store.putObject(after);
    bus.emit(PrismEvents.ObjectUpdated, { object: after });

    undo.push(`Delete ${before.type} "${before.name}"`, [
      { kind: "object", before: structuredClone(before), after: structuredClone(after) },
    ]);

    activityStore.record({
      objectId: id,
      verb: "deleted",
      meta: { objectType: before.type, objectName: before.name },
    });

    return true;
  }

  function createEdge(
    partial: Omit<ObjectEdge, "id" | "createdAt">,
  ): ObjectEdge {
    const edge: ObjectEdge = {
      ...partial,
      id: edgeId(genId()),
      createdAt: new Date().toISOString(),
    };

    store.putEdge(edge);
    bus.emit(PrismEvents.EdgeCreated, { edge });

    undo.push(`Create edge "${edge.relation}"`, [
      { kind: "edge", before: null, after: structuredClone(edge) },
    ]);

    return edge;
  }

  function deleteEdge(id: EdgeId): boolean {
    const before = store.getEdge(id);
    if (!before) return false;

    store.removeEdge(id);
    bus.emit(PrismEvents.EdgeDeleted, { id });

    undo.push(`Delete edge "${before.relation}"`, [
      { kind: "edge", before: structuredClone(before), after: null },
    ]);

    return true;
  }

  function select(id: ObjectId | null): void {
    atoms.getState().setSelectedId(id);
    bus.emit(PrismEvents.SelectionChanged, { ids: id ? [id] : [] });
  }

  // ── Clipboard ────────────────────────────────────────────────────────────

  function clipboardCopy(ids: ObjectId[]): void {
    const objects: GraphObject[] = [];
    const edges: ObjectEdge[] = [];
    const allIds = new Set<string>();

    for (const id of ids) {
      const { objects: sub, edges: subEdges } = collectSubtree(id);
      for (const o of sub) {
        if (!allIds.has(o.id as string)) {
          allIds.add(o.id as string);
          objects.push(structuredClone(o));
        }
      }
      edges.push(...subEdges.map((e) => structuredClone(e)));
    }

    clipboardEntry = { mode: "copy", objects, edges };
  }

  function clipboardCut(ids: ObjectId[]): void {
    clipboardCopy(ids);
    if (clipboardEntry) {
      clipboardEntry.mode = "cut";
    }
  }

  function clipboardPaste(parentId: ObjectId | null, position?: number): PasteResult | null {
    if (!clipboardEntry) return null;

    const { mode, objects, edges: clipEdges } = clipboardEntry;
    const idMap = new Map<string, string>();
    const created: GraphObject[] = [];
    const snapshots: ObjectSnapshot[] = [];

    // Generate new IDs for all objects
    for (const obj of objects) {
      idMap.set(obj.id as string, genId());
    }

    // Sort by depth (roots first) — objects whose parentId is NOT in the set are roots
    const rootIds = new Set(
      objects
        .filter((o) => !idMap.has(o.parentId as string))
        .map((o) => o.id as string),
    );

    const now = new Date().toISOString();
    let posCounter = position ?? 0;

    for (const obj of objects) {
      const newId = objectId(idMap.get(obj.id as string) as string);
      const isRoot = rootIds.has(obj.id as string);

      const newObj: GraphObject = {
        ...obj,
        id: newId,
        parentId: isRoot ? parentId : objectId(idMap.get(obj.parentId as string) as string),
        position: isRoot ? posCounter++ : obj.position,
        createdAt: now,
        updatedAt: now,
        deletedAt: null,
      };

      store.putObject(newObj);
      bus.emit(PrismEvents.ObjectCreated, { object: newObj });
      created.push(newObj);
      snapshots.push({ kind: "object", before: null, after: structuredClone(newObj) });

      tracker.track(newObj.id, trackableAdapter);
    }

    // Re-create internal edges with remapped IDs
    for (const edge of clipEdges) {
      const newSourceId = idMap.get(edge.sourceId as string);
      const newTargetId = idMap.get(edge.targetId as string);
      if (newSourceId && newTargetId) {
        const newEdge: ObjectEdge = {
          ...edge,
          id: edgeId(genId()),
          sourceId: objectId(newSourceId),
          targetId: objectId(newTargetId),
          createdAt: now,
        };
        store.putEdge(newEdge);
        bus.emit(PrismEvents.EdgeCreated, { edge: newEdge });
        snapshots.push({ kind: "edge", before: null, after: structuredClone(newEdge) });
      }
    }

    undo.push(`Paste ${created.length} object(s)`, snapshots);

    // If cut, delete the originals
    if (mode === "cut") {
      const cutSnapshots: ObjectSnapshot[] = [];
      for (const obj of objects) {
        const original = store.getObject(obj.id);
        if (original) {
          const deleted: GraphObject = {
            ...original,
            deletedAt: now,
            updatedAt: now,
          };
          store.putObject(deleted);
          bus.emit(PrismEvents.ObjectUpdated, { object: deleted });
          cutSnapshots.push({
            kind: "object",
            before: structuredClone(original),
            after: structuredClone(deleted),
          });
        }
      }
      if (cutSnapshots.length > 0) {
        undo.push(`Cut ${cutSnapshots.length} object(s)`, cutSnapshots);
      }
      clipboardEntry = null; // cut is one-time
    }

    return { created, idMap };
  }

  // ── Batch Operations ─────────────────────────────────────────────────────

  function batchOps(
    label: string,
    operations: Array<
      | { kind: "create"; draft: Omit<GraphObject, "id" | "createdAt" | "updatedAt"> }
      | { kind: "update"; id: ObjectId; patch: Partial<GraphObject> }
      | { kind: "delete"; id: ObjectId }
    >,
  ): GraphObject[] {
    const snapshots: ObjectSnapshot[] = [];
    const results: GraphObject[] = [];
    const now = new Date().toISOString();

    for (const op of operations) {
      switch (op.kind) {
        case "create": {
          const obj: GraphObject = {
            ...op.draft,
            id: objectId(genId()),
            createdAt: now,
            updatedAt: now,
          };
          store.putObject(obj);
          bus.emit(PrismEvents.ObjectCreated, { object: obj });
          snapshots.push({ kind: "object", before: null, after: structuredClone(obj) });
          results.push(obj);
          tracker.track(obj.id, trackableAdapter);
          break;
        }
        case "update": {
          const before = store.getObject(op.id);
          if (before) {
            const after: GraphObject = {
              ...before,
              ...op.patch,
              id: before.id,
              updatedAt: now,
            };
            store.putObject(after);
            bus.emit(PrismEvents.ObjectUpdated, { object: after });
            snapshots.push({
              kind: "object",
              before: structuredClone(before),
              after: structuredClone(after),
            });
            results.push(after);
          }
          break;
        }
        case "delete": {
          const before = store.getObject(op.id);
          if (before) {
            const after: GraphObject = {
              ...before,
              deletedAt: now,
              updatedAt: now,
            };
            store.putObject(after);
            bus.emit(PrismEvents.ObjectUpdated, { object: after });
            snapshots.push({
              kind: "object",
              before: structuredClone(before),
              after: structuredClone(after),
            });
            results.push(after);
          }
          break;
        }
      }
    }

    if (snapshots.length > 0) {
      undo.push(label, snapshots);
    }

    return results;
  }

  // ── Templates ────────────────────────────────────────────────────────────

  function registerTemplate(template: ObjectTemplate): void {
    templates.set(template.id, template);
  }

  function listTemplates(category?: string): ObjectTemplate[] {
    const all = [...templates.values()];
    if (!category) return all;
    return all.filter((t) => t.category === category);
  }

  function instantiateTemplateNode(
    node: TemplateNode,
    parentId: ObjectId | null,
    position: number,
    vars: Record<string, string>,
    idMap: Map<string, string>,
    created: GraphObject[],
    snapshots: ObjectSnapshot[],
  ): void {
    const now = new Date().toISOString();
    const newId = genId();
    idMap.set(node.placeholderId, newId);

    // Interpolate string fields
    const data: Record<string, unknown> = {};
    if (node.data) {
      for (const [k, v] of Object.entries(node.data)) {
        data[k] = typeof v === "string" ? interpolate(v, vars) : v;
      }
    }

    const obj: GraphObject = {
      type: node.type,
      name: interpolate(node.name, vars),
      parentId,
      position,
      status: node.status ? interpolate(node.status, vars) : null,
      tags: node.tags ?? [],
      date: null,
      endDate: null,
      description: node.description ? interpolate(node.description, vars) : "",
      color: node.color ?? null,
      image: null,
      pinned: node.pinned ?? false,
      data,
      id: objectId(newId),
      createdAt: now,
      updatedAt: now,
    };

    store.putObject(obj);
    bus.emit(PrismEvents.ObjectCreated, { object: obj });
    created.push(obj);
    snapshots.push({ kind: "object", before: null, after: structuredClone(obj) });
    tracker.track(obj.id, trackableAdapter);

    // Recurse into children
    if (node.children) {
      for (let i = 0; i < node.children.length; i++) {
        instantiateTemplateNode(
          node.children[i] as TemplateNode,
          obj.id,
          i,
          vars,
          idMap,
          created,
          snapshots,
        );
      }
    }
  }

  function instantiateTemplate(
    templateId: string,
    options?: { parentId?: ObjectId | null; position?: number; variables?: Record<string, string> },
  ): InstantiateResult | null {
    const template = templates.get(templateId);
    if (!template) return null;

    const vars = options?.variables ?? {};
    const parentId = options?.parentId ?? null;
    const position = options?.position ?? 0;
    const idMap = new Map<string, string>();
    const created: GraphObject[] = [];
    const createdEdges: ObjectEdge[] = [];
    const snapshots: ObjectSnapshot[] = [];

    instantiateTemplateNode(template.root, parentId, position, vars, idMap, created, snapshots);

    // Create template edges with remapped IDs
    if (template.edges) {
      const now = new Date().toISOString();
      for (const te of template.edges) {
        const sourceId = idMap.get(te.sourcePlaceholderId);
        const targetId = idMap.get(te.targetPlaceholderId);
        if (sourceId && targetId) {
          const edge: ObjectEdge = {
            id: edgeId(genId()),
            sourceId: objectId(sourceId),
            targetId: objectId(targetId),
            relation: te.relation,
            data: te.data ?? {},
            createdAt: now,
          };
          store.putEdge(edge);
          bus.emit(PrismEvents.EdgeCreated, { edge });
          createdEdges.push(edge);
          snapshots.push({ kind: "edge", before: null, after: structuredClone(edge) });
        }
      }
    }

    if (snapshots.length > 0) {
      undo.push(`Instantiate template "${template.name}"`, snapshots);
    }

    return { created, createdEdges, idMap };
  }

  // ── LiveView factory ─────────────────────────────────────────────────────

  function makeLiveView(options?: LiveViewOptions): LiveView {
    return createLiveView(store, options);
  }

  // ── Dispose ──────────────────────────────────────────────────────────────

  function dispose(): void {
    relay.dispose();
    disconnectAtoms();
    disconnectObjectAtoms();
    disconnectStoreSync();
    tracker.untrackAll();
  }

  return {
    registry,
    store,
    bus,
    atoms,
    objectAtoms,
    undo,
    notifications,
    search,
    activity: activityStore,
    activityTracker: tracker,
    relay,
    createObject,
    updateObject,
    deleteObject,
    createEdge,
    deleteEdge,
    select,
    clipboardCopy,
    clipboardCut,
    clipboardPaste,
    get clipboardHasContent() {
      return clipboardEntry !== null;
    },
    batch: batchOps,
    registerTemplate,
    listTemplates,
    instantiateTemplate,
    createLiveView: makeLiveView,
    dispose,
  };
}
