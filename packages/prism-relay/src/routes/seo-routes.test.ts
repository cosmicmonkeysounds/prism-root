import { describe, it, expect, beforeAll, afterAll } from "vitest";
import { createIdentity } from "@prism/core/identity";
import type { PrismIdentity } from "@prism/core/identity";
import {
  createRelayBuilder,
  blindMailboxModule,
  relayRouterModule,
  sovereignPortalModule,
  collectionHostModule,
} from "@prism/core/relay";
import type { RelayInstance, PortalRegistry } from "@prism/core/relay";
import { RELAY_CAPABILITIES } from "@prism/core/relay";
import { createRelayServer } from "../server/relay-server.js";

let relay: RelayInstance;
let identity: PrismIdentity;
let port: number;
let close: () => Promise<void>;

beforeAll(async () => {
  identity = await createIdentity({ method: "key" });
  relay = createRelayBuilder({ relayDid: identity.did })
    .use(blindMailboxModule())
    .use(relayRouterModule())
    .use(sovereignPortalModule())
    .use(collectionHostModule())
    .build();
  await relay.start();

  // Register a public portal for SEO tests
  const portals = relay.getCapability<PortalRegistry>(RELAY_CAPABILITIES.PORTALS);
  if (!portals) throw new Error("portals not available");
  portals.register({
    name: "Test Portal",
    level: 2,
    collectionId: "test-col",
    basePath: "/test",
    isPublic: true,
  });

  const server = createRelayServer({
    relay,
    port: 0,
    publicUrl: "https://example.com",
    disableCsrf: true,
  });
  const info = await server.start();
  port = info.port;
  close = info.close;
});

afterAll(async () => {
  await close();
  await relay.stop();
});

function url(path: string): string {
  return `http://127.0.0.1:${port}${path}`;
}

describe("seo-routes", () => {
  it("GET /sitemap.xml returns valid XML with portal entries", async () => {
    const res = await fetch(url("/sitemap.xml"));
    expect(res.status).toBe(200);
    expect(res.headers.get("content-type")).toContain("application/xml");
    const body = await res.text();
    expect(body).toContain("<?xml version");
    expect(body).toContain("<urlset");
    expect(body).toContain("https://example.com/portals/");
    expect(body).toContain("<changefreq>hourly</changefreq>"); // Level 2 = hourly
  });

  it("GET /robots.txt returns correct directives", async () => {
    const res = await fetch(url("/robots.txt"));
    expect(res.status).toBe(200);
    expect(res.headers.get("content-type")).toContain("text/plain");
    const body = await res.text();
    expect(body).toContain("Allow: /portals/");
    expect(body).toContain("Disallow: /api/");
    expect(body).toContain("Sitemap: https://example.com/sitemap.xml");
  });

  it("sitemap includes portal index page", async () => {
    const res = await fetch(url("/sitemap.xml"));
    const body = await res.text();
    expect(body).toContain("https://example.com/portals");
    expect(body).toContain("<priority>1.0</priority>");
  });
});
