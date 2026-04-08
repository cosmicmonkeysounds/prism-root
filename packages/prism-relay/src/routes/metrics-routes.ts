/**
 * /metrics — Prometheus exposition endpoint.
 *
 * Refreshes live process gauges (modules, peers, federation peers, uptime,
 * WebSocket connections) on each scrape, then renders the Prometheus text
 * exposition format. The route is mounted *before* CSRF and rate limiting so
 * Prometheus servers can scrape without ceremony, but it remains a plain GET.
 */

import { Hono } from "hono";
import type { RelayInstance, RelayRouter, FederationRegistry } from "@prism/core/relay";
import { RELAY_CAPABILITIES } from "@prism/core/relay";
import type { MetricsRegistry } from "../middleware/metrics.js";

export interface MetricsRoutesOptions {
  /** Relay instance — used to refresh gauges on each scrape. */
  relay: RelayInstance;
  /** Metrics registry shared with the request middleware. */
  registry: MetricsRegistry;
  /** Returns the current count of open WebSocket connections. */
  websocketCount?: () => number;
  /** Process start time in milliseconds since epoch (defaults to module-load time). */
  startedAt?: number;
}

export function createMetricsRoutes(options: MetricsRoutesOptions): Hono {
  const { relay, registry, websocketCount } = options;
  const startedAt = options.startedAt ?? Date.now();
  const app = new Hono();

  app.get("/", (c) => {
    refreshGauges(relay, registry, websocketCount, startedAt);
    return c.body(registry.exposition(), 200, {
      "Content-Type": "text/plain; version=0.0.4; charset=utf-8",
      // Prometheus scrapes do not need caching.
      "Cache-Control": "no-store",
    });
  });

  return app;
}

function refreshGauges(
  relay: RelayInstance,
  registry: MetricsRegistry,
  websocketCount: (() => number) | undefined,
  startedAt: number,
): void {
  const router = relay.getCapability<RelayRouter>(RELAY_CAPABILITIES.ROUTER);
  const federation = relay.getCapability<FederationRegistry>(RELAY_CAPABILITIES.FEDERATION);

  registry.setGauge("relay_modules_total", relay.modules.length);
  registry.setGauge("relay_peers_online", router?.onlinePeers().length ?? 0);
  registry.setGauge("relay_federation_peers", federation?.getPeers().length ?? 0);
  registry.setGauge("relay_uptime_seconds", Math.max(0, (Date.now() - startedAt) / 1000));
  if (websocketCount) {
    registry.setGauge("relay_websocket_connections", websocketCount());
  }
}
