/**
 * Pure data layer for the Sitemap panel.
 *
 * Projects a slice of the kernel (app, routes, nav-bar children, behavior
 * scripts) into a graph-node/edge model a PrismGraph can render. No React,
 * no WASM, no Puck — all kernel lookups come in as plain GraphObject arrays
 * so vitest can exercise every edge inference rule without booting the
 * studio kernel.
 *
 * Three edge flavors are inferred:
 *
 *   1. "hierarchy"  — parentRouteId on a route → its parent route.
 *   2. "navigation" — nav-bar / site-nav / button children sitting inside an
 *                     app-shell that point at a route via `targetRouteId`
 *                     (or, for buttons, via `href` matching a route `path`).
 *   3. "transition" — `behavior` object whose Luau source calls
 *                     `ui.navigate("/path")`; path resolution happens via a
 *                     caller-supplied `navigateTargets` map so the caller
 *                     controls whether the parser is WASM-backed (panel) or
 *                     pre-parsed (tests).
 */

import type { GraphObject } from "@prism/core/object-model";

export interface SitemapNode {
  /** Kernel object id of the route this node represents. */
  id: string;
  /** Display label. */
  label: string;
  /** URL path, e.g. `/`, `/tasks/:id`. */
  path: string;
  /** True if this route is the home route for the owning app. */
  isHome: boolean;
  /** Route nesting depth (via parentRouteId chain). */
  depth: number;
}

export type SitemapEdgeKind = "hierarchy" | "navigation" | "transition";

export interface SitemapEdge {
  id: string;
  source: string;
  target: string;
  kind: SitemapEdgeKind;
  /** Human-friendly edge label, e.g. "nav", "click→", "parent". */
  label: string;
}

export interface SitemapGraph {
  nodes: SitemapNode[];
  edges: SitemapEdge[];
}

export interface BuildSitemapInput {
  /** Every non-deleted kernel object (studio passes `kernel.store.allObjects()`). */
  objects: ReadonlyArray<GraphObject>;
  /**
   * Behavior id → list of navigate target paths. Usually produced in the
   * panel by running `findNavigateCalls(findUiCalls(behavior.source))` for
   * each enabled behavior. Tests pass a plain object literal.
   */
  navigateTargets?: ReadonlyMap<string, ReadonlyArray<string>>;
  /** Optional app id to restrict the projection to a single buildable app. */
  appId?: string;
}

/** Pure projection — given the kernel slice, return a sitemap graph. */
export function buildSitemapGraph(input: BuildSitemapInput): SitemapGraph {
  const { objects, navigateTargets = new Map(), appId } = input;
  const live = objects.filter((o) => !o.deletedAt);

  // 1. Pick the routes scoped to (optionally) a single app.
  const routes = live.filter(
    (o) =>
      o.type === "route" &&
      (appId === undefined || o.parentId === (appId as unknown as GraphObject["parentId"])),
  );
  if (routes.length === 0) return { nodes: [], edges: [] };

  const routeById = new Map<string, GraphObject>();
  for (const r of routes) routeById.set(r.id as unknown as string, r);

  // The owning app is the parent of the first route when the caller didn't
  // pin an explicit appId — lets the projector find the home route on the
  // app object, if any.
  const resolvedAppId: string | undefined =
    appId ?? (routes[0]?.parentId as unknown as string | null) ?? undefined;

  const app = resolvedAppId ? live.find((o) => (o.id as unknown as string) === resolvedAppId && o.type === "app") : undefined;
  const homeRouteId =
    app && typeof (app.data as Record<string, unknown>)["homeRouteId"] === "string"
      ? ((app.data as Record<string, unknown>)["homeRouteId"] as string)
      : undefined;

  // path → routeId, used to resolve navigation / transition edges.
  const routeByPath = new Map<string, string>();
  for (const r of routes) {
    const path = (r.data as Record<string, unknown>)["path"];
    if (typeof path === "string" && path !== "") {
      routeByPath.set(path, r.id as unknown as string);
    }
  }

  const depthCache = new Map<string, number>();
  const depthOf = (id: string): number => {
    const cached = depthCache.get(id);
    if (cached !== undefined) return cached;
    const guard = new Set<string>();
    let depth = 0;
    let cursor: GraphObject | undefined = routeById.get(id);
    while (cursor) {
      const parentId = (cursor.data as Record<string, unknown>)["parentRouteId"];
      if (typeof parentId !== "string" || parentId === "") break;
      if (guard.has(parentId)) break;
      guard.add(parentId);
      const next = routeById.get(parentId);
      if (!next) break;
      cursor = next;
      depth += 1;
    }
    depthCache.set(id, depth);
    return depth;
  };

  const nodes: SitemapNode[] = routes
    .slice()
    .sort((a, b) => a.position - b.position)
    .map((r) => {
      const data = r.data as Record<string, unknown>;
      const id = r.id as unknown as string;
      return {
        id,
        label: (data["label"] as string) || r.name,
        path: (data["path"] as string) || "",
        isHome: homeRouteId === id,
        depth: depthOf(id),
      };
    });

  const edges: SitemapEdge[] = [];
  const pushEdge = (edge: SitemapEdge) => {
    // Dedup by (source,target,kind) so multiple scripts/nav items pointing at
    // the same target don't render as an overlapping bundle of arrows.
    const key = `${edge.kind}:${edge.source}:${edge.target}`;
    if (edges.some((e) => `${e.kind}:${e.source}:${e.target}` === key)) return;
    edges.push(edge);
  };

  // 2. Hierarchy edges (parentRouteId).
  for (const r of routes) {
    const data = r.data as Record<string, unknown>;
    const parentId = data["parentRouteId"];
    if (typeof parentId !== "string" || parentId === "") continue;
    if (!routeById.has(parentId)) continue;
    pushEdge({
      id: `hier-${parentId}-${r.id as unknown as string}`,
      source: parentId,
      target: r.id as unknown as string,
      kind: "hierarchy",
      label: "parent",
    });
  }

  // 3. Navigation edges: every nav-bar / site-nav / button child of the app
  //    or its app-shell that names a route (by id or by path).
  const navSources = live.filter((o) => {
    if (o.type !== "nav-bar" && o.type !== "site-nav" && o.type !== "button") {
      return false;
    }
    // Walk up to the nearest app / app-shell; accept anything that ends up
    // under the owning app when appId is pinned, or any app when it isn't.
    let cursor: GraphObject | undefined = o;
    while (cursor) {
      if (cursor.type === "app") {
        return resolvedAppId ? (cursor.id as unknown as string) === resolvedAppId : true;
      }
      if (!cursor.parentId) return false;
      cursor = live.find((p) => p.id === cursor!.parentId);
    }
    return false;
  });

  for (const nav of navSources) {
    const data = nav.data as Record<string, unknown>;
    const targetId = typeof data["targetRouteId"] === "string" ? (data["targetRouteId"] as string) : "";
    const href = typeof data["href"] === "string" ? (data["href"] as string) : "";
    let routeId: string | undefined;
    if (targetId && routeById.has(targetId)) routeId = targetId;
    else if (href && routeByPath.has(href)) routeId = routeByPath.get(href);
    if (!routeId) continue;

    // Which route is the "source" of the edge? The nav element itself lives
    // at the app scope (app-shell chrome), so we model it as originating
    // from the home route if known, otherwise the first route.
    const sourceRoute = homeRouteId ?? (nodes[0]?.id ?? "");
    if (!sourceRoute || sourceRoute === routeId) continue;
    pushEdge({
      id: `nav-${nav.id as unknown as string}-${routeId}`,
      source: sourceRoute,
      target: routeId,
      kind: "navigation",
      label: (data["label"] as string) || "nav",
    });
  }

  // 4. Transition edges: behavior objects whose script calls ui.navigate.
  const behaviors = live.filter((o) => o.type === "behavior");
  for (const b of behaviors) {
    const data = b.data as Record<string, unknown>;
    if (data["enabled"] === false) continue;
    const targets = navigateTargets.get(b.id as unknown as string) ?? [];
    if (targets.length === 0) continue;

    // Resolve the source route: if the behavior targets a route, start
    // there; otherwise walk up from the target object to find the nearest
    // enclosing route (via route.pageId) or fall back to the home route.
    const targetObjectId =
      typeof data["targetObjectId"] === "string" ? (data["targetObjectId"] as string) : "";
    const sourceRouteId = resolveOwningRoute(targetObjectId, routes, live) ?? homeRouteId ?? nodes[0]?.id;
    if (!sourceRouteId) continue;

    for (const path of targets) {
      const dest = routeByPath.get(path);
      if (!dest) continue;
      if (dest === sourceRouteId) continue;
      pushEdge({
        id: `tr-${b.id as unknown as string}-${dest}`,
        source: sourceRouteId,
        target: dest,
        kind: "transition",
        label: "click→",
      });
    }
  }

  return { nodes, edges };
}

/**
 * Walk up from `targetObjectId` through the `parentId` chain and return the
 * id of the first enclosing route (directly matched, or indirectly matched
 * via a route whose `pageId` points at an ancestor). Returns undefined when
 * no route claims this subtree.
 */
function resolveOwningRoute(
  targetObjectId: string,
  routes: ReadonlyArray<GraphObject>,
  live: ReadonlyArray<GraphObject>,
): string | undefined {
  if (!targetObjectId) return undefined;

  // Direct match: the behavior targets a route itself.
  const directRoute = routes.find((r) => (r.id as unknown as string) === targetObjectId);
  if (directRoute) return directRoute.id as unknown as string;

  // Walk ancestors; at each step, see if any route.pageId points at the
  // current ancestor. That's the "which page owns this block" resolver.
  const byId = new Map<string, GraphObject>();
  for (const o of live) byId.set(o.id as unknown as string, o);

  let cursor: GraphObject | undefined = byId.get(targetObjectId);
  const guard = new Set<string>();
  while (cursor) {
    const id = cursor.id as unknown as string;
    if (guard.has(id)) return undefined;
    guard.add(id);
    const owner = routes.find(
      (r) => (r.data as Record<string, unknown>)["pageId"] === id,
    );
    if (owner) return owner.id as unknown as string;
    if (!cursor.parentId) return undefined;
    cursor = byId.get(cursor.parentId as unknown as string);
  }
  return undefined;
}
