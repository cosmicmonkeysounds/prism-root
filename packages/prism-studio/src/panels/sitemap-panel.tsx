/**
 * Sitemap Panel — interactive route/navigation graph for a Prism App.
 *
 * Reuses the `PrismGraph` binding from `@prism/core/graph` to render the
 * routes of the currently-selected app plus three edge kinds (hierarchy,
 * navigation, transition). Selecting a node selects the underlying route
 * object so the inspector picks it up; double-click selects the route's
 * target page so the Layout panel can edit it.
 *
 * All projection logic lives in `./sitemap-data.ts`. This file is just
 * wiring: subscribe to the kernel, parse behavior scripts, push the result
 * into a local graph store.
 */

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { LoroDoc } from "loro-crdt";
import { createGraphStore, type GraphStore } from "@prism/core/stores";
import { PrismGraph, GraphToolbar, applyElkLayout } from "@prism/core/graph";
import { findUiCalls, findNavigateCalls } from "@prism/core/luau";
import type { Viewport } from "@xyflow/react";
import type { StoreApi } from "zustand";
import type { GraphObject } from "@prism/core/object-model";
import { lensId } from "@prism/core/lens";
import type { LensManifest } from "@prism/core/lens";
import { defineLensBundle, type LensBundle } from "../lenses/bundle.js";
import { useKernel } from "../kernel/index.js";

const VIEWPORT_KEY = "lens:sitemap";
import {
  buildSitemapGraph,
  type SitemapEdge,
  type SitemapGraph,
} from "./sitemap-data.js";

const NODE_WIDTH = 220;
const NODE_HEIGHT = 80;

/** Resolve "which app is selected right now?" from the kernel selection. */
function resolveSelectedAppId(
  selection: ReadonlyArray<string>,
  objects: ReadonlyArray<GraphObject>,
): string | undefined {
  // Walk each selected object up its parent chain, returning the id of the
  // first enclosing `app` we find. Falls back to the first `app` in the
  // workspace when the user hasn't selected anything.
  const byId = new Map<string, GraphObject>();
  for (const o of objects) byId.set(o.id as unknown as string, o);
  for (const sel of selection) {
    let cursor = byId.get(sel);
    while (cursor) {
      if (cursor.type === "app") return cursor.id as unknown as string;
      if (!cursor.parentId) break;
      cursor = byId.get(cursor.parentId as unknown as string);
    }
  }
  const firstApp = objects.find((o) => o.type === "app" && !o.deletedAt);
  return firstApp ? (firstApp.id as unknown as string) : undefined;
}

function writeGraph(state: GraphStore, graph: SitemapGraph): void {
  // Initial position is (0, 0) for every node — `applyElkLayout` rewrites
  // them in `runInitialLayout` after the store is populated. The custom
  // `sitemap` node renderer (in `@prism/core/graph`) draws path/home glyphs.
  for (const node of graph.nodes) {
    state.addNode({
      id: node.id as unknown as Parameters<typeof state.addNode>[0]["id"],
      type: "sitemap",
      x: 0,
      y: 0,
      width: NODE_WIDTH,
      height: NODE_HEIGHT,
      data: {
        label: node.label,
        path: node.path,
        isHome: node.isHome,
      },
    });
  }
  for (const edge of graph.edges) {
    writeEdge(state, edge);
  }
}

function writeEdge(state: GraphStore, edge: SitemapEdge): void {
  // Three sitemap edge kinds map onto Prism's three wire types so the
  // graph theme (hard = solid cyan, weak = dashed slate, stream =
  // animated amber) makes the route relationships visually distinct.
  const wireType: "hard" | "weak" | "stream" =
    edge.kind === "hierarchy"
      ? "hard"
      : edge.kind === "transition"
        ? "stream"
        : "weak";
  state.addEdge({
    id: edge.id as unknown as Parameters<typeof state.addEdge>[0]["id"],
    source: edge.source as unknown as Parameters<typeof state.addEdge>[0]["source"],
    target: edge.target as unknown as Parameters<typeof state.addEdge>[0]["target"],
    wireType,
    label: edge.label,
  });
}

async function runInitialLayout(store: StoreApi<GraphStore>): Promise<void> {
  const state = store.getState();
  if (state.nodes.length === 0) return;
  const elkNodes = state.nodes.map((n) => ({
    id: n.id,
    position: { x: n.x, y: n.y },
    data: {},
    width: n.width ?? NODE_WIDTH,
    height: n.height ?? NODE_HEIGHT,
  }));
  const elkEdges = state.edges.map((e) => ({
    id: e.id,
    source: e.source,
    target: e.target,
  }));
  const laidOut = await applyElkLayout(elkNodes, elkEdges, { direction: "DOWN" });
  const moveNode = store.getState().moveNode;
  for (const n of laidOut) moveNode(n.id, n.position.x, n.position.y);
}

export function SitemapPanel() {
  const kernel = useKernel();
  const storeRef = useRef<StoreApi<GraphStore> | null>(null);
  const [ready, setReady] = useState(false);
  const [version, setVersion] = useState(0);
  const [navigateTargets, setNavigateTargets] = useState<Map<string, string[]>>(
    () => new Map(),
  );

  const rebuild = useMemo(
    () => async () => {
      const objects = kernel.store.allObjects();
      const selectedId = kernel.atoms.getState().selectedId;
      const selection = selectedId ? [selectedId as unknown as string] : [];
      const appId = resolveSelectedAppId(selection, objects);

      // Parse every enabled behavior in parallel so we can stamp navigate
      // targets before projecting the graph.
      const behaviors = objects.filter(
        (o) => o.type === "behavior" && !o.deletedAt,
      );
      const entries: Array<[string, string[]]> = [];
      for (const b of behaviors) {
        const data = b.data as Record<string, unknown>;
        if (data["enabled"] === false) continue;
        const src = typeof data["source"] === "string" ? (data["source"] as string) : "";
        if (!src) continue;
        try {
          const parsed = await findUiCalls(src);
          const targets = findNavigateCalls(parsed.calls);
          if (targets.length > 0) entries.push([b.id as unknown as string, targets]);
        } catch {
          // Swallow parser failures — user can still fix the script.
        }
      }
      const newTargets = new Map(entries);
      setNavigateTargets(newTargets);

      const graph = buildSitemapGraph({
        objects,
        navigateTargets: newTargets,
        ...(appId ? { appId } : {}),
      });

      const doc = new LoroDoc();
      const store = createGraphStore(doc);
      storeRef.current = store;
      writeGraph(store.getState(), graph);
      // Run elk before flushing the version bump so the first paint already
      // shows nodes in their final positions instead of stacked at (0,0).
      await runInitialLayout(store);
      setVersion((v) => v + 1);
      setReady(true);
    },
    [kernel],
  );

  useEffect(() => {
    void rebuild();
    const unsub = kernel.store.onChange(() => {
      void rebuild();
    });
    return unsub;
  }, [kernel, rebuild]);

  // Exposed for debugging / future "counter" display; not rendered.
  void navigateTargets;

  // Viewport persistence — survives tab switches via kernel.viewportCache.
  const cache = kernel.viewportCache;
  const initialViewport = useMemo(
    () => cache.getState().get(VIEWPORT_KEY),
    [cache],
  );
  const handleViewportChange = useCallback(
    (v: Viewport) => {
      cache.getState().set(VIEWPORT_KEY, v);
    },
    [cache],
  );

  if (!ready || !storeRef.current) {
    return (
      <div
        data-testid="sitemap-panel"
        style={{ padding: 16, color: "#888", fontSize: 12 }}
      >
        Loading sitemap…
      </div>
    );
  }

  const store = storeRef.current;
  const nodeCount = store.getState().nodes.length;
  const edgeCount = store.getState().edges.length;

  return (
    <div
      data-testid="sitemap-panel"
      style={{ width: "100%", height: "100%", background: "#1e1e1e" }}
    >
      <PrismGraph
        key={`sitemap-${version}`}
        store={store}
        className="prism-sitemap-panel"
        minimap
        background
        controls
        {...(initialViewport !== undefined ? { initialViewport } : {})}
        onViewportChange={handleViewportChange}
        toolbar={
          <GraphToolbar
            title={
              <span>
                Sitemap · {nodeCount} route{nodeCount === 1 ? "" : "s"} ·{" "}
                {edgeCount} link{edgeCount === 1 ? "" : "s"}
              </span>
            }
            store={store}
          />
        }
      />
    </div>
  );
}

// ── Lens registration ──────────────────────────────────────────────────────

export const SITEMAP_LENS_ID = lensId("sitemap");

export const sitemapLensManifest: LensManifest = {
  id: SITEMAP_LENS_ID,
  name: "Sitemap",
  icon: "\u{1F9ED}",
  category: "custom",
  contributes: {
    views: [{ slot: "main" }],
    commands: [
      {
        id: "switch-sitemap",
        name: "Switch to Sitemap",
        shortcut: ["shift+m"],
        section: "Navigation",
      },
    ],
  },
};

export const sitemapLensBundle: LensBundle = defineLensBundle(
  sitemapLensManifest,
  SitemapPanel,
);
