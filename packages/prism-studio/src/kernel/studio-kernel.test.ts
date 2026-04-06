import { describe, it, expect, beforeEach } from "vitest";
import { createStudioKernel } from "./studio-kernel.js";
import type { StudioKernel } from "./studio-kernel.js";

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

  // ── Dispose ─────────────────────────────────────────────────────────────

  describe("dispose", () => {
    it("should not throw on dispose", () => {
      expect(() => kernel.dispose()).not.toThrow();
    });
  });
});
