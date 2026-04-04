/**
 * Zustand store subscribed to a LoroDoc via the Loro bridge.
 * Atomic: subscribes to specific keys, not the entire document.
 */

import { createStore } from "zustand/vanilla";
import type { LoroBridge } from "../loro-bridge.js";

export type CrdtStoreState = {
  /** Current key-value state from the CRDT root map. */
  data: Record<string, unknown>;
  /** Whether the store is connected to a Loro bridge. */
  connected: boolean;
};

export type CrdtStoreActions = {
  /** Connect to a Loro bridge and start syncing. Returns cleanup function. */
  connect: (bridge: LoroBridge) => () => void;
  /** Write a value through the bridge. */
  set: (key: string, value: string) => void;
  /** Read a value from local state. */
  get: (key: string) => unknown;
};

export type CrdtStore = CrdtStoreState & CrdtStoreActions;

/**
 * Creates a vanilla Zustand store backed by a Loro CRDT bridge.
 * Use this for non-React contexts. For React, wrap with `useStore()`.
 */
export function createCrdtStore() {
  let activeBridge: LoroBridge | null = null;

  const store = createStore<CrdtStore>((set, get) => ({
    data: {},
    connected: false,

    connect(bridge: LoroBridge) {
      activeBridge = bridge;

      // Hydrate from current state
      set({ data: bridge.toJSON(), connected: true });

      // Subscribe to future changes
      const unsubscribe = bridge.onChange((key, value) => {
        set((state) => ({
          data: { ...state.data, [key]: value },
        }));
      });

      return () => {
        unsubscribe();
        activeBridge = null;
        set({ connected: false });
      };
    },

    set(key: string, value: string) {
      if (!activeBridge) {
        throw new Error("Store not connected to a Loro bridge");
      }
      activeBridge.set(key, value);
      // Optimistic update — Loro subscription will confirm
      set((state) => ({
        data: { ...state.data, [key]: value },
      }));
    },

    get(key: string) {
      return get().data[key];
    },
  }));

  return store;
}
