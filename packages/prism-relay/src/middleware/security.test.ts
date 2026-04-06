import { describe, it, expect, beforeAll, afterAll } from "vitest";
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
