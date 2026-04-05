/**
 * @prism/core — Auto-REST route generator
 *
 * Reads ObjectRegistry.allDefs() and generates RouteSpec[] that any HTTP
 * framework adapter (Hono, Express, Fastify, Next.js) can consume.
 *
 * This file is framework-agnostic. No imports from any HTTP library.
 *
 * Generated routes per type (when api config present):
 *   list      GET    /api/{path}
 *   get       GET    /api/{path}/:id
 *   create    POST   /api/{path}
 *   update    PUT    /api/{path}/:id
 *   delete    DELETE /api/{path}/:id
 *   restore   POST   /api/{path}/:id/restore
 *   move      POST   /api/{path}/:id/move
 *   duplicate POST   /api/{path}/:id/duplicate
 *
 * Edge routes:
 *   GET/POST/PUT/DELETE /api/edges[/:id]
 *   GET /api/objects/:id/related
 *
 * Usage:
 *   const specs = generateRouteSpecs(registry);
 *   for (const spec of specs) {
 *     app[spec.method.toLowerCase()](spec.path, myHandlerFactory(spec));
 *   }
 */

import type {
  EntityDef,
  ApiOperation,
} from "../object-model/types.js";
import type { ObjectRegistry } from "../object-model/registry.js";

// ── Route spec ──────────────────────────────────────────────────────────────────

export type HttpMethod = "GET" | "POST" | "PUT" | "PATCH" | "DELETE";

export type RouteOperation =
  | ApiOperation
  | "edges-list"
  | "edges-get"
  | "edges-create"
  | "edges-update"
  | "edges-delete"
  | "related";

export interface RouteSpec {
  method: HttpMethod;
  /** Full path including prefix, e.g. '/api/tasks/:id' */
  path: string;
  operation: RouteOperation;
  /** The EntityDef this route belongs to (null for edge/global routes) */
  typeDef: EntityDef | null;
  /** Route-level metadata extracted from typeDef.api */
  meta: {
    type?: string;
    path: string;
    softDelete?: boolean;
    cascadeEdges?: boolean;
    filterBy?: string[] | undefined;
    defaultSort?: { field: string; dir: "asc" | "desc" } | undefined;
    hooks?: Record<string, string> | undefined;
  };
}

// ── Generator options ───────────────────────────────────────────────────────────

export interface RouteGenOptions {
  /** URL prefix for all routes. Default: '/api' */
  prefix?: string;
  /** Generate edge routes. Default: true */
  includeEdgeRoutes?: boolean;
  /** Generate global object search route. Default: true */
  includeObjectSearch?: boolean;
}

// ── generateRouteSpecs ──────────────────────────────────────────────────────────

/**
 * Generate RouteSpec[] from a populated ObjectRegistry.
 * Only types with an `api` config are included.
 */
export function generateRouteSpecs(
  registry: ObjectRegistry,
  options: RouteGenOptions = {},
): RouteSpec[] {
  const prefix = options.prefix ?? "/api";
  const edgeRoutes = options.includeEdgeRoutes ?? true;
  const objSearch = options.includeObjectSearch ?? true;
  const specs: RouteSpec[] = [];

  for (const def of registry.allDefs()) {
    if (!def.api) continue;
    const api = def.api;
    const typePath = api.path ?? def.type;
    const operations = api.operations ?? [
      "list",
      "get",
      "create",
      "update",
      "delete",
    ];
    const base = `${prefix}/${typePath}`;
    const meta = {
      type: def.type,
      path: typePath,
      softDelete: api.softDelete ?? true,
      cascadeEdges: api.cascadeEdges ?? true,
      filterBy: api.filterBy,
      defaultSort: api.defaultSort,
      hooks: api.hooks,
    };

    if (operations.includes("list"))
      specs.push({ method: "GET", path: base, operation: "list", typeDef: def, meta });
    if (operations.includes("get"))
      specs.push({ method: "GET", path: `${base}/:id`, operation: "get", typeDef: def, meta });
    if (operations.includes("create"))
      specs.push({ method: "POST", path: base, operation: "create", typeDef: def, meta });
    if (operations.includes("update"))
      specs.push({ method: "PUT", path: `${base}/:id`, operation: "update", typeDef: def, meta });
    if (operations.includes("delete"))
      specs.push({ method: "DELETE", path: `${base}/:id`, operation: "delete", typeDef: def, meta });
    if (operations.includes("restore"))
      specs.push({ method: "POST", path: `${base}/:id/restore`, operation: "restore", typeDef: def, meta });
    if (operations.includes("move"))
      specs.push({ method: "POST", path: `${base}/:id/move`, operation: "move", typeDef: def, meta });
    if (operations.includes("duplicate"))
      specs.push({ method: "POST", path: `${base}/:id/duplicate`, operation: "duplicate", typeDef: def, meta });
  }

  if (objSearch) {
    specs.push({
      method: "GET",
      path: `${prefix}/objects`,
      operation: "list",
      typeDef: null,
      meta: { path: "objects" },
    });
    specs.push({
      method: "GET",
      path: `${prefix}/objects/:id`,
      operation: "get",
      typeDef: null,
      meta: { path: "objects" },
    });
  }

  if (edgeRoutes) {
    const em = { path: "edges" };
    specs.push({ method: "GET", path: `${prefix}/edges`, operation: "edges-list", typeDef: null, meta: em });
    specs.push({ method: "POST", path: `${prefix}/edges`, operation: "edges-create", typeDef: null, meta: em });
    specs.push({ method: "GET", path: `${prefix}/edges/:id`, operation: "edges-get", typeDef: null, meta: em });
    specs.push({ method: "PUT", path: `${prefix}/edges/:id`, operation: "edges-update", typeDef: null, meta: em });
    specs.push({ method: "DELETE", path: `${prefix}/edges/:id`, operation: "edges-delete", typeDef: null, meta: em });
    specs.push({ method: "GET", path: `${prefix}/objects/:id/related`, operation: "related", typeDef: null, meta: em });
  }

  return specs;
}

// ── Route adapter interface ─────────────────────────────────────────────────────

export interface RouteRequest {
  params: Record<string, string>;
  query: Record<string, string | string[]>;
  body: unknown;
  headers: Record<string, string>;
}

export interface RouteResponse {
  status: number;
  body: unknown;
}

export type RouteHandler = (req: RouteRequest) => Promise<RouteResponse>;

export interface RouteAdapter {
  register(spec: RouteSpec, handler: RouteHandler): void;
}

/**
 * Register all routes for a registry using a framework adapter.
 * The handlerFactory supplies the actual CRUD logic.
 */
export function registerRoutes(
  registry: ObjectRegistry,
  adapter: RouteAdapter,
  handlerFactory: (spec: RouteSpec) => RouteHandler,
  options?: RouteGenOptions,
): void {
  const specs = generateRouteSpecs(registry, options);
  for (const spec of specs) {
    adapter.register(spec, handlerFactory(spec));
  }
}

// ── Utilities ───────────────────────────────────────────────────────────────────

/** Group route specs by their typeDef.type (null key = shared routes). */
export function groupByType(
  specs: RouteSpec[],
): Map<string | null, RouteSpec[]> {
  const map = new Map<string | null, RouteSpec[]>();
  for (const spec of specs) {
    const key = spec.typeDef?.type ?? null;
    const arr = map.get(key) ?? [];
    arr.push(spec);
    map.set(key, arr);
  }
  return map;
}

/** Print a route table for debugging / documentation. */
export function printRouteTable(specs: RouteSpec[]): string {
  const rows = specs.map(
    (s) =>
      `${s.method.padEnd(7)} ${s.path.padEnd(50)} (${s.operation})`,
  );
  return rows.join("\n");
}
