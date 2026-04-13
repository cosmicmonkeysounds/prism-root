# server/

Framework-agnostic Server Factory. Turns a populated `ObjectRegistry` into
a set of REST route specs and a complete OpenAPI 3.1 document. Nothing here
speaks Express / Fastify / Hono directly — a `RouteAdapter` is the seam that
binds generated specs to whichever HTTP framework your server embeds. This
is what Relay's AutoREST gateway is built on.

```ts
import { generateRouteSpecs, buildOpenApiDocument } from "@prism/core/server";
```

## Key exports

### Route generation

- `generateRouteSpecs(registry, options?)` — emits `RouteSpec[]` from every
  `def.api`-tagged type in the registry. Covers `list`/`get`/`create`/
  `update`/`delete`/`restore`/`move`/`duplicate`, plus optional global
  `/objects` and `/objects/:id` routes and edge routes.
- `registerRoutes(registry, adapter, handlerFactory, options?)` — generates
  specs for a registry and hands each one to a `RouteAdapter.register(spec,
  handler)`, pulling the handler from a caller-supplied factory.
- `groupByType(specs)` — groups routes by entity type for printing/docs.
- `printRouteTable(specs)` — formatted text table for CLI / debug output.
- `RouteSpec` / `RouteOperation` / `HttpMethod` — generated spec shape.
- `RouteGenOptions` — `{ prefix?, includeEdgeRoutes?, includeObjectSearch? }`.
- `RouteAdapter` / `RouteHandler` / `RouteRequest` / `RouteResponse` — the
  framework-binding seam. `RouteHandler` is `(req) => Promise<RouteResponse>`.

### OpenAPI

- `buildOpenApiDocument(specs, registry, options)` — builds a full
  OpenAPI 3.1.0 document (paths, operations, component schemas) from specs
  and the registry.
- `generateOpenApiJson(...)` — same, serialized to a JSON string.
- `OpenApiOptions` — `{ title, version?, description?, servers?, emitDataSchemas? }`.

## Usage

```ts
import {
  generateRouteSpecs,
  registerRoutes,
  buildOpenApiDocument,
} from "@prism/core/server";

const specs = generateRouteSpecs(registry, { prefix: "/api" });

registerRoutes(
  registry,
  {
    register(spec, handler) {
      app[spec.method.toLowerCase()](spec.path, async (req, res) => {
        const result = await handler({
          params: req.params,
          query: req.query,
          body: req.body,
          headers: req.headers,
        });
        res.status(result.status).json(result.body);
      });
    },
  },
  (spec) => async (req) => runCrud(spec, req),
);

const openapi = buildOpenApiDocument(specs, registry, {
  title: "My Prism Server",
  version: "1.0.0",
});
```

Both helpers are pure — call them at boot (or on schema change) and hand
the result to whichever HTTP framework you host.
