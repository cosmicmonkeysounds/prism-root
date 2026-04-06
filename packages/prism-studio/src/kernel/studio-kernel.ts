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
import { createPageBuilderRegistry } from "./entities.js";

// ── Kernel Interface ────────────────────────────────────────────────────────

export interface StudioKernel {
  readonly registry: ObjectRegistry<string>;
  readonly store: CollectionStore;
  readonly bus: PrismBus;
  readonly atoms: AtomStore;
  readonly objectAtoms: ObjectAtomStore;
  readonly undo: UndoRedoManager;
  readonly notifications: NotificationStore;

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

  /** Dispose all subscriptions. */
  dispose(): void;
}

// ── Helpers ─────────────────────────────────────────────────────────────────

let counter = 0;
function genId(): string {
  return `obj_${Date.now().toString(36)}_${(counter++).toString(36)}`;
}

// ── Factory ─────────────────────────────────────────────────────────────────

export function createStudioKernel(): StudioKernel {
  const registry = createPageBuilderRegistry();
  const store = createCollectionStore();
  const bus = createPrismBus();
  const atoms = createAtomStore();
  const objectAtoms = createObjectAtomStore();
  const notifications = createNotificationStore({ maxItems: 100 });

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

  // ── CRUD with undo + bus ────────────────────────────────────────────────

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

  function dispose(): void {
    disconnectAtoms();
    disconnectObjectAtoms();
    disconnectStoreSync();
  }

  return {
    registry,
    store,
    bus,
    atoms,
    objectAtoms,
    undo,
    notifications,
    createObject,
    updateObject,
    deleteObject,
    createEdge,
    deleteEdge,
    select,
    dispose,
  };
}
