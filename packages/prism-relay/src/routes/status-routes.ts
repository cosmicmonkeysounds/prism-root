import { Hono } from "hono";
import type { RelayInstance, RelayRouter } from "@prism/core/relay";
import { RELAY_CAPABILITIES } from "@prism/core/relay";

export function createStatusRoutes(relay: RelayInstance): Hono {
  const app = new Hono();

  app.get("/status", (c) => {
    const router = relay.getCapability<RelayRouter>(RELAY_CAPABILITIES.ROUTER);
    return c.json({
      running: relay.running,
      did: relay.did,
      modules: relay.modules,
      peers: router?.onlinePeers() ?? [],
    });
  });

  app.get("/modules", (c) => {
    return c.json(
      relay.modules.map((name) => ({ name })),
    );
  });

  return app;
}
