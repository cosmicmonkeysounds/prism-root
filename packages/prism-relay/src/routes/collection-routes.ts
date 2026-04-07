import { Hono } from "hono";
import type { RelayInstance, CollectionHost, FederationRegistry } from "@prism/core/relay";
import { RELAY_CAPABILITIES } from "@prism/core/relay";
import { encodeBase64, decodeBase64 } from "../protocol/relay-protocol.js";

/**
 * Push a collection snapshot to all federation peers.
 * Fire-and-forget: failures are logged but never block the caller.
 */
function pushSnapshotToPeers(
  relay: RelayInstance,
  collectionId: string,
  snapshotBase64: string,
): void {
  const federation = relay.getCapability<FederationRegistry>(RELAY_CAPABILITIES.FEDERATION);
  if (!federation) return;

  const peers = federation.getPeers();
  for (const peer of peers) {
    fetch(`${peer.url}/api/federation/sync`, {
      method: "POST",
      headers: { "Content-Type": "application/json", "X-Prism-CSRF": "1" },
      body: JSON.stringify({ collectionId, snapshot: snapshotBase64 }),
    }).catch(() => {
      // Peer unreachable — silently ignore for resilience
    });
  }
}

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
    const collectionId = c.req.param("id");
    const store = host().get(collectionId);
    if (!store) return c.json({ error: "collection not found" }, 404);
    const body = await c.req.json<{ data: string }>();
    store.import(decodeBase64(body.data));

    // After import, push the full snapshot to federation peers (async, fire-and-forget)
    const updatedSnapshot = encodeBase64(store.exportSnapshot());
    pushSnapshotToPeers(relay, collectionId, updatedSnapshot);

    return c.json({ ok: true });
  });

  app.delete("/:id", (c) => {
    const ok = host().remove(c.req.param("id"));
    if (!ok) return c.json({ error: "collection not found" }, 404);
    return c.json({ ok: true });
  });

  return app;
}
