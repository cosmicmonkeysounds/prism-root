/**
 * Viewport Cache — ephemeral pan/zoom (and any other "where am I in this view")
 * state, keyed by an arbitrary string. Survives tab switches because it lives
 * on the kernel, not on a React component.
 *
 * Used by the Graph lens, Sitemap lens and Schema Designer lens to remember
 * pan/zoom across tab switches, but generic enough to back any scrollable or
 * zoomable surface (timeline, spatial canvas, viewport3d, …).
 *
 * No persistence to disk — the cache lives for the lifetime of the kernel.
 * That matches user expectation: "the same session remembers, fresh boot doesn't".
 */

import { createStore } from "zustand/vanilla";
import type { StoreApi } from "zustand";

export interface ViewportState {
  x: number;
  y: number;
  zoom: number;
}

export interface ViewportCacheState {
  viewports: Record<string, ViewportState>;
}

export interface ViewportCacheActions {
  /** Get the saved viewport for a key, or `undefined` if none. */
  get(key: string): ViewportState | undefined;
  /** Save a viewport for a key. Replaces any existing entry. */
  set(key: string, viewport: ViewportState): void;
  /** Drop the saved viewport for a key. */
  clear(key: string): void;
  /** Drop every saved viewport. */
  clearAll(): void;
}

export type ViewportCache = ViewportCacheState & ViewportCacheActions;

export function createViewportCache(): StoreApi<ViewportCache> {
  return createStore<ViewportCache>((set, get) => ({
    viewports: {},

    get(key) {
      return get().viewports[key];
    },

    set(key, viewport) {
      set({ viewports: { ...get().viewports, [key]: viewport } });
    },

    clear(key) {
      const { [key]: _removed, ...rest } = get().viewports;
      void _removed;
      set({ viewports: rest });
    },

    clearAll() {
      set({ viewports: {} });
    },
  }));
}
