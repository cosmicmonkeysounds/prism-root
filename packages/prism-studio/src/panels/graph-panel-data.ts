/**
 * Pure data helpers for GraphPanel. Split out of `graph-panel.tsx` so tests
 * can import them without pulling in React, the kernel, or transitive DOM-
 * dependent modules (same pattern as `chart-data.ts`, `map-data.ts`).
 */

import type { GraphStore, GraphNode, GraphEdge } from "@prism/core/stores";
import type { GraphObject, ObjectId } from "@prism/core/object-model";

/**
 * Build the desired set of `GraphNode` + `GraphEdge` records from the kernel
 * snapshot. Pure — no Loro/store mutations. Positions are seeded as `0,0`;
 * `applyElkLayout` runs over the result to assign real coordinates.
 */
export function buildGraphFromKernel(
  objects: readonly GraphObject[],
  edges: readonly {
    id: string;
    sourceId: ObjectId;
    targetId: ObjectId;
    relation: string;
  }[],
  registry: { get(type: string): { icon?: unknown; color?: unknown } | undefined },
): { nodes: GraphNode[]; edges: GraphEdge[] } {
  const liveObjects = objects.filter((o) => !o.deletedAt);
  const nodes: GraphNode[] = liveObjects.map((obj) => {
    const def = registry.get(obj.type);
    const icon = typeof def?.icon === "string" ? def.icon : "";
    return {
      id: obj.id,
      type: "default",
      x: 0,
      y: 0,
      width: 220,
      height: 60,
      data: { label: `${icon} ${obj.name}`, objectType: obj.type },
    };
  });

  const liveIds = new Set(liveObjects.map((o) => o.id));
  const out: GraphEdge[] = [];

  // Containment edges (parent → child)
  for (const obj of liveObjects) {
    if (obj.parentId && liveIds.has(obj.parentId)) {
      out.push({
        id: `e-${obj.parentId}-${obj.id}`,
        source: obj.parentId,
        target: obj.id,
        wireType: "hard",
      });
    }
  }

  // ObjectEdges as weak refs
  for (const e of edges) {
    if (!liveIds.has(e.sourceId) || !liveIds.has(e.targetId)) continue;
    out.push({
      id: e.id,
      source: e.sourceId,
      target: e.targetId,
      wireType: "weak",
      label: e.relation,
    });
  }

  return { nodes, edges: out };
}

/**
 * Reconcile the graph store with a freshly built node/edge set. Adds new
 * nodes, removes nodes that disappeared, and resyncs edges. Position updates
 * for existing nodes are intentionally NOT pushed — user-dragged positions
 * survive store rebuilds. Returns the reconciled node id set.
 */
export function reconcileGraph(
  state: GraphStore,
  desired: { nodes: GraphNode[]; edges: GraphEdge[] },
): Set<string> {
  const desiredIds = new Set(desired.nodes.map((n) => n.id));
  const existingIds = new Set(state.nodes.map((n) => n.id));

  for (const id of existingIds) {
    if (!desiredIds.has(id)) state.removeNode(id);
  }
  for (const node of desired.nodes) {
    if (!existingIds.has(node.id)) state.addNode(node);
  }

  // Edges are simpler: clear-and-reinsert. Edge identity can change on rename.
  const existingEdgeIds = new Set(state.edges.map((e) => e.id));
  const desiredEdgeIds = new Set(desired.edges.map((e) => e.id));
  for (const id of existingEdgeIds) {
    if (!desiredEdgeIds.has(id)) state.removeEdge(id);
  }
  for (const edge of desired.edges) {
    if (!existingEdgeIds.has(edge.id)) state.addEdge(edge);
  }

  return desiredIds;
}
