/**
 * Prism Atom Store — Zustand-based reactive state atoms.
 *
 * Adapted from legacy @core/atom nanostores pattern to use Zustand.
 * createAtomStore() creates an isolated store instance (no singletons).
 *
 * Atoms:
 *   selectedId        — single focused object
 *   selectionIds      — multi-selection
 *   editingObjectId   — ID of the object in edit mode
 *   activePanel       — open side panel name
 *   searchQuery       — committed search string
 *   navigationTarget  — last navigation target payload
 */

import { createStore } from "zustand/vanilla";
import type { ObjectId } from "../object-model/types.js";

// ── State ────────────────────────────────────────────────────────────────────

export interface NavigationTarget {
  type: string;
  [key: string]: unknown;
}

export interface AtomState {
  selectedId: ObjectId | null;
  selectionIds: ObjectId[];
  editingObjectId: ObjectId | null;
  activePanel: string | null;
  searchQuery: string;
  navigationTarget: NavigationTarget | null;
}

export interface AtomActions {
  setSelectedId: (id: ObjectId | null) => void;
  setSelectionIds: (ids: ObjectId[]) => void;
  setEditingObjectId: (id: ObjectId | null) => void;
  setActivePanel: (panel: string | null) => void;
  setSearchQuery: (query: string) => void;
  setNavigationTarget: (target: NavigationTarget | null) => void;
  reset: () => void;
}

export type AtomStore = ReturnType<typeof createAtomStore>;

const INITIAL_STATE: AtomState = {
  selectedId: null,
  selectionIds: [],
  editingObjectId: null,
  activePanel: null,
  searchQuery: "",
  navigationTarget: null,
};

export function createAtomStore() {
  return createStore<AtomState & AtomActions>()((set) => ({
    ...INITIAL_STATE,

    setSelectedId: (id) => set({ selectedId: id }),
    setSelectionIds: (ids) => set({ selectionIds: ids }),
    setEditingObjectId: (id) => set({ editingObjectId: id }),
    setActivePanel: (panel) => set({ activePanel: panel }),
    setSearchQuery: (query) => set({ searchQuery: query }),
    setNavigationTarget: (target) => set({ navigationTarget: target }),
    reset: () => set(INITIAL_STATE),
  }));
}
