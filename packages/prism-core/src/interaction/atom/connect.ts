/**
 * Bus-to-Atom bridges — wire PrismBus events into atom stores.
 *
 * These are the only files that bridge the push world (PrismBus events)
 * into the pull world (Zustand atom stores).
 *
 * Call once at app startup. Returns a cleanup function.
 */

import type { PrismBus } from "./event-bus.js";
import { PrismEvents } from "./event-bus.js";
import type { AtomStore } from "./atoms.js";
import type { ObjectAtomStore } from "./object-atoms.js";
import type { GraphObject, ObjectEdge, ObjectId } from "@prism/core/object-model";
import { objectId } from "@prism/core/object-model";

// ── Event payload types ──────────────────────────────────────────────────────

interface ObjectPayload {
  object: GraphObject;
}

interface ObjectDeletedPayload {
  id: string;
}

interface ObjectMovedPayload {
  id: string;
  newParentId?: string;
}

interface EdgePayload {
  edge: ObjectEdge;
}

interface EdgeDeletedPayload {
  id: string;
}

interface NavigationPayload {
  target: { type: string; id?: string; [key: string]: unknown };
}

interface PanelToggledPayload {
  panel: string;
  open: boolean;
}

interface EditModePayload {
  objectId: string;
  editing: boolean;
}

interface SelectionPayload {
  ids: string[];
}

interface SearchPayload {
  query: string;
}

// ── connectBusToObjectAtoms ──────────────────────────────────────────────────

/**
 * Wire object/edge events from the bus into the ObjectAtomStore.
 * Returns a cleanup function that unsubscribes all handlers.
 */
export function connectBusToObjectAtoms(
  bus: PrismBus,
  store: ObjectAtomStore,
): () => void {
  const off: Array<() => void> = [];

  off.push(
    bus.on<ObjectPayload>(PrismEvents.ObjectCreated, ({ object }) => {
      store.getState().setObject(object);
    }),
  );

  off.push(
    bus.on<ObjectPayload>(PrismEvents.ObjectUpdated, ({ object }) => {
      store.getState().setObject(object);
    }),
  );

  off.push(
    bus.on<ObjectDeletedPayload>(PrismEvents.ObjectDeleted, ({ id }) => {
      store.getState().removeObject(id);
    }),
  );

  off.push(
    bus.on<ObjectMovedPayload>(PrismEvents.ObjectMoved, ({ id, newParentId }) => {
      store
        .getState()
        .moveObject(id, newParentId ? (newParentId as ObjectId) : null);
    }),
  );

  off.push(
    bus.on<EdgePayload>(PrismEvents.EdgeCreated, ({ edge }) => {
      store.getState().setEdge(edge);
    }),
  );

  off.push(
    bus.on<EdgeDeletedPayload>(PrismEvents.EdgeDeleted, ({ id }) => {
      store.getState().removeEdge(id);
    }),
  );

  return () => off.forEach((fn) => fn());
}

// ── connectBusToAtoms ────────────────────────────────────────────────────────

/**
 * Wire UI/navigation events from the bus into the AtomStore.
 * Returns a cleanup function that unsubscribes all handlers.
 */
export function connectBusToAtoms(
  bus: PrismBus,
  store: AtomStore,
): () => void {
  const off: Array<() => void> = [];

  off.push(
    bus.on<NavigationPayload>(
      PrismEvents.NavigationNavigate,
      ({ target }) => {
        store.getState().setNavigationTarget(target);
        if (target.type === "object" && typeof target.id === "string") {
          store.getState().setSelectedId(objectId(target.id));
        }
      },
    ),
  );

  off.push(
    bus.on<PanelToggledPayload>(
      PrismEvents.NavigationPanelToggled,
      ({ panel, open }) => {
        const current = store.getState().activePanel;
        store.getState().setActivePanel(open ? panel : current === panel ? null : current);
      },
    ),
  );

  off.push(
    bus.on<EditModePayload>(PrismEvents.EditModeChanged, ({ objectId: oid, editing }) => {
      store.getState().setEditingObjectId(editing ? objectId(oid) : null);
    }),
  );

  off.push(
    bus.on<SelectionPayload>(PrismEvents.SelectionChanged, ({ ids }) => {
      store.getState().setSelectionIds(ids.map((id) => objectId(id)));
    }),
  );

  off.push(
    bus.on<SearchPayload>(PrismEvents.SearchCommit, ({ query }) => {
      store.getState().setSearchQuery(query);
    }),
  );

  off.push(
    bus.on(PrismEvents.SearchClear, () => {
      store.getState().setSearchQuery("");
    }),
  );

  return () => off.forEach((fn) => fn());
}
