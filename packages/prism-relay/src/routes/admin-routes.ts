/**
 * Admin dashboard routes for Prism Relay.
 *
 * Mounts at `/admin`:
 *   GET  /admin              — self-contained HTML dashboard
 *   GET  /admin/api/snapshot — JSON AdminSnapshot for live polling
 *
 * The HTML page uses `@prism/admin-kit/html` to render a dashboard that
 * auto-refreshes by polling the JSON endpoint. No React, no Puck — just
 * a standalone page served by the relay while it runs.
 */

import { Hono } from "hono";
import type {
  RelayInstance,
  RelayRouter,
  FederationRegistry,
  CollectionHost,
  PortalRegistry,
} from "@prism/core/relay";
import { RELAY_CAPABILITIES } from "@prism/core/relay";
import { renderAdminHtml } from "@prism/admin-kit/html";
import type { AdminSnapshot, HealthLevel, Metric, Service } from "@prism/admin-kit/types.js";

export interface AdminRoutesOptions {
  relay: RelayInstance;
  /** Process start time — used to compute uptime. */
  startedAt?: number;
  /** Poll interval for the HTML dashboard (ms). Default 5000. */
  pollMs?: number;
}

export function createAdminRoutes(options: AdminRoutesOptions): Hono {
  const { relay, startedAt = Date.now(), pollMs = 5000 } = options;
  const app = new Hono();

  function buildSnapshot(): AdminSnapshot {
    const router = relay.getCapability<RelayRouter>(RELAY_CAPABILITIES.ROUTER);
    const federation = relay.getCapability<FederationRegistry>(RELAY_CAPABILITIES.FEDERATION);
    const collections = relay.getCapability<CollectionHost>(RELAY_CAPABILITIES.COLLECTIONS);
    const portals = relay.getCapability<PortalRegistry>(RELAY_CAPABILITIES.PORTALS);

    const uptimeMs = Date.now() - startedAt;
    const uptimeSeconds = Math.floor(uptimeMs / 1000);
    const mem = process.memoryUsage();

    const onlinePeers = router?.onlinePeers().length ?? 0;
    const fedPeers = federation?.getPeers().length ?? 0;
    const collectionCount = collections?.list().length ?? 0;
    const portalCount = portals?.list().length ?? 0;

    const health: HealthLevel = relay.running ? "ok" : "error";

    const metrics: Metric[] = [
      { id: "modules", label: "Modules", value: relay.modules.length },
      { id: "peers", label: "Online Peers", value: onlinePeers },
      { id: "federation", label: "Federation Peers", value: fedPeers },
      { id: "collections", label: "Collections", value: collectionCount },
      { id: "portals", label: "Portals", value: portalCount },
      { id: "memory-rss", label: "Memory (RSS)", value: Math.round(mem.rss / (1024 * 1024) * 10) / 10, unit: "MB" },
      { id: "memory-heap", label: "Heap Used", value: Math.round(mem.heapUsed / (1024 * 1024) * 10) / 10, unit: "MB" },
    ];

    const services: Service[] = relay.modules.map((name) => ({
      id: name,
      name,
      health: "ok" as HealthLevel,
      status: "loaded",
    }));

    return {
      sourceId: `relay:${relay.did}`,
      sourceLabel: `Relay (${relay.did.slice(0, 16)}...)`,
      capturedAt: new Date().toISOString(),
      health: {
        level: health,
        label: relay.running ? "Healthy" : "Stopped",
        detail: relay.running ? `${relay.modules.length} modules loaded` : "Relay is not running",
      },
      uptimeSeconds,
      metrics,
      services,
      activity: [],
    };
  }

  // ── JSON API ──────────────────────────────────────────────────────────
  app.get("/api/snapshot", (c) => {
    return c.json(buildSnapshot());
  });

  // ── HTML dashboard ────────────────────────────────────────────────────
  app.get("/", (c) => {
    const snap = buildSnapshot();
    const html = renderAdminHtml({
      title: "Prism Relay Admin",
      runtimeLabel: "Relay",
      snapshotUrl: "/admin/api/snapshot",
      pollMs,
      initialSnapshot: snap,
    });
    return c.html(html);
  });

  return app;
}
