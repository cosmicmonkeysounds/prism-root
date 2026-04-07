import { Hono } from "hono";
import type { RelayInstance, RelayRouter, FederationRegistry } from "@prism/core/relay";
import { RELAY_CAPABILITIES } from "@prism/core/relay";

/** Timestamp when the relay server started. */
let startedAt: number | undefined;

export function createStatusRoutes(relay: RelayInstance): Hono {
  const app = new Hono();
  startedAt = Date.now();

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

  /**
   * Health check endpoint for load balancers, Docker HEALTHCHECK, and `prism-relay status`.
   * Returns 200 if relay is running, 503 if not.
   */
  app.get("/health", (c) => {
    const router = relay.getCapability<RelayRouter>(RELAY_CAPABILITIES.ROUTER);
    const federation = relay.getCapability<FederationRegistry>(RELAY_CAPABILITIES.FEDERATION);
    const uptime = startedAt !== undefined ? Date.now() - startedAt : 0;
    const mem = process.memoryUsage();

    if (!relay.running) {
      return c.json({
        status: "unhealthy",
        uptime,
      }, 503);
    }

    return c.json({
      status: "healthy",
      did: relay.did,
      uptime,
      modules: relay.modules.length,
      peers: router?.onlinePeers().length ?? 0,
      federationPeers: federation?.getPeers().length ?? 0,
      memory: {
        rss: mem.rss,
        heapUsed: mem.heapUsed,
        heapTotal: mem.heapTotal,
      },
    });
  });

  return app;
}
