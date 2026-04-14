/**
 * Admin routes unit tests.
 *
 * Tests the admin dashboard HTML page and JSON snapshot endpoint
 * without starting a full relay server.
 */

import { describe, expect, it, beforeAll } from "vitest";
import { createIdentity } from "@prism/core/identity";
import {
  createRelayBuilder,
  blindMailboxModule,
  relayRouterModule,
  sovereignPortalModule,
  collectionHostModule,
} from "@prism/core/relay";
import type { RelayInstance } from "@prism/core/relay";
import { createAdminRoutes } from "./admin-routes.js";

let relay: RelayInstance;

beforeAll(async () => {
  const identity = await createIdentity({ method: "key" });
  relay = createRelayBuilder({ relayDid: identity.did })
    .use(blindMailboxModule())
    .use(relayRouterModule())
    .use(sovereignPortalModule())
    .use(collectionHostModule())
    .build();
  await relay.start();
});

describe("admin routes", () => {
  it("GET /api/snapshot returns a valid AdminSnapshot JSON", async () => {
    const app = createAdminRoutes({ relay, startedAt: Date.now() - 60_000 });
    const res = await app.request("/api/snapshot");
    expect(res.status).toBe(200);

    const snap = await res.json();
    expect(snap.sourceId).toContain("relay:");
    expect(snap.sourceLabel).toContain("Relay");
    expect(snap.health.level).toBe("ok");
    expect(snap.health.label).toBe("Healthy");
    expect(snap.uptimeSeconds).toBeGreaterThanOrEqual(59);
    expect(snap.capturedAt).toBeTruthy();

    // Should list metrics
    expect(Array.isArray(snap.metrics)).toBe(true);
    const moduleMetric = snap.metrics.find((m: { id: string }) => m.id === "modules");
    expect(moduleMetric).toBeTruthy();
    expect(moduleMetric.value).toBeGreaterThanOrEqual(4);

    // Should list services (one per module)
    expect(Array.isArray(snap.services)).toBe(true);
    expect(snap.services.length).toBeGreaterThanOrEqual(4);
    expect(snap.services.every((s: { health: string }) => s.health === "ok")).toBe(true);
  });

  it("GET / returns an HTML admin dashboard", async () => {
    const app = createAdminRoutes({ relay, startedAt: Date.now() - 5000 });
    const res = await app.request("/");
    expect(res.status).toBe(200);

    const ct = res.headers.get("content-type");
    expect(ct).toContain("text/html");

    const html = await res.text();
    expect(html).toContain("<!DOCTYPE html>");
    expect(html).toContain("Prism Relay Admin");
    expect(html).toContain("/admin/api/snapshot");
    expect(html).toContain("admin-shell");
    // Should embed an initial snapshot as seed data
    expect(html).toContain("Relay");
  });

  it("snapshot contains memory metrics", async () => {
    const app = createAdminRoutes({ relay });
    const res = await app.request("/api/snapshot");
    const snap = await res.json();

    const rssMetric = snap.metrics.find((m: { id: string }) => m.id === "memory-rss");
    expect(rssMetric).toBeTruthy();
    expect(typeof rssMetric.value).toBe("number");
    expect(rssMetric.unit).toBe("MB");

    const heapMetric = snap.metrics.find((m: { id: string }) => m.id === "memory-heap");
    expect(heapMetric).toBeTruthy();
  });

  it("snapshot includes portal and collection counts", async () => {
    const app = createAdminRoutes({ relay });
    const res = await app.request("/api/snapshot");
    const snap = await res.json();

    const portals = snap.metrics.find((m: { id: string }) => m.id === "portals");
    expect(portals).toBeTruthy();
    expect(typeof portals.value).toBe("number");

    const collections = snap.metrics.find((m: { id: string }) => m.id === "collections");
    expect(collections).toBeTruthy();
    expect(typeof collections.value).toBe("number");
  });

  it("respects custom pollMs in HTML", async () => {
    const app = createAdminRoutes({ relay, pollMs: 15000 });
    const res = await app.request("/");
    const html = await res.text();
    expect(html).toContain("15000");
  });
});
