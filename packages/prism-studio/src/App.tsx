import { useState, useEffect, useMemo } from "react";
import { Panel, PanelGroup, PanelResizeHandle } from "react-resizable-panels";
import { createLoroBridge } from "@prism/core/layer1/loro-bridge";
import { createCrdtStore } from "@prism/core/layer1/stores/use-crdt-store";
import { PrismKBarProvider } from "@prism/core/layer2/kbar/prism-kbar";
import { CrdtPanel } from "./panels/crdt-panel.js";
import { EditorPanel } from "./panels/editor-panel.js";
import { LayoutPanel } from "./panels/layout-panel.js";
import { GraphPanel } from "./panels/graph-panel.js";

const bridge = createLoroBridge();
const store = createCrdtStore();

/**
 * Phase 2 demo: The Eyes.
 * Visual editing of CRDT state via CodeMirror and Puck, with KBar command palette.
 */
export function App() {
  const [activeTab, setActiveTab] = useState<
    "editor" | "layout" | "graph" | "crdt"
  >("editor");

  useEffect(() => {
    const disconnect = store.getState().connect(bridge);
    return disconnect;
  }, []);

  const globalActions = useMemo(
    () => [
      {
        id: "switch-editor",
        name: "Switch to Editor",
        shortcut: ["e"],
        perform: () => setActiveTab("editor"),
        section: "Navigation",
      },
      {
        id: "switch-layout",
        name: "Switch to Layout Builder",
        shortcut: ["l"],
        perform: () => setActiveTab("layout"),
        section: "Navigation",
      },
      {
        id: "switch-graph",
        name: "Switch to Graph",
        shortcut: ["g"],
        perform: () => setActiveTab("graph"),
        section: "Navigation",
      },
      {
        id: "switch-crdt",
        name: "Switch to CRDT Inspector",
        shortcut: ["c"],
        perform: () => setActiveTab("crdt"),
        section: "Navigation",
      },
    ],
    [],
  );

  return (
    <PrismKBarProvider globalActions={globalActions}>
      <div
        style={{
          fontFamily: "system-ui",
          height: "100vh",
          display: "flex",
          flexDirection: "column",
        }}
      >
        <header
          style={{
            padding: "8px 16px",
            borderBottom: "1px solid #e0e0e0",
            display: "flex",
            alignItems: "center",
            gap: 16,
            background: "#fafafa",
          }}
        >
          <strong>Prism Studio</strong>
          <nav style={{ display: "flex", gap: 4 }}>
            {(["editor", "layout", "graph", "crdt"] as const).map((tab) => (
              <button
                key={tab}
                onClick={() => setActiveTab(tab)}
                style={{
                  padding: "4px 12px",
                  border: "1px solid #ccc",
                  borderRadius: 4,
                  background: activeTab === tab ? "#333" : "white",
                  color: activeTab === tab ? "white" : "#333",
                  cursor: "pointer",
                  fontSize: 13,
                }}
              >
                {tab === "editor"
                  ? "Editor"
                  : tab === "layout"
                    ? "Layout"
                    : tab === "graph"
                      ? "Graph"
                      : "CRDT"}
              </button>
            ))}
          </nav>
          <span style={{ fontSize: 12, color: "#999", marginLeft: "auto" }}>
            CMD+K for command palette
          </span>
        </header>

        <div style={{ flex: 1, overflow: "hidden" }}>
          {activeTab === "editor" && (
            <PanelGroup direction="horizontal">
              <Panel defaultSize={70} minSize={30}>
                <EditorPanel doc={bridge.doc} />
              </Panel>
              <PanelResizeHandle
                style={{ width: 4, background: "#e0e0e0", cursor: "col-resize" }}
              />
              <Panel defaultSize={30} minSize={20}>
                <CrdtPanel store={store} />
              </Panel>
            </PanelGroup>
          )}
          {activeTab === "layout" && <LayoutPanel />}
          {activeTab === "graph" && <GraphPanel />}
          {activeTab === "crdt" && (
            <CrdtPanel store={store} fullWidth />
          )}
        </div>
      </div>
    </PrismKBarProvider>
  );
}
