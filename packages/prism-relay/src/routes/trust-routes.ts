import { Hono } from "hono";
import type { RelayInstance } from "@prism/core/relay";
import { RELAY_CAPABILITIES } from "@prism/core/relay";
import type { PeerTrustGraph } from "@prism/core/trust";

export function createTrustRoutes(relay: RelayInstance): Hono {
  const app = new Hono();

  function graph(): PeerTrustGraph {
    return relay.getCapability<PeerTrustGraph>(RELAY_CAPABILITIES.TRUST) as PeerTrustGraph;
  }

  app.use("/*", async (c, next) => {
    if (!relay.getCapability(RELAY_CAPABILITIES.TRUST)) {
      return c.json({ error: "trust module not installed" }, 404);
    }
    await next();
  });

  app.get("/", (c) => {
    return c.json(graph().allPeers());
  });

  app.get("/:did", (c) => {
    const peer = graph().getPeer(c.req.param("did"));
    if (!peer) return c.json({ error: "peer not found" }, 404);
    return c.json(peer);
  });

  app.post("/:did/ban", async (c) => {
    const body = await c.req.json<{ reason: string }>();
    graph().ban(c.req.param("did"), body.reason);
    return c.json({ ok: true });
  });

  app.post("/:did/unban", (c) => {
    graph().unban(c.req.param("did"));
    return c.json({ ok: true });
  });

  return app;
}
