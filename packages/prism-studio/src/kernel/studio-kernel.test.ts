import { describe, it, expect, beforeEach, vi } from "vitest";
import { createStudioKernel } from "./studio-kernel.js";
import type { StudioKernel } from "./studio-kernel.js";
import { createSavedView } from "@prism/core/view";
import { createStaticValueList } from "@prism/core/facet";
import { createPrivilegeSet } from "@prism/core/manifest";

describe("StudioKernel", () => {
  let kernel: StudioKernel;

  beforeEach(() => {
    kernel = createStudioKernel();
  });

  // ── Registry ────────────────────────────────────────────────────────────

  describe("entity registry", () => {
    it("should register page-builder entity types", () => {
      expect(kernel.registry.has("page")).toBe(true);
      expect(kernel.registry.has("section")).toBe(true);
      expect(kernel.registry.has("heading")).toBe(true);
      expect(kernel.registry.has("text-block")).toBe(true);
      expect(kernel.registry.has("image")).toBe(true);
      expect(kernel.registry.has("button")).toBe(true);
      expect(kernel.registry.has("card")).toBe(true);
      expect(kernel.registry.has("luau-block")).toBe(true);
      expect(kernel.registry.has("folder")).toBe(true);
    });

    it("should register edge types", () => {
      expect(kernel.registry.getEdgeType("references")).toBeDefined();
      expect(kernel.registry.getEdgeType("links-to")).toBeDefined();
    });

    it("should enforce containment rules", () => {
      expect(kernel.registry.canBeChildOf("section", "page")).toBe(true);
      expect(kernel.registry.canBeChildOf("heading", "section")).toBe(true);
      expect(kernel.registry.canBeChildOf("page", "heading")).toBe(false);
    });

    it("should have fields for page type", () => {
      const fields = kernel.registry.getEntityFields("page");
      const fieldIds = fields.map((f) => f.id);
      expect(fieldIds).toContain("title");
      expect(fieldIds).toContain("slug");
      expect(fieldIds).toContain("layout");
      expect(fieldIds).toContain("published");
    });

    it("should have fields for heading type", () => {
      const fields = kernel.registry.getEntityFields("heading");
      const fieldIds = fields.map((f) => f.id);
      expect(fieldIds).toContain("text");
      expect(fieldIds).toContain("level");
      expect(fieldIds).toContain("align");
    });
  });

  // ── CRUD ────────────────────────────────────────────────────────────────

  describe("createObject", () => {
    it("should create an object in the CollectionStore", () => {
      const obj = kernel.createObject({
        type: "page",
        name: "Test Page",
        parentId: null,
        position: 0,
        status: "draft",
        tags: [],
        date: null,
        endDate: null,
        description: "",
        color: null,
        image: null,
        pinned: false,
        data: { title: "Test", slug: "/test" },
      });

      expect(obj.id).toBeTruthy();
      expect(obj.name).toBe("Test Page");
      expect(obj.createdAt).toBeTruthy();

      // Should be in the store
      const stored = kernel.store.getObject(obj.id);
      expect(stored).toBeDefined();
      expect(stored?.name).toBe("Test Page");
    });

    it("should push an undo entry", () => {
      kernel.createObject({
        type: "page",
        name: "Undo Test",
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
      });

      expect(kernel.undo.canUndo).toBe(true);
      expect(kernel.undo.undoLabel).toContain("Undo Test");
    });

    it("should sync to ObjectAtomStore", () => {
      const obj = kernel.createObject({
        type: "heading",
        name: "Atom Test",
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
        data: { text: "Hello", level: "h1" },
      });

      const cached = kernel.objectAtoms.getState().objects[obj.id];
      expect(cached).toBeDefined();
      expect(cached?.name).toBe("Atom Test");
    });
  });

  describe("updateObject", () => {
    it("should update an existing object", () => {
      const obj = kernel.createObject({
        type: "page",
        name: "Original",
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
      });

      const updated = kernel.updateObject(obj.id, { name: "Renamed" });
      expect(updated?.name).toBe("Renamed");
      expect(kernel.store.getObject(obj.id)?.name).toBe("Renamed");
    });

    it("should update data fields", () => {
      const obj = kernel.createObject({
        type: "page",
        name: "Data Test",
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
        data: { title: "Old Title" },
      });

      kernel.updateObject(obj.id, {
        data: { ...obj.data, title: "New Title" },
      });

      const stored = kernel.store.getObject(obj.id);
      expect(stored?.data["title"]).toBe("New Title");
    });
  });

  describe("deleteObject", () => {
    it("should soft-delete an object", () => {
      const obj = kernel.createObject({
        type: "page",
        name: "To Delete",
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
      });

      const result = kernel.deleteObject(obj.id);
      expect(result).toBe(true);

      const stored = kernel.store.getObject(obj.id);
      expect(stored?.deletedAt).toBeTruthy();
    });
  });

  // ── Undo/Redo ───────────────────────────────────────────────────────────

  describe("undo/redo", () => {
    it("should undo a create (removes the object)", () => {
      const obj = kernel.createObject({
        type: "page",
        name: "Undo Create",
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
      });

      expect(kernel.store.getObject(obj.id)).toBeDefined();

      kernel.undo.undo();

      // Object should be removed from store
      expect(kernel.store.getObject(obj.id)).toBeUndefined();
    });

    it("should redo a create (restores the object)", () => {
      const obj = kernel.createObject({
        type: "page",
        name: "Redo Create",
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
      });

      kernel.undo.undo();
      expect(kernel.store.getObject(obj.id)).toBeUndefined();

      kernel.undo.redo();
      expect(kernel.store.getObject(obj.id)).toBeDefined();
      expect(kernel.store.getObject(obj.id)?.name).toBe("Redo Create");
    });

    it("should undo an update (restores previous state)", () => {
      const obj = kernel.createObject({
        type: "page",
        name: "Before Update",
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
      });

      kernel.updateObject(obj.id, { name: "After Update" });
      expect(kernel.store.getObject(obj.id)?.name).toBe("After Update");

      kernel.undo.undo();
      expect(kernel.store.getObject(obj.id)?.name).toBe("Before Update");
    });
  });

  // ── Selection ───────────────────────────────────────────────────────────

  describe("selection", () => {
    it("should update AtomStore on select", () => {
      const obj = kernel.createObject({
        type: "page",
        name: "Select Test",
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
      });

      kernel.select(obj.id);
      expect(kernel.atoms.getState().selectedId).toBe(obj.id);

      kernel.select(null);
      expect(kernel.atoms.getState().selectedId).toBeNull();
    });
  });

  // ── Edges ───────────────────────────────────────────────────────────────

  describe("edges", () => {
    it("should create and delete edges", () => {
      const page1 = kernel.createObject({
        type: "page",
        name: "Page 1",
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
      });
      const page2 = kernel.createObject({
        type: "page",
        name: "Page 2",
        parentId: null,
        position: 1,
        status: null,
        tags: [],
        date: null,
        endDate: null,
        description: "",
        color: null,
        image: null,
        pinned: false,
        data: {},
      });

      const edge = kernel.createEdge({
        sourceId: page1.id,
        targetId: page2.id,
        relation: "references",
        data: {},
      });

      expect(kernel.store.getEdge(edge.id)).toBeDefined();

      kernel.deleteEdge(edge.id);
      expect(kernel.store.getEdge(edge.id)).toBeUndefined();
    });
  });

  // ── Notifications ───────────────────────────────────────────────────────

  describe("notifications", () => {
    it("should add and retrieve notifications", () => {
      kernel.notifications.add({ title: "Test", kind: "info" });
      const all = kernel.notifications.getAll();
      expect(all).toHaveLength(1);
      expect(all[0]?.title).toBe("Test");
    });
  });

  // ── Search ──────────────────────────────────────────────────────────────

  describe("search", () => {
    it("should expose a search engine on the kernel", () => {
      expect(kernel.search).toBeDefined();
      expect(typeof kernel.search.search).toBe("function");
    });

    it("should find objects by name", () => {
      kernel.createObject({
        type: "page",
        name: "Searchable Page",
        parentId: null,
        position: 0,
        status: "draft",
        tags: [],
        date: null,
        endDate: null,
        description: "",
        color: null,
        image: null,
        pinned: false,
        data: { title: "Searchable Page", slug: "/searchable" },
      });

      const result = kernel.search.search({ query: "Searchable" });
      expect(result.hits.length).toBeGreaterThan(0);
      expect(result.hits[0]?.object.name).toBe("Searchable Page");
    });

    it("should filter by type", () => {
      kernel.createObject({
        type: "page",
        name: "Filterable Page",
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
        data: { title: "Filterable Page", slug: "/filterable" },
      });

      kernel.createObject({
        type: "heading",
        name: "Filterable Heading",
        parentId: null,
        position: 1,
        status: null,
        tags: [],
        date: null,
        endDate: null,
        description: "",
        color: null,
        image: null,
        pinned: false,
        data: { text: "Filterable Heading", level: "h1" },
      });

      const result = kernel.search.search({ query: "Filterable", types: ["page"] });
      expect(result.hits.length).toBeGreaterThan(0);
      expect(result.hits.every((h) => h.object.type === "page")).toBe(true);
    });

    it("should return empty results for non-matching query", () => {
      kernel.createObject({
        type: "page",
        name: "Something",
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
      });

      const result = kernel.search.search({ query: "zzzznonexistent" });
      expect(result.hits.length).toBe(0);
    });

    it("should auto-index objects created after engine initialization", () => {
      // The default collection is indexed at kernel creation time,
      // but new objects added via createObject go through CollectionStore
      // which triggers auto-reindexing via the onChange subscription.
      const obj = kernel.createObject({
        type: "page",
        name: "Late Addition",
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
      });

      const result = kernel.search.search({ query: "Late Addition" });
      expect(result.hits.length).toBe(1);
      expect(result.hits[0]?.objectId).toBe(obj.id);
    });
  });

  // ── Clipboard ───────────────────────────────────────────────────────────

  describe("clipboard", () => {
    it("should copy and paste an object subtree", () => {
      const page = kernel.createObject({
        type: "page", name: "Copy Source", parentId: null, position: 0,
        status: null, tags: [], date: null, endDate: null, description: "",
        color: null, image: null, pinned: false, data: { title: "Copy" },
      });
      const section = kernel.createObject({
        type: "section", name: "Section", parentId: page.id, position: 0,
        status: null, tags: [], date: null, endDate: null, description: "",
        color: null, image: null, pinned: false, data: {},
      });

      kernel.clipboardCopy([page.id]);
      expect(kernel.clipboardHasContent).toBe(true);

      const result = kernel.clipboardPaste(null, 5);
      expect(result).not.toBeNull();
      expect(result?.created.length).toBe(2); // page + section
      // New IDs should be different
      expect(result?.created[0]?.id).not.toBe(page.id);
      expect(result?.created[1]?.id).not.toBe(section.id);
      // Parent-child relationship preserved
      expect(result?.created[1]?.parentId).toBe(result?.created[0]?.id);
    });

    it("should cut (one-time paste + soft-delete originals)", () => {
      const page = kernel.createObject({
        type: "page", name: "Cut Source", parentId: null, position: 0,
        status: null, tags: [], date: null, endDate: null, description: "",
        color: null, image: null, pinned: false, data: {},
      });

      kernel.clipboardCut([page.id]);
      const result = kernel.clipboardPaste(null, 0);
      expect(result?.created.length).toBe(1);

      // Original should be soft-deleted
      const original = kernel.store.getObject(page.id);
      expect(original?.deletedAt).toBeTruthy();

      // Clipboard should be empty after cut-paste
      expect(kernel.clipboardHasContent).toBe(false);
    });

    it("should copy preserves internal edges", () => {
      const p1 = kernel.createObject({
        type: "page", name: "P1", parentId: null, position: 0,
        status: null, tags: [], date: null, endDate: null, description: "",
        color: null, image: null, pinned: false, data: {},
      });
      const p2 = kernel.createObject({
        type: "page", name: "P2", parentId: null, position: 1,
        status: null, tags: [], date: null, endDate: null, description: "",
        color: null, image: null, pinned: false, data: {},
      });
      kernel.createEdge({ sourceId: p1.id, targetId: p2.id, relation: "references", data: {} });

      kernel.clipboardCopy([p1.id, p2.id]);
      const result = kernel.clipboardPaste(null, 10);
      expect(result).not.toBeNull();
      // Should have created new edge between copied objects
      const newEdges = kernel.store.allEdges().filter(
        (e) => result?.idMap.has(e.sourceId as string) || e.sourceId !== p1.id,
      );
      expect(newEdges.length).toBeGreaterThan(0);
    });

    it("should return null when pasting with empty clipboard", () => {
      expect(kernel.clipboardPaste(null)).toBeNull();
    });
  });

  // ── Batch Operations ───────────────────────────────────────────────────

  describe("batch", () => {
    it("should execute multiple creates atomically", () => {
      const results = kernel.batch("Batch create", [
        { kind: "create", draft: {
          type: "page", name: "Batch A", parentId: null, position: 0,
          status: null, tags: [], date: null, endDate: null, description: "",
          color: null, image: null, pinned: false, data: {},
        }},
        { kind: "create", draft: {
          type: "page", name: "Batch B", parentId: null, position: 1,
          status: null, tags: [], date: null, endDate: null, description: "",
          color: null, image: null, pinned: false, data: {},
        }},
      ]);

      expect(results).toHaveLength(2);
      expect(results[0]?.name).toBe("Batch A");
      expect(results[1]?.name).toBe("Batch B");
    });

    it("should produce a single undo entry for the batch", () => {
      kernel.batch("Batch for undo", [
        { kind: "create", draft: {
          type: "page", name: "U1", parentId: null, position: 0,
          status: null, tags: [], date: null, endDate: null, description: "",
          color: null, image: null, pinned: false, data: {},
        }},
        { kind: "create", draft: {
          type: "page", name: "U2", parentId: null, position: 1,
          status: null, tags: [], date: null, endDate: null, description: "",
          color: null, image: null, pinned: false, data: {},
        }},
      ]);

      // One undo should remove both
      kernel.undo.undo();
      const remaining = kernel.store.allObjects().filter((o) => !o.deletedAt);
      expect(remaining.every((o) => o.name !== "U1" && o.name !== "U2")).toBe(true);
    });

    it("should support mixed create/update/delete operations", () => {
      const page = kernel.createObject({
        type: "page", name: "Mixed", parentId: null, position: 0,
        status: null, tags: [], date: null, endDate: null, description: "",
        color: null, image: null, pinned: false, data: {},
      });
      kernel.undo.clear();

      const results = kernel.batch("Mixed batch", [
        { kind: "update", id: page.id, patch: { name: "Updated" } },
        { kind: "create", draft: {
          type: "heading", name: "New Heading", parentId: page.id, position: 0,
          status: null, tags: [], date: null, endDate: null, description: "",
          color: null, image: null, pinned: false, data: { text: "Hi", level: "h1" },
        }},
      ]);

      expect(results).toHaveLength(2);
      expect(kernel.store.getObject(page.id)?.name).toBe("Updated");
    });
  });

  // ── Activity ───────────────────────────────────────────────────────────

  describe("activity", () => {
    it("should record create events", () => {
      const obj = kernel.createObject({
        type: "page", name: "Activity Test", parentId: null, position: 0,
        status: null, tags: [], date: null, endDate: null, description: "",
        color: null, image: null, pinned: false, data: {},
      });

      const events = kernel.activity.getEvents(obj.id);
      expect(events.length).toBeGreaterThan(0);
      expect(events[0]?.verb).toBe("created");
    });

    it("should record delete events", () => {
      const obj = kernel.createObject({
        type: "page", name: "Delete Log", parentId: null, position: 0,
        status: null, tags: [], date: null, endDate: null, description: "",
        color: null, image: null, pinned: false, data: {},
      });

      kernel.deleteObject(obj.id);
      const events = kernel.activity.getEvents(obj.id);
      const deleteEvent = events.find((e) => e.verb === "deleted");
      expect(deleteEvent).toBeDefined();
    });

    it("should expose activity tracker", () => {
      expect(kernel.activityTracker).toBeDefined();
      expect(typeof kernel.activityTracker.track).toBe("function");
    });
  });

  // ── Templates ──────────────────────────────────────────────────────────

  describe("templates", () => {
    it("should register and list templates", () => {
      kernel.registerTemplate({
        id: "tpl-hero",
        name: "Hero Section",
        category: "sections",
        root: {
          placeholderId: "root",
          type: "section",
          name: "Hero",
          data: { variant: "hero", padding: "lg" },
          children: [
            { placeholderId: "heading", type: "heading", name: "{{title}}", data: { text: "{{title}}", level: "h1" } },
            { placeholderId: "text", type: "text-block", name: "Subtitle", data: { content: "{{subtitle}}" } },
          ],
        },
        variables: [
          { name: "title", label: "Title", required: true },
          { name: "subtitle", label: "Subtitle", defaultValue: "Welcome" },
        ],
        createdAt: new Date().toISOString(),
      });

      expect(kernel.listTemplates()).toHaveLength(1);
      expect(kernel.listTemplates("sections")).toHaveLength(1);
      expect(kernel.listTemplates("pages")).toHaveLength(0);
    });

    it("should instantiate a template with variable interpolation", () => {
      kernel.registerTemplate({
        id: "tpl-page",
        name: "Landing Page",
        category: "pages",
        root: {
          placeholderId: "page",
          type: "page",
          name: "{{pageName}}",
          data: { title: "{{pageName}}", slug: "/{{slug}}" },
          children: [
            {
              placeholderId: "hero",
              type: "section",
              name: "Hero",
              data: { variant: "hero" },
              children: [
                { placeholderId: "h1", type: "heading", name: "Heading", data: { text: "{{headline}}", level: "h1" } },
              ],
            },
          ],
        },
        createdAt: new Date().toISOString(),
      });

      const result = kernel.instantiateTemplate("tpl-page", {
        parentId: null,
        variables: { pageName: "Products", slug: "products", headline: "Our Products" },
      });

      expect(result).not.toBeNull();
      expect(result?.created).toHaveLength(3); // page + section + heading
      expect(result?.created[0]?.name).toBe("Products");
      expect(result?.created[0]?.data["title"]).toBe("Products");
      expect(result?.created[0]?.data["slug"]).toBe("/products");

      // Heading should have interpolated text
      const heading = result?.created[2];
      expect(heading?.data["text"]).toBe("Our Products");

      // Parent-child chain preserved
      expect(result?.created[1]?.parentId).toBe(result?.created[0]?.id);
      expect(result?.created[2]?.parentId).toBe(result?.created[1]?.id);
    });

    it("should return null for unknown template", () => {
      expect(kernel.instantiateTemplate("nonexistent")).toBeNull();
    });

    it("should create template edges with remapped IDs", () => {
      kernel.registerTemplate({
        id: "tpl-with-edges",
        name: "Linked Pages",
        root: {
          placeholderId: "p1",
          type: "page",
          name: "Page 1",
          children: [
            { placeholderId: "p2", type: "section", name: "Section" },
          ],
        },
        edges: [
          { sourcePlaceholderId: "p1", targetPlaceholderId: "p2", relation: "references" },
        ],
        createdAt: new Date().toISOString(),
      });

      const result = kernel.instantiateTemplate("tpl-with-edges");
      expect(result?.createdEdges).toHaveLength(1);
      const edge = result?.createdEdges[0];
      expect(edge?.sourceId).toBe(result?.created[0]?.id);
      expect(edge?.targetId).toBe(result?.created[1]?.id);
    });

    it("should be undoable as a single entry", () => {
      kernel.registerTemplate({
        id: "tpl-undo",
        name: "Undo Test",
        root: {
          placeholderId: "root",
          type: "page",
          name: "Undo Page",
          children: [
            { placeholderId: "s1", type: "section", name: "S1" },
            { placeholderId: "s2", type: "section", name: "S2" },
          ],
        },
        createdAt: new Date().toISOString(),
      });

      const before = kernel.store.objectCount();
      kernel.instantiateTemplate("tpl-undo");
      expect(kernel.store.objectCount()).toBe(before + 3);

      kernel.undo.undo();
      // All 3 objects should be removed
      expect(kernel.store.objectCount()).toBe(before);
    });
  });

  // ── LiveView ───────────────────────────────────────────────────────────

  describe("liveView", () => {
    it("should create a live view from the collection", () => {
      kernel.createObject({
        type: "page", name: "LV Page", parentId: null, position: 0,
        status: null, tags: [], date: null, endDate: null, description: "",
        color: null, image: null, pinned: false, data: {},
      });

      const view = kernel.createLiveView();
      expect(view.snapshot.objects.length).toBeGreaterThan(0);
      expect(view.snapshot.total).toBeGreaterThan(0);
    });

    it("should update when collection changes", () => {
      const view = kernel.createLiveView();
      const initialCount = view.snapshot.total;

      kernel.createObject({
        type: "page", name: "New LV", parentId: null, position: 0,
        status: null, tags: [], date: null, endDate: null, description: "",
        color: null, image: null, pinned: false, data: {},
      });

      // LiveView should have updated
      expect(view.snapshot.total).toBe(initialCount + 1);
    });

    it("should support filtering", () => {
      kernel.createObject({
        type: "page", name: "Filter Page", parentId: null, position: 0,
        status: "draft", tags: [], date: null, endDate: null, description: "",
        color: null, image: null, pinned: false, data: {},
      });
      kernel.createObject({
        type: "heading", name: "Filter Heading", parentId: null, position: 1,
        status: null, tags: [], date: null, endDate: null, description: "",
        color: null, image: null, pinned: false, data: { text: "Hi", level: "h1" },
      });

      const view = kernel.createLiveView();
      view.setFilters([{ field: "type", op: "eq", value: "page" }]);

      expect(view.snapshot.objects.every((o) => o.type === "page")).toBe(true);
    });

    it("should provide type facets", () => {
      kernel.createObject({
        type: "page", name: "Facet P", parentId: null, position: 0,
        status: null, tags: [], date: null, endDate: null, description: "",
        color: null, image: null, pinned: false, data: {},
      });
      kernel.createObject({
        type: "heading", name: "Facet H", parentId: null, position: 1,
        status: null, tags: [], date: null, endDate: null, description: "",
        color: null, image: null, pinned: false, data: { text: "X", level: "h1" },
      });

      const view = kernel.createLiveView();
      expect(view.snapshot.typeFacets["page"]).toBeGreaterThan(0);
      expect(view.snapshot.typeFacets["heading"]).toBeGreaterThan(0);
    });

    it("should dispose cleanly", () => {
      const view = kernel.createLiveView();
      expect(() => view.dispose()).not.toThrow();
    });
  });

  // ── Identity ────────────────────────────────────────────────────────────

  describe("identity", () => {
    it("should start with no identity", () => {
      expect(kernel.identity).toBeNull();
    });

    it("should generate a DID identity", async () => {
      const identity = await kernel.generateIdentity();
      expect(identity.did).toMatch(/^did:key:z/);
      expect(kernel.identity).toBe(identity);
    });

    it("should sign and verify data", async () => {
      await kernel.generateIdentity();
      const data = new TextEncoder().encode("test payload");
      const sig = await kernel.signData(data);
      expect(sig).not.toBeNull();
      const sigBytes = sig as Uint8Array;

      const valid = await kernel.verifyData(data, sigBytes);
      expect(valid).toBe(true);

      const tampered = new TextEncoder().encode("tampered");
      const invalid = await kernel.verifyData(tampered, sigBytes);
      expect(invalid).toBe(false);
    });

    it("should export and import identity", async () => {
      const original = await kernel.generateIdentity();
      const exported = await kernel.exportIdentity();
      expect(exported).not.toBeNull();
      const exp = exported as NonNullable<typeof exported>;
      expect(exp.did).toBe(original.did);

      // Import into a new kernel
      const kernel2 = createStudioKernel();
      const imported = await kernel2.importIdentity(exp);
      expect(imported.did).toBe(original.did);
      kernel2.dispose();
    });

    it("should notify on identity change", async () => {
      let called = 0;
      const unsub = kernel.onIdentityChange(() => { called++; });
      await kernel.generateIdentity();
      expect(called).toBe(1);
      unsub();
    });

    it("should return null when signing without identity", async () => {
      const result = await kernel.signData(new Uint8Array([1, 2, 3]));
      expect(result).toBeNull();
    });
  });

  // ── Virtual File System ────────────────────────────────────────────────

  describe("vfs", () => {
    it("should import a file and return BinaryRef", async () => {
      const data = new TextEncoder().encode("hello vfs");
      const ref = await kernel.importFile(data, "test.txt", "text/plain");
      expect(ref.filename).toBe("test.txt");
      expect(ref.mimeType).toBe("text/plain");
      expect(ref.size).toBe(9);
      expect(ref.hash).toBeTruthy();
    });

    it("should export a file by ref", async () => {
      const data = new TextEncoder().encode("round trip");
      const ref = await kernel.importFile(data, "round.txt", "text/plain");
      const exported = await kernel.exportFile(ref);
      expect(exported).not.toBeNull();
      expect(new TextDecoder().decode(exported as Uint8Array)).toBe("round trip");
    });

    it("should remove a file", async () => {
      const data = new TextEncoder().encode("delete me");
      const ref = await kernel.importFile(data, "del.txt", "text/plain");
      const removed = await kernel.removeFile(ref.hash);
      expect(removed).toBe(true);

      const again = await kernel.exportFile(ref);
      expect(again).toBeNull();
    });

    it("should deduplicate identical content", async () => {
      const data = new TextEncoder().encode("same content");
      const ref1 = await kernel.importFile(data, "a.txt", "text/plain");
      const ref2 = await kernel.importFile(data, "b.txt", "text/plain");
      expect(ref1.hash).toBe(ref2.hash);
    });

    it("should acquire and release locks", async () => {
      const data = new TextEncoder().encode("lockable");
      const ref = await kernel.importFile(data, "lock.bin", "application/octet-stream");

      const lock = kernel.acquireLock(ref.hash, "editing");
      expect(lock.hash).toBe(ref.hash);
      expect(lock.reason).toBe("editing");

      expect(kernel.listLocks()).toHaveLength(1);

      kernel.releaseLock(ref.hash);
      expect(kernel.listLocks()).toHaveLength(0);
    });

    it("should notify on VFS changes", async () => {
      let called = 0;
      const unsub = kernel.onVfsChange(() => { called++; });
      await kernel.importFile(new TextEncoder().encode("a"), "a.txt", "text/plain");
      expect(called).toBe(1);
      unsub();
    });

    it("should list imported files and update on remove", async () => {
      expect(kernel.listFiles()).toHaveLength(0);
      const ref1 = await kernel.importFile(new TextEncoder().encode("one"), "one.txt", "text/plain");
      await kernel.importFile(new TextEncoder().encode("two"), "two.txt", "text/plain");
      expect(kernel.listFiles()).toHaveLength(2);
      expect(kernel.listFiles().map((r) => r.filename).sort()).toEqual(["one.txt", "two.txt"]);
      await kernel.removeFile(ref1.hash);
      expect(kernel.listFiles()).toHaveLength(1);
      expect(kernel.listFiles()[0]?.filename).toBe("two.txt");
    });
  });

  // ── Trust & Safety ─────────────────────────────────────────────────────

  describe("trust", () => {
    it("should start with no peers", () => {
      expect(kernel.listPeers()).toHaveLength(0);
    });

    it("should add and trust a peer", () => {
      kernel.trustPeer("peer-a");
      const peers = kernel.listPeers();
      expect(peers).toHaveLength(1);
      const first = peers[0];
      expect(first).toBeDefined();
      expect(first?.peerId).toBe("peer-a");
      expect(first?.positiveInteractions).toBeGreaterThan(0);
    });

    it("should distrust a peer", () => {
      kernel.trustPeer("peer-b");
      kernel.distrustPeer("peer-b");
      const peer = kernel.listPeers().find((p) => p.peerId === "peer-b");
      expect(peer).toBeDefined();
      expect(peer?.negativeInteractions).toBeGreaterThan(0);
    });

    it("should ban and unban a peer", () => {
      kernel.trustPeer("peer-c");
      kernel.banPeer("peer-c", "bad actor");
      const peer = kernel.listPeers().find((p) => p.peerId === "peer-c");
      expect(peer).toBeDefined();
      expect(peer?.banned).toBe(true);
      expect(peer?.banReason).toBe("bad actor");

      kernel.unbanPeer("peer-c");
      const unbanned = kernel.listPeers().find((p) => p.peerId === "peer-c");
      expect(unbanned).toBeDefined();
      expect(unbanned?.banned).toBe(false);
    });

    it("should validate import data", () => {
      const result = kernel.validateImport({ name: "safe", value: 42 });
      expect(result.valid).toBe(true);
    });

    it("should flag content", () => {
      kernel.flagContent("hash123", "spam");
      const flagged = kernel.listFlaggedContent();
      expect(flagged).toHaveLength(1);
      expect(flagged[0]?.category).toBe("spam");
    });

    it("should create a sandbox", () => {
      const sandbox = kernel.createSandbox({
        pluginId: "test-plugin",
        capabilities: ["crdt:read", "ui:notify"],
        maxDurationMs: 5000,
        maxMemoryBytes: 0,
        allowedUrls: [],
        allowedPaths: [],
      });
      expect(sandbox.hasCapability("crdt:read")).toBe(true);
      expect(sandbox.hasCapability("crdt:write")).toBe(false);
    });

    it("should split and combine Shamir shares", () => {
      const secret = new TextEncoder().encode("my-secret-key");
      const config = { totalShares: 5, threshold: 3 };
      const shares = kernel.splitSecret(secret, config);
      expect(shares).toHaveLength(5);

      // Reconstruct with 3 shares
      const recovered = kernel.combineShares(shares.slice(0, 3), config);
      expect(new TextDecoder().decode(recovered)).toBe("my-secret-key");
    });

    it("should deposit and list escrow", async () => {
      await kernel.generateIdentity();
      const deposit = kernel.depositEscrow("encrypted-payload-123");
      expect(deposit).not.toBeNull();
      expect(deposit?.encryptedPayload).toBe("encrypted-payload-123");

      const deposits = kernel.listEscrowDeposits();
      expect(deposits).toHaveLength(1);
    });

    it("should return null escrow without identity", () => {
      const result = kernel.depositEscrow("no-id");
      expect(result).toBeNull();
    });

    it("should notify on trust changes", () => {
      let called = 0;
      const unsub = kernel.onTrustChange(() => { called++; });
      kernel.trustPeer("notify-peer");
      expect(called).toBeGreaterThan(0);
      unsub();
    });
  });

  // ── Facet System ──────────────────────────────────────────────────────────

  describe("facet parser", () => {
    it("should detect YAML format", () => {
      expect(kernel.detectFormat("key: value")).toBe("yaml");
    });

    it("should detect JSON format", () => {
      expect(kernel.detectFormat('{"key": "value"}')).toBe("json");
    });

    it("should parse YAML values", () => {
      const values = kernel.parseValues("name: Alice\nage: 30", "yaml");
      expect(values.name).toBe("Alice");
      expect(values.age).toBe(30);
    });

    it("should parse JSON values", () => {
      const values = kernel.parseValues('{"name": "Bob", "active": true}', "json");
      expect(values.name).toBe("Bob");
      expect(values.active).toBe(true);
    });

    it("should serialize values back to YAML", () => {
      const original = "name: Alice\nage: 30";
      const yaml = kernel.serializeValues({ name: "Bob", age: 25 }, "yaml", original);
      expect(yaml).toContain("name:");
      expect(yaml).toContain("Bob");
      expect(yaml).toContain("25");
    });

    it("should serialize values back to JSON", () => {
      const json = kernel.serializeValues({ name: "Bob" }, "json", "{}");
      expect(JSON.parse(json)).toEqual({ name: "Bob" });
    });

    it("should infer field schemas from values", () => {
      const fields = kernel.inferFields({
        name: "Alice",
        age: 30,
        active: true,
        email: "alice@example.com",
      });
      expect(fields.length).toBe(4);
      const nameField = fields.find((f) => f.id === "name");
      expect(nameField?.type).toBe("text");
      const ageField = fields.find((f) => f.id === "age");
      expect(ageField?.type).toBe("number");
      const activeField = fields.find((f) => f.id === "active");
      expect(activeField?.type).toBe("boolean");
      const emailField = fields.find((f) => f.id === "email");
      expect(emailField?.type).toBe("email");
    });
  });

  describe("spell checker", () => {
    it("should have a spell checker instance", () => {
      expect(kernel.spellChecker).toBeDefined();
    });

    it("should return suggestions for misspelled words", async () => {
      await kernel.spellChecker.loadDictionary();
      const suggestions = kernel.spellSuggest("teh");
      expect(suggestions).toContain("the");
    });
  });

  describe("prose codec", () => {
    it("should convert markdown to nodes", () => {
      const node = kernel.markdownToNodes("# Hello\n\nWorld");
      expect(node.type).toBe("doc");
      expect(node.content).toBeDefined();
      expect(node.content?.length).toBeGreaterThan(0);
    });

    it("should round-trip markdown", () => {
      const md = "# Title\n\nParagraph text.";
      const node = kernel.markdownToNodes(md);
      const result = kernel.nodesToMarkdown(node);
      expect(result).toContain("Title");
      expect(result).toContain("Paragraph text.");
    });
  });

  describe("sequencer", () => {
    it("should emit condition Luau", () => {
      const luau = kernel.emitConditionLuau({
        combinator: "all",
        clauses: [
          { id: "c1", subjectKind: "variable", subject: "score", operator: "gt", value: "100" },
        ],
      });
      expect(luau).toContain("100");
    });

    it("should emit script Luau", () => {
      const luau = kernel.emitScriptLuau({
        steps: [
          { id: "s1", actionKind: "set-variable", target: "scope.health", value: "100" },
        ],
      });
      expect(luau).toContain("health");
    });

    it("should emit empty condition as true", () => {
      const luau = kernel.emitConditionLuau({ combinator: "all", clauses: [] });
      expect(luau).toBe("true");
    });
  });

  describe("emitters", () => {
    it("should emit TypeScript code", () => {
      const code = kernel.emitCode({
        namespace: "test",
        declarations: [
          { kind: "interface", name: "Person", fields: [{ name: "name", type: "string" }] },
        ],
      }, "typescript");
      expect(code).toContain("interface Person");
      expect(code).toContain("name");
    });

    it("should emit JSON code", () => {
      const code = kernel.emitCode({
        declarations: [
          { kind: "interface", name: "Item", fields: [{ name: "id", type: "number" }] },
        ],
      }, "json");
      expect(code).toContain("Item");
    });

    it("should emit Luau code", () => {
      const code = kernel.emitCode({
        declarations: [
          { kind: "interface", name: "Config", fields: [{ name: "enabled", type: "boolean" }] },
        ],
      }, "luau");
      expect(code).toContain("Config");
    });
  });

  describe("facet definitions", () => {
    it("should register and list facet definitions", () => {
      const def = kernel.buildFacetDefinition("test-form", "page", "form")
        .name("Test Form")
        .build();
      kernel.registerFacetDefinition(def);
      expect(kernel.listFacetDefinitions()).toHaveLength(1);
      expect(kernel.getFacetDefinition("test-form")).toBeDefined();
    });

    it("should remove facet definitions", () => {
      const def = kernel.buildFacetDefinition("rem-form", "page", "form").build();
      kernel.registerFacetDefinition(def);
      expect(kernel.removeFacetDefinition("rem-form")).toBe(true);
      expect(kernel.listFacetDefinitions()).toHaveLength(0);
    });

    it("should notify on facet changes", () => {
      let called = 0;
      const unsub = kernel.onFacetChange(() => { called++; });
      const def = kernel.buildFacetDefinition("notify-form", "page", "form").build();
      kernel.registerFacetDefinition(def);
      expect(called).toBe(1);
      kernel.removeFacetDefinition("notify-form");
      expect(called).toBe(2);
      unsub();
    });

    it("should build definitions with parts and fields", () => {
      const def = kernel.buildFacetDefinition("detailed", "contact", "form")
        .name("Contact Form")
        .description("A test form")
        .addPart({ kind: "header" })
        .addPart({ kind: "body" })
        .addField({ fieldPath: "name", part: "header", order: 0 })
        .addField({ fieldPath: "email", part: "body", order: 0 })
        .build();
      expect(def.name).toBe("Contact Form");
      expect(def.parts).toHaveLength(2);
      expect(def.slots).toHaveLength(2);
    });
  });

  // ── Page Builder Integration ──────────────────────────────────────────────

  describe("page builder integration", () => {
    it("should list allowed child types for page", () => {
      const allowed = kernel.registry.getAllowedChildTypes("page");
      expect(allowed).toContain("section");
      expect(allowed).toContain("heading");
      expect(allowed).toContain("text-block");
      expect(allowed).toContain("image");
      expect(allowed).toContain("button");
      expect(allowed).toContain("card");
      expect(allowed).toContain("luau-block");
    });

    it("should list allowed child types for section", () => {
      const allowed = kernel.registry.getAllowedChildTypes("section");
      expect(allowed).toContain("heading");
      expect(allowed).toContain("text-block");
      expect(allowed).toContain("luau-block");
      expect(allowed).not.toContain("page");
      expect(allowed).not.toContain("section");
    });

    it("should build page→section→component hierarchy", () => {
      const page = kernel.createObject({
        type: "page",
        name: "Test Page",
        parentId: null,
        position: 0,
        status: "draft",
        tags: [],
        date: null,
        endDate: null,
        description: "",
        color: null,
        image: null,
        pinned: false,
        data: { title: "Test Page", slug: "", layout: "single", published: false },
      });

      const section = kernel.createObject({
        type: "section",
        name: "Hero Section",
        parentId: page.id,
        position: 0,
        status: "draft",
        tags: [],
        date: null,
        endDate: null,
        description: "",
        color: null,
        image: null,
        pinned: false,
        data: { variant: "hero", padding: "lg" },
      });

      kernel.createObject({
        type: "heading",
        name: "Welcome",
        parentId: section.id,
        position: 0,
        status: "draft",
        tags: [],
        date: null,
        endDate: null,
        description: "",
        color: null,
        image: null,
        pinned: false,
        data: { text: "Welcome", level: "h1", align: "center" },
      });

      kernel.createObject({
        type: "text-block",
        name: "Intro",
        parentId: section.id,
        position: 1,
        status: "draft",
        tags: [],
        date: null,
        endDate: null,
        description: "",
        color: null,
        image: null,
        pinned: false,
        data: { content: "Hello world", format: "markdown" },
      });

      // Verify hierarchy
      const pageChildren = kernel.store.listObjects({ parentId: page.id });
      expect(pageChildren).toHaveLength(1);
      expect(pageChildren[0]?.type).toBe("section");

      const sectionChildren = kernel.store.listObjects({ parentId: section.id });
      expect(sectionChildren).toHaveLength(2);
      expect(sectionChildren.map((c) => c.type).sort()).toEqual(["heading", "text-block"]);
    });

    it("should reorder children by updating position", () => {
      const page = kernel.createObject({
        type: "page",
        name: "Reorder Page",
        parentId: null,
        position: 0,
        status: "draft",
        tags: [],
        date: null,
        endDate: null,
        description: "",
        color: null,
        image: null,
        pinned: false,
        data: {},
      });

      const s1 = kernel.createObject({
        type: "section",
        name: "Section A",
        parentId: page.id,
        position: 0,
        status: "draft",
        tags: [],
        date: null,
        endDate: null,
        description: "",
        color: null,
        image: null,
        pinned: false,
        data: {},
      });

      const s2 = kernel.createObject({
        type: "section",
        name: "Section B",
        parentId: page.id,
        position: 1,
        status: "draft",
        tags: [],
        date: null,
        endDate: null,
        description: "",
        color: null,
        image: null,
        pinned: false,
        data: {},
      });

      // Swap positions
      kernel.updateObject(s1.id, { position: 1 });
      kernel.updateObject(s2.id, { position: 0 });

      const children = kernel.store
        .listObjects({ parentId: page.id })
        .sort((a, b) => a.position - b.position);

      expect(children[0]?.name).toBe("Section B");
      expect(children[1]?.name).toBe("Section A");
    });

    it("should reparent objects via updateObject", () => {
      const page = kernel.createObject({
        type: "page",
        name: "Move Page",
        parentId: null,
        position: 0,
        status: "draft",
        tags: [],
        date: null,
        endDate: null,
        description: "",
        color: null,
        image: null,
        pinned: false,
        data: {},
      });

      const s1 = kernel.createObject({
        type: "section",
        name: "Source Section",
        parentId: page.id,
        position: 0,
        status: "draft",
        tags: [],
        date: null,
        endDate: null,
        description: "",
        color: null,
        image: null,
        pinned: false,
        data: {},
      });

      const s2 = kernel.createObject({
        type: "section",
        name: "Target Section",
        parentId: page.id,
        position: 1,
        status: "draft",
        tags: [],
        date: null,
        endDate: null,
        description: "",
        color: null,
        image: null,
        pinned: false,
        data: {},
      });

      const heading = kernel.createObject({
        type: "heading",
        name: "Move Me",
        parentId: s1.id,
        position: 0,
        status: "draft",
        tags: [],
        date: null,
        endDate: null,
        description: "",
        color: null,
        image: null,
        pinned: false,
        data: { text: "Move Me", level: "h2" },
      });

      // Move heading from s1 to s2
      kernel.updateObject(heading.id, { parentId: s2.id, position: 0 });

      const s1Children = kernel.store.listObjects({ parentId: s1.id });
      const s2Children = kernel.store.listObjects({ parentId: s2.id });

      expect(s1Children).toHaveLength(0);
      expect(s2Children).toHaveLength(1);
      expect(s2Children[0]?.name).toBe("Move Me");
    });

    it("should duplicate objects", () => {
      const page = kernel.createObject({
        type: "page",
        name: "Dup Page",
        parentId: null,
        position: 0,
        status: "draft",
        tags: ["test"],
        date: null,
        endDate: null,
        description: "",
        color: null,
        image: null,
        pinned: false,
        data: { title: "Dup Page", slug: "", layout: "single", published: false },
      });

      // Duplicate via createObject with same data
      const dup = kernel.createObject({
        type: page.type,
        name: `${page.name} (copy)`,
        parentId: page.parentId,
        position: 1,
        status: page.status,
        tags: [...page.tags],
        date: page.date,
        endDate: page.endDate,
        description: page.description,
        color: page.color,
        image: page.image,
        pinned: page.pinned,
        data: { ...(page.data as Record<string, unknown>) },
      });

      expect(dup.name).toBe("Dup Page (copy)");
      expect(dup.id).not.toBe(page.id);
      expect(dup.type).toBe("page");
    });

    it("should generate Puck-compatible component list from registry", () => {
      const allDefs = kernel.registry.allDefs();
      const componentDefs = allDefs.filter(
        (d) => d.category === "component" || d.category === "section",
      );

      // Should have section + all component types including luau-block
      expect(componentDefs.length).toBeGreaterThanOrEqual(7);
      const types = componentDefs.map((d) => d.type);
      expect(types).toContain("section");
      expect(types).toContain("heading");
      expect(types).toContain("text-block");
      expect(types).toContain("image");
      expect(types).toContain("button");
      expect(types).toContain("card");
      expect(types).toContain("luau-block");
    });

    it("should register new form-input / layout / data / content widgets", () => {
      const types = kernel.registry
        .allDefs()
        .filter((d) => d.category === "component")
        .map((d) => d.type);

      // Form inputs
      expect(types).toContain("text-input");
      expect(types).toContain("textarea-input");
      expect(types).toContain("select-input");
      expect(types).toContain("checkbox-input");
      expect(types).toContain("number-input");
      expect(types).toContain("date-input");

      // Layout primitives
      expect(types).toContain("columns");
      expect(types).toContain("divider");
      expect(types).toContain("spacer");

      // Data display
      expect(types).toContain("stat-widget");
      expect(types).toContain("badge");
      expect(types).toContain("alert");
      expect(types).toContain("progress-bar");

      // Content
      expect(types).toContain("markdown-widget");
      expect(types).toContain("iframe-widget");
      expect(types).toContain("code-block");
      expect(types).toContain("video-widget");
      expect(types).toContain("audio-widget");
    });

    it("should create and update luau-block with Luau source", () => {
      const page = kernel.createObject({
        type: "page",
        name: "Luau Test Page",
        parentId: null,
        position: 0,
        status: "draft",
        tags: [],
        date: null,
        endDate: null,
        description: "",
        color: null,
        image: null,
        pinned: false,
        data: { title: "Luau Test", slug: "", layout: "single", published: false },
      });

      const section = kernel.createObject({
        type: "section",
        name: "Luau Section",
        parentId: page.id,
        position: 0,
        status: "draft",
        tags: [],
        date: null,
        endDate: null,
        description: "",
        color: null,
        image: null,
        pinned: false,
        data: {},
      });

      const luauBlock = kernel.createObject({
        type: "luau-block",
        name: "Status Widget",
        parentId: section.id,
        position: 0,
        status: "draft",
        tags: [],
        date: null,
        endDate: null,
        description: "",
        color: null,
        image: null,
        pinned: false,
        data: {
          title: "Status",
          source: 'return ui.label("Hello")',
        },
      });

      expect(luauBlock.type).toBe("luau-block");
      const data = luauBlock.data as Record<string, unknown>;
      expect(data["source"]).toBe('return ui.label("Hello")');
      expect(data["title"]).toBe("Status");

      // Update the Luau source
      const updated = kernel.updateObject(luauBlock.id, {
        data: { ...data, source: 'return ui.button("Click")' },
      });
      expect(updated).toBeDefined();
      const updatedData = updated?.data as Record<string, unknown>;
      expect(updatedData["source"]).toBe('return ui.button("Click")');
    });

    it("should delete objects and verify cleanup", () => {
      const page = kernel.createObject({
        type: "page",
        name: "Delete Test",
        parentId: null,
        position: 0,
        status: "draft",
        tags: [],
        date: null,
        endDate: null,
        description: "",
        color: null,
        image: null,
        pinned: false,
        data: {},
      });

      const section = kernel.createObject({
        type: "section",
        name: "Delete Section",
        parentId: page.id,
        position: 0,
        status: "draft",
        tags: [],
        date: null,
        endDate: null,
        description: "",
        color: null,
        image: null,
        pinned: false,
        data: {},
      });

      kernel.deleteObject(section.id);
      const liveChildren = kernel.store
        .listObjects({ parentId: page.id })
        .filter((o) => !o.deletedAt);
      expect(liveChildren).toHaveLength(0);
    });
  });

  // ── Saved Views ────────────────────────────────────────────────────────

  describe("savedViews", () => {
    it("exposes a SavedViewRegistry", () => {
      expect(kernel.savedViews).toBeDefined();
      expect(typeof kernel.savedViews.add).toBe("function");
      expect(typeof kernel.savedViews.all).toBe("function");
    });

    it("can add and list saved views", () => {
      const view = createSavedView("test-view", "task", {}, "Test View");
      kernel.savedViews.add(view);
      expect(kernel.savedViews.all()).toHaveLength(1);
      expect(kernel.savedViews.all()[0]?.name).toBe("Test View");
    });

    it("notifies listeners on change", () => {
      const listener = vi.fn();
      kernel.onSavedViewChange(listener);
      kernel.savedViews.add(createSavedView("sv2", "task", {}, "V2"));
      expect(listener).toHaveBeenCalled();
    });
  });

  // ── Value Lists ────────────────────────────────────────────────────────

  describe("valueLists", () => {
    it("exposes a ValueListRegistry", () => {
      expect(kernel.valueLists).toBeDefined();
      expect(typeof kernel.valueLists.register).toBe("function");
      expect(typeof kernel.valueLists.all).toBe("function");
    });

    it("can register and list value lists", () => {
      const vl = createStaticValueList("test-vl", "Colors", [
        { value: "red", label: "Red" },
        { value: "blue", label: "Blue" },
      ]);
      kernel.valueLists.register(vl);
      expect(kernel.valueLists.all().length).toBeGreaterThanOrEqual(1);
    });

    it("notifies listeners on change", () => {
      const listener = vi.fn();
      kernel.onValueListChange(listener);
      kernel.valueLists.register(createStaticValueList("vl2", "Sizes", []));
      expect(listener).toHaveBeenCalled();
    });
  });

  // ── Privilege Sets ─────────────────────────────────────────────────────

  describe("privilegeSets", () => {
    it("starts with no privilege sets", () => {
      expect(kernel.listPrivilegeSets()).toHaveLength(0);
    });

    it("can save and list privilege sets", () => {

      const ps = createPrivilegeSet("admin", "Admin", { collections: { "*": "full" } });
      kernel.savePrivilegeSet(ps);
      expect(kernel.listPrivilegeSets()).toHaveLength(1);
      expect(kernel.listPrivilegeSets()[0]?.name).toBe("Admin");
    });

    it("can remove a privilege set", () => {

      kernel.savePrivilegeSet(createPrivilegeSet("rm-test", "Remove Me", { collections: {} }));
      expect(kernel.removePrivilegeSet("rm-test")).toBe(true);
      expect(kernel.listPrivilegeSets().find((p) => p.id === "rm-test")).toBeUndefined();
    });

    it("can get an enforcer for a privilege set", () => {

      kernel.savePrivilegeSet(createPrivilegeSet("enf-test", "Enforced", { collections: { "*": "read" } }));
      const enforcer = kernel.getEnforcer("enf-test");
      expect(enforcer).toBeDefined();
    });

    it("returns undefined for unknown privilege set", () => {
      expect(kernel.getEnforcer("nonexistent")).toBeUndefined();
    });

    it("can assign and list roles", () => {
      kernel.assignRole("did:key:abc", "admin");
      const roles = kernel.listRoleAssignments();
      expect(roles.find((r) => r.did === "did:key:abc")).toBeDefined();
    });

    it("can remove a role assignment", () => {
      kernel.assignRole("did:key:remove-me", "admin");
      kernel.removeRoleAssignment("did:key:remove-me");
      expect(kernel.listRoleAssignments().find((r) => r.did === "did:key:remove-me")).toBeUndefined();
    });

    it("notifies listeners on change", () => {

      const listener = vi.fn();
      kernel.onPrivilegeSetChange(listener);
      kernel.savePrivilegeSet(createPrivilegeSet("notify-test", "N", { collections: {} }));
      expect(listener).toHaveBeenCalled();
    });
  });

  // ── Dispose ─────────────────────────────────────────────────────────────

  describe("dispose", () => {
    it("should not throw on dispose", () => {
      expect(() => kernel.dispose()).not.toThrow();
    });
  });
});
