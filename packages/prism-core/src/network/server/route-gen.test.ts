import { describe, it, expect, beforeEach } from "vitest";
import { ObjectRegistry } from "@prism/core/object-model";
import type { CategoryRule, EntityDef } from "@prism/core/object-model";
import {
  generateRouteSpecs,
  groupByType,
  printRouteTable,
  registerRoutes,
} from "./route-gen.js";
import type { RouteSpec, RouteAdapter, RouteHandler } from "./route-gen.js";

// ── Fixtures ──────────────────────────────────────────────────────────────────

const RULES: CategoryRule[] = [
  { category: "container", canParent: ["container", "content"] },
  { category: "content", canParent: [] },
];

const TASK: EntityDef = {
  type: "task",
  category: "content",
  label: "Task",
  pluralLabel: "Tasks",
  api: {
    path: "tasks",
    operations: ["list", "get", "create", "update", "delete", "restore"],
    softDelete: true,
    filterBy: ["status", "tags"],
  },
};

const NOTE: EntityDef = {
  type: "note",
  category: "content",
  label: "Note",
  api: {
    operations: ["list", "get", "create", "delete"],
  },
};

const PROJECT: EntityDef = {
  type: "project",
  category: "container",
  label: "Project",
  // No api config — should NOT generate routes
};

const FULL_OPS: EntityDef = {
  type: "item",
  category: "content",
  label: "Item",
  api: {
    operations: ["list", "get", "create", "update", "delete", "restore", "move", "duplicate"],
  },
};

let registry: ObjectRegistry;

beforeEach(() => {
  registry = new ObjectRegistry(RULES);
  registry.registerAll([TASK, NOTE, PROJECT]);
});

// ── generateRouteSpecs ────────────────────────────────────────────────────────

describe("generateRouteSpecs", () => {
  it("generates routes only for types with api config", () => {
    const specs = generateRouteSpecs(registry);
    const types = new Set(specs.filter((s) => s.typeDef).map((s) => s.typeDef?.type));
    expect(types.has("task")).toBe(true);
    expect(types.has("note")).toBe(true);
    expect(types.has("project")).toBe(false);
  });

  it("generates correct CRUD routes for tasks", () => {
    const specs = generateRouteSpecs(registry);
    const taskSpecs = specs.filter((s) => s.typeDef?.type === "task");
    const ops = taskSpecs.map((s) => s.operation);
    expect(ops).toContain("list");
    expect(ops).toContain("get");
    expect(ops).toContain("create");
    expect(ops).toContain("update");
    expect(ops).toContain("delete");
    expect(ops).toContain("restore");
    expect(ops).not.toContain("move");
  });

  it("uses api.path for URL path segment", () => {
    const specs = generateRouteSpecs(registry);
    const taskList = specs.find((s) => s.typeDef?.type === "task" && s.operation === "list");
    expect(taskList?.path).toBe("/api/tasks");
  });

  it("falls back to type string when api.path is omitted", () => {
    const specs = generateRouteSpecs(registry);
    const noteList = specs.find((s) => s.typeDef?.type === "note" && s.operation === "list");
    expect(noteList?.path).toBe("/api/note");
  });

  it("generates correct HTTP methods", () => {
    const specs = generateRouteSpecs(registry);
    const taskSpecs = specs.filter((s) => s.typeDef?.type === "task");
    const methodMap = new Map(taskSpecs.map((s) => [s.operation, s.method]));
    expect(methodMap.get("list")).toBe("GET");
    expect(methodMap.get("get")).toBe("GET");
    expect(methodMap.get("create")).toBe("POST");
    expect(methodMap.get("update")).toBe("PUT");
    expect(methodMap.get("delete")).toBe("DELETE");
    expect(methodMap.get("restore")).toBe("POST");
  });

  it("generates :id param for single-resource routes", () => {
    const specs = generateRouteSpecs(registry);
    const taskGet = specs.find((s) => s.typeDef?.type === "task" && s.operation === "get");
    expect(taskGet?.path).toBe("/api/tasks/:id");
  });

  it("respects custom prefix", () => {
    const specs = generateRouteSpecs(registry, { prefix: "/v2" });
    const taskList = specs.find((s) => s.typeDef?.type === "task" && s.operation === "list");
    expect(taskList?.path).toBe("/v2/tasks");
  });

  it("generates all eight operation types", () => {
    const fullReg = new ObjectRegistry(RULES);
    fullReg.registerAll([FULL_OPS]);
    const specs = generateRouteSpecs(fullReg);
    const itemSpecs = specs.filter((s) => s.typeDef?.type === "item");
    const ops = itemSpecs.map((s) => s.operation);
    expect(ops).toEqual(["list", "get", "create", "update", "delete", "restore", "move", "duplicate"]);
  });

  it("generates move/duplicate as POST with :id", () => {
    const fullReg = new ObjectRegistry(RULES);
    fullReg.registerAll([FULL_OPS]);
    const specs = generateRouteSpecs(fullReg);
    const move = specs.find((s) => s.operation === "move");
    const dup = specs.find((s) => s.operation === "duplicate");
    expect(move?.method).toBe("POST");
    expect(move?.path).toBe("/api/item/:id/move");
    expect(dup?.method).toBe("POST");
    expect(dup?.path).toBe("/api/item/:id/duplicate");
  });

  it("includes meta with filterBy and softDelete", () => {
    const specs = generateRouteSpecs(registry);
    const taskList = specs.find((s) => s.typeDef?.type === "task" && s.operation === "list");
    expect(taskList?.meta.filterBy).toEqual(["status", "tags"]);
    expect(taskList?.meta.softDelete).toBe(true);
  });

  // ── Edge routes ──────────────────────────────────────────────────────────

  it("generates edge routes by default", () => {
    const specs = generateRouteSpecs(registry);
    const edgeSpecs = specs.filter((s) => s.operation.toString().startsWith("edges-"));
    expect(edgeSpecs).toHaveLength(5);
    const ops = edgeSpecs.map((s) => s.operation);
    expect(ops).toContain("edges-list");
    expect(ops).toContain("edges-create");
    expect(ops).toContain("edges-get");
    expect(ops).toContain("edges-update");
    expect(ops).toContain("edges-delete");
  });

  it("generates related route", () => {
    const specs = generateRouteSpecs(registry);
    const related = specs.find((s) => s.operation === "related");
    expect(related?.method).toBe("GET");
    expect(related?.path).toBe("/api/objects/:id/related");
  });

  it("omits edge routes when disabled", () => {
    const specs = generateRouteSpecs(registry, { includeEdgeRoutes: false });
    const edgeSpecs = specs.filter(
      (s) => s.operation.toString().startsWith("edges-") || s.operation === "related",
    );
    expect(edgeSpecs).toHaveLength(0);
  });

  // ── Object search ─────────────────────────────────────────────────────────

  it("generates global object search routes by default", () => {
    const specs = generateRouteSpecs(registry);
    const objSpecs = specs.filter((s) => s.typeDef === null && s.meta.path === "objects");
    expect(objSpecs).toHaveLength(2);
    expect(objSpecs.map((s) => s.method)).toEqual(["GET", "GET"]);
  });

  it("omits object search when disabled", () => {
    const specs = generateRouteSpecs(registry, { includeObjectSearch: false });
    const objSpecs = specs.filter((s) => s.typeDef === null && s.meta.path === "objects");
    expect(objSpecs).toHaveLength(0);
  });
});

// ── groupByType ───────────────────────────────────────────────────────────────

describe("groupByType", () => {
  it("groups specs by type", () => {
    const specs = generateRouteSpecs(registry);
    const grouped = groupByType(specs);
    expect(grouped.has("task")).toBe(true);
    expect(grouped.has("note")).toBe(true);
    expect(grouped.has(null)).toBe(true); // edge + object search routes
  });

  it("null group contains edge and object search routes", () => {
    const specs = generateRouteSpecs(registry);
    const grouped = groupByType(specs);
    const nullSpecs = grouped.get(null) ?? [];
    expect(nullSpecs.length).toBeGreaterThan(0);
    expect(nullSpecs.every((s) => s.typeDef === null)).toBe(true);
  });
});

// ── printRouteTable ─────────────────────────────────────────────────────────

describe("printRouteTable", () => {
  it("formats routes as padded lines", () => {
    const specs = generateRouteSpecs(registry);
    const table = printRouteTable(specs);
    expect(table).toContain("GET");
    expect(table).toContain("/api/tasks");
    expect(table).toContain("(list)");
  });
});

// ── registerRoutes ──────────────────────────────────────────────────────────

describe("registerRoutes", () => {
  it("registers all specs via adapter", () => {
    const registered: Array<{ spec: RouteSpec; handler: RouteHandler }> = [];
    const adapter: RouteAdapter = {
      register(spec, handler) {
        registered.push({ spec, handler });
      },
    };
    const handlerFactory = (_spec: RouteSpec): RouteHandler => {
      return async () => ({ status: 200, body: {} });
    };
    registerRoutes(registry, adapter, handlerFactory);
    expect(registered.length).toBeGreaterThan(0);
    // Should have task + note routes + edge routes + object search
    const taskRoutes = registered.filter((r) => r.spec.typeDef?.type === "task");
    expect(taskRoutes).toHaveLength(6); // list, get, create, update, delete, restore
  });
});
