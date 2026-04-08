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

// ─── Stream Edge (animated dashed bezier) ────────────────────
//
// For EdgeBehavior = "stream": continuous data flow from source to
// target (DSP/pipeline semantics). Visually animated dashes crawl
// along the curve to indicate direction and "liveness".

type StreamRefData = {
  wireType: "stream";
};

export type StreamEdge = Edge<StreamRefData, "stream">;

const STREAM_KEYFRAMES_ID = "prism-stream-edge-keyframes";

function ensureStreamKeyframes(): void {
  if (typeof document === "undefined") return;
  if (document.getElementById(STREAM_KEYFRAMES_ID)) return;
  const style = document.createElement("style");
  style.id = STREAM_KEYFRAMES_ID;
  style.textContent = `
    @keyframes prism-stream-dash {
      from { stroke-dashoffset: 24; }
      to   { stroke-dashoffset: 0; }
    }
    .prism-stream-edge {
      animation: prism-stream-dash 0.6s linear infinite;
    }
  `;
  document.head.appendChild(style);
}

export function StreamEdgeComponent(props: EdgeProps<StreamEdge>) {
  // Inject keyframes once per document. Safe to call on every render —
  // getElementById short-circuits after the first mount.
  ensureStreamKeyframes();

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
    className: "prism-stream-edge",
    style: {
      stroke: "var(--prism-edge-stream, #10b981)",
      strokeWidth: 2,
      strokeDasharray: "8 4",
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
  stream: StreamEdgeComponent,
} as const;
