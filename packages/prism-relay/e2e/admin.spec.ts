/**
 * Prism Relay Admin E2E tests.
 *
 * Starts a relay server on an ephemeral port and exercises the admin
 * dashboard HTML page and JSON snapshot API.
 */

import { test, expect } from "@playwright/test";
import { createIdentity } from "@prism/core/identity";
import {
  createRelayBuilder,
  blindMailboxModule,
  relayRouterModule,
  sovereignPortalModule,
  collectionHostModule,
  hashcashModule,
  peerTrustModule,
  federationModule,
} from "@prism/core/relay";
import type { RelayInstance } from "@prism/core/relay";
import { createRelayServer } from "@prism/relay/server";

let relay: RelayInstance;
let serverPort: number;
let close: () => Promise<void>;
let baseUrl: string;

test.beforeAll(async () => {
  const identity = await createIdentity({ method: "key" });
  relay = createRelayBuilder({ relayDid: identity.did })
    .use(blindMailboxModule())
    .use(relayRouterModule())
    .use(sovereignPortalModule())
    .use(collectionHostModule())
    .use(hashcashModule({ bits: 4 }))
    .use(peerTrustModule())
    .use(federationModule())
    .build();
  await relay.start();

  const server = createRelayServer({
    relay,
    port: 0,
    publicUrl: "http://localhost:0",
    disableCsrf: true,
  });
  const info = await server.start();
  serverPort = info.port;
  baseUrl = `http://localhost:${serverPort}`;
  close = info.close;
});

test.afterAll(async () => {
  await close();
  await relay.stop();
});

test.describe("Relay Admin Dashboard", () => {
  test("GET /admin returns an HTML page with admin dashboard", async ({ request }) => {
    const res = await request.get(`${baseUrl}/admin`);
    expect(res.status()).toBe(200);

    const contentType = res.headers()["content-type"];
    expect(contentType).toContain("text/html");

    const html = await res.text();
    expect(html).toContain("<!DOCTYPE html>");
    expect(html).toContain("Prism Relay Admin");
    expect(html).toContain("admin-shell");
    expect(html).toContain("/admin/api/snapshot");
    // Should contain embedded seed data
    expect(html).toContain("Relay");
  });

  test("GET /admin/api/snapshot returns a valid AdminSnapshot JSON", async ({ request }) => {
    const res = await request.get(`${baseUrl}/admin/api/snapshot`);
    expect(res.status()).toBe(200);

    const snap = await res.json();

    // Core snapshot fields
    expect(snap.sourceId).toBeTruthy();
    expect(snap.sourceLabel).toContain("Relay");
    expect(snap.capturedAt).toBeTruthy();
    expect(snap.health.level).toBe("ok");
    expect(snap.health.label).toBe("Healthy");
    expect(snap.uptimeSeconds).toBeGreaterThanOrEqual(0);

    // Metrics
    expect(Array.isArray(snap.metrics)).toBe(true);
    const moduleMetric = snap.metrics.find((m: { id: string }) => m.id === "modules");
    expect(moduleMetric).toBeTruthy();
    expect(moduleMetric.value).toBeGreaterThanOrEqual(7);

    // Memory metrics
    const rssMetric = snap.metrics.find((m: { id: string }) => m.id === "memory-rss");
    expect(rssMetric).toBeTruthy();
    expect(typeof rssMetric.value).toBe("number");
    expect(rssMetric.unit).toBe("MB");

    // Services — one per installed module
    expect(Array.isArray(snap.services)).toBe(true);
    expect(snap.services.length).toBeGreaterThanOrEqual(7);
    const moduleNames = snap.services.map((s: { name: string }) => s.name);
    expect(moduleNames).toContain("blind-mailbox");
    expect(moduleNames).toContain("relay-router");
    expect(moduleNames).toContain("sovereign-portals");
  });

  test("admin dashboard renders in a browser", async ({ page }) => {
    await page.goto(`${baseUrl}/admin`);

    // Should show the dashboard title
    await expect(page.locator("h1")).toHaveText("Prism Relay Admin");

    // Should render after JS polling
    await expect(page.locator(".source-header")).toBeVisible({ timeout: 10_000 });

    // Should show health badge
    await expect(page.locator(".health-badge")).toBeVisible();

    // Should show uptime card
    await expect(page.locator(".uptime-value")).toBeVisible();

    // Should show metric cards
    const cards = page.locator(".card");
    const cardCount = await cards.count();
    expect(cardCount).toBeGreaterThanOrEqual(3);

    // Should show services list
    await expect(page.locator(".services-card")).toBeVisible();
    const serviceRows = page.locator(".service-row");
    const serviceCount = await serviceRows.count();
    expect(serviceCount).toBeGreaterThanOrEqual(7);
  });

  test("admin dashboard auto-refreshes", async ({ page }) => {
    await page.goto(`${baseUrl}/admin`);

    // Wait for initial render
    await expect(page.locator(".source-header")).toBeVisible({ timeout: 10_000 });

    // Wait for at least one refresh cycle (poll interval is 5s)
    const refreshInfo = page.locator("#refresh-info");
    await expect(refreshInfo).not.toBeEmpty({ timeout: 10_000 });
    const text = await refreshInfo.textContent();
    expect(text).toContain("Last refresh:");
  });
});
