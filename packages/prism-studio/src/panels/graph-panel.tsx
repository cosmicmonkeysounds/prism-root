/**
 * Graph Panel — Spatial node graph view backed by Loro CRDT.
 *
 * Renders PrismGraph showing the object tree from the kernel's
 * CollectionStore. Nodes represent GraphObjects, edges represent
 * parent-child containment and ObjectEdges.
 *
 * Subscribes to kernel store changes so the graph updates live
 * when objects are created, updated, or deleted. Uses elkjs auto-layout
 * for initial placement, and persists pan/zoom across tab switches via
 * `kernel.viewportCache`.
 */

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { LoroDoc } from "loro-crdt";
import { createGraphStore } from "@prism/core/stores";
import type { GraphStore } from "@prism/core/stores";
import { PrismGraph, GraphToolbar, applyElkLayout } from "@prism/core/graph";
import type { Viewport } from "@xyflow/react";
import type { StoreApi } from "zustand";
import type { ObjectId } from "@prism/core/object-model";
import { useKernel } from "../kernel/index.js";

import { lensId } from "@prism/core/lens";
import type { LensManifest } from "@prism/core/lens";
import { defineLensBundle, type LensBundle } from "../lenses/bundle.js";
import { buildGraphFromKernel, reconcileGraph } from "./graph-panel-data.js";

export { buildGraphFromKernel, reconcileGraph } from "./graph-panel-data.js";

const VIEWPORT_KEY = "lens:graph";

// ── Component ───────────────────────────────────────────────────────────────

export function GraphPanel() {
  const kernel = useKernel();
  const storeRef = useRef<StoreApi<GraphStore> | null>(null);
  const [ready, setReady] = useState(false);
  const [, setTick] = useState(0);
  const bump = useCallback(() => setTick((t) => t + 1), []);
  const [selectedNodeId, setSelectedNodeId] = useState<string | null>(null);

  // ── Initial build (one-time, then diffed) ───────────────────────────────
  useEffect(() => {
    const doc = new LoroDoc();
    const graphStore = createGraphStore(doc);
    storeRef.current = graphStore;

    const objects = kernel.store.allObjects();
    const edges = kernel.store.allEdges();
    const desired = buildGraphFromKernel(objects, edges, kernel.registry);
    reconcileGraph(graphStore.getState(), desired);

    // Run elkjs once so the initial graph isn't all stacked at (0,0).
    void runInitialLayout(graphStore, "DOWN").then(() => {
      setReady(true);
      bump();
    });
  }, [kernel, bump]);

  // ── Subscribe to kernel mutations and diff into the existing store ──────
  useEffect(() => {
    if (!storeRef.current) return;

    const unsub = kernel.store.onChange(() => {
      const graphStore = storeRef.current;
      if (!graphStore) return;
      const objects = kernel.store.allObjects();
      const edges = kernel.store.allEdges();
      const desired = buildGraphFromKernel(objects, edges, kernel.registry);
      reconcileGraph(graphStore.getState(), desired);
      bump();
    });

    return unsub;
  }, [kernel, ready, bump]);

  // ── Viewport persistence via kernel.viewportCache ───────────────────────
  const cache = kernel.viewportCache;
  const initialViewport = useMemo(
    () => cache.getState().get(VIEWPORT_KEY),
    // Read once on mount — re-reading on every render would defeat the
    // point of persistence (it would clobber live pan/zoom).
    [cache],
  );
  const handleViewportChange = useCallback(
    (v: Viewport) => {
      cache.getState().set(VIEWPORT_KEY, v);
    },
    [cache],
  );

  // ── Selection inspector (bottom strip) ──────────────────────────────────
  const selectedObject = useMemo(() => {
    if (!selectedNodeId) return null;
    return kernel.store.getObject(selectedNodeId as ObjectId) ?? null;
  }, [kernel, selectedNodeId]);

  if (!ready || !storeRef.current) {
    return <div style={{ padding: 16, color: "#888" }}>Loading graph...</div>;
  }

  const store = storeRef.current;
  const nodeCount = store.getState().nodes.length;
  const edgeCount = store.getState().edges.length;

  return (
    <div
      style={{
        width: "100%",
        height: "100%",
        display: "flex",
        flexDirection: "column",
      }}
      data-testid="graph-panel"
    >
      <div style={{ flex: 1, minHeight: 0, position: "relative" }}>
        <PrismGraph
          store={store}
          className="prism-graph-panel"
          minimap
          background
          controls
          {...(initialViewport !== undefined
            ? { initialViewport }
            : {})}
          onViewportChange={handleViewportChange}
          onNodeClick={(_, node) => setSelectedNodeId(node.id)}
          onCanvasClick={() => setSelectedNodeId(null)}
          toolbar={
            <GraphToolbar
              title={
                <span data-testid="graph-stats">
                  Graph · {nodeCount} node{nodeCount === 1 ? "" : "s"} ·{" "}
                  {edgeCount} edge{edgeCount === 1 ? "" : "s"}
                </span>
              }
              store={store}
            />
          }
        />
      </div>
      {selectedObject ? (
        <div
          data-testid="graph-inspector"
          style={{
            borderTop: "1px solid #333",
            background: "#1a1a1a",
            color: "#ccc",
            padding: "8px 12px",
            fontSize: 12,
            display: "flex",
            gap: 16,
            alignItems: "center",
          }}
        >
          <strong style={{ color: "#eee" }}>{selectedObject.name}</strong>
          <span style={{ color: "#888" }}>{selectedObject.type}</span>
          <span style={{ color: "#666" }}>id: {selectedObject.id}</span>
          {selectedObject.parentId ? (
            <span style={{ color: "#666" }}>
              parent: {selectedObject.parentId}
            </span>
          ) : null}
          <div style={{ flex: 1 }} />
          <button
            type="button"
            data-testid="graph-inspector-select"
            style={{
              background: "#2a2a2a",
              border: "1px solid #444",
              borderRadius: 3,
              padding: "3px 8px",
              color: "#ccc",
              cursor: "pointer",
              fontSize: 11,
            }}
            onClick={() => kernel.select(selectedObject.id)}
          >
            Select in tree
          </button>
        </div>
      ) : null}
    </div>
  );
}

// ── Layout helper ───────────────────────────────────────────────────────────

async function runInitialLayout(
  store: StoreApi<GraphStore>,
  direction: "DOWN" | "RIGHT" | "UP" | "LEFT",
): Promise<void> {
  const state = store.getState();
  const elkNodes = state.nodes.map((n) => ({
    id: n.id,
    position: { x: n.x, y: n.y },
    data: {},
    width: n.width,
    height: n.height,
  }));
  const elkEdges = state.edges.map((e) => ({
    id: e.id,
    source: e.source,
    target: e.target,
  }));
  const laidOut = await applyElkLayout(elkNodes, elkEdges, { direction });
  const moveNode = store.getState().moveNode;
  for (const n of laidOut) {
    moveNode(n.id, n.position.x, n.position.y);
  }
}

// ── Lens registration ──────────────────────────────────────────────────────

export const GRAPH_LENS_ID = lensId("graph");

export const graphLensManifest: LensManifest = {
  id: GRAPH_LENS_ID,
  name: "Graph",
  icon: "\u2B21",
  category: "visual",
  contributes: {
    views: [{ slot: "main" }],
    commands: [
      {
        id: "switch-graph",
        name: "Switch to Graph",
        shortcut: ["g"],
        section: "Navigation",
      },
    ],
  },
};

export const graphLensBundle: LensBundle = defineLensBundle(
  graphLensManifest,
  GraphPanel,
);
