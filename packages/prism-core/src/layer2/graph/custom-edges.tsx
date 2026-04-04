/**
 * Custom React Flow edge types for the Prism spatial graph.
 *
 * - HardRefEdge: solid orthogonal line — structural parent-child ownership
 * - WeakRefEdge: dashed bezier curve — semantic wiki-link references
 */

import React from "react";
import {
  BaseEdge,
  getSmoothStepPath,
  getBezierPath,
  type EdgeProps,
  type Edge,
} from "@xyflow/react";

// ─── Hard Ref Edge (solid, orthogonal step path) ────────────

type HardRefData = {
  wireType: "hard";
};

export type HardRefEdge = Edge<HardRefData, "hardRef">;

export function HardRefEdgeComponent(props: EdgeProps<HardRefEdge>) {
  const [edgePath] = getSmoothStepPath({
    sourceX: props.sourceX,
    sourceY: props.sourceY,
    targetX: props.targetX,
    targetY: props.targetY,
    sourcePosition: props.sourcePosition,
    targetPosition: props.targetPosition,
    borderRadius: 8,
  });

  const baseProps: Record<string, unknown> = {
    id: props.id,
    path: edgePath,
    style: {
      stroke: "var(--prism-edge-hard, #334155)",
      strokeWidth: 2,
      ...props.style,
    },
  };
  if (props.markerEnd !== undefined) {
    baseProps["markerEnd"] = props.markerEnd;
  }

  return <BaseEdge {...(baseProps as { path: string })} />;
}

// ─── Weak Ref Edge (dashed bezier curve) ─────────────────────

type WeakRefData = {
  wireType: "weak";
};

export type WeakRefEdge = Edge<WeakRefData, "weakRef">;

export function WeakRefEdgeComponent(props: EdgeProps<WeakRefEdge>) {
  const [edgePath] = getBezierPath({
    sourceX: props.sourceX,
    sourceY: props.sourceY,
    targetX: props.targetX,
    targetY: props.targetY,
    sourcePosition: props.sourcePosition,
    targetPosition: props.targetPosition,
  });

  const baseProps: Record<string, unknown> = {
    id: props.id,
    path: edgePath,
    style: {
      stroke: "var(--prism-edge-weak, #94a3b8)",
      strokeWidth: 1.5,
      strokeDasharray: "6 3",
      ...props.style,
    },
  };
  if (props.markerEnd !== undefined) {
    baseProps["markerEnd"] = props.markerEnd;
  }

  return <BaseEdge {...(baseProps as { path: string })} />;
}

// ─── Edge type registry ──────────────────────────────────────

export const prismEdgeTypes = {
  hardRef: HardRefEdgeComponent,
  weakRef: WeakRefEdgeComponent,
} as const;
