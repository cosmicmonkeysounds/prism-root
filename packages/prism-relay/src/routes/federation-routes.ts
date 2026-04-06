import { Hono } from "hono";
import type { RelayInstance, FederationRegistry } from "@prism/core/relay";
import type { DID } from "@prism/core/identity";
import { RELAY_CAPABILITIES } from "@prism/core/relay";
import type { SerializedEnvelope } from "../protocol/relay-protocol.js";
import { deserializeEnvelope } from "../protocol/relay-protocol.js";

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

  return app;
}
