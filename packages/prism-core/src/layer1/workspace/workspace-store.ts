/**
 * Shell Store — Zustand vanilla store for IDE shell state.
 *
 * Manages tabs, active tab, and panel layout for the editor shell.
 * This is UI infrastructure, NOT the spec's "Workspace" (Manifest
 * pointing to Collections). See SPEC.md §234.
 *
 * Follows the same factory function pattern as createCrdtStore()
 * and createGraphStore().
 */

import { createStore } from "zustand/vanilla";
import type { StoreApi } from "zustand";
import type { LensId, TabId } from "./lens-types.js";
import { tabId } from "./lens-types.js";

export interface TabEntry {
  id: TabId;
  lensId: LensId;
  label: string;
  pinned: boolean;
  order: number;
}

export interface PanelLayout {
  sidebar: boolean;
  inspector: boolean;
  sidebarWidth: number;
  inspectorWidth: number;
}

export interface ShellState {
  tabs: TabEntry[];
  activeTabId: TabId | null;
  panelLayout: PanelLayout;
}

export interface ShellActions {
  openTab(lensId: LensId, label: string): TabId;
  closeTab(id: TabId): void;
  pinTab(id: TabId): void;
  unpinTab(id: TabId): void;
  reorderTab(id: TabId, newOrder: number): void;
  setActiveTab(id: TabId): void;
  toggleSidebar(): void;
  toggleInspector(): void;
  setSidebarWidth(width: number): void;
  setInspectorWidth(width: number): void;
}

export type ShellStore = ShellState & ShellActions;

function makeTabId(): TabId {
  return tabId(`tab-${Date.now()}-${Math.random().toString(36).slice(2, 7)}`);
}

export function createShellStore(): StoreApi<ShellStore> {
  return createStore<ShellStore>((set, get) => ({
    tabs: [],
    activeTabId: null,
    panelLayout: {
      sidebar: true,
      inspector: false,
      sidebarWidth: 20,
      inspectorWidth: 25,
    },

    openTab(lensId: LensId, label: string): TabId {
      const state = get();

      // Singleton: focus existing unpinned tab with same lensId
      const existing = state.tabs.find(
        (t) => t.lensId === lensId && !t.pinned,
      );
      if (existing) {
        set({ activeTabId: existing.id });
        return existing.id;
      }

      const id = makeTabId();
      const order =
        state.tabs.length > 0
          ? Math.max(...state.tabs.map((t) => t.order)) + 1
          : 0;
      const tab: TabEntry = { id, lensId, label, pinned: false, order };

      set({ tabs: [...state.tabs, tab], activeTabId: id });
      return id;
    },

    closeTab(id: TabId): void {
      const state = get();
      const sorted = [...state.tabs].sort((a, b) => a.order - b.order);
      const idx = sorted.findIndex((t) => t.id === id);
      if (idx === -1) return;

      const remaining = sorted.filter((t) => t.id !== id);
      let nextActive = state.activeTabId;

      if (state.activeTabId === id) {
        if (remaining.length === 0) {
          nextActive = null;
        } else {
          // Activate adjacent tab by visual order (prefer right, then left)
          const neighbor = remaining[Math.min(idx, remaining.length - 1)];
          nextActive = neighbor?.id ?? null;
        }
      }

      set({ tabs: remaining, activeTabId: nextActive });
    },

    pinTab(id: TabId): void {
      set({
        tabs: get().tabs.map((t) =>
          t.id === id ? { ...t, pinned: true } : t,
        ),
      });
    },

    unpinTab(id: TabId): void {
      set({
        tabs: get().tabs.map((t) =>
          t.id === id ? { ...t, pinned: false } : t,
        ),
      });
    },

    reorderTab(id: TabId, newOrder: number): void {
      set({
        tabs: get().tabs.map((t) =>
          t.id === id ? { ...t, order: newOrder } : t,
        ),
      });
    },

    setActiveTab(id: TabId): void {
      if (get().tabs.some((t) => t.id === id)) {
        set({ activeTabId: id });
      }
    },

    toggleSidebar(): void {
      const layout = get().panelLayout;
      set({ panelLayout: { ...layout, sidebar: !layout.sidebar } });
    },

    toggleInspector(): void {
      const layout = get().panelLayout;
      set({ panelLayout: { ...layout, inspector: !layout.inspector } });
    },

    setSidebarWidth(width: number): void {
      set({
        panelLayout: {
          ...get().panelLayout,
          sidebarWidth: Math.max(10, Math.min(50, width)),
        },
      });
    },

    setInspectorWidth(width: number): void {
      set({
        panelLayout: {
          ...get().panelLayout,
          inspectorWidth: Math.max(10, Math.min(50, width)),
        },
      });
    },
  }));
}
