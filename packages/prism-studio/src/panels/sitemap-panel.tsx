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

import { useEffect, useMemo, useRef, useState } from "react";
import { LoroDoc } from "loro-crdt";
import { createGraphStore, type GraphStore } from "@prism/core/stores";
import { PrismGraph } from "@prism/core/graph";
import { findUiCalls, findNavigateCalls } from "@prism/core/luau";
import type { StoreApi } from "zustand";
import type { GraphObject } from "@prism/core/object-model";
import { lensId } from "@prism/core/lens";
import type { LensManifest } from "@prism/core/lens";
import { defineLensBundle, type LensBundle } from "../lenses/bundle.js";
import { useKernel } from "../kernel/index.js";
import {
  buildSitemapGraph,
  type SitemapEdge,
  type SitemapGraph,
  type SitemapNode,
} from "./sitemap-data.js";

const NODE_WIDTH = 220;
const NODE_HEIGHT = 72;
const COL_GUTTER = 120;
const ROW_GUTTER = 40;

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

function layoutNodes(graph: SitemapGraph): Array<{
  node: SitemapNode;
  x: number;
  y: number;
}> {
  // Stable by-depth columns, y stacked by order inside each depth bucket.
  const buckets = new Map<number, SitemapNode[]>();
  for (const n of graph.nodes) {
    const bucket = buckets.get(n.depth) ?? [];
    bucket.push(n);
    buckets.set(n.depth, bucket);
  }
  const laid: Array<{ node: SitemapNode; x: number; y: number }> = [];
  const depths = Array.from(buckets.keys()).sort((a, b) => a - b);
  for (const d of depths) {
    const bucket = buckets.get(d) ?? [];
    bucket.forEach((node, i) => {
      laid.push({
        node,
        x: 80 + d * (NODE_WIDTH + COL_GUTTER),
        y: 80 + i * (NODE_HEIGHT + ROW_GUTTER),
      });
    });
  }
  return laid;
}

function writeGraph(
  state: GraphStore,
  graph: SitemapGraph,
): void {
  for (const { node, x, y } of layoutNodes(graph)) {
    state.addNode({
      id: node.id as unknown as Parameters<typeof state.addNode>[0]["id"],
      type: "default",
      x,
      y,
      width: NODE_WIDTH,
      height: NODE_HEIGHT,
      data: {
        label: `${node.isHome ? "\u2302 " : ""}${node.label}\n${node.path}`,
        objectType: "route",
      },
    });
  }
  for (const edge of graph.edges) {
    writeEdge(state, edge);
  }
}

function writeEdge(state: GraphStore, edge: SitemapEdge): void {
  state.addEdge({
    id: edge.id as unknown as Parameters<typeof state.addEdge>[0]["id"],
    source: edge.source as unknown as Parameters<typeof state.addEdge>[0]["source"],
    target: edge.target as unknown as Parameters<typeof state.addEdge>[0]["target"],
    wireType: edge.kind === "hierarchy" ? "hard" : "weak",
    label: edge.label,
  });
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

  return (
    <div
      data-testid="sitemap-panel"
      style={{ width: "100%", height: "100%", background: "#1e1e1e" }}
    >
      <PrismGraph
        key={`sitemap-${version}`}
        store={storeRef.current}
        className="prism-sitemap-panel"
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
