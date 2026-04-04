/**
 * Graph Panel — Spatial node graph view backed by Loro CRDT.
 *
 * Renders PrismGraph with a sample set of nodes for Phase 3 demo.
 * Uses the graph store from @prism/core backed by a LoroDoc.
 */

import { useEffect, useRef, useState } from "react";
import { LoroDoc } from "loro-crdt";
import { createGraphStore } from "@prism/core/layer1/stores/use-graph-store";
import type { GraphStore } from "@prism/core/layer1/stores/use-graph-store";
import { PrismGraph } from "@prism/core/layer2/graph/prism-graph";
import type { StoreApi } from "zustand";

export function GraphPanel() {
  const storeRef = useRef<StoreApi<GraphStore> | null>(null);
  const [ready, setReady] = useState(false);

  useEffect(() => {
    const doc = new LoroDoc();
    const graphStore = createGraphStore(doc);

    // Seed demo nodes if empty
    const state = graphStore.getState();
    if (state.nodes.length === 0) {
      state.addNode({
        id: "node-1",
        type: "codemirror",
        x: 100,
        y: 100,
        width: 280,
        height: 180,
        data: { label: "main.ts", code: 'console.log("Hello Prism");' },
      });
      state.addNode({
        id: "node-2",
        type: "markdown",
        x: 450,
        y: 100,
        width: 260,
        height: 160,
        data: { label: "README", content: "# Prism\n\nDistributed Visual OS" },
      });
      state.addNode({
        id: "node-3",
        type: "default",
        x: 250,
        y: 350,
        width: 200,
        height: 80,
        data: { label: "config.json" },
      });
      state.addEdge({
        id: "edge-1",
        source: "node-1",
        target: "node-3",
        wireType: "hard",
      });
      state.addEdge({
        id: "edge-2",
        source: "node-2",
        target: "node-3",
        wireType: "weak",
        label: "references",
      });
    }

    storeRef.current = graphStore;
    setReady(true);
  }, []);

  if (!ready || !storeRef.current) {
    return <div style={{ padding: 16 }}>Loading graph...</div>;
  }

  return (
    <div style={{ width: "100%", height: "100%" }}>
      <PrismGraph
        store={storeRef.current}
        className="prism-graph-panel"
      />
    </div>
  );
}
