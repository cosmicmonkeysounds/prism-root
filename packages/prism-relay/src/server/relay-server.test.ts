import { describe, it, expect, beforeAll, afterAll } from "vitest";
import { createIdentity } from "@prism/core/identity";
import type { PrismIdentity } from "@prism/core/identity";
import {
  createRelayBuilder,
  blindMailboxModule,
  relayRouterModule,
  capabilityTokenModule,
  webhookModule,
  sovereignPortalModule,
  collectionHostModule,
  hashcashModule,
  peerTrustModule,
  escrowModule,
  federationModule,
} from "@prism/core/relay";
import { createHashcashMinter } from "@prism/core/trust";
import type { RelayInstance } from "@prism/core/relay";
import { createRelayServer } from "./relay-server.js";

let relay: RelayInstance;
let identity: PrismIdentity;
let serverPort: number;
let close: () => Promise<void>;

beforeAll(async () => {
  identity = await createIdentity({ method: "key" });
  relay = createRelayBuilder({ relayDid: identity.did })
    .use(blindMailboxModule())
    .use(relayRouterModule())
    .use(capabilityTokenModule(identity))
    .use(webhookModule())
    .use(sovereignPortalModule())
    .use(collectionHostModule())
    .use(hashcashModule({ bits: 8 }))
    .use(peerTrustModule())
    .use(escrowModule())
    .use(federationModule())
    .build();
  await relay.start();

  const server = createRelayServer({ relay, port: 0, disableCsrf: true });
  const info = await server.start();
  serverPort = info.port;
  close = info.close;
});

afterAll(async () => {
  await close();
  await relay.stop();
});

function url(path: string): string {
  return `http://localhost:${serverPort}${path}`;
}

function post(path: string, body: unknown): Promise<Response> {
  return fetch(url(path), {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(body),
  });
}

describe("relay-server integration", () => {
  // ── Status ────────────────────────────────────────────────────────────

  it("GET /api/status returns relay state", async () => {
    const res = await fetch(url("/api/status"));
    expect(res.status).toBe(200);
    const body = await res.json() as Record<string, unknown>;
    expect(body["running"]).toBe(true);
    expect(body["did"]).toBe(identity.did);
    expect((body["modules"] as string[]).length).toBe(10);
  });

  it("GET /api/modules returns all 10 modules", async () => {
    const res = await fetch(url("/api/modules"));
    const body = await res.json() as Array<{ name: string }>;
    expect(body.length).toBe(10);
    expect(body.some((m) => m.name === "collection-host")).toBe(true);
    expect(body.some((m) => m.name === "federation")).toBe(true);
  });

  // ── Phase 1 routes (regression) ───────────────────────────────────────

  it("POST /api/webhooks registers a webhook", async () => {
    const res = await post("/api/webhooks", { url: "https://test.example/hook", events: ["*"], active: true });
    expect(res.status).toBe(201);
  });

  it("POST /api/portals registers a portal", async () => {
    const res = await post("/api/portals", { name: "Test", level: 1, collectionId: "c1", basePath: "/", isPublic: true });
    expect(res.status).toBe(201);
  });

  it("POST /api/tokens/issue creates a token", async () => {
    const res = await post("/api/tokens/issue", { subject: "*", permissions: ["read"], scope: "test" });
    expect(res.status).toBe(201);
  });

  // ── Collections ───────────────────────────────────────────────────────

  it("collection create → snapshot → import round-trip", async () => {
    let res = await post("/api/collections", { id: "integ-col" });
    expect(res.status).toBe(201);

    res = await fetch(url("/api/collections/integ-col/snapshot"));
    expect(res.status).toBe(200);
    const body = await res.json() as { snapshot: string };
    expect(typeof body.snapshot).toBe("string");

    res = await post("/api/collections/integ-col/import", { data: body.snapshot });
    expect(res.status).toBe(200);
  });

  it("GET /api/collections lists IDs", async () => {
    const res = await fetch(url("/api/collections"));
    const body = await res.json() as string[];
    expect(body).toContain("integ-col");
  });

  // ── Hashcash ──────────────────────────────────────────────────────────

  it("hashcash challenge → verify flow", async () => {
    let res = await post("/api/hashcash/challenge", { resource: "test-relay" });
    expect(res.status).toBe(200);
    const challenge = await res.json() as { resource: string; bits: number; issuedAt: string; salt: string };
    expect(challenge.bits).toBe(8);

    const minter = createHashcashMinter();
    const proof = await minter.mint(challenge);

    res = await post("/api/hashcash/verify", proof);
    expect(res.status).toBe(200);
    const result = await res.json() as { valid: boolean };
    expect(result.valid).toBe(true);
  });

  // ── Trust ─────────────────────────────────────────────────────────────

  it("trust ban/unban lifecycle", async () => {
    let res = await post("/api/trust/peer-x/ban", { reason: "spam" });
    expect(res.status).toBe(200);

    res = await fetch(url("/api/trust/peer-x"));
    expect(res.status).toBe(200);
    const peer = await res.json() as Record<string, unknown>;
    expect(peer["banned"]).toBe(true);

    res = await post("/api/trust/peer-x/unban", {});
    expect(res.status).toBe(200);
  });

  it("GET /api/trust lists all peers", async () => {
    const res = await fetch(url("/api/trust"));
    expect(res.status).toBe(200);
    const peers = await res.json() as unknown[];
    expect(Array.isArray(peers)).toBe(true);
  });

  // ── Escrow ────────────────────────────────────────────────────────────

  it("escrow deposit → claim lifecycle", async () => {
    let res = await post("/api/escrow/deposit", {
      depositorId: "user-integ",
      encryptedPayload: "encrypted-vault-key-abc",
    });
    expect(res.status).toBe(201);
    const deposit = await res.json() as { id: string; claimed: boolean };
    expect(deposit.claimed).toBe(false);

    res = await post("/api/escrow/claim", { depositId: deposit.id });
    expect(res.status).toBe(200);
    const claimed = await res.json() as { claimed: boolean };
    expect(claimed.claimed).toBe(true);

    // Second claim fails
    res = await post("/api/escrow/claim", { depositId: deposit.id });
    expect(res.status).toBe(404);
  });

  it("GET /api/escrow/:depositorId lists deposits", async () => {
    const res = await fetch(url("/api/escrow/user-integ"));
    const body = await res.json() as unknown[];
    expect(body.length).toBeGreaterThanOrEqual(1);
  });

  // ── Federation ────────────────────────────────────────────────────────

  it("federation announce + peers listing", async () => {
    let res = await post("/api/federation/announce", {
      relayDid: "did:key:zPeerRelay",
      url: "http://peer-relay:4444",
    });
    expect(res.status).toBe(200);

    res = await fetch(url("/api/federation/peers"));
    const peers = await res.json() as Array<{ relayDid: string; url: string }>;
    expect(peers.some((p) => p.relayDid === "did:key:zPeerRelay")).toBe(true);
  });

  // ── Auth routes ──────────────────────────────────────────────────────

  it("GET /api/auth/providers lists providers", async () => {
    const res = await fetch(url("/api/auth/providers"));
    expect(res.status).toBe(200);
    const body = await res.json() as { providers: string[] };
    expect(Array.isArray(body.providers)).toBe(true);
  });

  it("POST /api/auth/escrow/derive + recover round-trip", async () => {
    let res = await post("/api/auth/escrow/derive", {
      depositorId: "user-auth-integ",
      password: "test-password-123",
      oauthSalt: "google-salt-abc",
      encryptedVaultKey: "encrypted-key-data",
    });
    expect(res.status).toBe(201);
    const derived = await res.json() as { ok: boolean; depositId: string };
    expect(derived.ok).toBe(true);

    res = await post("/api/auth/escrow/recover", {
      depositorId: "user-auth-integ",
      password: "test-password-123",
      oauthSalt: "google-salt-abc",
    });
    expect(res.status).toBe(200);
    const recovered = await res.json() as { ok: boolean; encryptedVaultKey: string };
    expect(recovered.ok).toBe(true);
    expect(recovered.encryptedVaultKey).toBe("encrypted-key-data");
  });

  // ── Safety routes ────────────────────────────────────────────────────

  it("POST /api/safety/report flags content", async () => {
    const res = await post("/api/safety/report", {
      contentHash: "abc123toxic",
      category: "spam",
      reportedBy: "did:key:zReporter",
    });
    expect(res.status).toBe(201);
  });

  it("GET /api/safety/hashes lists flagged hashes", async () => {
    const res = await fetch(url("/api/safety/hashes"));
    expect(res.status).toBe(200);
    const body = await res.json() as { hashes: Array<{ hash: string }>; count: number };
    expect(body.hashes.some((h) => h.hash === "abc123toxic")).toBe(true);
  });

  it("POST /api/safety/check verifies hashes", async () => {
    const res = await post("/api/safety/check", { hashes: ["abc123toxic", "safe-hash"] });
    expect(res.status).toBe(200);
    const body = await res.json() as { results: Record<string, boolean> };
    expect(body.results["abc123toxic"]).toBe(true);
    expect(body.results["safe-hash"]).toBe(false);
  });

  // ── AutoREST routes ──────────────────────────────────────────────────

  it("AutoREST CRUD on collection", async () => {
    // Create a collection first
    await post("/api/collections", { id: "rest-col" });

    // Issue a capability token for AutoREST access
    const tokenRes = await post("/api/tokens/issue", {
      subject: "*",
      permissions: ["read", "write", "delete"],
      scope: "*",
    });
    const token = await tokenRes.json() as Record<string, unknown>;
    // Base64-encode the serialized token for Bearer header
    const bearerToken = Buffer.from(JSON.stringify(token)).toString("base64");
    const authHeaders = { "Content-Type": "application/json", Authorization: `Bearer ${bearerToken}` };

    // Create an object via AutoREST
    let res = await fetch(url("/api/rest/rest-col"), {
      method: "POST",
      headers: authHeaders,
      body: JSON.stringify({ name: "REST Object", type: "task" }),
    });
    expect(res.status).toBe(201);
    const created = await res.json() as { ok: boolean; objectId: string };
    expect(created.ok).toBe(true);

    // List objects
    res = await fetch(url("/api/rest/rest-col"), { headers: { Authorization: `Bearer ${bearerToken}` } });
    expect(res.status).toBe(200);
    const list = await res.json() as { objects: Array<{ id: string }>; total: number };
    expect(list.objects.length).toBeGreaterThanOrEqual(1);

    // Get single object
    res = await fetch(url(`/api/rest/rest-col/${created.objectId}`), { headers: { Authorization: `Bearer ${bearerToken}` } });
    expect(res.status).toBe(200);

    // Delete object
    res = await fetch(url(`/api/rest/rest-col/${created.objectId}`), { method: "DELETE", headers: { Authorization: `Bearer ${bearerToken}` } });
    expect(res.status).toBe(200);
  });

  // ── Ping routes ──────────────────────────────────────────────────────

  it("POST /api/pings/register + GET /api/pings/devices", async () => {
    let res = await post("/api/pings/register", {
      did: "did:key:zPingUser",
      token: "device-token-abc",
      platform: "apns",
    });
    expect(res.status).toBe(201);

    res = await fetch(url("/api/pings/devices"));
    expect(res.status).toBe(200);
    const body = await res.json() as { devices: Array<{ did: string }>; count: number };
    expect(body.devices.some((d) => d.did === "did:key:zPingUser")).toBe(true);
  });

  // ── SEO routes ───────────────────────────────────────────────────────

  it("GET /sitemap.xml returns valid XML", async () => {
    const res = await fetch(url("/sitemap.xml"));
    expect(res.status).toBe(200);
    const text = await res.text();
    expect(text).toContain("<?xml");
    expect(text).toContain("<urlset");
    expect(text).toContain("/portals");
  });

  it("GET /robots.txt returns directives", async () => {
    const res = await fetch(url("/robots.txt"));
    expect(res.status).toBe(200);
    const text = await res.text();
    expect(text).toContain("User-agent:");
    expect(text).toContain("Disallow: /api/");
  });
});
