// ── Route Generation ─────────────────────────────────────────────────────────
export {
  generateRouteSpecs,
  registerRoutes,
  groupByType,
  printRouteTable,
} from "./route-gen.js";

export type {
  HttpMethod,
  RouteOperation,
  RouteSpec,
  RouteGenOptions,
  RouteRequest,
  RouteResponse,
  RouteHandler,
  RouteAdapter,
} from "./route-gen.js";

// ── OpenAPI ──────────────────────────────────────────────────────────────────
export {
  buildOpenApiDocument,
  generateOpenApiJson,
} from "./openapi.js";

export type { OpenApiOptions } from "./openapi.js";
