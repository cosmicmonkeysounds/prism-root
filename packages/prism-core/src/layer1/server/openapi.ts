/**
 * @prism/core — OpenAPI 3.1.0 emitter
 *
 * Generates a complete OpenAPI document from RouteSpec[] + ObjectRegistry.
 * Framework-agnostic — pure TypeScript, no HTTP library deps.
 *
 * Features:
 *   - Full path + operation objects for every RouteSpec
 *   - Per-type component schemas from EntityFieldDef arrays
 *   - GraphObject base schema with allOf extension per type
 *   - ObjectEdge + ResolvedEdge shared schemas
 *   - filterBy fields -> query parameters on list routes
 *   - Proper operationIds (listTasks, getTask, createTask, etc.)
 *   - Tags grouped by entity label
 */

import type { EntityFieldDef, EntityFieldType } from "../object-model/types.js";
import type { ObjectRegistry } from "../object-model/registry.js";
import { pascal, singular } from "../object-model/str.js";
import type { RouteSpec } from "./route-gen.js";

// ── Options ─────────────────────────────────────────────────────────────────────

export interface OpenApiOptions {
  title: string;
  version?: string;
  description?: string;
  servers?: Array<{ url: string; description?: string }>;
  /** Whether to emit schemas for entity data payloads. Default: true */
  emitDataSchemas?: boolean;
}

// ── OpenAPI document type (minimal) ─────────────────────────────────────────────

interface OpenApiDocument {
  openapi: "3.1.0";
  info: { title: string; version: string; description?: string };
  servers?: Array<{ url: string; description?: string }>;
  paths: Record<string, Record<string, unknown>>;
  components: { schemas: Record<string, unknown> };
}

// ── buildOpenApiDocument ────────────────────────────────────────────────────────

/**
 * Build a complete OpenAPI 3.1.0 document from route specs and a registry.
 * Returns the document as a plain object (JSON-serializable).
 */
export function buildOpenApiDocument(
  specs: RouteSpec[],
  registry: ObjectRegistry,
  options: OpenApiOptions,
): OpenApiDocument {
  const emitData = options.emitDataSchemas ?? true;

  const doc: OpenApiDocument = {
    openapi: "3.1.0",
    info: {
      title: options.title,
      version: options.version ?? "0.1.0",
      ...(options.description ? { description: options.description } : {}),
    },
    ...(options.servers ? { servers: options.servers } : {}),
    paths: {},
    components: {
      schemas: {
        GraphObject: graphObjectBaseSchema(),
        ObjectEdge: objectEdgeSchema(),
        ResolvedEdge: resolvedEdgeSchema(),
      },
    },
  };

  // ── Per-type component schemas ───────────────────────────────────────────
  if (emitData) {
    for (const def of registry.allDefs()) {
      if (!def.api) continue;
      const typeName = pascal(def.type);
      const fields = registry.getEntityFields(def.type);

      if (fields.length > 0) {
        const dataSchema = {
          type: "object" as const,
          properties: Object.fromEntries(
            fields.map((f) => [f.id, fieldTypeToSchema(f)]),
          ),
          required: fields.filter((f) => f.required).map((f) => f.id),
        };
        doc.components.schemas[`${typeName}Data`] = dataSchema;
        doc.components.schemas[typeName] = {
          allOf: [
            { $ref: "#/components/schemas/GraphObject" },
            {
              type: "object",
              ...(def.description ? { description: def.description } : {}),
              properties: {
                type: { const: def.type },
                data: { $ref: `#/components/schemas/${typeName}Data` },
              },
            },
          ],
        };
      } else {
        doc.components.schemas[typeName] = {
          allOf: [
            { $ref: "#/components/schemas/GraphObject" },
            {
              type: "object",
              ...(def.description ? { description: def.description } : {}),
              properties: { type: { const: def.type } },
            },
          ],
        };
      }
    }
  }

  // ── Paths ───────────────────────────────────────────────────────────────────
  for (const spec of specs) {
    const oaPath = pathToOpenApi(spec.path);
    const item = (doc.paths[oaPath] ?? {}) as Record<string, unknown>;

    const op = buildOperation(spec, registry);
    if (op) {
      item[spec.method.toLowerCase()] = op;
    }
    doc.paths[oaPath] = item;
  }

  return doc;
}

/**
 * Build and serialize an OpenAPI document as a JSON string.
 */
export function generateOpenApiJson(
  specs: RouteSpec[],
  registry: ObjectRegistry,
  options: OpenApiOptions,
): string {
  return JSON.stringify(buildOpenApiDocument(specs, registry, options), null, 2);
}

// ── Helpers ─────────────────────────────────────────────────────────────────────

function pathToOpenApi(routePath: string): string {
  return routePath.replace(/:([a-zA-Z_][a-zA-Z0-9_]*)/g, "{$1}");
}

function fieldTypeToSchema(
  fieldDef: EntityFieldDef,
): Record<string, unknown> {
  const ft = fieldDef.type as EntityFieldType;
  switch (ft) {
    case "bool":
      return { type: "boolean", description: fieldDef.description };
    case "int":
      return { type: "integer", description: fieldDef.description };
    case "float":
      return { type: "number", description: fieldDef.description };
    case "date":
      return { type: "string", format: "date", description: fieldDef.description };
    case "datetime":
      return { type: "string", format: "date-time", description: fieldDef.description };
    case "url":
      return { type: "string", format: "uri", description: fieldDef.description };
    case "object_ref":
      return { type: "string", description: `${fieldDef.description ?? ""} (ObjectId ref)`.trim() };
    case "enum":
      if (fieldDef.enumOptions && fieldDef.enumOptions.length > 0) {
        return { type: "string", enum: fieldDef.enumOptions.map((o) => o.value), description: fieldDef.description };
      }
      return { type: "string", description: fieldDef.description };
    default:
      return { type: "string", description: fieldDef.description };
  }
}

function graphObjectBaseSchema(): Record<string, unknown> {
  return {
    type: "object",
    required: ["id", "type", "name", "parentId", "position", "tags", "description", "pinned", "data", "createdAt", "updatedAt"],
    properties: {
      id: { type: "string", description: "ObjectId (UUID)" },
      type: { type: "string", description: "Registered entity type string" },
      name: { type: "string" },
      parentId: { type: "string", description: "Parent ObjectId, or null for root" },
      position: { type: "number", description: "Sort order among siblings" },
      status: { type: "string" },
      tags: { type: "array", items: { type: "string" } },
      date: { type: "string", format: "date" },
      endDate: { type: "string", format: "date" },
      description: { type: "string" },
      color: { type: "string" },
      image: { type: "string" },
      pinned: { type: "boolean" },
      data: { type: "object", additionalProperties: true, description: "Type-specific payload" },
      createdAt: { type: "string", format: "date-time" },
      updatedAt: { type: "string", format: "date-time" },
      deletedAt: { type: "string", format: "date-time" },
    },
  };
}

function objectEdgeSchema(): Record<string, unknown> {
  return {
    type: "object",
    required: ["id", "sourceId", "targetId", "relation", "createdAt", "data"],
    properties: {
      id: { type: "string", description: "EdgeId (UUID)" },
      sourceId: { type: "string" },
      targetId: { type: "string" },
      relation: { type: "string", description: "Registered edge type string" },
      position: { type: "number" },
      createdAt: { type: "string", format: "date-time" },
      data: { type: "object", additionalProperties: true },
    },
  };
}

function resolvedEdgeSchema(): Record<string, unknown> {
  return {
    allOf: [
      { $ref: "#/components/schemas/ObjectEdge" },
      {
        type: "object",
        required: ["target"],
        properties: {
          target: { $ref: "#/components/schemas/GraphObject" },
          via: {
            type: "object",
            properties: {
              id: { type: "string" },
              name: { type: "string" },
              type: { type: "string" },
            },
          },
        },
      },
    ],
  };
}

const ID_PARAM = {
  name: "id",
  in: "path",
  required: true,
  schema: { type: "string" },
  description: "Object ID (UUID)",
};

function buildOperation(
  spec: RouteSpec,
  registry: ObjectRegistry,
): Record<string, unknown> | null {
  switch (spec.operation) {
    case "list":
      return listOp(spec, registry);
    case "get":
      return getOp(spec, registry);
    case "create":
      return createOp(spec, registry);
    case "update":
      return updateOp(spec, registry);
    case "delete":
      return deleteOp(spec, registry);
    case "restore":
      return restoreOp(spec, registry);
    case "move":
      return moveOp(spec, registry);
    case "duplicate":
      return duplicateOp(spec, registry);
    case "edges-list":
      return edgeListOp();
    case "edges-create":
      return edgeCreateOp();
    case "edges-get":
      return edgeGetOp();
    case "edges-update":
      return edgeUpdateOp();
    case "edges-delete":
      return edgeDeleteOp();
    case "related":
      return relatedOp();
    default:
      return null;
  }
}

function typeMeta(spec: RouteSpec, registry: ObjectRegistry) {
  const def = spec.typeDef;
  const typeName = def ? pascal(def.type) : "GraphObject";
  const label = def ? registry.getLabel(def.type) : "object";
  const tag = def ? registry.getLabel(def.type) : "Objects";
  const pathStr = def?.api?.path ?? def?.type ?? "objects";
  return { def, typeName, label, tag, pathStr };
}

function listOp(spec: RouteSpec, registry: ObjectRegistry) {
  const { typeName, tag, pathStr } = typeMeta(spec, registry);
  const label = spec.typeDef ? registry.getPluralLabel(spec.typeDef.type) : "objects";
  const filterBy = spec.meta.filterBy ?? ["type", "parentId", "status", "tags", "date", "search"];
  const params = [
    { name: "search", in: "query", schema: { type: "string" }, description: "Search by name" },
    { name: "limit", in: "query", schema: { type: "integer" }, description: "Max results" },
    { name: "offset", in: "query", schema: { type: "integer" }, description: "Pagination offset" },
  ];
  for (const field of filterBy) {
    if (field === "search") continue;
    params.push({ name: field, in: "query", schema: { type: "string" }, description: `Filter by ${field}` });
  }
  return {
    operationId: spec.typeDef ? `list${pascal(pathStr)}` : "listObjects",
    summary: `List ${label}`,
    tags: [tag],
    parameters: params,
    responses: {
      "200": {
        description: `Array of ${label}`,
        content: { "application/json": { schema: { type: "array", items: { $ref: `#/components/schemas/${typeName}` } } } },
      },
    },
  };
}

function getOp(spec: RouteSpec, registry: ObjectRegistry) {
  const { typeName, label, tag, pathStr } = typeMeta(spec, registry);
  return {
    operationId: spec.typeDef ? `get${pascal(singular(pathStr))}` : "getObject",
    summary: `Get ${label} by ID`,
    tags: [tag],
    parameters: [ID_PARAM],
    responses: {
      "200": { description: label, content: { "application/json": { schema: { $ref: `#/components/schemas/${typeName}` } } } },
      "404": { description: "Not found" },
    },
  };
}

function createOp(spec: RouteSpec, registry: ObjectRegistry) {
  const { typeName, label, tag, pathStr } = typeMeta(spec, registry);
  return {
    operationId: spec.typeDef ? `create${pascal(singular(pathStr))}` : "createObject",
    summary: `Create ${label}`,
    tags: [tag],
    requestBody: { required: true, content: { "application/json": { schema: { $ref: `#/components/schemas/${typeName}` } } } },
    responses: {
      "201": { description: `Created ${label}`, content: { "application/json": { schema: { $ref: `#/components/schemas/${typeName}` } } } },
      "422": { description: "Validation error" },
    },
  };
}

function updateOp(spec: RouteSpec, registry: ObjectRegistry) {
  const { typeName, label, tag, pathStr } = typeMeta(spec, registry);
  return {
    operationId: spec.typeDef ? `update${pascal(singular(pathStr))}` : "updateObject",
    summary: `Update ${label}`,
    tags: [tag],
    parameters: [ID_PARAM],
    requestBody: { required: true, content: { "application/json": { schema: { $ref: `#/components/schemas/${typeName}` } } } },
    responses: {
      "200": { description: `Updated ${label}`, content: { "application/json": { schema: { $ref: `#/components/schemas/${typeName}` } } } },
      "404": { description: "Not found" },
    },
  };
}

function deleteOp(spec: RouteSpec, _registry: ObjectRegistry) {
  const def = spec.typeDef;
  const label = def ? _registry.getLabel(def.type) : "object";
  const tag = def ? _registry.getLabel(def.type) : "Objects";
  const pathStr = def?.api?.path ?? def?.type ?? "objects";
  const isSoft = spec.meta.softDelete ?? true;
  return {
    operationId: def ? `delete${pascal(singular(pathStr))}` : "deleteObject",
    summary: `Delete ${label}${isSoft ? " (soft)" : ""}`,
    tags: [tag],
    parameters: [ID_PARAM],
    responses: {
      "204": { description: isSoft ? `${label} soft-deleted` : `${label} deleted` },
      "404": { description: "Not found" },
    },
  };
}

function restoreOp(spec: RouteSpec, registry: ObjectRegistry) {
  const { typeName, label, tag, pathStr } = typeMeta(spec, registry);
  return {
    operationId: spec.typeDef ? `restore${pascal(singular(pathStr))}` : "restoreObject",
    summary: `Restore deleted ${label}`,
    tags: [tag],
    parameters: [ID_PARAM],
    responses: {
      "200": { description: `Restored ${label}`, content: { "application/json": { schema: { $ref: `#/components/schemas/${typeName}` } } } },
      "404": { description: "Not found" },
    },
  };
}

function moveOp(spec: RouteSpec, registry: ObjectRegistry) {
  const { typeName, label, tag, pathStr } = typeMeta(spec, registry);
  return {
    operationId: spec.typeDef ? `move${pascal(singular(pathStr))}` : "moveObject",
    summary: `Move ${label} to a new parent`,
    tags: [tag],
    parameters: [ID_PARAM],
    requestBody: {
      required: true,
      content: {
        "application/json": {
          schema: {
            type: "object",
            required: ["parentId"],
            properties: {
              parentId: { type: "string", description: "Target parent ObjectId" },
              position: { type: "number", description: "Sort position" },
            },
          },
        },
      },
    },
    responses: {
      "200": { description: `Moved ${label}`, content: { "application/json": { schema: { $ref: `#/components/schemas/${typeName}` } } } },
      "422": { description: "Invalid parentId" },
    },
  };
}

function duplicateOp(spec: RouteSpec, registry: ObjectRegistry) {
  const { typeName, label, tag, pathStr } = typeMeta(spec, registry);
  return {
    operationId: spec.typeDef ? `duplicate${pascal(singular(pathStr))}` : "duplicateObject",
    summary: `Duplicate ${label}`,
    tags: [tag],
    parameters: [ID_PARAM],
    requestBody: {
      required: false,
      content: {
        "application/json": {
          schema: {
            type: "object",
            properties: {
              deep: { type: "boolean", description: "Recursively duplicate children" },
              targetParentId: { type: "string", description: "Parent for the copy" },
              copyEdges: { type: "boolean", description: "Copy edges from original" },
            },
          },
        },
      },
    },
    responses: {
      "201": { description: "Created copies", content: { "application/json": { schema: { type: "array", items: { $ref: `#/components/schemas/${typeName}` } } } } },
    },
  };
}

function edgeListOp() {
  return {
    operationId: "listEdges", summary: "List edges", tags: ["Edges"],
    parameters: [
      { name: "sourceId", in: "query", schema: { type: "string" } },
      { name: "targetId", in: "query", schema: { type: "string" } },
      { name: "relation", in: "query", schema: { type: "string" } },
    ],
    responses: { "200": { description: "Array of edges", content: { "application/json": { schema: { type: "array", items: { $ref: "#/components/schemas/ObjectEdge" } } } } } },
  };
}

function edgeCreateOp() {
  return {
    operationId: "createEdge", summary: "Create edge", tags: ["Edges"],
    requestBody: { required: true, content: { "application/json": { schema: { $ref: "#/components/schemas/ObjectEdge" } } } },
    responses: { "201": { description: "Created edge", content: { "application/json": { schema: { $ref: "#/components/schemas/ObjectEdge" } } } } },
  };
}

function edgeGetOp() {
  return {
    operationId: "getEdge", summary: "Get edge by ID", tags: ["Edges"],
    parameters: [{ name: "id", in: "path", required: true, schema: { type: "string" } }],
    responses: { "200": { description: "Edge", content: { "application/json": { schema: { $ref: "#/components/schemas/ObjectEdge" } } } }, "404": { description: "Not found" } },
  };
}

function edgeUpdateOp() {
  return {
    operationId: "updateEdge", summary: "Update edge", tags: ["Edges"],
    parameters: [{ name: "id", in: "path", required: true, schema: { type: "string" } }],
    requestBody: { required: true, content: { "application/json": { schema: { $ref: "#/components/schemas/ObjectEdge" } } } },
    responses: { "200": { description: "Updated edge", content: { "application/json": { schema: { $ref: "#/components/schemas/ObjectEdge" } } } } },
  };
}

function edgeDeleteOp() {
  return {
    operationId: "deleteEdge", summary: "Delete edge", tags: ["Edges"],
    parameters: [{ name: "id", in: "path", required: true, schema: { type: "string" } }],
    responses: { "204": { description: "Deleted" }, "404": { description: "Not found" } },
  };
}

function relatedOp() {
  return {
    operationId: "getRelated", summary: "Get related objects (resolved edges)", tags: ["Edges"],
    parameters: [
      { name: "id", in: "path", required: true, schema: { type: "string" } },
      { name: "relation", in: "query", schema: { type: "string" }, description: "Filter by relation type" },
    ],
    responses: { "200": { description: "Resolved edges with target objects", content: { "application/json": { schema: { type: "array", items: { $ref: "#/components/schemas/ResolvedEdge" } } } } } },
  };
}
