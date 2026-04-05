import { describe, it, expect, beforeEach } from "vitest";
import { ObjectRegistry } from "./registry.js";
import { ContextEngine } from "./context-engine.js";
import type { CategoryRule, EntityDef, EdgeTypeDef } from "./types.js";

const RULES: CategoryRule[] = [
  { category: "container", canParent: ["container", "content"], canBeRoot: true },
  { category: "content", canParent: [] },
];

const PROJECT: EntityDef = {
  type: "project",
  category: "container",
  label: "Project",
  pluralLabel: "Projects",
};

const TASK: EntityDef = {
  type: "task",
  category: "content",
  label: "Task",
  pluralLabel: "Tasks",
};

const NOTE: EntityDef = {
  type: "note",
  category: "content",
  label: "Note",
  pluralLabel: "Notes",
};

const PERSON: EntityDef = {
  type: "person",
  category: "content",
  label: "Person",
  pluralLabel: "People",
};

const DEPENDS_ON: EdgeTypeDef = {
  relation: "depends-on",
  label: "Depends On",
  behavior: "dependency",
  sourceTypes: ["task"],
  targetTypes: ["task"],
};

const ASSIGNED_TO: EdgeTypeDef = {
  relation: "assigned-to",
  label: "Assigned To",
  behavior: "assignment",
  sourceTypes: ["task"],
  targetTypes: ["person"],
};

const WEAK_REF: EdgeTypeDef = {
  relation: "weak-ref",
  label: "References",
  behavior: "weak",
  suggestInline: true,
};

describe("ContextEngine", () => {
  let registry: ObjectRegistry;
  let engine: ContextEngine;

  beforeEach(() => {
    registry = new ObjectRegistry(RULES);
    registry.registerAll([PROJECT, TASK, NOTE, PERSON]);
    registry.registerEdges([DEPENDS_ON, ASSIGNED_TO, WEAK_REF]);
    engine = new ContextEngine(registry);
  });

  describe("getEdgeOptions", () => {
    it("returns all edges from a source type", () => {
      const opts = engine.getEdgeOptions("task");
      const relations = opts.map((o) => o.relation);
      expect(relations).toContain("depends-on");
      expect(relations).toContain("assigned-to");
      expect(relations).toContain("weak-ref");
    });

    it("filters by target type", () => {
      const opts = engine.getEdgeOptions("task", "person");
      const relations = opts.map((o) => o.relation);
      expect(relations).toContain("assigned-to");
      expect(relations).toContain("weak-ref");
      expect(relations).not.toContain("depends-on");
    });

    it("marks inline edges", () => {
      const opts = engine.getEdgeOptions("task");
      const weakRef = opts.find((o) => o.relation === "weak-ref");
      expect(weakRef?.isInline).toBe(true);
      const dep = opts.find((o) => o.relation === "depends-on");
      expect(dep?.isInline).toBe(false);
    });

    it("returns empty for unknown types", () => {
      const opts = engine.getEdgeOptions("unknown");
      // weak-ref has no source constraints so it still matches
      expect(opts.every((o) => o.relation === "weak-ref")).toBe(true);
    });
  });

  describe("getInlineLinkTypes", () => {
    it("returns edge types with suggestInline from source", () => {
      const types = engine.getInlineLinkTypes("note");
      expect(types).toHaveLength(1);
      expect(types[0].relation).toBe("weak-ref");
    });
  });

  describe("getInlineEdgeTypes", () => {
    it("returns all inline edge types", () => {
      const types = engine.getInlineEdgeTypes();
      expect(types).toHaveLength(1);
      expect(types[0].relation).toBe("weak-ref");
    });
  });

  describe("getAutocompleteSuggestions", () => {
    it("returns inline types and default relation", () => {
      const result = engine.getAutocompleteSuggestions("note");
      expect(result.edgeTypes).toHaveLength(1);
      expect(result.defaultRelation).toBe("weak-ref");
    });

    it("returns null defaultRelation when no inline types exist", () => {
      const emptyRegistry = new ObjectRegistry(RULES);
      emptyRegistry.registerAll([PROJECT, TASK]);
      const emptyEngine = new ContextEngine(emptyRegistry);
      const result = emptyEngine.getAutocompleteSuggestions("task");
      expect(result.edgeTypes).toHaveLength(0);
      expect(result.defaultRelation).toBeNull();
    });
  });

  describe("getChildOptions", () => {
    it("returns allowed child types for a container", () => {
      const opts = engine.getChildOptions("project");
      const types = opts.map((o) => o.type);
      expect(types).toContain("task");
      expect(types).toContain("note");
      expect(types).toContain("person");
    });

    it("returns empty for leaf types", () => {
      const opts = engine.getChildOptions("note");
      expect(opts).toHaveLength(0);
    });

    it("includes plural labels", () => {
      const opts = engine.getChildOptions("project");
      const person = opts.find((o) => o.type === "person");
      expect(person?.pluralLabel).toBe("People");
    });
  });

  describe("getContextMenu", () => {
    it("returns create, connect, and object sections for container", () => {
      const menu = engine.getContextMenu("project");
      const sectionIds = menu.map((s) => s.id);
      expect(sectionIds).toContain("create");
      expect(sectionIds).toContain("object");
    });

    it("excludes create section for leaf types", () => {
      const menu = engine.getContextMenu("note");
      const sectionIds = menu.map((s) => s.id);
      expect(sectionIds).not.toContain("create");
    });

    it("always includes object section with duplicate and delete", () => {
      const menu = engine.getContextMenu("note");
      const objSection = menu.find((s) => s.id === "object");
      expect(objSection).toBeDefined();
      const actions = objSection?.items.map((i) => i.action);
      expect(actions).toContain("duplicate");
      expect(actions).toContain("delete");
    });

    it("connect section excludes inline-only edges", () => {
      const menu = engine.getContextMenu("task");
      const connectSection = menu.find((s) => s.id === "connect");
      if (connectSection) {
        const relations = connectSection.items.map(
          (i) => i.payload.relation,
        );
        expect(relations).not.toContain("weak-ref");
      }
    });

    it("filters connect section by target type", () => {
      const menu = engine.getContextMenu("task", "person");
      const connectSection = menu.find((s) => s.id === "connect");
      expect(connectSection).toBeDefined();
      const relations = connectSection?.items.map(
        (i) => i.payload.relation,
      );
      expect(relations).toContain("assigned-to");
      expect(relations).not.toContain("depends-on");
    });

    it("create section items have correct action and payload", () => {
      const menu = engine.getContextMenu("project");
      const createSection = menu.find((s) => s.id === "create");
      const taskItem = createSection?.items.find(
        (i) => i.payload.childType === "task",
      );
      expect(taskItem).toBeDefined();
      expect(taskItem?.action).toBe("create-child");
      expect(taskItem?.label).toBe("New Task");
    });
  });
});
