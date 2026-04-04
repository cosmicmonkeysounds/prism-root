import { useMemo, useEffect, useState } from "react";
import type { Action } from "kbar";
import { createLoroBridge } from "@prism/core/layer1/loro-bridge";
import { createCrdtStore } from "@prism/core/layer1/stores/use-crdt-store";
import { createLensRegistry, createShellStore } from "@prism/core/layer1/workspace/index";
import { LensProvider, ShellLayout } from "@prism/core/layer2/shell/index";
import { PrismKBarProvider } from "@prism/core/layer2/kbar/prism-kbar";
import {
  registerBuiltinLenses,
  createLensComponentMap,
  EDITOR_LENS_ID,
} from "./lenses/index.js";

const bridge = createLoroBridge();
const crdtStore = createCrdtStore();

export function App() {
  const lensRegistry = useMemo(() => createLensRegistry(), []);
  const shellStore = useMemo(() => createShellStore(), []);
  const components = useMemo(
    () => createLensComponentMap(bridge, crdtStore),
    [],
  );

  const [globalActions, setGlobalActions] = useState<Action[]>([]);

  // Connect CRDT bridge
  useEffect(() => {
    const disconnect = crdtStore.getState().connect(bridge);
    return disconnect;
  }, []);

  // Register built-in lenses and derive KBar actions
  useEffect(() => {
    const unregister = registerBuiltinLenses(lensRegistry);

    function deriveActions(): void {
      setGlobalActions(
        lensRegistry.allLenses().map((m) => {
          const action: Action = {
            id: `switch-${m.id}`,
            name: `Switch to ${m.name}`,
            perform: () => shellStore.getState().openTab(m.id, m.name),
            section: "Navigation",
          };
          const shortcut = m.contributes.commands[0]?.shortcut;
          if (shortcut) action.shortcut = shortcut;
          return action;
        }),
      );
    }
    deriveActions();

    const unsubscribe = lensRegistry.subscribe(() => deriveActions());

    return () => {
      unregister();
      unsubscribe();
    };
  }, [lensRegistry, shellStore]);

  // Open default editor tab
  useEffect(() => {
    if (shellStore.getState().tabs.length === 0) {
      shellStore.getState().openTab(EDITOR_LENS_ID, "Editor");
    }
  }, [shellStore]);

  return (
    <PrismKBarProvider globalActions={globalActions}>
      <LensProvider
        registry={lensRegistry}
        store={shellStore}
        components={components}
      >
        <ShellLayout />
      </LensProvider>
    </PrismKBarProvider>
  );
}
