/**
 * Studio Shell — custom shell layout that extends the core ShellLayout
 * with real sidebar (object explorer) and inspector panel content.
 *
 * Replaces the core ShellLayout which renders placeholder text
 * for sidebar/inspector.
 */

import { useLensContext, useShellStore } from "@prism/core/shell";
import { ActivityBar, TabBar } from "@prism/core/shell";
import { ObjectExplorer } from "./object-explorer.js";
import { InspectorPanel } from "./inspector-panel.js";
import { UndoStatusBar } from "./undo-status-bar.js";

export function StudioShell() {
  const { store, components } = useLensContext();
  const { tabs, activeTabId, panelLayout } = useShellStore();

  const activeTab = tabs.find((t) => t.id === activeTabId);
  const ActiveComponent = activeTab
    ? components.get(activeTab.lensId)
    : undefined;

  return (
    <div
      data-testid="workspace-shell"
      style={{
        display: "flex",
        height: "100vh",
        fontFamily: "system-ui",
        background: "#1e1e1e",
        color: "#ccc",
      }}
    >
      <ActivityBar />

      {panelLayout.sidebar && (
        <div
          data-testid="sidebar"
          style={{
            width: `${panelLayout.sidebarWidth}%`,
            minWidth: 180,
            maxWidth: 400,
            background: "#252526",
            borderRight: "1px solid #333",
            display: "flex",
            flexDirection: "column",
          }}
        >
          <div
            style={{
              padding: "8px 12px",
              fontSize: 11,
              fontWeight: 600,
              textTransform: "uppercase",
              color: "#888",
              borderBottom: "1px solid #333",
              display: "flex",
              justifyContent: "space-between",
              alignItems: "center",
            }}
          >
            <span>Explorer</span>
            <button
              data-testid="toggle-sidebar"
              title="Toggle sidebar"
              onClick={() => store.getState().toggleSidebar()}
              style={{
                background: "none",
                border: "none",
                color: "#888",
                cursor: "pointer",
                fontSize: 14,
              }}
            >
              {"\u2190"}
            </button>
          </div>
          <ObjectExplorer />
        </div>
      )}

      <div style={{ flex: 1, display: "flex", flexDirection: "column", minWidth: 0 }}>
        <header
          style={{
            display: "flex",
            alignItems: "center",
            background: "#2d2d2d",
            borderBottom: "1px solid #333",
          }}
        >
          <TabBar />
          <div
            style={{
              marginLeft: "auto",
              display: "flex",
              gap: 4,
              padding: "0 8px",
              alignItems: "center",
            }}
          >
            <UndoStatusBar />
            {!panelLayout.sidebar && (
              <button
                data-testid="toggle-sidebar"
                title="Toggle sidebar"
                onClick={() => store.getState().toggleSidebar()}
                style={{
                  background: "none",
                  border: "none",
                  color: "#888",
                  cursor: "pointer",
                  fontSize: 12,
                }}
              >
                {"\u2261"}
              </button>
            )}
            <span style={{ fontSize: 11, color: "#666", lineHeight: "32px" }}>
              CMD+K
            </span>
          </div>
        </header>

        <div style={{ flex: 1, overflow: "hidden", background: "#1e1e1e" }}>
          {ActiveComponent && activeTab ? (
            <ActiveComponent key={activeTab.id} />
          ) : (
            <div
              style={{
                display: "flex",
                alignItems: "center",
                justifyContent: "center",
                height: "100%",
                color: "#555",
                fontSize: 14,
              }}
            >
              No tab open. Click a lens in the activity bar.
            </div>
          )}
        </div>
      </div>

      {panelLayout.inspector && (
        <div
          data-testid="inspector"
          style={{
            width: `${panelLayout.inspectorWidth}%`,
            minWidth: 200,
            maxWidth: 400,
            background: "#252526",
            borderLeft: "1px solid #333",
            display: "flex",
            flexDirection: "column",
            overflow: "hidden",
          }}
        >
          <InspectorPanel />
        </div>
      )}
    </div>
  );
}
