import { describe, it, expect } from "vitest";
import { buildSitemapGraph } from "./sitemap-data.js";
import type { GraphObject, ObjectId } from "@prism/core/object-model";

function mk(
  id: string,
  type: string,
  opts: {
    parentId?: string | null;
    position?: number;
    data?: Record<string, unknown>;
    name?: string;
  } = {},
): GraphObject {
  return {
    id: id as unknown as ObjectId,
    type,
    name: opts.name ?? id,
    parentId: (opts.parentId ?? null) as unknown as GraphObject["parentId"],
    position: opts.position ?? 0,
    status: null,
    tags: [],
    date: null,
    endDate: null,
    description: null,
    color: null,
    image: null,
    pinned: false,
    data: opts.data ?? {},
    deletedAt: null,
    createdAt: "2024-01-01T00:00:00Z",
    updatedAt: "2024-01-01T00:00:00Z",
  } as unknown as GraphObject;
}

const app = mk("app1", "app", { data: { name: "Demo", homeRouteId: "r-home" } });
const appShell = mk("shell1", "app-shell", { parentId: "app1" });
const homeRoute = mk("r-home", "route", {
  parentId: "app1",
  position: 1,
  data: { path: "/", label: "Home", pageId: "p-home" },
});
const docsRoute = mk("r-docs", "route", {
  parentId: "app1",
  position: 2,
  data: { path: "/docs", label: "Docs", pageId: "p-docs" },
});
const guideRoute = mk("r-guide", "route", {
  parentId: "app1",
  position: 3,
  data: {
    path: "/docs/guide",
    label: "Guide",
    pageId: "p-guide",
    parentRouteId: "r-docs",
  },
});

describe("buildSitemapGraph — nodes", () => {
  it("returns empty graph when there are no routes", () => {
    const graph = buildSitemapGraph({ objects: [app, appShell] });
    expect(graph.nodes).toEqual([]);
    expect(graph.edges).toEqual([]);
  });

  it("projects every live route as a node with path and label", () => {
    const graph = buildSitemapGraph({
      objects: [app, appShell, homeRoute, docsRoute],
    });
    expect(graph.nodes.map((n) => n.id).sort()).toEqual(["r-docs", "r-home"]);
    const home = graph.nodes.find((n) => n.id === "r-home");
    expect(home?.label).toBe("Home");
    expect(home?.path).toBe("/");
  });

  it("marks the app's home route with isHome=true", () => {
    const graph = buildSitemapGraph({
      objects: [app, appShell, homeRoute, docsRoute],
    });
    expect(graph.nodes.find((n) => n.id === "r-home")?.isHome).toBe(true);
    expect(graph.nodes.find((n) => n.id === "r-docs")?.isHome).toBe(false);
  });

  it("computes depth from parentRouteId chains", () => {
    const graph = buildSitemapGraph({
      objects: [app, appShell, homeRoute, docsRoute, guideRoute],
    });
    expect(graph.nodes.find((n) => n.id === "r-home")?.depth).toBe(0);
    expect(graph.nodes.find((n) => n.id === "r-docs")?.depth).toBe(0);
    expect(graph.nodes.find((n) => n.id === "r-guide")?.depth).toBe(1);
  });
});

describe("buildSitemapGraph — hierarchy edges", () => {
  it("emits a hierarchy edge for every parentRouteId", () => {
    const graph = buildSitemapGraph({
      objects: [app, appShell, homeRoute, docsRoute, guideRoute],
    });
    const hier = graph.edges.filter((e) => e.kind === "hierarchy");
    expect(hier).toHaveLength(1);
    expect(hier[0]).toMatchObject({
      source: "r-docs",
      target: "r-guide",
      kind: "hierarchy",
    });
  });

  it("drops hierarchy edges pointing at unknown parents", () => {
    const orphan = mk("r-orphan", "route", {
      parentId: "app1",
      data: { path: "/x", label: "X", parentRouteId: "r-missing" },
    });
    const graph = buildSitemapGraph({
      objects: [app, appShell, homeRoute, orphan],
    });
    expect(graph.edges.filter((e) => e.kind === "hierarchy")).toHaveLength(0);
  });
});

describe("buildSitemapGraph — navigation edges", () => {
  it("infers a nav edge when a nav-bar inside the app links to a route path", () => {
    const link = mk("n1", "nav-bar", {
      parentId: "shell1",
      data: { label: "Docs", href: "/docs" },
    });
    const graph = buildSitemapGraph({
      objects: [app, appShell, homeRoute, docsRoute, link],
    });
    const navEdges = graph.edges.filter((e) => e.kind === "navigation");
    expect(navEdges).toHaveLength(1);
    expect(navEdges[0]).toMatchObject({
      source: "r-home", // originates at home by convention
      target: "r-docs",
      kind: "navigation",
    });
  });

  it("accepts targetRouteId as an explicit route ref", () => {
    const link = mk("b1", "button", {
      parentId: "shell1",
      data: { targetRouteId: "r-docs", label: "Go" },
    });
    const graph = buildSitemapGraph({
      objects: [app, appShell, homeRoute, docsRoute, link],
    });
    expect(graph.edges.filter((e) => e.kind === "navigation")).toHaveLength(1);
  });

  it("ignores nav elements that don't name a known route", () => {
    const link = mk("n1", "nav-bar", {
      parentId: "shell1",
      data: { href: "/nope" },
    });
    const graph = buildSitemapGraph({
      objects: [app, appShell, homeRoute, docsRoute, link],
    });
    expect(graph.edges.filter((e) => e.kind === "navigation")).toHaveLength(0);
  });
});

describe("buildSitemapGraph — transition edges", () => {
  it("adds a transition edge from the owning route to each navigate target", () => {
    const page = mk("p-home", "page", { parentId: "app1" });
    const hero = mk("h1", "hero", { parentId: "p-home" });
    const beh = mk("b1", "behavior", {
      parentId: "app1",
      data: {
        trigger: "onClick",
        enabled: true,
        targetObjectId: "h1",
        source: "ui.navigate('/docs')",
      },
    });
    const targets = new Map<string, string[]>([["b1", ["/docs"]]]);
    const graph = buildSitemapGraph({
      objects: [app, appShell, homeRoute, docsRoute, page, hero, beh],
      navigateTargets: targets,
    });
    const trEdges = graph.edges.filter((e) => e.kind === "transition");
    expect(trEdges).toHaveLength(1);
    expect(trEdges[0]).toMatchObject({
      source: "r-home",
      target: "r-docs",
      kind: "transition",
    });
  });

  it("skips disabled behaviors", () => {
    const beh = mk("b1", "behavior", {
      parentId: "app1",
      data: { enabled: false, source: "ui.navigate('/docs')" },
    });
    const targets = new Map<string, string[]>([["b1", ["/docs"]]]);
    const graph = buildSitemapGraph({
      objects: [app, appShell, homeRoute, docsRoute, beh],
      navigateTargets: targets,
    });
    expect(graph.edges.filter((e) => e.kind === "transition")).toHaveLength(0);
  });

  it("dedupes duplicate transitions coming from different scripts", () => {
    const beh1 = mk("b1", "behavior", { parentId: "app1", data: { enabled: true } });
    const beh2 = mk("b2", "behavior", { parentId: "app1", data: { enabled: true } });
    const targets = new Map<string, string[]>([
      ["b1", ["/docs"]],
      ["b2", ["/docs"]],
    ]);
    const graph = buildSitemapGraph({
      objects: [app, appShell, homeRoute, docsRoute, beh1, beh2],
      navigateTargets: targets,
    });
    expect(graph.edges.filter((e) => e.kind === "transition")).toHaveLength(1);
  });

  it("drops navigate targets that don't resolve to a known route", () => {
    const beh = mk("b1", "behavior", { parentId: "app1", data: { enabled: true } });
    const targets = new Map<string, string[]>([["b1", ["/missing"]]]);
    const graph = buildSitemapGraph({
      objects: [app, appShell, homeRoute, docsRoute, beh],
      navigateTargets: targets,
    });
    expect(graph.edges.filter((e) => e.kind === "transition")).toHaveLength(0);
  });
});

describe("buildSitemapGraph — appId scoping", () => {
  it("only returns routes parented under the given app", () => {
    const otherApp = mk("app2", "app", {});
    const otherRoute = mk("r-other", "route", {
      parentId: "app2",
      data: { path: "/x", label: "X" },
    });
    const graph = buildSitemapGraph({
      objects: [app, otherApp, homeRoute, otherRoute],
      appId: "app1",
    });
    expect(graph.nodes.map((n) => n.id)).toEqual(["r-home"]);
  });
});
