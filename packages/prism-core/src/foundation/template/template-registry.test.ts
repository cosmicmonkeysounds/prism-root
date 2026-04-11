import { describe, it, expect, beforeEach } from "vitest";
import { TreeModel } from "@prism/core/object-model";
import { EdgeModel } from "@prism/core/object-model";
import { UndoRedoManager } from "@prism/core/undo";
import { createTemplateRegistry } from "./template-registry.js";
import type { TemplateRegistry } from "./template-registry.js";
import type { ObjectTemplate } from "./template-types.js";

// ── Helpers ───────────────────────────────────────────────────────────────────

let idCounter = 0;
const genId = () => `id-${++idCounter}`;
const noop = () => {};

function makeTree() {
  return new TreeModel({ generateId: genId });
}

function makeEdges() {
  return new EdgeModel({ generateId: genId });
}

function makeUndo() {
  return new UndoRedoManager(noop, { maxHistory: 50 });
}

function makeTemplate(overrides: Partial<ObjectTemplate> = {}): ObjectTemplate {
  return {
    id: "tpl-1",
    name: "Task Template",
    category: "productivity",
    root: {
      placeholderId: "p1",
      type: "task",
      name: "{{name}}",
      description: "Created by template",
      data: { priority: "{{priority}}" },
    },
    createdAt: "2026-04-01T00:00:00.000Z",
    ...overrides,
  };
}

function makeNestedTemplate(): ObjectTemplate {
  return {
    id: "tpl-nested",
    name: "Project Template",
    category: "productivity",
    root: {
      placeholderId: "p-root",
      type: "project",
      name: "{{name}}",
      children: [
        {
          placeholderId: "p-task-1",
          type: "task",
          name: "Task 1",
          data: { assignee: "{{lead}}" },
        },
        {
          placeholderId: "p-task-2",
          type: "task",
          name: "Task 2",
        },
      ],
    },
    edges: [
      {
        sourcePlaceholderId: "p-task-2",
        targetPlaceholderId: "p-task-1",
        relation: "depends-on",
      },
    ],
    variables: [
      { name: "name", label: "Project Name", required: true },
      { name: "lead", label: "Lead", defaultValue: "unassigned" },
    ],
    createdAt: "2026-04-01T00:00:00.000Z",
  };
}

// ── Tests ────────────────────────────────────────────────────────────────────

describe("TemplateRegistry", () => {
  let tree: TreeModel;
  let edges: EdgeModel;
  let undo: UndoRedoManager;
  let registry: TemplateRegistry;

  beforeEach(() => {
    idCounter = 0;
    tree = makeTree();
    edges = makeEdges();
    undo = makeUndo();
    registry = createTemplateRegistry({ tree, edges, undo, generateId: genId });
  });

  // ── register / unregister / get / has ─────────────────────────────────────

  describe("registration", () => {
    it("registers and retrieves a template", () => {
      const tpl = makeTemplate();
      registry.register(tpl);
      expect(registry.has("tpl-1")).toBe(true);
      expect(registry.get("tpl-1")).toBe(tpl);
      expect(registry.size).toBe(1);
    });

    it("overwrites on duplicate id", () => {
      registry.register(makeTemplate());
      registry.register(makeTemplate({ name: "Updated" }));
      expect(registry.size).toBe(1);
      expect(registry.get("tpl-1")?.name).toBe("Updated");
    });

    it("unregisters a template", () => {
      registry.register(makeTemplate());
      expect(registry.unregister("tpl-1")).toBe(true);
      expect(registry.has("tpl-1")).toBe(false);
      expect(registry.size).toBe(0);
    });

    it("unregister returns false for nonexistent", () => {
      expect(registry.unregister("nope")).toBe(false);
    });
  });

  // ── list / filter ─────────────────────────────────────────────────────────

  describe("list", () => {
    beforeEach(() => {
      registry.register(makeTemplate({ id: "t1", name: "Task A", category: "productivity" }));
      registry.register(
        makeTemplate({
          id: "t2",
          name: "Monster B",
          category: "game",
          root: { placeholderId: "p2", type: "monster", name: "M" },
        }),
      );
      registry.register(makeTemplate({ id: "t3", name: "Task C", category: "productivity" }));
    });

    it("lists all templates", () => {
      expect(registry.list()).toHaveLength(3);
    });

    it("filters by category", () => {
      const result = registry.list({ category: "productivity" });
      expect(result).toHaveLength(2);
    });

    it("filters by root type", () => {
      const result = registry.list({ type: "monster" });
      expect(result).toHaveLength(1);
      expect(result[0].id).toBe("t2");
    });

    it("filters by search string (case-insensitive)", () => {
      const result = registry.list({ search: "monster" });
      expect(result).toHaveLength(1);
    });

    it("combines filters", () => {
      const result = registry.list({ category: "productivity", search: "task c" });
      expect(result).toHaveLength(1);
      expect(result[0].id).toBe("t3");
    });
  });

  // ── instantiate (simple) ──────────────────────────────────────────────────

  describe("instantiate (simple)", () => {
    it("creates an object from a simple template", () => {
      registry.register(makeTemplate());
      const result = registry.instantiate("tpl-1", {
        variables: { name: "My Task", priority: "high" },
      });
      expect(result.created).toHaveLength(1);
      expect(result.created[0].name).toBe("My Task");
      expect(result.created[0].type).toBe("task");
      expect(result.created[0].data["priority"]).toBe("high");
      expect(tree.size).toBe(1);
    });

    it("leaves unreplaced variables as-is", () => {
      registry.register(makeTemplate());
      const result = registry.instantiate("tpl-1");
      expect(result.created[0].name).toBe("{{name}}");
      expect(result.created[0].data["priority"]).toBe("{{priority}}");
    });

    it("creates under specified parent", () => {
      const folder = tree.add({ type: "folder", name: "F" });
      registry.register(makeTemplate());
      const result = registry.instantiate("tpl-1", {
        parentId: folder.id,
        variables: { name: "A", priority: "low" },
      });
      expect(result.created[0].parentId).toBe(folder.id);
    });

    it("throws for nonexistent template", () => {
      expect(() => registry.instantiate("nope")).toThrow("not found");
    });
  });

  // ── instantiate (nested) ──────────────────────────────────────────────────

  describe("instantiate (nested with edges)", () => {
    it("creates root + children", () => {
      registry.register(makeNestedTemplate());
      const result = registry.instantiate("tpl-nested", {
        variables: { name: "Sprint 23", lead: "Alice" },
      });
      expect(result.created).toHaveLength(3); // project + 2 tasks
      expect(result.created[0].name).toBe("Sprint 23");
      expect(result.created[0].type).toBe("project");

      const children = tree.getChildren(result.created[0].id);
      expect(children).toHaveLength(2);
      expect(children[0].data["assignee"]).toBe("Alice");
    });

    it("creates edges with remapped IDs", () => {
      registry.register(makeNestedTemplate());
      const result = registry.instantiate("tpl-nested", {
        variables: { name: "S", lead: "Bob" },
      });
      expect(result.createdEdges).toHaveLength(1);
      const edge = result.createdEdges[0];
      expect(edge.relation).toBe("depends-on");
      // Source and target should be real IDs in the created set
      const createdIds = new Set(result.created.map((o) => o.id as string));
      expect(createdIds.has(edge.sourceId as string)).toBe(true);
      expect(createdIds.has(edge.targetId as string)).toBe(true);
    });

    it("provides placeholder-to-real idMap", () => {
      registry.register(makeNestedTemplate());
      const result = registry.instantiate("tpl-nested", {
        variables: { name: "P" },
      });
      expect(result.idMap.size).toBe(3);
      expect(result.idMap.has("p-root")).toBe(true);
      expect(result.idMap.has("p-task-1")).toBe(true);
      expect(result.idMap.has("p-task-2")).toBe(true);
    });
  });

  // ── undo integration ─────────────────────────────────────────────────────

  describe("undo integration", () => {
    it("pushes single undo entry", () => {
      registry.register(makeNestedTemplate());
      registry.instantiate("tpl-nested", { variables: { name: "P" } });
      expect(undo.history).toHaveLength(1);
      expect(undo.history[0].description).toContain("Project Template");
    });

    it("does not push undo without undo manager", () => {
      const reg = createTemplateRegistry({ tree, edges, generateId: genId });
      reg.register(makeTemplate());
      reg.instantiate("tpl-1", { variables: { name: "A", priority: "low" } });
      expect(tree.size).toBe(1);
    });
  });

  // ── createFromObject ──────────────────────────────────────────────────────

  describe("createFromObject", () => {
    it("captures a single object as template", () => {
      const obj = tree.add({
        type: "task",
        name: "Original",
        status: "open",
        data: { priority: "high" },
      });
      const tpl = registry.createFromObject(obj.id, {
        id: "from-obj",
        name: "My Template",
        category: "test",
      });
      expect(tpl.id).toBe("from-obj");
      expect(tpl.name).toBe("My Template");
      expect(tpl.root.type).toBe("task");
      expect(tpl.root.name).toBe("Original");
      expect(tpl.root.status).toBe("open");
      expect(tpl.root.data?.["priority"]).toBe("high");
    });

    it("captures descendants as children", () => {
      const folder = tree.add({ type: "folder", name: "F" });
      tree.add({ type: "task", name: "A" }, { parentId: folder.id });
      tree.add({ type: "task", name: "B" }, { parentId: folder.id });
      const tpl = registry.createFromObject(folder.id, {
        id: "nested",
        name: "Folder Template",
      });
      expect(tpl.root.children).toHaveLength(2);
      expect(tpl.root.children?.[0].name).toBe("A");
      expect(tpl.root.children?.[1].name).toBe("B");
    });

    it("captures internal edges", () => {
      const a = tree.add({ type: "task", name: "A" });
      const b = tree.add({ type: "task", name: "B" });
      const folder = tree.add({ type: "folder", name: "F" });
      tree.move(a.id, folder.id);
      tree.move(b.id, folder.id);
      edges.add({
        sourceId: a.id,
        targetId: b.id,
        relation: "dep",
        data: {},
      });
      const tpl = registry.createFromObject(folder.id, {
        id: "edge-tpl",
        name: "With Edges",
      });
      expect(tpl.edges).toHaveLength(1);
      expect(tpl.edges?.[0].relation).toBe("dep");
    });

    it("round-trips: create template from object, then instantiate", () => {
      const folder = tree.add({ type: "folder", name: "F" });
      tree.add({ type: "task", name: "T1" }, { parentId: folder.id });
      tree.add({ type: "task", name: "T2" }, { parentId: folder.id });

      const tpl = registry.createFromObject(folder.id, {
        id: "round-trip",
        name: "RT",
      });
      registry.register(tpl);

      const before = tree.size;
      const result = registry.instantiate("round-trip");
      expect(result.created).toHaveLength(3);
      expect(tree.size).toBe(before + 3);
    });

    it("throws for nonexistent object", () => {
      expect(() =>
        registry.createFromObject("nope", { id: "x", name: "X" }),
      ).toThrow("not found");
    });
  });

  // ── variable interpolation edge cases ─────────────────────────────────────

  describe("variable interpolation", () => {
    it("interpolates description field", () => {
      registry.register(
        makeTemplate({
          root: {
            placeholderId: "p1",
            type: "note",
            name: "Note",
            description: "Written by {{author}} on {{date}}",
          },
        }),
      );
      const result = registry.instantiate("tpl-1", {
        variables: { author: "Alice", date: "2026-04-01" },
      });
      expect(result.created[0].description).toBe(
        "Written by Alice on 2026-04-01",
      );
    });

    it("interpolates status field", () => {
      registry.register(
        makeTemplate({
          root: {
            placeholderId: "p1",
            type: "task",
            name: "T",
            status: "{{initialStatus}}",
          },
        }),
      );
      const result = registry.instantiate("tpl-1", {
        variables: { initialStatus: "in-progress" },
      });
      expect(result.created[0].status).toBe("in-progress");
    });

    it("does not interpolate non-string data values", () => {
      registry.register(
        makeTemplate({
          root: {
            placeholderId: "p1",
            type: "task",
            name: "T",
            data: { count: 42, label: "{{tag}}" },
          },
        }),
      );
      const result = registry.instantiate("tpl-1", {
        variables: { tag: "urgent" },
      });
      expect(result.created[0].data["count"]).toBe(42);
      expect(result.created[0].data["label"]).toBe("urgent");
    });

    it("handles multiple variables in one string", () => {
      registry.register(
        makeTemplate({
          root: {
            placeholderId: "p1",
            type: "task",
            name: "{{prefix}}-{{suffix}}",
          },
        }),
      );
      const result = registry.instantiate("tpl-1", {
        variables: { prefix: "PROJ", suffix: "001" },
      });
      expect(result.created[0].name).toBe("PROJ-001");
    });
  });

  // ── instantiate at position ───────────────────────────────────────────────

  describe("instantiate at position", () => {
    it("inserts at specified position", () => {
      tree.add({ type: "task", name: "Existing" });
      registry.register(makeTemplate());
      registry.instantiate("tpl-1", {
        variables: { name: "New", priority: "low" },
        position: 0,
      });
      const roots = tree.getChildren(null);
      expect(roots[0].name).toBe("New");
    });
  });
});
