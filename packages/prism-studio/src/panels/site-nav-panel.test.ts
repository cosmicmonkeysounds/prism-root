import { describe, it, expect } from "vitest";
import { buildSiteNav } from "./site-nav-panel.js";
import type { GraphObject, ObjectId } from "@prism/core/object-model";

function mkPage(
  id: string,
  name: string,
  position: number,
  data: Record<string, unknown> = {},
  deletedAt: string | null = null,
): GraphObject {
  return {
    id: id as unknown as ObjectId,
    type: "page",
    name,
    parentId: null,
    position,
    status: null,
    tags: [],
    date: null,
    endDate: null,
    description: null,
    color: null,
    image: null,
    pinned: false,
    data,
    deletedAt,
    createdAt: "2024-01-01T00:00:00Z",
    updatedAt: "2024-01-01T00:00:00Z",
  } as unknown as GraphObject;
}

function mkRoute(
  id: string,
  name: string,
  position: number,
  data: Record<string, unknown> = {},
): GraphObject {
  return {
    id: id as unknown as ObjectId,
    type: "route",
    name,
    parentId: null,
    position,
    status: null,
    tags: [],
    date: null,
    endDate: null,
    description: null,
    color: null,
    image: null,
    pinned: false,
    data,
    deletedAt: null,
    createdAt: "2024-01-01T00:00:00Z",
    updatedAt: "2024-01-01T00:00:00Z",
  } as unknown as GraphObject;
}

describe("buildSiteNav", () => {
  it("skips non-page objects", () => {
    const items = buildSiteNav([
      mkPage("p1", "Home", 0),
      { ...mkPage("s1", "Section", 1), type: "section" } as GraphObject,
    ]);
    expect(items.map((i) => i.id)).toEqual(["p1"]);
  });

  it("skips deleted pages", () => {
    const items = buildSiteNav([
      mkPage("p1", "Keep", 0),
      mkPage("p2", "Gone", 1, {}, "2024-01-02T00:00:00Z"),
    ]);
    expect(items.map((i) => i.id)).toEqual(["p1"]);
  });

  it("sorts pages by position", () => {
    const items = buildSiteNav([
      mkPage("b", "Second", 1),
      mkPage("a", "First", 0),
      mkPage("c", "Third", 2),
    ]);
    expect(items.map((i) => i.id)).toEqual(["a", "b", "c"]);
  });

  it("reads label from data.title, falling back to name", () => {
    const items = buildSiteNav([
      mkPage("p1", "page-one", 0, { title: "Landing Page" }),
      mkPage("p2", "page-two", 1, {}),
    ]);
    expect(items.map((i) => i.label)).toEqual(["Landing Page", "page-two"]);
  });

  it("reads slug, isHome, and hidden flags from data", () => {
    const items = buildSiteNav([
      mkPage("p1", "Home", 0, { slug: "home", isHome: true }),
      mkPage("p2", "Secret", 1, { slug: "hidden", hiddenInNav: true }),
    ]);
    expect(items[0]).toMatchObject({ slug: "home", isHome: true, hidden: false });
    expect(items[1]).toMatchObject({ slug: "hidden", isHome: false, hidden: true });
  });

  it("uses routes when any route objects are present", () => {
    // Routes + pages in the same set; routes take over.
    const items = buildSiteNav([
      mkPage("p1", "ignored", 0, { title: "Ignored" }),
      mkRoute("r1", "root", 0, { path: "/", label: "Home", showInNav: true, isHome: true }),
      mkRoute("r2", "docs", 1, { path: "/docs", label: "Docs", showInNav: true }),
    ]);
    expect(items.map((i) => i.id)).toEqual(["r1", "r2"]);
    expect(items[0]).toMatchObject({ label: "Home", slug: "/", isHome: true });
  });

  it("computes depth from parentRouteId chains", () => {
    const items = buildSiteNav([
      mkRoute("root", "root", 0, { path: "/", label: "Home" }),
      mkRoute("docs", "docs", 1, { path: "/docs", label: "Docs", parentRouteId: "root" }),
      mkRoute("guide", "guide", 2, {
        path: "/docs/guide",
        label: "Guide",
        parentRouteId: "docs",
      }),
    ]);
    expect(items.find((i) => i.id === "root")?.depth).toBe(0);
    expect(items.find((i) => i.id === "docs")?.depth).toBe(1);
    expect(items.find((i) => i.id === "guide")?.depth).toBe(2);
  });

  it("hides routes with showInNav=false", () => {
    const items = buildSiteNav([
      mkRoute("r1", "home", 0, { path: "/", label: "Home", showInNav: true }),
      mkRoute("r2", "admin", 1, { path: "/admin", label: "Admin", showInNav: false }),
    ]);
    expect(items.find((i) => i.id === "r1")?.hidden).toBe(false);
    expect(items.find((i) => i.id === "r2")?.hidden).toBe(true);
  });

  it("guards against parentRouteId cycles without infinite looping", () => {
    const items = buildSiteNav([
      mkRoute("a", "a", 0, { path: "/a", label: "A", parentRouteId: "b" }),
      mkRoute("b", "b", 1, { path: "/b", label: "B", parentRouteId: "a" }),
    ]);
    // Both routes still produce nav items (no infinite loop) and get some depth.
    expect(items.map((i) => i.id).sort()).toEqual(["a", "b"]);
  });
});
