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
});
