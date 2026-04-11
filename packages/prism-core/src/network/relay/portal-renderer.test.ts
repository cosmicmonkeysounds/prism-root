import { describe, it, expect } from "vitest";
import { extractPortalSnapshot, escapeHtml, renderPortalHtml } from "./portal-renderer.js";
import { createCollectionStore } from "@prism/core/persistence";
import type { PortalManifest } from "./relay-types.js";
import type { GraphObject } from "@prism/core/object-model";

function makeObject(overrides: Partial<GraphObject> & { id: string; type: string; name: string }): GraphObject {
  return {
    parentId: null,
    position: 0,
    status: null,
    tags: [],
    date: null,
    endDate: null,
    description: "",
    color: null,
    image: null,
    pinned: false,
    data: {},
    createdAt: "2026-01-01T00:00:00Z",
    updatedAt: "2026-01-01T00:00:00Z",
    ...overrides,
  };
}

const portal: PortalManifest = {
  portalId: "portal-1",
  name: "Test Portal",
  level: 1,
  collectionId: "col-1",
  basePath: "/",
  isPublic: true,
  createdAt: "2026-01-01T00:00:00Z",
};

describe("extractPortalSnapshot", () => {
  it("extracts root objects from a collection", () => {
    const store = createCollectionStore();
    store.putObject(makeObject({ id: "a", type: "task", name: "Task A", position: 0 }));
    store.putObject(makeObject({ id: "b", type: "task", name: "Task B", position: 1 }));

    const snapshot = extractPortalSnapshot(portal, store);
    expect(snapshot.portal).toBe(portal);
    expect(snapshot.objectCount).toBe(2);
    expect(snapshot.objects).toHaveLength(2);
    expect(snapshot.objects[0].name).toBe("Task A");
    expect(snapshot.objects[1].name).toBe("Task B");
    expect(snapshot.generatedAt).toBeTruthy();
  });

  it("preserves parent-child hierarchy", () => {
    const store = createCollectionStore();
    store.putObject(makeObject({ id: "parent", type: "project", name: "Project", position: 0 }));
    store.putObject(makeObject({ id: "child1", type: "task", name: "Child 1", parentId: "parent", position: 0 }));
    store.putObject(makeObject({ id: "child2", type: "task", name: "Child 2", parentId: "parent", position: 1 }));

    const snapshot = extractPortalSnapshot(portal, store);
    expect(snapshot.objects).toHaveLength(1);
    expect(snapshot.objects[0].children).toHaveLength(2);
    expect(snapshot.objects[0].children[0].name).toBe("Child 1");
    expect(snapshot.objects[0].children[1].name).toBe("Child 2");
    expect(snapshot.objectCount).toBe(3);
  });

  it("sorts children by position", () => {
    const store = createCollectionStore();
    store.putObject(makeObject({ id: "root", type: "project", name: "Root", position: 0 }));
    store.putObject(makeObject({ id: "c", type: "task", name: "Third", parentId: "root", position: 2 }));
    store.putObject(makeObject({ id: "a", type: "task", name: "First", parentId: "root", position: 0 }));
    store.putObject(makeObject({ id: "b", type: "task", name: "Second", parentId: "root", position: 1 }));

    const snapshot = extractPortalSnapshot(portal, store);
    const names = snapshot.objects[0].children.map((c) => c.name);
    expect(names).toEqual(["First", "Second", "Third"]);
  });

  it("includes edges", () => {
    const store = createCollectionStore();
    store.putObject(makeObject({ id: "a", type: "task", name: "A" }));
    store.putObject(makeObject({ id: "b", type: "task", name: "B" }));
    store.putEdge({
      id: "e1",
      sourceId: "a",
      targetId: "b",
      relation: "depends-on",
      position: 0,
      data: {},
      createdAt: "2026-01-01T00:00:00Z",
    });

    const snapshot = extractPortalSnapshot(portal, store);
    expect(snapshot.edges).toHaveLength(1);
    expect(snapshot.edges[0].relation).toBe("depends-on");
    expect(snapshot.edges[0].sourceId).toBe("a");
  });

  it("copies data fields to prevent mutation", () => {
    const store = createCollectionStore();
    store.putObject(makeObject({ id: "a", type: "task", name: "A", data: { foo: "bar" }, tags: ["urgent"] }));

    const snapshot = extractPortalSnapshot(portal, store);
    expect(snapshot.objects[0].data).toEqual({ foo: "bar" });
    expect(snapshot.objects[0].tags).toEqual(["urgent"]);
  });

  it("returns empty snapshot for empty collection", () => {
    const store = createCollectionStore();
    const snapshot = extractPortalSnapshot(portal, store);
    expect(snapshot.objects).toHaveLength(0);
    expect(snapshot.edges).toHaveLength(0);
    expect(snapshot.objectCount).toBe(0);
  });
});

describe("escapeHtml", () => {
  it("escapes all HTML entities", () => {
    expect(escapeHtml('<script>alert("xss")</script>')).toBe(
      "&lt;script&gt;alert(&quot;xss&quot;)&lt;/script&gt;",
    );
  });

  it("escapes ampersands and single quotes", () => {
    expect(escapeHtml("Tom & Jerry's")).toBe("Tom &amp; Jerry&#39;s");
  });
});

describe("renderPortalHtml", () => {
  it("renders a complete HTML document", () => {
    const store = createCollectionStore();
    store.putObject(makeObject({ id: "a", type: "task", name: "My Task", status: "active", tags: ["v1"] }));

    const snapshot = extractPortalSnapshot(portal, store);
    const html = renderPortalHtml(snapshot);

    expect(html).toContain("<!DOCTYPE html>");
    expect(html).toContain("<title>Test Portal</title>");
    expect(html).toContain("My Task");
    expect(html).toContain("active");
    expect(html).toContain("v1");
    expect(html).toContain("1 objects");
    expect(html).toContain("Level 1 portal");
  });

  it("renders nested objects with increasing heading levels", () => {
    const store = createCollectionStore();
    store.putObject(makeObject({ id: "p", type: "project", name: "Project" }));
    store.putObject(makeObject({ id: "c", type: "task", name: "Child", parentId: "p" }));

    const snapshot = extractPortalSnapshot(portal, store);
    const html = renderPortalHtml(snapshot);

    expect(html).toContain("<h2>Project");
    expect(html).toContain("<h3>Child");
  });

  it("escapes user content in rendered HTML", () => {
    const store = createCollectionStore();
    store.putObject(makeObject({ id: "x", type: "task", name: '<img src=x onerror="alert(1)">' }));

    const snapshot = extractPortalSnapshot(portal, store);
    const html = renderPortalHtml(snapshot);

    expect(html).not.toContain('<img src=x onerror="alert(1)">');
    expect(html).toContain("&lt;img src=x onerror=&quot;alert(1)&quot;&gt;");
  });
});
