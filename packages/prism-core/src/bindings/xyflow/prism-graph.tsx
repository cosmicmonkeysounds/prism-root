/**
 * PrismGraph — The main spatial node graph component.
 *
 * Wraps @xyflow/react with Prism's custom node types (CodeMirror, Markdown),
 * custom edge types (Hard Ref, Weak Ref), Loro-backed graph store integration,
 * an optional minimap, background grid, controls, a toolbar render slot, and
 * viewport persistence hooks (initialViewport / onViewportChange) so callers
 * can restore pan/zoom across tab switches.
 */

import React, {
  useCallback,
  useMemo,
  useRef,
  type ReactNode,
} from "react";
import {
  ReactFlow,
  ReactFlowProvider,
  Background,
  BackgroundVariant,
  Controls,
  MarkerType,
  MiniMap,
  Panel,
  useReactFlow,
  useOnViewportChange,
  type OnNodesChange,
  type OnEdgesChange,
  type NodeMouseHandler,
  type Node,
  type Edge,
  type Viewport,
  type ReactFlowInstance,
} from "@xyflow/react";
import type { StoreApi } from "zustand";
import { prismNodeTypes } from "./custom-nodes.js";
import { prismEdgeTypes } from "./custom-edges.js";
import { applyElkLayout, type LayoutOptions } from "./auto-layout.js";
import type { GraphStore, GraphNode, GraphEdge } from "@prism/core/stores";
import "./prism-graph.css";

export type PrismGraphProps = {
  /** The Zustand graph store (created via createGraphStore). */
  store: StoreApi<GraphStore>;
  /** Callback when a node is double-clicked (e.g., enter edit mode). */
  onNodeDoubleClick?: NodeMouseHandler;
  /** Callback when a node is single-clicked. */
  onNodeClick?: NodeMouseHandler;
  /** Callback when the canvas background is clicked. */
  onCanvasClick?: () => void;
  /** Whether to render a minimap overlay. Defaults to false. */
  minimap?: boolean;
  /** Whether to render the background grid. Defaults to true. */
  background?: boolean;
  /** Background variant: dots / lines / cross. Defaults to dots. */
  backgroundVariant?: "dots" | "lines" | "cross";
  /** Whether to render zoom/fit/lock controls. Defaults to true. */
  controls?: boolean;
  /**
   * Render slot for a toolbar — placed inside the ReactFlow viewport via
   * the xyflow `<Panel>` primitive. Use the exported `<GraphToolbar>` for
   * the standard set, or pass any custom node.
   */
  toolbar?: ReactNode;
  /** Position for the toolbar `<Panel>`. Defaults to "top-left". */
  toolbarPosition?:
    | "top-left"
    | "top-center"
    | "top-right"
    | "bottom-left"
    | "bottom-center"
    | "bottom-right";
  /**
   * Initial viewport (pan + zoom) on mount. If supplied, ReactFlow boots with
   * this viewport instead of fitting to content. Use together with
   * `onViewportChange` to persist viewport state across tab switches.
   */
  initialViewport?: Viewport;
  /**
   * Callback fired on every pan/zoom change. Throttled to one call per
   * viewport-change event from xyflow — caller is responsible for any
   * additional debouncing if writing to disk.
   */
  onViewportChange?: (viewport: Viewport) => void;
  /** Called once after ReactFlow mounts, with the imperative API. */
  onInit?: (instance: ReactFlowInstance) => void;
  /** Whether to fit the graph to view on first render. Ignored if `initialViewport` is set. Defaults to true. */
  fitView?: boolean;
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
    type:
      e.wireType === "hard"
        ? "hardRef"
        : e.wireType === "stream"
          ? "stream"
          : "weakRef",
    label: e.label,
    data: { wireType: e.wireType },
  }));
}

const BACKGROUND_VARIANTS: Record<
  NonNullable<PrismGraphProps["backgroundVariant"]>,
  BackgroundVariant
> = {
  dots: BackgroundVariant.Dots,
  lines: BackgroundVariant.Lines,
  cross: BackgroundVariant.Cross,
};

const DEFAULT_EDGE_OPTIONS = {
  markerEnd: { type: MarkerType.ArrowClosed, width: 18, height: 18, color: "#94a3b8" },
} as const;

/** Internal viewport-bridge component — only mounted when onViewportChange is set. */
function ViewportBridge({
  onViewportChange,
}: {
  onViewportChange: (v: Viewport) => void;
}) {
  useOnViewportChange({ onChange: onViewportChange });
  return null;
}

function PrismGraphInner({
  store,
  onNodeDoubleClick,
  onNodeClick,
  onCanvasClick,
  className,
  minimap = false,
  background = true,
  backgroundVariant = "dots",
  controls = true,
  toolbar,
  toolbarPosition = "top-left",
  initialViewport,
  onViewportChange,
  onInit,
  fitView = true,
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

  // Decide between defaultViewport and fitView. xyflow requires an exact
  // either/or — passing both produces a warning and unpredictable boot state.
  const useInitialViewport = initialViewport !== undefined;

  const wrapperClass = className
    ? `${className} prism-graph-themed`
    : "prism-graph-themed";

  return (
    <div className={wrapperClass} style={{ width: "100%", height: "100%" }}>
      <ReactFlow
        nodes={flowNodes}
        edges={flowEdges}
        nodeTypes={prismNodeTypes}
        edgeTypes={prismEdgeTypes}
        defaultEdgeOptions={DEFAULT_EDGE_OPTIONS}
        onNodesChange={onNodesChange}
        onEdgesChange={onEdgesChange}
        {...(onNodeDoubleClick !== undefined ? { onNodeDoubleClick } : {})}
        {...(onNodeClick !== undefined ? { onNodeClick } : {})}
        onPaneClick={handlePaneClick}
        {...(useInitialViewport
          ? { defaultViewport: initialViewport }
          : { fitView })}
        {...(onInit !== undefined ? { onInit } : {})}
        proOptions={{ hideAttribution: true }}
      >
        {background ? (
          <Background
            color="#2a2e38"
            gap={24}
            size={1.4}
            variant={BACKGROUND_VARIANTS[backgroundVariant]}
          />
        ) : null}
        {controls ? <Controls showInteractive={false} /> : null}
        {minimap ? (
          <MiniMap
            pannable
            zoomable
            nodeStrokeColor="#4b5366"
            nodeColor="#262a33"
            nodeBorderRadius={4}
            maskColor="rgba(8, 10, 14, 0.65)"
            style={{ background: "#15171c", border: "1px solid #3a414f" }}
          />
        ) : null}
        {toolbar !== undefined && toolbar !== null ? (
          <Panel position={toolbarPosition}>{toolbar}</Panel>
        ) : null}
        {onViewportChange !== undefined ? (
          <ViewportBridge onViewportChange={onViewportChange} />
        ) : null}
      </ReactFlow>
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

// ── GraphToolbar ────────────────────────────────────────────────────────────

export type GraphToolbarProps = {
  /** Optional title rendered in the toolbar. */
  title?: ReactNode;
  /** Show fit-view button. Default true. */
  showFit?: boolean;
  /** Show zoom in/out buttons. Default true. */
  showZoom?: boolean;
  /** Show auto-layout button. Default true. */
  showAutoLayout?: boolean;
  /**
   * Direction options for the auto-layout button. Defaults to all four. Pass
   * an empty array to render only a single "Re-layout" button using
   * `defaultDirection`.
   */
  layoutDirections?: ReadonlyArray<NonNullable<LayoutOptions["direction"]>>;
  /** Direction used for the single "Re-layout" button. Default "DOWN". */
  defaultLayoutDirection?: NonNullable<LayoutOptions["direction"]>;
  /**
   * Called after `applyElkLayout` produces new positions. Receives the laid-out
   * nodes — caller is responsible for writing them back to its store. If
   * omitted, the toolbar falls back to writing positions through the
   * underlying Zustand graph store.
   */
  onAutoLayout?: (
    nodes: ReadonlyArray<{ id: string; x: number; y: number }>,
    direction: NonNullable<LayoutOptions["direction"]>,
  ) => void;
  /** Underlying graph store, used as the fallback writer for auto-layout. */
  store?: StoreApi<GraphStore>;
  /** Extra elements rendered after the standard buttons. */
  children?: ReactNode;
};

const TOOLBAR_BTN_STYLE: React.CSSProperties = {
  background: "#2a2a2a",
  border: "1px solid #444",
  borderRadius: 3,
  padding: "4px 8px",
  color: "#ccc",
  cursor: "pointer",
  fontSize: 11,
  lineHeight: 1.2,
};

const TOOLBAR_WRAPPER_STYLE: React.CSSProperties = {
  display: "flex",
  gap: 4,
  padding: 6,
  background: "rgba(20,20,20,0.85)",
  border: "1px solid #333",
  borderRadius: 4,
  alignItems: "center",
};

/**
 * GraphToolbar — built-in toolbar for PrismGraph. Wires fit/zoom/auto-layout
 * actions through `useReactFlow()` so it Just Works™ when placed via the
 * `toolbar` prop. Must be rendered inside a ReactFlowProvider — `PrismGraph`
 * mounts one automatically, so passing this via `toolbar={<GraphToolbar … />}`
 * is the correct usage pattern.
 */
export function GraphToolbar({
  title,
  showFit = true,
  showZoom = true,
  showAutoLayout = true,
  layoutDirections = ["DOWN", "RIGHT", "UP", "LEFT"],
  defaultLayoutDirection = "DOWN",
  onAutoLayout,
  store,
  children,
}: GraphToolbarProps) {
  const flow = useReactFlow();

  const runAutoLayout = useCallback(
    async (direction: NonNullable<LayoutOptions["direction"]>) => {
      const nodes = flow.getNodes();
      const edges = flow.getEdges();
      const laidOut = await applyElkLayout(nodes, edges, { direction });
      const positions = laidOut.map((n) => ({
        id: n.id,
        x: n.position.x,
        y: n.position.y,
      }));
      if (onAutoLayout) {
        onAutoLayout(positions, direction);
      } else if (store) {
        const moveNode = store.getState().moveNode;
        for (const p of positions) moveNode(p.id, p.x, p.y);
      } else {
        // Fall back to xyflow's internal node state.
        flow.setNodes(laidOut);
      }
      // Fit after layout so the result is always visible.
      window.setTimeout(() => flow.fitView({ duration: 250, padding: 0.1 }), 50);
    },
    [flow, onAutoLayout, store],
  );

  const directionsToShow =
    layoutDirections.length > 0 ? layoutDirections : [defaultLayoutDirection];
  const showSingleLayoutButton = layoutDirections.length === 0;

  return (
    <div
      style={TOOLBAR_WRAPPER_STYLE}
      data-testid="prism-graph-toolbar"
      role="toolbar"
      aria-label="Graph controls"
    >
      {title !== undefined ? (
        <span style={{ color: "#ddd", fontWeight: 600, marginRight: 8 }}>
          {title}
        </span>
      ) : null}
      {showFit ? (
        <button
          type="button"
          style={TOOLBAR_BTN_STYLE}
          title="Fit graph to view"
          data-testid="graph-toolbar-fit"
          onClick={() => flow.fitView({ duration: 250, padding: 0.1 })}
        >
          Fit
        </button>
      ) : null}
      {showZoom ? (
        <>
          <button
            type="button"
            style={TOOLBAR_BTN_STYLE}
            title="Zoom in"
            data-testid="graph-toolbar-zoom-in"
            onClick={() => flow.zoomIn({ duration: 200 })}
          >
            +
          </button>
          <button
            type="button"
            style={TOOLBAR_BTN_STYLE}
            title="Zoom out"
            data-testid="graph-toolbar-zoom-out"
            onClick={() => flow.zoomOut({ duration: 200 })}
          >
            −
          </button>
        </>
      ) : null}
      {showAutoLayout
        ? showSingleLayoutButton
          ? (
              <button
                type="button"
                style={TOOLBAR_BTN_STYLE}
                title={`Re-layout (${defaultLayoutDirection})`}
                data-testid="graph-toolbar-relayout"
                onClick={() => void runAutoLayout(defaultLayoutDirection)}
              >
                Re-layout
              </button>
            )
          : directionsToShow.map((dir) => (
              <button
                key={dir}
                type="button"
                style={TOOLBAR_BTN_STYLE}
                title={`Auto-layout ${dir.toLowerCase()}`}
                data-testid={`graph-toolbar-layout-${dir.toLowerCase()}`}
                onClick={() => void runAutoLayout(dir)}
              >
                {DIRECTION_GLYPH[dir]}
              </button>
            ))
        : null}
      {children}
    </div>
  );
}

const DIRECTION_GLYPH: Record<NonNullable<LayoutOptions["direction"]>, string> = {
  DOWN: "↓",
  UP: "↑",
  LEFT: "←",
  RIGHT: "→",
};
