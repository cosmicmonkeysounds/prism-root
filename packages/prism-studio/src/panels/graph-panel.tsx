/**
 * Graph Panel — Spatial node graph view backed by Loro CRDT.
 *
 * Renders PrismGraph showing the object tree from the kernel's
 * CollectionStore. Nodes represent GraphObjects, edges represent
 * parent-child containment and ObjectEdges.
 */

import { useEffect, useRef, useState } from "react";
import { LoroDoc } from "loro-crdt";
import { createGraphStore } from "@prism/core/stores";
import type { GraphStore } from "@prism/core/stores";
import { PrismGraph } from "@prism/core/graph";
import type { StoreApi } from "zustand";
import { useKernel } from "../kernel/index.js";

export function GraphPanel() {
  const kernel = useKernel();
  const storeRef = useRef<StoreApi<GraphStore> | null>(null);
  const [ready, setReady] = useState(false);

  useEffect(() => {
    const doc = new LoroDoc();
    const graphStore = createGraphStore(doc);

    // Build graph from the kernel's CollectionStore objects
    const objects = kernel.store.allObjects().filter((o) => !o.deletedAt);
    const state = graphStore.getState();

    // Layout objects in a tree-like arrangement
    const rootObjects = objects.filter((o) => !o.parentId);
    const childMap = new Map<string, typeof objects>();
    for (const obj of objects) {
      if (obj.parentId) {
        const kids = childMap.get(obj.parentId) ?? [];
        kids.push(obj);
        childMap.set(obj.parentId, kids);
      }
    }

    let yOffset = 0;
    for (const root of rootObjects) {
      const def = kernel.registry.get(root.type);
      state.addNode({
        id: root.id,
        type: "default",
        x: 100,
        y: yOffset,
        width: 240,
        height: 80,
        data: { label: `${def?.icon ?? ""} ${root.name}`, objectType: root.type },
      });

      const children = childMap.get(root.id) ?? [];
      let childY = yOffset;
      for (const child of children) {
        const childDef = kernel.registry.get(child.type);
        state.addNode({
          id: child.id,
          type: "default",
          x: 400,
          y: childY,
          width: 220,
          height: 60,
          data: { label: `${childDef?.icon ?? ""} ${child.name}`, objectType: child.type },
        });

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
          const gcDef = kernel.registry.get(gc.type);
          state.addNode({
            id: gc.id,
            type: "default",
            x: 700,
            y: gcY,
            width: 200,
            height: 50,
            data: { label: `${gcDef?.icon ?? ""} ${gc.name}`, objectType: gc.type },
          });
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

    // Also render ObjectEdges as weak refs
    const edges = kernel.store.allEdges();
    for (const edge of edges) {
      state.addEdge({
        id: edge.id,
        source: edge.sourceId,
        target: edge.targetId,
        wireType: "weak",
        label: edge.relation,
      });
    }

    storeRef.current = graphStore;
    setReady(true);
  }, [kernel]);

  if (!ready || !storeRef.current) {
    return <div style={{ padding: 16, color: "#888" }}>Loading graph...</div>;
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
