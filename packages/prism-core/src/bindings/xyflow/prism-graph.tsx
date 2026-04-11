/**
 * PrismGraph — The main spatial node graph component.
 *
 * Wraps @xyflow/react with Prism's custom node types (CodeMirror, Markdown),
 * custom edge types (Hard Ref, Weak Ref), and Loro-backed graph store integration.
 */

import React, { useCallback, useMemo, useRef } from "react";
import {
  ReactFlow,
  ReactFlowProvider,
  type OnNodesChange,
  type OnEdgesChange,
  type NodeMouseHandler,
  type Node,
  type Edge,
} from "@xyflow/react";
import type { StoreApi } from "zustand";
import { prismNodeTypes } from "./custom-nodes.js";
import { prismEdgeTypes } from "./custom-edges.js";
import type { GraphStore, GraphNode, GraphEdge } from "@prism/core/stores";

export type PrismGraphProps = {
  /** The Zustand graph store (created via createGraphStore). */
  store: StoreApi<GraphStore>;
  /** Callback when a node is double-clicked (e.g., enter edit mode). */
  onNodeDoubleClick?: NodeMouseHandler;
  /** Callback when the canvas background is clicked. */
  onCanvasClick?: () => void;
  /** Whether to show minimap. */
  minimap?: boolean;
  /** CSS class for the container. */
  className?: string;
};

/** Convert graph store nodes to React Flow nodes. */
function toFlowNodes(graphNodes: GraphNode[]): Node[] {
  return graphNodes.map((n) => ({
    id: n.id,
    type: n.type || "default",
    position: { x: n.x, y: n.y },
    data: { label: n.id, ...n.data },
    width: n.width,
    height: n.height,
  }));
}

/** Convert graph store edges to React Flow edges. */
function toFlowEdges(graphEdges: GraphEdge[]): Edge[] {
  return graphEdges.map((e) => ({
    id: e.id,
    source: e.source,
    target: e.target,
    type: e.wireType === "hard" ? "hardRef" : "weakRef",
    label: e.label,
    data: { wireType: e.wireType },
  }));
}

function PrismGraphInner({
  store,
  onNodeDoubleClick,
  onCanvasClick,
  className,
}: PrismGraphProps) {
  const nodesRef = useRef<Node[]>([]);
  const edgesRef = useRef<Edge[]>([]);

  // Subscribe to store changes
  const state = store.getState();
  const flowNodes = useMemo(() => toFlowNodes(state.nodes), [state.nodes]);
  const flowEdges = useMemo(() => toFlowEdges(state.edges), [state.edges]);

  // Keep refs in sync
  nodesRef.current = flowNodes;
  edgesRef.current = flowEdges;

  const onNodesChange: OnNodesChange = useCallback(
    (changes) => {
      // Apply position changes back to the Loro-backed store
      for (const change of changes) {
        if (change.type === "position" && change.position) {
          store.getState().moveNode(change.id, change.position.x, change.position.y);
        }
      }
    },
    [store],
  );

  const onEdgesChange: OnEdgesChange = useCallback(
    (changes) => {
      for (const change of changes) {
        if (change.type === "remove") {
          store.getState().removeEdge(change.id);
        }
      }
    },
    [store],
  );

  const handlePaneClick = useCallback(() => {
    onCanvasClick?.();
  }, [onCanvasClick]);

  return (
    <div className={className} style={{ width: "100%", height: "100%" }}>
      <ReactFlow
        nodes={flowNodes}
        edges={flowEdges}
        nodeTypes={prismNodeTypes}
        edgeTypes={prismEdgeTypes}
        onNodesChange={onNodesChange}
        onEdgesChange={onEdgesChange}
        {...(onNodeDoubleClick !== undefined ? { onNodeDoubleClick } : {})}
        onPaneClick={handlePaneClick}
        fitView
        proOptions={{ hideAttribution: true }}
      />
    </div>
  );
}

/**
 * PrismGraph — Spatial node graph backed by Loro CRDT.
 *
 * Wraps ReactFlow with Prism's custom node/edge types and store integration.
 * Must be provided a graph store created via `createGraphStore(doc)`.
 */
export function PrismGraph(props: PrismGraphProps) {
  return (
    <ReactFlowProvider>
      <PrismGraphInner {...props} />
    </ReactFlowProvider>
  );
}
