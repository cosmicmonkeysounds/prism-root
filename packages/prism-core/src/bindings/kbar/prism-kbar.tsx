/**
 * Prism KBar provider — wraps the kbar library with focus-depth routing.
 * CMD+K opens the palette. Actions are filtered by current focus depth.
 */

import React, {
  createContext,
  useContext,
  useState,
  useEffect,
  useMemo,
} from "react";
import {
  KBarProvider,
  KBarPortal,
  KBarPositioner,
  KBarAnimator,
  KBarSearch,
  KBarResults,
  useMatches,
  useRegisterActions,
  type Action,
} from "kbar";
import {
  createActionRegistry,
  type ActionRegistry,
  type FocusDepth,
} from "./focus-depth.js";

type PrismKBarContextValue = {
  registry: ActionRegistry;
  currentDepth: FocusDepth;
  setDepth: (depth: FocusDepth) => void;
};

const PrismKBarContext = createContext<PrismKBarContextValue | null>(null);

/** Access the Prism KBar context (registry + depth). */
export function usePrismKBar() {
  const ctx = useContext(PrismKBarContext);
  if (!ctx) throw new Error("usePrismKBar must be used within PrismKBarProvider");
  return ctx;
}

function RenderResults() {
  const { results } = useMatches();

  return (
    <KBarResults
      items={results}
      onRender={({ item, active }) =>
        typeof item === "string" ? (
          <div style={{ padding: "8px 16px", fontSize: 11, opacity: 0.5 }}>
            {item}
          </div>
        ) : (
          <div
            style={{
              padding: "12px 16px",
              background: active ? "rgba(0,0,0,0.05)" : "transparent",
              cursor: "pointer",
              display: "flex",
              alignItems: "center",
              justifyContent: "space-between",
            }}
          >
            <span>{item.name}</span>
            {item.shortcut?.length ? (
              <kbd style={{ fontSize: 12, opacity: 0.5 }}>
                {item.shortcut.join(" ")}
              </kbd>
            ) : null}
          </div>
        )
      }
    />
  );
}

export type PrismKBarProviderProps = {
  /** Initial actions to register at global depth. */
  globalActions?: Action[];
  children: React.ReactNode;
};

/**
 * Prism KBar provider with focus-depth routing.
 *
 * Wraps your app and provides:
 * - CMD+K command palette
 * - Focus-depth-aware action filtering
 * - Action registry for dynamic registration
 */
/** Syncs focus-depth-filtered actions into kbar's internal registry. */
function ActionSync({
  actions,
  children,
}: {
  actions: Action[];
  children: React.ReactNode;
}) {
  useRegisterActions(actions, [actions]);
  return <>{children}</>;
}

export function PrismKBarProvider({
  globalActions = [],
  children,
}: PrismKBarProviderProps) {
  const registry = useMemo(() => createActionRegistry(), []);
  const [currentDepth, setDepth] = useState<FocusDepth>("global");
  const [actions, setActions] = useState<Action[]>([]);

  // Register initial global actions
  useEffect(() => {
    if (globalActions.length > 0) {
      return registry.register("global", globalActions);
    }
  }, [registry, globalActions]);

  // Update visible actions when depth or registry changes
  useEffect(() => {
    const update = () => setActions(registry.getActions(currentDepth));
    update();
    return registry.subscribe(update);
  }, [registry, currentDepth]);

  const contextValue = useMemo(
    () => ({ registry, currentDepth, setDepth }),
    [registry, currentDepth],
  );

  return (
    <PrismKBarContext.Provider value={contextValue}>
      <KBarProvider>
        <ActionSync actions={actions}>
          {children}
        </ActionSync>
        <KBarPortal>
          <KBarPositioner
            style={{ zIndex: 9999, background: "rgba(0,0,0,0.4)" }}
          >
            <KBarAnimator
              style={{
                maxWidth: 600,
                width: "100%",
                background: "white",
                borderRadius: 8,
                overflow: "hidden",
                boxShadow: "0 16px 70px rgba(0,0,0,0.2)",
              }}
            >
              <KBarSearch
                style={{
                  padding: "12px 16px",
                  fontSize: 16,
                  width: "100%",
                  boxSizing: "border-box",
                  outline: "none",
                  border: "none",
                  borderBottom: "1px solid #eee",
                }}
              />
              <RenderResults />
            </KBarAnimator>
          </KBarPositioner>
        </KBarPortal>
      </KBarProvider>
    </PrismKBarContext.Provider>
  );
}
