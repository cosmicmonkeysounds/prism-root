import { describe, it, expect, beforeEach } from "vitest";
import { ObjectRegistry } from "../object-model/registry.js";
import type { CategoryRule, EntityDef } from "../object-model/types.js";
import { generateRouteSpecs } from "./route-gen.js";
import { buildOpenApiDocument, generateOpenApiJson } from "./openapi.js";

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
  description: "A trackable work item",
  fields: [
    { id: "priority", type: "enum", enumOptions: [{ value: "low", label: "Low" }, { value: "high", label: "High" }], required: true },
    { id: "estimate", type: "float", description: "Hours estimated" },
    { id: "due", type: "date" },
    { id: "done", type: "bool" },
    { id: "count", type: "int" },
    { id: "link", type: "url" },
    { id: "started", type: "datetime" },
    { id: "assignee", type: "object_ref", refTypes: ["person"] },
    { id: "body", type: "text" },
  ],
  api: {
    path: "tasks",
    operations: ["list", "get", "create", "update", "delete", "restore", "move", "duplicate"],
    softDelete: true,
    filterBy: ["status", "tags"],
    defaultSort: { field: "createdAt", dir: "desc" },
  },
};

const NOTE: EntityDef = {
  type: "note",
  category: "content",
  label: "Note",
  api: {
    operations: ["list", "get"],
  },
};

let registry: ObjectRegistry;

beforeEach(() => {
  registry = new ObjectRegistry(RULES);
  registry.registerAll([TASK, NOTE]);
});

describe("buildOpenApiDocument", () => {
  it("produces a valid 3.1.0 document", () => {
    const specs = generateRouteSpecs(registry);
    const doc = buildOpenApiDocument(specs, registry, { title: "Prism API" });
    expect(doc.openapi).toBe("3.1.0");
    expect(doc.info.title).toBe("Prism API");
    expect(doc.info.version).toBe("0.1.0");
  });

  it("includes base schemas in components", () => {
    const specs = generateRouteSpecs(registry);
    const doc = buildOpenApiDocument(specs, registry, { title: "Test" });
    expect(doc.components.schemas).toHaveProperty("GraphObject");
    expect(doc.components.schemas).toHaveProperty("ObjectEdge");
    expect(doc.components.schemas).toHaveProperty("ResolvedEdge");
  });

  it("generates per-type data schemas", () => {
    const specs = generateRouteSpecs(registry);
    const doc = buildOpenApiDocument(specs, registry, { title: "Test" });
    expect(doc.components.schemas).toHaveProperty("TaskData");
    expect(doc.components.schemas).toHaveProperty("Task");
  });

  it("maps field types to OpenAPI types", () => {
    const specs = generateRouteSpecs(registry);
    const doc = buildOpenApiDocument(specs, registry, { title: "Test" });
    const dataSchema = doc.components.schemas["TaskData"] as Record<string, unknown>;
    const props = (dataSchema as { properties: Record<string, Record<string, unknown>> }).properties;
    expect(props["priority"]).toHaveProperty("enum", ["low", "high"]);
    expect(props["estimate"]).toHaveProperty("type", "number");
    expect(props["due"]).toHaveProperty("format", "date");
    expect(props["done"]).toHaveProperty("type", "boolean");
    expect(props["count"]).toHaveProperty("type", "integer");
    expect(props["link"]).toHaveProperty("format", "uri");
    expect(props["started"]).toHaveProperty("format", "date-time");
    expect(props["assignee"]).toHaveProperty("type", "string");
    expect(props["body"]).toHaveProperty("type", "string");
  });

  it("marks required fields", () => {
    const specs = generateRouteSpecs(registry);
    const doc = buildOpenApiDocument(specs, registry, { title: "Test" });
    const dataSchema = doc.components.schemas["TaskData"] as { required: string[] };
    expect(dataSchema.required).toContain("priority");
    expect(dataSchema.required).not.toContain("estimate");
  });

  it("generates type schema as allOf with base", () => {
    const specs = generateRouteSpecs(registry);
    const doc = buildOpenApiDocument(specs, registry, { title: "Test" });
    const taskSchema = doc.components.schemas["Task"] as { allOf: Array<Record<string, unknown>> };
    expect(taskSchema.allOf).toHaveLength(2);
    expect(taskSchema.allOf[0]).toEqual({ $ref: "#/components/schemas/GraphObject" });
    const ext = taskSchema.allOf[1] as { properties: Record<string, unknown> };
    expect(ext.properties).toHaveProperty("type");
    expect(ext.properties).toHaveProperty("data");
  });

  it("creates type schema without data ref when no fields", () => {
    const specs = generateRouteSpecs(registry);
    const doc = buildOpenApiDocument(specs, registry, { title: "Test" });
    const noteSchema = doc.components.schemas["Note"] as { allOf: Array<Record<string, unknown>> };
    expect(noteSchema.allOf).toHaveLength(2);
    const ext = noteSchema.allOf[1] as { properties: Record<string, unknown> };
    expect(ext.properties).not.toHaveProperty("data");
  });

  it("skips data schemas when emitDataSchemas is false", () => {
    const specs = generateRouteSpecs(registry);
    const doc = buildOpenApiDocument(specs, registry, { title: "Test", emitDataSchemas: false });
    expect(doc.components.schemas).not.toHaveProperty("TaskData");
    expect(doc.components.schemas).not.toHaveProperty("Task");
  });

  // ── Paths ──────────────────────────────────────────────────────────────────

  it("converts :id params to {id} in paths", () => {
    const specs = generateRouteSpecs(registry);
    const doc = buildOpenApiDocument(specs, registry, { title: "Test" });
    expect(doc.paths).toHaveProperty("/api/tasks/{id}");
    expect(doc.paths).not.toHaveProperty("/api/tasks/:id");
  });

  it("generates operations for each route", () => {
    const specs = generateRouteSpecs(registry);
    const doc = buildOpenApiDocument(specs, registry, { title: "Test" });
    const tasksPath = doc.paths["/api/tasks"] as Record<string, Record<string, unknown>>;
    expect(tasksPath).toHaveProperty("get"); // list
    expect(tasksPath).toHaveProperty("post"); // create
    const taskIdPath = doc.paths["/api/tasks/{id}"] as Record<string, Record<string, unknown>>;
    expect(taskIdPath).toHaveProperty("get"); // get
    expect(taskIdPath).toHaveProperty("put"); // update
    expect(taskIdPath).toHaveProperty("delete"); // delete
  });

  it("sets correct operationIds", () => {
    const specs = generateRouteSpecs(registry);
    const doc = buildOpenApiDocument(specs, registry, { title: "Test" });
    const tasksPath = doc.paths["/api/tasks"] as Record<string, Record<string, unknown>>;
    expect((tasksPath["get"] as Record<string, string>)["operationId"]).toBe("listTasks");
    expect((tasksPath["post"] as Record<string, string>)["operationId"]).toBe("createTask");
  });

  it("generates edge routes", () => {
    const specs = generateRouteSpecs(registry);
    const doc = buildOpenApiDocument(specs, registry, { title: "Test" });
    expect(doc.paths).toHaveProperty("/api/edges");
    expect(doc.paths).toHaveProperty("/api/edges/{id}");
    const edgesPath = doc.paths["/api/edges"] as Record<string, Record<string, unknown>>;
    expect(edgesPath).toHaveProperty("get");
    expect(edgesPath).toHaveProperty("post");
  });

  it("generates related route", () => {
    const specs = generateRouteSpecs(registry);
    const doc = buildOpenApiDocument(specs, registry, { title: "Test" });
    expect(doc.paths).toHaveProperty("/api/objects/{id}/related");
  });

  it("includes servers when provided", () => {
    const specs = generateRouteSpecs(registry);
    const doc = buildOpenApiDocument(specs, registry, {
      title: "Test",
      servers: [{ url: "http://localhost:3000", description: "Dev" }],
    });
    expect(doc.servers).toHaveLength(1);
    expect(doc.servers?.[0]?.url).toBe("http://localhost:3000");
  });

  it("includes description when provided", () => {
    const specs = generateRouteSpecs(registry);
    const doc = buildOpenApiDocument(specs, registry, {
      title: "Test",
      description: "My API",
    });
    expect(doc.info.description).toBe("My API");
  });
});

describe("generateOpenApiJson", () => {
  it("returns valid JSON string", () => {
    const specs = generateRouteSpecs(registry);
    const json = generateOpenApiJson(specs, registry, { title: "Test" });
    const parsed = JSON.parse(json);
    expect(parsed.openapi).toBe("3.1.0");
    expect(parsed.paths).toBeDefined();
  });
});
