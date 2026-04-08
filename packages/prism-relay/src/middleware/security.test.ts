import { describe, it, expect, beforeAll, afterAll } from "vitest";
import { Hono } from "hono";
import { createIdentity } from "@prism/core/identity";
import type { PrismIdentity } from "@prism/core/identity";
import {
  createRelayBuilder,
  blindMailboxModule,
  relayRouterModule,
  sovereignPortalModule,
  collectionHostModule,
  peerTrustModule,
} from "@prism/core/relay";
import type { RelayInstance } from "@prism/core/relay";
import { RELAY_CAPABILITIES } from "@prism/core/relay";
import type { PeerTrustGraph } from "@prism/core/trust";
import { createRelayServer } from "../server/relay-server.js";
import { rateLimitMiddleware } from "./security.js";

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
    .use(peerTrustModule())
    .build();
  await relay.start();

  // CSRF enabled (no disableCsrf)
  const server = createRelayServer({ relay, port: 0 });
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

describe("security middleware", () => {
  it("rejects POST without CSRF header", async () => {
    const res = await fetch(url("/api/portals"), {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ name: "test" }),
    });
    expect(res.status).toBe(403);
    const body = await res.json() as { error: string };
    expect(body.error).toContain("CSRF");
  });

  it("allows POST with CSRF header", async () => {
    const res = await fetch(url("/api/portals"), {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
        "X-Prism-CSRF": "1",
      },
      body: JSON.stringify({
        name: "test",
        level: 1,
        collectionId: "col-1",
        basePath: "/t",
        isPublic: true,
      }),
    });
    expect(res.status).toBe(201);
  });

  it("allows GET without CSRF header", async () => {
    const res = await fetch(url("/api/status"));
    expect(res.status).toBe(200);
  });

  it("rejects oversized Content-Length via http module", async () => {
    // Node fetch enforces Content-Length matching the actual body,
    // so we use http.request directly to test the middleware.
    const http = await import("node:http");
    const result = await new Promise<number>((resolve) => {
      const req = http.request(
        { hostname: "127.0.0.1", port, path: "/api/portals", method: "POST",
          headers: { "Content-Type": "application/json", "X-Prism-CSRF": "1", "Content-Length": "999999999" } },
        (res) => resolve(res.statusCode ?? 0),
      );
      req.write(JSON.stringify({ name: "test" }));
      req.end();
    });
    expect(result).toBe(413);
  });

  it("rejects requests from banned peers via X-Prism-DID header", async () => {
    const trust = relay.getCapability<PeerTrustGraph>(RELAY_CAPABILITIES.TRUST);
    if (!trust) throw new Error("trust not available");
    trust.ban("did:key:banned-peer", "testing");

    const res = await fetch(url("/api/status"), {
      headers: { "X-Prism-DID": "did:key:banned-peer" },
    });
    expect(res.status).toBe(403);

    // Clean up
    trust.unban("did:key:banned-peer");
  });
});

describe("rateLimitMiddleware (token bucket)", () => {
  function buildApp(opts: { max: number; refillRate: number; now?: () => number }) {
    const app = new Hono();
    app.use("/*", rateLimitMiddleware(opts));
    app.get("/", (c) => c.text("ok"));
    return app;
  }

  it("returns 429 once a single key exhausts its bucket", async () => {
    const t = 0;
    const app = buildApp({ max: 3, refillRate: 0.1, now: () => t });
    const headers = { "x-prism-did": "did:key:rl-test-1" };

    const r1 = await app.request("/", { headers });
    const r2 = await app.request("/", { headers });
    const r3 = await app.request("/", { headers });
    const r4 = await app.request("/", { headers });

    expect(r1.status).toBe(200);
    expect(r2.status).toBe(200);
    expect(r3.status).toBe(200);
    expect(r4.status).toBe(429);
    expect(r4.headers.get("retry-after")).not.toBeNull();
  });

  it("buckets per DID — separate identities are isolated", async () => {
    const t = 0;
    const app = buildApp({ max: 1, refillRate: 0.1, now: () => t });
    const a = await app.request("/", { headers: { "x-prism-did": "did:key:rl-a" } });
    const b = await app.request("/", { headers: { "x-prism-did": "did:key:rl-b" } });
    const aAgain = await app.request("/", { headers: { "x-prism-did": "did:key:rl-a" } });

    expect(a.status).toBe(200);
    expect(b.status).toBe(200);
    expect(aAgain.status).toBe(429);
  });

  it("falls back to X-Forwarded-For when no DID is present", async () => {
    const t = 0;
    const app = buildApp({ max: 2, refillRate: 0.1, now: () => t });
    const headers = { "x-forwarded-for": "10.0.0.5" };

    expect((await app.request("/", { headers })).status).toBe(200);
    expect((await app.request("/", { headers })).status).toBe(200);
    expect((await app.request("/", { headers })).status).toBe(429);
  });

  it("refills tokens over time", async () => {
    // Manual clock: 1 token max, 1000 tokens/sec → a 20ms advance fully refills.
    let t = 0;
    const app = buildApp({ max: 1, refillRate: 1000, now: () => t });
    const headers = { "x-prism-did": "did:key:rl-refill" };

    expect((await app.request("/", { headers })).status).toBe(200);
    expect((await app.request("/", { headers })).status).toBe(429);

    t += 20; // 20ms → 20 tokens refilled, capped at max=1
    expect((await app.request("/", { headers })).status).toBe(200);
  });
});
