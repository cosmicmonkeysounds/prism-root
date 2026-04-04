/**
 * elkjs auto-layout for the Prism spatial graph.
 *
 * Computes orthogonal layouts for nodes dropped into structured zones.
 * Uses the Eclipse Layout Kernel (elkjs) for 90-degree wire routing.
 */

import ELK, { type ElkNode, type ElkExtendedEdge } from "elkjs";
import type { Node, Edge } from "@xyflow/react";

const elk = new ELK();

export type LayoutOptions = {
  direction?: "DOWN" | "RIGHT" | "UP" | "LEFT";
  spacing?: number;
  nodeWidth?: number;
  nodeHeight?: number;
};

const DEFAULTS: Required<LayoutOptions> = {
  direction: "DOWN",
  spacing: 80,
  nodeWidth: 200,
  nodeHeight: 100,
};

/**
 * Apply elkjs auto-layout to a set of React Flow nodes and edges.
 * Returns new node positions (does not mutate input).
 */
export async function applyElkLayout<
  N extends Node = Node,
  E extends Edge = Edge,
>(
  nodes: N[],
  edges: E[],
  options?: LayoutOptions,
): Promise<N[]> {
  const opts = { ...DEFAULTS, ...options };

  const elkNodes: ElkNode[] = nodes.map((node) => ({
    id: node.id,
    width: (node.measured?.width ?? node.width) ?? opts.nodeWidth,
    height: (node.measured?.height ?? node.height) ?? opts.nodeHeight,
  }));

  const elkEdges: ElkExtendedEdge[] = edges.map((edge) => ({
    id: edge.id,
    sources: [edge.source],
    targets: [edge.target],
  }));

  const graph: ElkNode = {
    id: "root",
    layoutOptions: {
      "elk.algorithm": "layered",
      "elk.direction": opts.direction,
      "elk.spacing.nodeNode": String(opts.spacing),
      "elk.layered.spacing.nodeNodeBetweenLayers": String(opts.spacing),
      "elk.edgeRouting": "ORTHOGONAL",
    },
    children: elkNodes,
    edges: elkEdges,
  };

  const layoutResult = await elk.layout(graph);

  return nodes.map((node) => {
    const elkNode = layoutResult.children?.find((n) => n.id === node.id);
    if (!elkNode) return node;
    return {
      ...node,
      position: {
        x: elkNode.x ?? node.position?.x ?? 0,
        y: elkNode.y ?? node.position?.y ?? 0,
      },
    };
  });
}
