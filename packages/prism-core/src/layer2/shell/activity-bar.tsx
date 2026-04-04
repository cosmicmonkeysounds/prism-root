/**
 * Activity Bar — vertical icon sidebar listing registered lenses.
 *
 * Clicking an icon opens (or focuses) that lens's tab.
 */

import { useState, useEffect } from "react";
import type { LensManifest } from "../../layer1/workspace/index.js";
import { useLensContext, useShellStore } from "./lens-context.js";

export function ActivityBar() {
  const { registry, store } = useLensContext();
  const { activeTabId, tabs } = useShellStore();
  const [lenses, setLenses] = useState<LensManifest[]>(registry.allLenses());

  useEffect(() => {
    setLenses(registry.allLenses());
    return registry.subscribe(() => {
      setLenses(registry.allLenses());
    });
  }, [registry]);

  const activeLensId = tabs.find((t) => t.id === activeTabId)?.lensId;

  return (
    <div
      data-testid="activity-bar"
      style={{
        width: 48,
        minWidth: 48,
        background: "#1e1e1e",
        borderRight: "1px solid #333",
        display: "flex",
        flexDirection: "column",
        alignItems: "center",
        paddingTop: 8,
        gap: 4,
      }}
    >
      {lenses.map((lens) => (
        <button
          key={lens.id}
          data-testid={`activity-icon-${lens.id}`}
          title={lens.name}
          onClick={() => store.getState().openTab(lens.id, lens.name)}
          style={{
            width: 36,
            height: 36,
            border: "none",
            borderRadius: 6,
            background: activeLensId === lens.id ? "#333" : "transparent",
            color: activeLensId === lens.id ? "#fff" : "#888",
            cursor: "pointer",
            fontSize: 16,
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            borderLeft:
              activeLensId === lens.id
                ? "2px solid #007acc"
                : "2px solid transparent",
          }}
        >
          {lens.icon}
        </button>
      ))}
    </div>
  );
}
