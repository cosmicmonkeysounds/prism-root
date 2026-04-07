import { Hono } from "hono";
import type { RelayInstance, FederationRegistry, CollectionHost } from "@prism/core/relay";
import type { DID } from "@prism/core/identity";
import { RELAY_CAPABILITIES } from "@prism/core/relay";
import type { SerializedEnvelope } from "../protocol/relay-protocol.js";
import { deserializeEnvelope, decodeBase64 } from "../protocol/relay-protocol.js";

export function createFederationRoutes(relay: RelayInstance): Hono {
  const app = new Hono();

  function registry(): FederationRegistry {
    return relay.getCapability<FederationRegistry>(RELAY_CAPABILITIES.FEDERATION) as FederationRegistry;
  }

  app.use("/*", async (c, next) => {
    if (!relay.getCapability(RELAY_CAPABILITIES.FEDERATION)) {
      return c.json({ error: "federation module not installed" }, 404);
    }
    await next();
  });

  app.post("/announce", async (c) => {
    const body = await c.req.json<{ relayDid: DID; url: string }>();
    registry().announce(body.relayDid, body.url);
    return c.json({ ok: true });
  });

  app.get("/peers", (c) => {
    return c.json(registry().getPeers());
  });

  app.post("/forward", async (c) => {
    const body = await c.req.json<{ envelope: SerializedEnvelope; targetRelay: DID }>();
    const envelope = deserializeEnvelope(body.envelope);
    const result = await registry().forwardEnvelope(envelope, body.targetRelay);
    return c.json(result);
  });

  /**
   * Receive a CRDT collection snapshot from a federation peer.
   * Creates the collection locally if it doesn't exist, then imports
   * the snapshot — Loro CRDT merge handles convergence.
   */
  app.post("/sync", async (c) => {
    const collections = relay.getCapability<CollectionHost>(RELAY_CAPABILITIES.COLLECTIONS);
    if (!collections) {
      return c.json({ error: "collections module not installed" }, 404);
    }
    const body = await c.req.json<{ collectionId: string; snapshot: string }>();
    if (!body.collectionId || !body.snapshot) {
      return c.json({ error: "collectionId and snapshot are required" }, 400);
    }
    const store = collections.get(body.collectionId) ?? collections.create(body.collectionId);
    store.import(decodeBase64(body.snapshot));
    return c.json({ ok: true, collectionId: body.collectionId });
  });

  return app;
}
