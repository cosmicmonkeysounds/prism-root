/**
 * Lens context — React context providing LensRegistry, ShellStore,
 * and the component map for rendering active lenses.
 */

import { createContext, useContext, useSyncExternalStore } from "react";
import type { ComponentType, ReactNode } from "react";
import type { StoreApi } from "zustand";
import type {
  LensId,
  LensRegistry,
  ShellStore,
} from "../../layer1/workspace/index.js";

export type LensComponentMap = Map<LensId, ComponentType>;

export interface LensContextValue {
  registry: LensRegistry;
  store: StoreApi<ShellStore>;
  components: LensComponentMap;
}

const LensContext = createContext<LensContextValue | null>(null);

export interface LensProviderProps {
  registry: LensRegistry;
  store: StoreApi<ShellStore>;
  components: LensComponentMap;
  children: ReactNode;
}

export function LensProvider({
  registry,
  store,
  components,
  children,
}: LensProviderProps) {
  return (
    <LensContext.Provider value={{ registry, store, components }}>
      {children}
    </LensContext.Provider>
  );
}

export function useLensContext(): LensContextValue {
  const ctx = useContext(LensContext);
  if (!ctx) {
    throw new Error("useLensContext must be used within a LensProvider");
  }
  return ctx;
}

export function useShellStore(): ShellStore {
  const { store } = useLensContext();
  return useSyncExternalStore(
    store.subscribe,
    store.getState,
    store.getState,
  );
}
