/**
 * Graph Panel — Spatial node graph view backed by Loro CRDT.
 *
 * Renders PrismGraph showing the object tree from the kernel's
 * CollectionStore. Nodes represent GraphObjects, edges represent
 * parent-child containment and ObjectEdges.
 *
 * Subscribes to kernel store changes so the graph updates live
 * when objects are created, updated, or deleted.
 */

import { useEffect, useRef, useState } from "react";
import { LoroDoc } from "loro-crdt";
import { createGraphStore } from "@prism/core/stores";
import type { GraphStore } from "@prism/core/stores";
import { PrismGraph } from "@prism/core/graph";
import type { StoreApi } from "zustand";
import type { GraphObject } from "@prism/core/object-model";
import { useKernel } from "../kernel/index.js";

import { lensId } from "@prism/core/lens";
import type { LensManifest } from "@prism/core/lens";
import { defineLensBundle, type LensBundle } from "../lenses/bundle.js";
/**
 * Build (or rebuild) graph nodes and edges from the kernel's current state.
 * Returns the set of node IDs for diffing.
 */
function rebuildGraph(
  state: GraphStore,
  objects: GraphObject[],
  edges: Array<{ id: string; sourceId: string; targetId: string; relation: string }>,
  registry: { get(type: string): { icon?: unknown } | undefined },
): Set<string> {
  const nodeIds = new Set<string>();
  const liveObjects = objects.filter((o) => !o.deletedAt);

  // Build parent→children map
  const rootObjects = liveObjects.filter((o) => !o.parentId);
  const childMap = new Map<string, GraphObject[]>();
  for (const obj of liveObjects) {
    if (obj.parentId) {
      const kids = childMap.get(obj.parentId) ?? [];
      kids.push(obj);
      childMap.set(obj.parentId, kids);
    }
  }

  // Layout: roots at x=100, children at x=400, grandchildren at x=700
  let yOffset = 0;
  for (const root of rootObjects) {
    const def = registry.get(root.type);
    const icon = typeof def?.icon === "string" ? def.icon : "";
    state.addNode({
      id: root.id,
      type: "default",
      x: 100,
      y: yOffset,
      width: 240,
      height: 80,
      data: { label: `${icon} ${root.name}`, objectType: root.type },
    });
    nodeIds.add(root.id);

    const children = childMap.get(root.id) ?? [];
    let childY = yOffset;
    for (const child of children) {
      const childDef = registry.get(child.type);
      const childIcon = typeof childDef?.icon === "string" ? childDef.icon : "";
      state.addNode({
        id: child.id,
        type: "default",
        x: 400,
        y: childY,
        width: 220,
        height: 60,
        data: { label: `${childIcon} ${child.name}`, objectType: child.type },
      });
      nodeIds.add(child.id);

      state.addEdge({
        id: `e-${root.id}-${child.id}`,
        source: root.id,
        target: child.id,
        wireType: "hard",
      });

      // Grandchildren
      const grandchildren = childMap.get(child.id) ?? [];
      let gcY = childY;
      for (const gc of grandchildren) {
        const gcDef = registry.get(gc.type);
        const gcIcon = typeof gcDef?.icon === "string" ? gcDef.icon : "";
        state.addNode({
          id: gc.id,
          type: "default",
          x: 700,
          y: gcY,
          width: 200,
          height: 50,
          data: { label: `${gcIcon} ${gc.name}`, objectType: gc.type },
        });
        nodeIds.add(gc.id);
        state.addEdge({
          id: `e-${child.id}-${gc.id}`,
          source: child.id,
          target: gc.id,
          wireType: "hard",
        });
        gcY += 70;
      }

      childY = Math.max(childY + 80, gcY);
    }

    yOffset = Math.max(yOffset + 120, childY + 40);
  }

  // ObjectEdges as weak refs
  for (const edge of edges) {
    state.addEdge({
      id: edge.id,
      source: edge.sourceId,
      target: edge.targetId,
      wireType: "weak",
      label: edge.relation,
    });
  }

  return nodeIds;
}

export function GraphPanel() {
  const kernel = useKernel();
  const storeRef = useRef<StoreApi<GraphStore> | null>(null);
  const docRef = useRef<LoroDoc | null>(null);
  const nodeIdsRef = useRef<Set<string>>(new Set());
  const [ready, setReady] = useState(false);
  const [version, setVersion] = useState(0);

  // Initial build
  useEffect(() => {
    const doc = new LoroDoc();
    const graphStore = createGraphStore(doc);
    docRef.current = doc;
    storeRef.current = graphStore;

    const state = graphStore.getState();
    const objects = kernel.store.allObjects();
    const edges = kernel.store.allEdges();
    nodeIdsRef.current = rebuildGraph(state, objects, edges, kernel.registry);

    setReady(true);
  }, [kernel]);

  // Subscribe to kernel store changes — rebuild graph on mutations
  useEffect(() => {
    if (!storeRef.current) return;

    const unsub = kernel.store.onChange(() => {
      const graphStore = storeRef.current;
      if (!graphStore) return;

      // Clear existing graph and rebuild
      const doc = new LoroDoc();
      const newStore = createGraphStore(doc);
      docRef.current = doc;
      storeRef.current = newStore;

      const state = newStore.getState();
      const objects = kernel.store.allObjects();
      const edges = kernel.store.allEdges();
      nodeIdsRef.current = rebuildGraph(state, objects, edges, kernel.registry);

      setVersion((v) => v + 1);
    });

    return unsub;
  }, [kernel, ready]);

  // Force re-render key based on version
  const renderKey = `graph-${version}`;

  if (!ready || !storeRef.current) {
    return <div style={{ padding: 16, color: "#888" }}>Loading graph...</div>;
  }

  return (
    <div style={{ width: "100%", height: "100%" }} data-testid="graph-panel">
      <PrismGraph
        key={renderKey}
        store={storeRef.current}
        className="prism-graph-panel"
      />
    </div>
  );
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
    commands: [{ id: "switch-graph", name: "Switch to Graph", shortcut: ["g"], section: "Navigation" }],
  },
};

export const graphLensBundle: LensBundle = defineLensBundle(
  graphLensManifest,
  GraphPanel,
);
