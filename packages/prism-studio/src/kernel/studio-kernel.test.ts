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

  // ── Dispose ─────────────────────────────────────────────────────────────

  describe("dispose", () => {
    it("should not throw on dispose", () => {
      expect(() => kernel.dispose()).not.toThrow();
    });
  });
});
