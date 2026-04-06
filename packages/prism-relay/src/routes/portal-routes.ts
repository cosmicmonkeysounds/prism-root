import { Hono } from "hono";
import type { RelayInstance, PortalRegistry, PortalLevel } from "@prism/core/relay";
import { RELAY_CAPABILITIES } from "@prism/core/relay";

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

  return app;
}
