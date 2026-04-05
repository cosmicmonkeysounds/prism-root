/**
 * Object Atom Store — Zustand-based in-memory object/edge cache with derived queries.
 *
 * The object cache is the in-memory mirror of the CRDT state. It is populated by:
 *   1. connectBusToAtoms()  — keeps it in sync with action events
 *   2. Direct setObjects() — seeds it from initial data
 *
 * Derived selectors:
 *   selectObject(id)         — single object by ID
 *   selectQuery(predicate)   — filtered subset
 *   selectChildren(parentId) — direct children, sorted by position
 *   selectEdgesFrom(id)      — outgoing edges
 *   selectEdgesTo(id)        — incoming edges
 */

import { createStore } from "zustand/vanilla";
import type { GraphObject, ObjectEdge, ObjectId } from "../object-model/types.js";

// ── State ────────────────────────────────────────────────────────────────────

export interface ObjectAtomState {
  objects: Record<string, GraphObject>;
  edges: Record<string, ObjectEdge>;
}

export interface ObjectAtomActions {
  setObject: (obj: GraphObject) => void;
  setObjects: (objs: GraphObject[]) => void;
  removeObject: (id: string) => void;
  moveObject: (id: string, newParentId: ObjectId | null) => void;

  setEdge: (edge: ObjectEdge) => void;
  removeEdge: (id: string) => void;

  clear: () => void;
}

export type ObjectAtomStore = ReturnType<typeof createObjectAtomStore>;

// ── Selectors ────────────────────────────────────────────────────────────────

export function selectObject(
  state: ObjectAtomState,
  id: string,
): GraphObject | undefined {
  return state.objects[id];
}

export function selectQuery(
  state: ObjectAtomState,
  predicate: (obj: GraphObject) => boolean,
  sort?: (a: GraphObject, b: GraphObject) => number,
): GraphObject[] {
  const results = Object.values(state.objects).filter(predicate);
  if (sort) results.sort(sort);
  return results;
}

export function selectChildren(
  state: ObjectAtomState,
  parentId: string,
): GraphObject[] {
  return Object.values(state.objects)
    .filter((obj) => obj.parentId === parentId)
    .sort((a, b) => a.position - b.position);
}

export function selectEdgesFrom(
  state: ObjectAtomState,
  sourceId: string,
): ObjectEdge[] {
  return Object.values(state.edges).filter((e) => e.sourceId === sourceId);
}

export function selectEdgesTo(
  state: ObjectAtomState,
  targetId: string,
): ObjectEdge[] {
  return Object.values(state.edges).filter((e) => e.targetId === targetId);
}

export function selectAllObjects(state: ObjectAtomState): GraphObject[] {
  return Object.values(state.objects);
}

export function selectAllEdges(state: ObjectAtomState): ObjectEdge[] {
  return Object.values(state.edges);
}

// ── Store factory ────────────────────────────────────────────────────────────

export function createObjectAtomStore() {
  return createStore<ObjectAtomState & ObjectAtomActions>()((set) => ({
    objects: {},
    edges: {},

    setObject: (obj) =>
      set((state) => ({
        objects: { ...state.objects, [obj.id]: obj },
      })),

    setObjects: (objs) =>
      set((state) => {
        const next = { ...state.objects };
        for (const obj of objs) next[obj.id] = obj;
        return { objects: next };
      }),

    removeObject: (id) =>
      set((state) => {
        const next = { ...state.objects };
        Reflect.deleteProperty(next, id);
        return { objects: next };
      }),

    moveObject: (id, newParentId) =>
      set((state) => {
        const obj = state.objects[id];
        if (!obj) return state;
        return {
          objects: { ...state.objects, [id]: { ...obj, parentId: newParentId } },
        };
      }),

    setEdge: (edge) =>
      set((state) => ({
        edges: { ...state.edges, [edge.id]: edge },
      })),

    removeEdge: (id) =>
      set((state) => {
        const next = { ...state.edges };
        Reflect.deleteProperty(next, id);
        return { edges: next };
      }),

    clear: () => set({ objects: {}, edges: {} }),
  }));
}
