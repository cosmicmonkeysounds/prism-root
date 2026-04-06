import { Hono } from "hono";
import type { RelayInstance, CollectionHost } from "@prism/core/relay";
import { RELAY_CAPABILITIES } from "@prism/core/relay";
import { encodeBase64, decodeBase64 } from "../protocol/relay-protocol.js";

export function createCollectionRoutes(relay: RelayInstance): Hono {
  const app = new Hono();

  function host(): CollectionHost {
    return relay.getCapability<CollectionHost>(RELAY_CAPABILITIES.COLLECTIONS) as CollectionHost;
  }

  app.use("/*", async (c, next) => {
    if (!relay.getCapability(RELAY_CAPABILITIES.COLLECTIONS)) {
      return c.json({ error: "collections module not installed" }, 404);
    }
    await next();
  });

  app.get("/", (c) => {
    return c.json(host().list());
  });

  app.post("/", async (c) => {
    const body = await c.req.json<{ id: string }>();
    host().create(body.id);
    return c.json({ id: body.id }, 201);
  });

  app.get("/:id/snapshot", (c) => {
    const store = host().get(c.req.param("id"));
    if (!store) return c.json({ error: "collection not found" }, 404);
    const snapshot = store.exportSnapshot();
    return c.json({ snapshot: encodeBase64(snapshot) });
  });

  app.post("/:id/import", async (c) => {
    const store = host().get(c.req.param("id"));
    if (!store) return c.json({ error: "collection not found" }, 404);
    const body = await c.req.json<{ data: string }>();
    store.import(decodeBase64(body.data));
    return c.json({ ok: true });
  });

  app.delete("/:id", (c) => {
    const ok = host().remove(c.req.param("id"));
    if (!ok) return c.json({ error: "collection not found" }, 404);
    return c.json({ ok: true });
  });

  return app;
}
