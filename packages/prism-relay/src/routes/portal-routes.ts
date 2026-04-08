import { Hono } from "hono";
import type {
  RelayInstance,
  PortalRegistry,
  PortalLevel,
  CollectionHost,
} from "@prism/core/relay";
import { RELAY_CAPABILITIES } from "@prism/core/relay";
import { encodeBase64 } from "../protocol/relay-protocol.js";

export function createPortalRoutes(relay: RelayInstance): Hono {
  const app = new Hono();

  function registry(): PortalRegistry {
    return relay.getCapability<PortalRegistry>(RELAY_CAPABILITIES.PORTALS) as PortalRegistry;
  }

  app.use("/*", async (c, next) => {
    if (!relay.getCapability(RELAY_CAPABILITIES.PORTALS)) {
      return c.json({ error: "portals module not installed" }, 404);
    }
    await next();
  });

  app.get("/", (c) => {
    return c.json(registry().list());
  });

  app.post("/", async (c) => {
    const body = await c.req.json<{
      name: string;
      level: PortalLevel;
      collectionId: string;
      domain?: string;
      basePath: string;
      isPublic: boolean;
      accessScope?: string;
    }>();
    const manifest = registry().register(body);
    return c.json(manifest, 201);
  });

  app.get("/:id", (c) => {
    const portal = registry().get(c.req.param("id"));
    if (!portal) return c.json({ error: "portal not found" }, 404);
    return c.json(portal);
  });

  app.delete("/:id", (c) => {
    const ok = registry().unregister(c.req.param("id"));
    if (!ok) return c.json({ error: "portal not found" }, 404);
    return c.json({ ok: true });
  });

  // ── Template export ────────────────────────────────────────────────────
  //
  // GET /api/portals/:id/export
  // Bundles the portal manifest + its backing collection snapshot into a
  // single downloadable JSON document that can be re-imported elsewhere
  // ("Use as Template"). Shape:
  //   {
  //     version: 1,
  //     exportedAt: ISO timestamp,
  //     portal: PortalManifest,
  //     collection: { id, snapshot: base64 } | null
  //   }
  // Collection body is only included when the collection host module is
  // installed and the backing collection exists.
  app.get("/:id/export", (c) => {
    const portal = registry().get(c.req.param("id"));
    if (!portal) return c.json({ error: "portal not found" }, 404);

    let collection: { id: string; snapshot: string } | null = null;
    const host = relay.getCapability<CollectionHost>(RELAY_CAPABILITIES.COLLECTIONS);
    if (host) {
      const store = host.get(portal.collectionId);
      if (store) {
        collection = {
          id: portal.collectionId,
          snapshot: encodeBase64(store.exportSnapshot()),
        };
      }
    }

    const bundle = {
      version: 1,
      exportedAt: new Date().toISOString(),
      portal,
      collection,
    };

    const filename = `${portal.name.replace(/[^a-z0-9-]+/gi, "-").toLowerCase()}-template.json`;
    return c.body(JSON.stringify(bundle, null, 2), 200, {
      "Content-Type": "application/json",
      "Content-Disposition": `attachment; filename="${filename}"`,
    });
  });

  return app;
}
