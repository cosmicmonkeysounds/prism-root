/**
 * Prism Relay E2E tests — HTTP API + WebSocket protocol.
 *
 * Starts a relay server on an ephemeral port, exercises every route and
 * the full WS auth/envelope/sync/hashcash flow.
 */

import { test, expect } from "@playwright/test";
import { createIdentity } from "@prism/core/identity";
import type { PrismIdentity } from "@prism/core/identity";
import {
  createRelayBuilder,
  createRelayClient,
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
import type { RelayInstance, CollectionHost } from "@prism/core/relay";
import { RELAY_CAPABILITIES } from "@prism/core/relay";
import { createHashcashMinter } from "@prism/core/trust";
import { createRelayServer } from "@prism/relay/server";

let relay: RelayInstance;
let identity: PrismIdentity;
let serverPort: number;
let close: () => Promise<void>;
let baseUrl: string;

test.beforeAll(async () => {
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

  const server = createRelayServer({ relay, port: 0 });
  const info = await server.start();
  serverPort = info.port;
  baseUrl = `http://localhost:${serverPort}`;
  close = info.close;
});

test.afterAll(async () => {
  await close();
  await relay.stop();
});

// ── HTTP: Status & Modules ──────────────────────────────────────────────────

test.describe("HTTP API", () => {
  test("GET /api/status returns relay state", async ({ request }) => {
    const res = await request.get(`${baseUrl}/api/status`);
    expect(res.status()).toBe(200);
    const body = await res.json();
    expect(body.running).toBe(true);
    expect(body.did).toBe(identity.did);
    expect(body.modules).toHaveLength(10);
  });

  test("GET /api/modules lists all installed modules", async ({ request }) => {
    const res = await request.get(`${baseUrl}/api/modules`);
    const body = await res.json();
    expect(body).toHaveLength(10);
    const names = body.map((m: { name: string }) => m.name);
    expect(names).toContain("blind-mailbox");
    expect(names).toContain("relay-router");
    expect(names).toContain("collection-host");
    expect(names).toContain("hashcash");
    expect(names).toContain("peer-trust");
    expect(names).toContain("escrow");
    expect(names).toContain("federation");
  });

  // ── Webhooks ────────────────────────────────────────────────────────────

  test("POST /api/webhooks registers a webhook", async ({ request }) => {
    const res = await request.post(`${baseUrl}/api/webhooks`, {
      data: { url: "https://e2e.example/hook", events: ["*"], active: true },
    });
    expect(res.status()).toBe(201);
  });

  // ── Portals ─────────────────────────────────────────────────────────────

  test("POST /api/portals registers a portal", async ({ request }) => {
    const res = await request.post(`${baseUrl}/api/portals`, {
      data: { name: "E2E Portal", level: 1, collectionId: "c1", basePath: "/", isPublic: true },
    });
    expect(res.status()).toBe(201);
  });

  // ── Tokens ──────────────────────────────────────────────────────────────

  test("POST /api/tokens/issue creates a capability token", async ({ request }) => {
    const res = await request.post(`${baseUrl}/api/tokens/issue`, {
      data: { subject: "*", permissions: ["read"], scope: "e2e" },
    });
    expect(res.status()).toBe(201);
    const body = await res.json();
    expect(body.tokenId).toBeDefined();
    expect(body.signature).toBeDefined();
  });

  // ── Collections ─────────────────────────────────────────────────────────

  test("collection create → snapshot → import round-trip", async ({ request }) => {
    let res = await request.post(`${baseUrl}/api/collections`, {
      data: { id: "e2e-col" },
    });
    expect(res.status()).toBe(201);

    res = await request.get(`${baseUrl}/api/collections/e2e-col/snapshot`);
    expect(res.status()).toBe(200);
    const body = await res.json();
    expect(typeof body.snapshot).toBe("string");

    res = await request.post(`${baseUrl}/api/collections/e2e-col/import`, {
      data: { data: body.snapshot },
    });
    expect(res.status()).toBe(200);
  });

  test("GET /api/collections lists collection IDs", async ({ request }) => {
    // Ensure collection exists first
    await request.post(`${baseUrl}/api/collections`, {
      data: { id: "e2e-list-col" },
    });
    const res = await request.get(`${baseUrl}/api/collections`);
    const body = await res.json();
    expect(body).toContain("e2e-list-col");
  });

  // ── Hashcash ────────────────────────────────────────────────────────────

  test("hashcash challenge → verify flow", async ({ request }) => {
    let res = await request.post(`${baseUrl}/api/hashcash/challenge`, {
      data: { resource: "e2e-relay" },
    });
    expect(res.status()).toBe(200);
    const challenge = await res.json();
    expect(challenge.bits).toBe(8);

    const minter = createHashcashMinter();
    const proof = await minter.mint(challenge);

    res = await request.post(`${baseUrl}/api/hashcash/verify`, {
      data: proof,
    });
    expect(res.status()).toBe(200);
    const result = await res.json();
    expect(result.valid).toBe(true);
  });

  // ── Trust ───────────────────────────────────────────────────────────────

  test("trust ban/unban lifecycle", async ({ request }) => {
    let res = await request.post(`${baseUrl}/api/trust/peer-e2e/ban`, {
      data: { reason: "spam" },
    });
    expect(res.status()).toBe(200);

    res = await request.get(`${baseUrl}/api/trust/peer-e2e`);
    expect(res.status()).toBe(200);
    const peer = await res.json();
    expect(peer.banned).toBe(true);

    res = await request.post(`${baseUrl}/api/trust/peer-e2e/unban`, { data: {} });
    expect(res.status()).toBe(200);
  });

  test("GET /api/trust lists all peers", async ({ request }) => {
    const res = await request.get(`${baseUrl}/api/trust`);
    expect(res.status()).toBe(200);
    const body = await res.json();
    expect(Array.isArray(body)).toBe(true);
  });

  // ── Escrow ──────────────────────────────────────────────────────────────

  test("escrow deposit → claim lifecycle", async ({ request }) => {
    let res = await request.post(`${baseUrl}/api/escrow/deposit`, {
      data: { depositorId: "e2e-user", encryptedPayload: "encrypted-key" },
    });
    expect(res.status()).toBe(201);
    const deposit = await res.json();
    expect(deposit.claimed).toBe(false);

    res = await request.post(`${baseUrl}/api/escrow/claim`, {
      data: { depositId: deposit.id },
    });
    expect(res.status()).toBe(200);
    const claimed = await res.json();
    expect(claimed.claimed).toBe(true);

    // Second claim fails
    res = await request.post(`${baseUrl}/api/escrow/claim`, {
      data: { depositId: deposit.id },
    });
    expect(res.status()).toBe(404);
  });

  test("GET /api/escrow/:depositorId lists deposits", async ({ request }) => {
    const res = await request.get(`${baseUrl}/api/escrow/e2e-user`);
    const body = await res.json();
    expect(body.length).toBeGreaterThanOrEqual(1);
  });

  // ── Federation ──────────────────────────────────────────────────────────

  test("federation announce + peers listing", async ({ request }) => {
    let res = await request.post(`${baseUrl}/api/federation/announce`, {
      data: { relayDid: "did:key:zE2EPeer", url: "http://peer:5555" },
    });
    expect(res.status()).toBe(200);

    res = await request.get(`${baseUrl}/api/federation/peers`);
    const peers = await res.json();
    expect(peers.some((p: { relayDid: string }) => p.relayDid === "did:key:zE2EPeer")).toBe(true);
  });
});

// ── WebSocket ───────────────────────────────────────────────────────────────

test.describe("WebSocket protocol", () => {
  function connectWs(): Promise<WebSocket> {
    return new Promise((resolve, reject) => {
      const ws = new WebSocket(`ws://localhost:${serverPort}/ws/relay`);
      ws.addEventListener("open", () => resolve(ws));
      ws.addEventListener("error", (e) => reject(e));
    });
  }

  function nextMessage(ws: WebSocket): Promise<Record<string, unknown>> {
    return new Promise((resolve) => {
      ws.addEventListener("message", (evt) => {
        resolve(JSON.parse(String(evt.data)) as Record<string, unknown>);
      }, { once: true });
    });
  }

  test("auth handshake returns auth-ok", async () => {
    const ws = await connectWs();
    const reply = nextMessage(ws);
    ws.send(JSON.stringify({ type: "auth", did: "did:key:zE2ETest" }));
    const msg = await reply;
    expect(msg["type"]).toBe("auth-ok");
    expect(msg["relayDid"]).toBe(identity.did);
    expect(Array.isArray(msg["modules"])).toBe(true);
    ws.close();
  });

  test("ping/pong", async () => {
    const ws = await connectWs();
    const reply = nextMessage(ws);
    ws.send(JSON.stringify({ type: "ping" }));
    const msg = await reply;
    expect(msg["type"]).toBe("pong");
    ws.close();
  });

  test("envelope routing between two peers", async () => {
    const alice = await connectWs();
    const bob = await connectWs();

    // Auth both
    const aliceAuth = nextMessage(alice);
    alice.send(JSON.stringify({ type: "auth", did: "did:key:zE2EAlice" }));
    await aliceAuth;

    const bobAuth = nextMessage(bob);
    bob.send(JSON.stringify({ type: "auth", did: "did:key:zE2EBob" }));
    await bobAuth;

    // Alice sends envelope to Bob
    const bobIncoming = nextMessage(bob);
    const aliceRouteResult = nextMessage(alice);

    const ciphertext = Buffer.from([1, 2, 3]).toString("base64");
    alice.send(JSON.stringify({
      type: "envelope",
      envelope: {
        id: "e2e-env-1",
        from: "did:key:zE2EAlice",
        to: "did:key:zE2EBob",
        ciphertext,
        submittedAt: new Date().toISOString(),
        ttlMs: 60_000,
      },
    }));

    const routeResult = await aliceRouteResult;
    expect(routeResult["type"]).toBe("route-result");

    const incoming = await bobIncoming;
    expect(incoming["type"]).toBe("envelope");

    alice.close();
    bob.close();
  });

  test("rejects envelope before auth", async () => {
    const ws = await connectWs();
    const reply = nextMessage(ws);
    ws.send(JSON.stringify({
      type: "envelope",
      envelope: {
        id: "e-unauth",
        from: "did:key:zX",
        to: "did:key:zY",
        ciphertext: "AQID",
        submittedAt: new Date().toISOString(),
        ttlMs: 1000,
      },
    }));
    const msg = await reply;
    expect(msg["type"]).toBe("error");
    expect(msg["message"]).toContain("not authenticated");
    ws.close();
  });

  test("sync-request returns snapshot for hosted collection", async () => {
    // Create a collection via HTTP first
    const host = relay.getCapability<CollectionHost>(RELAY_CAPABILITIES.COLLECTIONS) as CollectionHost;
    host.create("ws-sync-col");

    const ws = await connectWs();
    const authReply = nextMessage(ws);
    ws.send(JSON.stringify({ type: "auth", did: "did:key:zSyncPeer" }));
    await authReply;

    const syncReply = nextMessage(ws);
    ws.send(JSON.stringify({ type: "sync-request", collectionId: "ws-sync-col" }));
    const msg = await syncReply;
    expect(msg["type"]).toBe("sync-snapshot");
    expect(msg["collectionId"]).toBe("ws-sync-col");
    expect(typeof msg["snapshot"]).toBe("string");
    ws.close();
  });

  test("sync-request for unknown collection returns error", async () => {
    const ws = await connectWs();
    const authReply = nextMessage(ws);
    ws.send(JSON.stringify({ type: "auth", did: "did:key:zSyncPeer2" }));
    await authReply;

    const reply = nextMessage(ws);
    ws.send(JSON.stringify({ type: "sync-request", collectionId: "nonexistent" }));
    const msg = await reply;
    expect(msg["type"]).toBe("error");
    ws.close();
  });
});

// ── Federation (multi-relay) ────────────────────────────────────────────────

test.describe("Federation", () => {
  let relay2: RelayInstance;
  let identity2: PrismIdentity;
  let port2: number;
  let close2: () => Promise<void>;

  test.beforeAll(async () => {
    identity2 = await createIdentity({ method: "key" });
    relay2 = createRelayBuilder({ relayDid: identity2.did })
      .use(blindMailboxModule())
      .use(relayRouterModule())
      .use(federationModule())
      .build();
    await relay2.start();

    const server2 = createRelayServer({ relay: relay2, port: 0 });
    const info2 = await server2.start();
    port2 = info2.port;
    close2 = info2.close;
  });

  test.afterAll(async () => {
    await close2();
    await relay2.stop();
  });

  test("two relays discover each other via federation announce", async ({ request }) => {
    // Relay 1 announces to Relay 2
    let res = await request.post(`http://localhost:${port2}/api/federation/announce`, {
      data: { relayDid: identity.did, url: `http://localhost:${serverPort}` },
    });
    expect(res.status()).toBe(200);

    // Relay 2 announces to Relay 1
    res = await request.post(`${baseUrl}/api/federation/announce`, {
      data: { relayDid: identity2.did, url: `http://localhost:${port2}` },
    });
    expect(res.status()).toBe(200);

    // Verify peers on Relay 1
    res = await request.get(`${baseUrl}/api/federation/peers`);
    const peers1 = await res.json();
    expect(peers1.some((p: { relayDid: string }) => p.relayDid === identity2.did)).toBe(true);

    // Verify peers on Relay 2
    res = await request.get(`http://localhost:${port2}/api/federation/peers`);
    const peers2 = await res.json();
    expect(peers2.some((p: { relayDid: string }) => p.relayDid === identity.did)).toBe(true);
  });
});

// ── Client SDK ──────────────────────────────────────────────────────────────

test.describe("Client SDK", () => {
  test("client connects, sends envelope, and recipient receives it", async () => {
    const alice = await createIdentity({ method: "key" });
    const bob = await createIdentity({ method: "key" });

    const aliceClient = createRelayClient({
      url: `ws://localhost:${serverPort}/ws/relay`,
      identity: alice,
      autoReconnect: false,
    });
    const bobClient = createRelayClient({
      url: `ws://localhost:${serverPort}/ws/relay`,
      identity: bob,
      autoReconnect: false,
    });

    await aliceClient.connect();
    expect(aliceClient.state).toBe("connected");
    expect(aliceClient.relayDid).toBe(identity.did);

    await bobClient.connect();
    expect(bobClient.state).toBe("connected");

    // Set up receiver
    const received = new Promise<Uint8Array>((resolve) => {
      bobClient.on("envelope", (env) => resolve(env.ciphertext));
    });

    // Send from Alice to Bob
    const payload = new TextEncoder().encode("Hello from Alice via deployed relay!");
    const result = await aliceClient.send({
      to: bob.did,
      ciphertext: payload,
      ttlMs: 60_000,
    });
    expect(result.status).toBe("delivered");

    // Bob receives it
    const data = await received;
    expect(new TextDecoder().decode(data)).toBe("Hello from Alice via deployed relay!");

    aliceClient.close();
    bobClient.close();
  });

  test("client syncs collection from relay", async () => {
    // Create collection on server side
    const host = relay.getCapability<CollectionHost>(RELAY_CAPABILITIES.COLLECTIONS) as CollectionHost;
    host.create("client-e2e-col");

    const alice = await createIdentity({ method: "key" });
    const client = createRelayClient({
      url: `ws://localhost:${serverPort}/ws/relay`,
      identity: alice,
      autoReconnect: false,
    });
    await client.connect();

    const snapshot = await client.syncRequest("client-e2e-col");
    expect(snapshot).toBeInstanceOf(Uint8Array);
    expect(snapshot.length).toBeGreaterThan(0);

    client.close();
  });

  test("client handles reconnection state", async () => {
    const alice = await createIdentity({ method: "key" });
    const client = createRelayClient({
      url: `ws://localhost:${serverPort}/ws/relay`,
      identity: alice,
      autoReconnect: false,
    });

    expect(client.state).toBe("disconnected");
    await client.connect();
    expect(client.state).toBe("connected");
    expect(client.modules.length).toBeGreaterThan(0);
    client.close();
    expect(client.state).toBe("disconnected");
  });
});
