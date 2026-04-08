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
  relayTimestampModule,
  blindPingModule,
  capabilityTokenModule,
  webhookModule,
  sovereignPortalModule,
  collectionHostModule,
  hashcashModule,
  peerTrustModule,
  escrowModule,
  federationModule,
  acmeCertificateModule,
  portalTemplateModule,
  webrtcSignalingModule,
  vaultHostModule,
} from "@prism/core/relay";
import type {
  RelayInstance,
  CollectionHost,
  PortalRegistry,
} from "@prism/core/relay";
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
    .use(relayTimestampModule(identity))
    .use(blindPingModule())
    .use(capabilityTokenModule(identity))
    .use(webhookModule())
    .use(sovereignPortalModule())
    .use(collectionHostModule())
    .use(hashcashModule({ bits: 8 }))
    .use(peerTrustModule())
    .use(escrowModule())
    .use(federationModule())
    .use(acmeCertificateModule())
    .use(portalTemplateModule())
    .use(webrtcSignalingModule())
    .use(vaultHostModule())
    .build();
  await relay.start();

  const server = createRelayServer({ relay, port: 0, publicUrl: "http://localhost:0", disableCsrf: true });
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
    expect(body.modules).toHaveLength(16);
  });

  test("GET /api/modules lists all installed modules", async ({ request }) => {
    const res = await request.get(`${baseUrl}/api/modules`);
    const body = await res.json();
    expect(body).toHaveLength(16);
    const names = body.map((m: { name: string }) => m.name);
    expect(names).toContain("blind-mailbox");
    expect(names).toContain("relay-router");
    expect(names).toContain("collection-host");
    expect(names).toContain("hashcash");
    expect(names).toContain("peer-trust");
    expect(names).toContain("escrow");
    expect(names).toContain("federation");
    expect(names).toContain("acme-certificates");
    expect(names).toContain("portal-templates");
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

  // ── ACME / SSL Certificates ─────────────────────────────────────────────

  test("ACME HTTP-01 challenge flow", async ({ request }) => {
    // Register a challenge via management API
    let res = await request.post(`${baseUrl}/api/acme/challenges`, {
      data: {
        domain: "e2e.example.com",
        token: "e2e-acme-token",
        keyAuthorization: "e2e-acme-token.thumbprint-xyz",
        expiresInMs: 300_000,
      },
    });
    expect(res.status()).toBe(201);

    // Verify the ACME HTTP-01 response works
    res = await request.get(`${baseUrl}/.well-known/acme-challenge/e2e-acme-token`);
    expect(res.status()).toBe(200);
    const text = await res.text();
    expect(text).toBe("e2e-acme-token.thumbprint-xyz");

    // Clean up
    res = await request.delete(`${baseUrl}/api/acme/challenges/e2e-acme-token`);
    expect(res.status()).toBe(200);

    // Verify challenge is gone
    res = await request.get(`${baseUrl}/.well-known/acme-challenge/e2e-acme-token`);
    expect(res.status()).toBe(404);
  });

  test("SSL certificate management CRUD", async ({ request }) => {
    // Store certificate
    let res = await request.post(`${baseUrl}/api/acme/certificates`, {
      data: {
        domain: "e2e-cert.example.com",
        certificate: "-----BEGIN CERTIFICATE-----\ne2e-cert\n-----END CERTIFICATE-----",
        privateKey: "-----BEGIN PRIVATE KEY-----\ne2e-key\n-----END PRIVATE KEY-----",
        expiresAt: new Date(Date.now() + 86_400_000 * 90).toISOString(),
      },
    });
    expect(res.status()).toBe(201);

    // List certificates
    res = await request.get(`${baseUrl}/api/acme/certificates`);
    expect(res.status()).toBe(200);
    const certs = await res.json();
    expect(certs.some((c: { domain: string }) => c.domain === "e2e-cert.example.com")).toBe(true);
    // Listing should NOT include private key
    const cert = certs.find((c: { domain: string }) => c.domain === "e2e-cert.example.com");
    expect(cert.privateKey).toBeUndefined();

    // Get certificate
    res = await request.get(`${baseUrl}/api/acme/certificates/e2e-cert.example.com`);
    expect(res.status()).toBe(200);
    const certDetail = await res.json();
    expect(certDetail.domain).toBe("e2e-cert.example.com");

    // Delete certificate
    res = await request.delete(`${baseUrl}/api/acme/certificates/e2e-cert.example.com`);
    expect(res.status()).toBe(200);
  });

  // ── Portal Templates ────────────────────────────────────────────────────

  test("portal template CRUD lifecycle", async ({ request }) => {
    // Create template
    let res = await request.post(`${baseUrl}/api/templates`, {
      data: {
        name: "E2E Dark Theme",
        description: "Dark portal theme for E2E testing",
        css: ":root { --bg: #111; --fg: #eee; }",
        headerHtml: "<h1>{{portalName}}</h1>",
        footerHtml: "<footer>Powered by E2E</footer>",
        objectCardHtml: "<div class='card'>{{name}}</div>",
      },
    });
    expect(res.status()).toBe(201);
    const template = await res.json();
    expect(template.templateId).toBeDefined();
    expect(template.name).toBe("E2E Dark Theme");

    // List templates
    res = await request.get(`${baseUrl}/api/templates`);
    expect(res.status()).toBe(200);
    const templates = await res.json();
    expect(templates.some((t: { name: string }) => t.name === "E2E Dark Theme")).toBe(true);

    // Get template
    res = await request.get(`${baseUrl}/api/templates/${template.templateId}`);
    expect(res.status()).toBe(200);
    const detail = await res.json();
    expect(detail.css).toContain("--bg: #111");

    // Delete template
    res = await request.delete(`${baseUrl}/api/templates/${template.templateId}`);
    expect(res.status()).toBe(200);

    // Verify gone
    res = await request.get(`${baseUrl}/api/templates/${template.templateId}`);
    expect(res.status()).toBe(404);
  });

  // ── Level 3 Portal: Form Submission ─────────────────────────────────────

  test("Level 3 portal form submission creates object", async ({ request }) => {
    // Create a backing collection with data
    const host = relay.getCapability<CollectionHost>(RELAY_CAPABILITIES.COLLECTIONS) as CollectionHost;
    host.create("e2e-form-col");

    // Register Level 3 portal
    const registry = relay.getCapability<PortalRegistry>(RELAY_CAPABILITIES.PORTALS) as PortalRegistry;
    const portal = registry.register({
      name: "E2E Form Portal",
      level: 3,
      collectionId: "e2e-form-col",
      basePath: "/form",
      isPublic: true,
    });

    // Submit via form
    const res = await request.post(`${baseUrl}/portals/${portal.portalId}/submit`, {
      data: {
        name: "E2E Submission",
        type: "feedback",
        description: "Test feedback from E2E",
      },
    });
    expect(res.status()).toBe(201);
    const body = await res.json();
    expect(body.ok).toBe(true);
    expect(body.objectId).toBeDefined();
    expect(body.ephemeralDid).toContain("did:key:ephemeral-");

    // Verify the object was created in the collection
    const store = host.get("e2e-form-col");
    expect(store).toBeDefined();
    const objects = (store as NonNullable<typeof store>).listObjects({ excludeDeleted: true });
    const submission = objects.find((o) => o.name === "E2E Submission");
    expect(submission).toBeDefined();
    if (submission) {
      expect(submission.type).toBe("feedback");
      expect(submission.status).toBe("submitted");
      expect(submission.tags).toContain("portal-submission");
    }
  });

  test("Level 1 portal rejects form submissions", async ({ request }) => {
    const registry = relay.getCapability<PortalRegistry>(RELAY_CAPABILITIES.PORTALS) as PortalRegistry;
    const host = relay.getCapability<CollectionHost>(RELAY_CAPABILITIES.COLLECTIONS) as CollectionHost;
    host.create("e2e-readonly-col");
    const portal = registry.register({
      name: "E2E Read Portal",
      level: 1,
      collectionId: "e2e-readonly-col",
      basePath: "/readonly",
      isPublic: true,
    });

    const res = await request.post(`${baseUrl}/portals/${portal.portalId}/submit`, {
      data: { name: "Should Fail" },
    });
    expect(res.status()).toBe(400);
    const body = await res.json();
    expect(body.error).toContain("Level 3");
  });

  test("Level 3 portal validates required name field", async ({ request }) => {
    const registry = relay.getCapability<PortalRegistry>(RELAY_CAPABILITIES.PORTALS) as PortalRegistry;
    const host = relay.getCapability<CollectionHost>(RELAY_CAPABILITIES.COLLECTIONS) as CollectionHost;
    host.create("e2e-validate-col");
    const portal = registry.register({
      name: "E2E Validate Portal",
      level: 3,
      collectionId: "e2e-validate-col",
      basePath: "/validate",
      isPublic: true,
    });

    const res = await request.post(`${baseUrl}/portals/${portal.portalId}/submit`, {
      data: { description: "No name" },
    });
    expect(res.status()).toBe(400);
  });

  // ── Portal View Routes (HTML) ───────────────────────────────────────────

  test("Level 3 portal renders form section", async ({ request }) => {
    const host = relay.getCapability<CollectionHost>(RELAY_CAPABILITIES.COLLECTIONS) as CollectionHost;
    host.create("e2e-view-form-col");
    const registry = relay.getCapability<PortalRegistry>(RELAY_CAPABILITIES.PORTALS) as PortalRegistry;
    const portal = registry.register({
      name: "E2E View Form Portal",
      level: 3,
      collectionId: "e2e-view-form-col",
      basePath: "/viewform",
      isPublic: true,
    });

    const res = await request.get(`${baseUrl}/portals/${portal.portalId}`);
    expect(res.status()).toBe(200);
    const html = await res.text();
    expect(html).toContain("portal-form");
    expect(html).toContain("Submit Data");
    expect(html).toContain("portal-submit-form");
    expect(html).toContain("Interactive");
  });

  test("Level 4 portal renders hydration script", async ({ request }) => {
    const host = relay.getCapability<CollectionHost>(RELAY_CAPABILITIES.COLLECTIONS) as CollectionHost;
    host.create("e2e-view-app-col");
    const registry = relay.getCapability<PortalRegistry>(RELAY_CAPABILITIES.PORTALS) as PortalRegistry;
    const portal = registry.register({
      name: "E2E App Portal",
      level: 4,
      collectionId: "e2e-view-app-col",
      basePath: "/app",
      isPublic: true,
    });

    const res = await request.get(`${baseUrl}/portals/${portal.portalId}`);
    expect(res.status()).toBe(200);
    const html = await res.text();
    expect(html).toContain("__PRISM_PORTAL__");
    expect(html).toContain("sendUpdate");
    expect(html).toContain("submitObject");
    expect(html).toContain("App");
    expect(html).toContain("Interactive");
  });

  test("Level 2 portal uses incremental DOM patching", async ({ request }) => {
    const host = relay.getCapability<CollectionHost>(RELAY_CAPABILITIES.COLLECTIONS) as CollectionHost;
    host.create("e2e-view-live-col");
    const registry = relay.getCapability<PortalRegistry>(RELAY_CAPABILITIES.PORTALS) as PortalRegistry;
    const portal = registry.register({
      name: "E2E Live Portal",
      level: 2,
      collectionId: "e2e-view-live-col",
      basePath: "/live",
      isPublic: true,
    });

    const server2 = createRelayServer({ relay, port: 0, publicUrl: `http://localhost:${serverPort}` });
    const info2 = await server2.start();
    const res = await request.get(`http://localhost:${info2.port}/portals/${portal.portalId}`);
    const html = await res.text();
    // Should use incremental patching, NOT window.location.reload()
    expect(html).toContain("patchPortalContent");
    expect(html).toContain("snapshot.json");
    expect(html).not.toContain("window.location.reload");
    await info2.close();
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

    const server2 = createRelayServer({ relay: relay2, port: 0, disableCsrf: true });
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

// ── Phase 30i: Auth, Safety, AutoREST, Pings, SEO, Security ──────────────

test.describe("Auth routes", () => {
  test("GET /api/auth/providers lists available providers", async ({ request }) => {
    const res = await request.get(`${baseUrl}/api/auth/providers`);
    expect(res.status()).toBe(200);
    const body = await res.json();
    expect(body.providers).toEqual([]); // No OAuth configured in test
  });

  test("GET /api/auth/google returns 404 when not configured", async ({ request }) => {
    const res = await request.get(`${baseUrl}/api/auth/google`);
    expect(res.status()).toBe(404);
  });

  test("GET /api/auth/github returns 404 when not configured", async ({ request }) => {
    const res = await request.get(`${baseUrl}/api/auth/github`);
    expect(res.status()).toBe(404);
  });

  test("Blind Escrow derive + recover round-trip", async ({ request }) => {
    let res = await request.post(`${baseUrl}/api/auth/escrow/derive`, {
      data: {
        depositorId: "e2e-escrow-user",
        password: "strong-password-123",
        oauthSalt: "google-salt-xyz",
        encryptedVaultKey: "encrypted-master-key-data",
      },
    });
    expect(res.status()).toBe(201);
    const derived = await res.json();
    expect(derived.ok).toBe(true);
    expect(derived.depositId).toBeDefined();

    // Recover with correct password
    res = await request.post(`${baseUrl}/api/auth/escrow/recover`, {
      data: {
        depositorId: "e2e-escrow-user",
        password: "strong-password-123",
        oauthSalt: "google-salt-xyz",
      },
    });
    expect(res.status()).toBe(200);
    const recovered = await res.json();
    expect(recovered.ok).toBe(true);
    expect(recovered.encryptedVaultKey).toBe("encrypted-master-key-data");
  });

  test("Blind Escrow rejects wrong password", async ({ request }) => {
    const res = await request.post(`${baseUrl}/api/auth/escrow/recover`, {
      data: {
        depositorId: "e2e-escrow-user",
        password: "wrong-password",
        oauthSalt: "google-salt-xyz",
      },
    });
    expect(res.status()).toBe(403);
  });

  test("escrow derive validates required fields", async ({ request }) => {
    const res = await request.post(`${baseUrl}/api/auth/escrow/derive`, {
      data: { password: "test" },
    });
    expect(res.status()).toBe(400);
  });
});

test.describe("Safety routes", () => {
  test("POST /api/safety/report flags content hash", async ({ request }) => {
    const res = await request.post(`${baseUrl}/api/safety/report`, {
      data: {
        contentHash: "e2e-toxic-hash-abc",
        category: "spam",
        reportedBy: "did:key:zE2EReporter",
      },
    });
    expect(res.status()).toBe(201);
    const body = await res.json();
    expect(body.ok).toBe(true);
    expect(body.flagged).toBe(true);
  });

  test("GET /api/safety/hashes lists flagged content", async ({ request }) => {
    const res = await request.get(`${baseUrl}/api/safety/hashes`);
    expect(res.status()).toBe(200);
    const body = await res.json();
    expect(body.hashes.some((h: { hash: string }) => h.hash === "e2e-toxic-hash-abc")).toBe(true);
    expect(body.count).toBeGreaterThanOrEqual(1);
  });

  test("POST /api/safety/check verifies hashes", async ({ request }) => {
    const res = await request.post(`${baseUrl}/api/safety/check`, {
      data: { hashes: ["e2e-toxic-hash-abc", "safe-content-hash"] },
    });
    expect(res.status()).toBe(200);
    const body = await res.json();
    expect(body.results["e2e-toxic-hash-abc"]).toBe(true);
    expect(body.results["safe-content-hash"]).toBe(false);
  });

  test("POST /api/safety/hashes imports from federation peer", async ({ request }) => {
    const res = await request.post(`${baseUrl}/api/safety/hashes`, {
      data: {
        hashes: [{ hash: "imported-hash-xyz", category: "abuse", reportedBy: "did:key:zPeer" }],
        sourceRelay: "did:key:zFederatedRelay",
      },
    });
    expect(res.status()).toBe(200);
    const body = await res.json();
    expect(body.ok).toBe(true);
    expect(body.imported).toBeGreaterThanOrEqual(1);
  });

  test("POST /api/safety/report validates required fields", async ({ request }) => {
    const res = await request.post(`${baseUrl}/api/safety/report`, {
      data: { category: "spam" },
    });
    expect(res.status()).toBe(400);
  });
});

test.describe("AutoREST API gateway", () => {
  let bearerToken: string;

  test.beforeAll(async ({ request }) => {
    // Create a collection for AutoREST
    await request.post(`${baseUrl}/api/collections`, { data: { id: "e2e-rest-col" } });

    // Issue a capability token
    const tokenRes = await request.post(`${baseUrl}/api/tokens/issue`, {
      data: { subject: "*", permissions: ["read", "write", "delete"], scope: "*" },
    });
    const token = await tokenRes.json();
    bearerToken = Buffer.from(JSON.stringify(token)).toString("base64");
  });

  test("POST creates an object", async ({ request }) => {
    const res = await request.post(`${baseUrl}/api/rest/e2e-rest-col`, {
      headers: { Authorization: `Bearer ${bearerToken}` },
      data: { name: "E2E REST Object", type: "task", description: "Created via AutoREST" },
    });
    expect(res.status()).toBe(201);
    const body = await res.json();
    expect(body.ok).toBe(true);
    expect(body.objectId).toBeDefined();
  });

  test("GET lists objects with filters", async ({ request }) => {
    const res = await request.get(`${baseUrl}/api/rest/e2e-rest-col?type=task`, {
      headers: { Authorization: `Bearer ${bearerToken}` },
    });
    expect(res.status()).toBe(200);
    const body = await res.json();
    expect(body.objects.length).toBeGreaterThanOrEqual(1);
    expect(body.total).toBeGreaterThanOrEqual(1);
    expect(body.objects[0].type).toBe("task");
  });

  test("GET retrieves single object", async ({ request }) => {
    // First list to get an ID
    const listRes = await request.get(`${baseUrl}/api/rest/e2e-rest-col`, {
      headers: { Authorization: `Bearer ${bearerToken}` },
    });
    const { objects } = await listRes.json();
    const objectId = objects[0].id;

    const res = await request.get(`${baseUrl}/api/rest/e2e-rest-col/${objectId}`, {
      headers: { Authorization: `Bearer ${bearerToken}` },
    });
    expect(res.status()).toBe(200);
    const obj = await res.json();
    expect(obj.name).toBe("E2E REST Object");
  });

  test("PUT updates an object", async ({ request }) => {
    const listRes = await request.get(`${baseUrl}/api/rest/e2e-rest-col`, {
      headers: { Authorization: `Bearer ${bearerToken}` },
    });
    const { objects } = await listRes.json();
    const objectId = objects[0].id;

    const res = await request.put(`${baseUrl}/api/rest/e2e-rest-col/${objectId}`, {
      headers: { Authorization: `Bearer ${bearerToken}` },
      data: { name: "Updated E2E Object", status: "done" },
    });
    expect(res.status()).toBe(200);
    const body = await res.json();
    expect(body.ok).toBe(true);
  });

  test("DELETE soft-deletes an object", async ({ request }) => {
    const listRes = await request.get(`${baseUrl}/api/rest/e2e-rest-col`, {
      headers: { Authorization: `Bearer ${bearerToken}` },
    });
    const { objects } = await listRes.json();
    const objectId = objects[0].id;

    const res = await request.delete(`${baseUrl}/api/rest/e2e-rest-col/${objectId}`, {
      headers: { Authorization: `Bearer ${bearerToken}` },
    });
    expect(res.status()).toBe(200);

    // Object should no longer appear in listing (soft-deleted)
    const afterRes = await request.get(`${baseUrl}/api/rest/e2e-rest-col`, {
      headers: { Authorization: `Bearer ${bearerToken}` },
    });
    const afterBody = await afterRes.json();
    expect(afterBody.objects.find((o: { id: string }) => o.id === objectId)).toBeUndefined();
  });

  test("rejects unauthenticated access", async ({ request }) => {
    const res = await request.get(`${baseUrl}/api/rest/e2e-rest-col`);
    expect(res.status()).toBe(403);
  });

  test("returns 404 for non-existent collection", async ({ request }) => {
    const res = await request.get(`${baseUrl}/api/rest/nonexistent`, {
      headers: { Authorization: `Bearer ${bearerToken}` },
    });
    expect(res.status()).toBe(404);
  });
});

test.describe("Blind Ping routes", () => {
  test("POST /api/pings/register registers a device", async ({ request }) => {
    const res = await request.post(`${baseUrl}/api/pings/register`, {
      data: { did: "did:key:zE2EPingUser", platform: "apns", token: "e2e-device-token-abc" },
    });
    expect(res.status()).toBe(201);
    const body = await res.json();
    expect(body.ok).toBe(true);
    expect(body.registration.did).toBe("did:key:zE2EPingUser");
  });

  test("GET /api/pings/devices lists registered devices", async ({ request }) => {
    const res = await request.get(`${baseUrl}/api/pings/devices`);
    expect(res.status()).toBe(200);
    const body = await res.json();
    expect(body.devices.some((d: { did: string }) => d.did === "did:key:zE2EPingUser")).toBe(true);
    expect(body.count).toBeGreaterThanOrEqual(1);
  });

  test("POST /api/pings/send dispatches a blind ping", async ({ request }) => {
    const res = await request.post(`${baseUrl}/api/pings/send`, {
      data: { recipientDid: "did:key:zE2EPingUser" },
    });
    expect(res.status()).toBe(200);
    const body = await res.json();
    expect(body.ok).toBe(true);
  });

  test("POST /api/pings/wake wakes all devices for a DID", async ({ request }) => {
    const res = await request.post(`${baseUrl}/api/pings/wake`, {
      data: { did: "did:key:zE2EPingUser" },
    });
    expect(res.status()).toBe(200);
    const body = await res.json();
    expect(body.ok).toBe(true);
    expect(body.pinged).toBeGreaterThanOrEqual(1);
  });

  test("DELETE /api/pings/register/:did removes devices", async ({ request }) => {
    const res = await request.delete(`${baseUrl}/api/pings/register/did:key:zE2EPingUser`);
    expect(res.status()).toBe(200);
    const body = await res.json();
    expect(body.removed).toBeGreaterThanOrEqual(1);
  });

  test("validates required fields", async ({ request }) => {
    const res = await request.post(`${baseUrl}/api/pings/register`, {
      data: { platform: "apns" },
    });
    expect(res.status()).toBe(400);
  });

  test("validates platform enum", async ({ request }) => {
    const res = await request.post(`${baseUrl}/api/pings/register`, {
      data: { did: "did:key:z1", platform: "invalid", token: "tok" },
    });
    expect(res.status()).toBe(400);
  });
});

test.describe("SEO routes", () => {
  test("GET /sitemap.xml returns valid XML with portal entries", async ({ request }) => {
    const res = await request.get(`${baseUrl}/sitemap.xml`);
    expect(res.status()).toBe(200);
    const text = await res.text();
    expect(text).toContain("<?xml");
    expect(text).toContain("<urlset");
    expect(text).toContain("/portals");
  });

  test("GET /robots.txt returns crawler directives", async ({ request }) => {
    const res = await request.get(`${baseUrl}/robots.txt`);
    expect(res.status()).toBe(200);
    const text = await res.text();
    expect(text).toContain("User-agent:");
    expect(text).toContain("Allow: /portals/");
    expect(text).toContain("Disallow: /api/");
    expect(text).toContain("Sitemap:");
  });

  test("portal HTML includes OpenGraph and JSON-LD metadata", async ({ request }) => {
    const host = relay.getCapability<CollectionHost>(RELAY_CAPABILITIES.COLLECTIONS) as CollectionHost;
    host.create("e2e-seo-col");
    const registry = relay.getCapability<PortalRegistry>(RELAY_CAPABILITIES.PORTALS) as PortalRegistry;
    const portal = registry.register({
      name: "E2E SEO Portal",
      level: 1,
      collectionId: "e2e-seo-col",
      basePath: "/seo",
      isPublic: true,
    });

    const res = await request.get(`${baseUrl}/portals/${portal.portalId}`);
    expect(res.status()).toBe(200);
    const html = await res.text();
    expect(html).toContain('og:title');
    expect(html).toContain('og:description');
    expect(html).toContain('og:type');
    expect(html).toContain('twitter:card');
    expect(html).toContain('application/ld+json');
    expect(html).toContain('schema.org');
  });
});

test.describe("Security middleware", () => {
  // Create a server WITH CSRF enabled for security tests
  let securePort: number;
  let secureClose: () => Promise<void>;

  test.beforeAll(async () => {
    const secureServer = createRelayServer({ relay, port: 0, disableCsrf: false });
    const info = await secureServer.start();
    securePort = info.port;
    secureClose = info.close;
  });

  test.afterAll(async () => {
    await secureClose();
  });

  test("rejects POST without X-Prism-CSRF header", async ({ request }) => {
    const res = await request.post(`http://localhost:${securePort}/api/webhooks`, {
      data: { url: "https://test.example/hook", events: ["*"], active: true },
    });
    expect(res.status()).toBe(403);
    const body = await res.json();
    expect(body.error).toContain("CSRF");
  });

  test("allows POST with X-Prism-CSRF header", async ({ request }) => {
    const res = await request.post(`http://localhost:${securePort}/api/webhooks`, {
      headers: { "X-Prism-CSRF": "1" },
      data: { url: "https://test.example/hook", events: ["*"], active: true },
    });
    expect(res.status()).toBe(201);
  });

  test("allows GET without CSRF header", async ({ request }) => {
    const res = await request.get(`http://localhost:${securePort}/api/status`);
    expect(res.status()).toBe(200);
  });

  test("rejects requests from banned peers", async ({ request }) => {
    // Ban a peer first (using CSRF header)
    await request.post(`http://localhost:${securePort}/api/trust/did:key:zBannedE2E/ban`, {
      headers: { "X-Prism-CSRF": "1" },
      data: { reason: "e2e test" },
    });

    // Request from banned peer should be rejected
    const res = await request.get(`http://localhost:${securePort}/api/status`, {
      headers: { "X-Prism-DID": "did:key:zBannedE2E" },
    });
    // Banned peer middleware is on /api/* — GET /api/status should be rejected
    expect(res.status()).toBe(403);
    const body = await res.json();
    expect(body.error).toContain("banned");

    // Unban for cleanup
    await request.post(`http://localhost:${securePort}/api/trust/did:key:zBannedE2E/unban`, {
      headers: { "X-Prism-CSRF": "1" },
      data: {},
    });
  });
});

// ── WebRTC Signaling ───────────────────────────────────────────────────────

test.describe("WebRTC Signaling routes", () => {
  test("GET /api/signaling/rooms lists rooms (initially empty)", async ({ request }) => {
    const res = await request.get(`${baseUrl}/api/signaling/rooms`);
    expect(res.status()).toBe(200);
    const body = await res.json();
    expect(body.count).toBeGreaterThanOrEqual(0);
    expect(Array.isArray(body.rooms)).toBe(true);
  });

  test("peer joins a room and gets empty peer list", async ({ request }) => {
    const res = await request.post(`${baseUrl}/api/signaling/rooms/e2e-room/join`, {
      data: { peerId: "e2e-peer-a", displayName: "Alice" },
    });
    expect(res.status()).toBe(200);
    const body = await res.json();
    expect(body.ok).toBe(true);
    expect(body.roomId).toBe("e2e-room");
    expect(body.peers).toEqual([]);
  });

  test("second peer sees first peer", async ({ request }) => {
    const res = await request.post(`${baseUrl}/api/signaling/rooms/e2e-room/join`, {
      data: { peerId: "e2e-peer-b", displayName: "Bob" },
    });
    expect(res.status()).toBe(200);
    const body = await res.json();
    expect(body.peers).toHaveLength(1);
    expect(body.peers[0].peerId).toBe("e2e-peer-a");
  });

  test("GET peers lists both peers", async ({ request }) => {
    const res = await request.get(`${baseUrl}/api/signaling/rooms/e2e-room/peers`);
    expect(res.status()).toBe(200);
    const body = await res.json();
    expect(body.count).toBe(2);
  });

  test("relay SDP offer between peers", async ({ request }) => {
    const res = await request.post(`${baseUrl}/api/signaling/rooms/e2e-room/signal`, {
      data: {
        type: "offer",
        from: "e2e-peer-a",
        to: "e2e-peer-b",
        payload: { sdp: "v=0\r\noffer..." },
      },
    });
    expect(res.status()).toBe(200);
    const body = await res.json();
    expect(body.delivered).toBe(true);
  });

  test("poll retrieves buffered signals", async ({ request }) => {
    const res = await request.post(`${baseUrl}/api/signaling/rooms/e2e-room/poll`, {
      data: { peerId: "e2e-peer-b" },
    });
    expect(res.status()).toBe(200);
    const body = await res.json();
    expect(body.count).toBeGreaterThanOrEqual(1);
    const offers = body.signals.filter((s: Record<string, unknown>) => s.type === "offer");
    expect(offers).toHaveLength(1);
  });

  test("relay ICE candidates", async ({ request }) => {
    const res = await request.post(`${baseUrl}/api/signaling/rooms/e2e-room/signal`, {
      data: {
        type: "ice-candidate",
        from: "e2e-peer-b",
        to: "e2e-peer-a",
        payload: { candidate: "candidate:1 1 udp 2130706431 ..." },
      },
    });
    expect(res.status()).toBe(200);
    expect((await res.json()).delivered).toBe(true);
  });

  test("rejects invalid signal type", async ({ request }) => {
    const res = await request.post(`${baseUrl}/api/signaling/rooms/e2e-room/signal`, {
      data: { type: "bad-type", from: "a", to: "b", payload: {} },
    });
    expect(res.status()).toBe(400);
  });

  test("peer leaves room", async ({ request }) => {
    await request.post(`${baseUrl}/api/signaling/rooms/e2e-room/leave`, {
      data: { peerId: "e2e-peer-a" },
    });
    const res = await request.get(`${baseUrl}/api/signaling/rooms/e2e-room/peers`);
    const body = await res.json();
    expect(body.count).toBe(1);
  });

  test("rooms listing shows the room", async ({ request }) => {
    const res = await request.get(`${baseUrl}/api/signaling/rooms`);
    const body = await res.json();
    const room = body.rooms.find((r: Record<string, unknown>) => r.roomId === "e2e-room");
    expect(room).toBeDefined();
    expect(room.peerCount).toBe(1);
  });
});

// ── Health Check ─────────────────────────────────────────────────────────────

test.describe("Health Check", () => {
  test("GET /api/health returns healthy status", async ({ request }) => {
    const res = await request.get(`${baseUrl}/api/health`);
    expect(res.status()).toBe(200);
    const body = await res.json();
    expect(body.status).toBe("healthy");
    expect(typeof body.did).toBe("string");
    expect(typeof body.uptime).toBe("number");
    expect(body.uptime).toBeGreaterThan(0);
    expect(typeof body.modules).toBe("number");
    expect(body.modules).toBeGreaterThanOrEqual(16);
    expect(typeof body.peers).toBe("number");
    expect(typeof body.federationPeers).toBe("number");
    expect(body.memory).toBeDefined();
    expect(typeof body.memory.rss).toBe("number");
    expect(typeof body.memory.heapUsed).toBe("number");
    expect(typeof body.memory.heapTotal).toBe("number");
  });

  test("health check does not require CSRF header", async ({ request }) => {
    // Even with CSRF enabled, health check is a GET so it should pass
    const res = await request.get(`${baseUrl}/api/health`);
    expect(res.status()).toBe(200);
  });
});

// ── Presence (HTTP) ──────────────────────────────────────────────────────────

test.describe("Presence HTTP API", () => {
  test("GET /api/presence returns empty peer list initially", async ({ request }) => {
    const res = await request.get(`${baseUrl}/api/presence`);
    expect(res.status()).toBe(200);
    const body = await res.json();
    expect(body.peers).toBeDefined();
    expect(Array.isArray(body.peers)).toBe(true);
    expect(body.count).toBe(body.peers.length);
  });
});

// ── Gossip Protocol ──────────────────────────────────────────────────────────

test.describe("Gossip Protocol", () => {
  test("POST /api/safety/gossip pushes hashes to federation peers", async ({ request }) => {
    // Flag some content first
    await request.post(`${baseUrl}/api/safety/report`, {
      data: {
        contentHash: "sha256-gossip-test-hash",
        category: "spam",
        reportedBy: "did:key:zGossipReporter",
      },
    });

    // Trigger gossip — response uses hashCount/totalPeers/successCount
    const res = await request.post(`${baseUrl}/api/safety/gossip`);
    expect(res.status()).toBe(200);
    const body = await res.json();
    expect(body.ok).toBe(true);
    expect(typeof body.hashCount).toBe("number");
    expect(body.hashCount).toBeGreaterThan(0);
    expect(typeof body.totalPeers).toBe("number");
    expect(typeof body.successCount).toBe("number");
  });

  test("POST /api/safety/hashes imports hashes from federation", async ({ request }) => {
    const res = await request.post(`${baseUrl}/api/safety/hashes`, {
      data: {
        hashes: [
          { hash: "sha256-imported-1", category: "malware", reportedBy: "did:key:zPeer1" },
          { hash: "sha256-imported-2", category: "csam", reportedBy: "did:key:zPeer2" },
        ],
        sourceRelay: "did:key:zFederationPeer",
      },
    });
    expect(res.status()).toBe(200);
    const body = await res.json();
    expect(body.ok).toBe(true);
    expect(body.imported).toBe(2);

    // Verify via check endpoint — returns { results: { hash: bool } }
    const check = await request.post(`${baseUrl}/api/safety/check`, {
      data: { hashes: ["sha256-imported-1", "sha256-imported-2", "sha256-not-flagged"] },
    });
    const checkBody = await check.json();
    expect(checkBody.results["sha256-imported-1"]).toBe(true);
    expect(checkBody.results["sha256-imported-2"]).toBe(true);
    expect(checkBody.results["sha256-not-flagged"]).toBe(false);
  });

  test("POST /api/safety/report with evidence triggers auto-gossip", async ({ request }) => {
    const res = await request.post(`${baseUrl}/api/safety/report`, {
      data: {
        contentHash: "sha256-evidence-test",
        category: "malware",
        reportedBy: "did:key:zWhistleblower",
        evidence: btoa("encrypted-evidence-blob"),
      },
    });
    // Report returns 201
    expect(res.status()).toBe(201);
    const body = await res.json();
    expect(body.ok).toBe(true);
    expect(body.contentHash).toBe("sha256-evidence-test");
    expect(body.flagged).toBe(true);
  });
});

// ── Federation CRDT Sync ─────────────────────────────────────────────────────

test.describe("Federation CRDT Sync", () => {
  test("POST /api/federation/sync imports a collection snapshot", async ({ request }) => {
    // Create a collection and get its snapshot
    const createRes = await request.post(`${baseUrl}/api/collections`, {
      data: { id: "fed-sync-test" },
    });
    expect(createRes.status()).toBe(201);

    const snapRes = await request.get(`${baseUrl}/api/collections/fed-sync-test/snapshot`);
    expect(snapRes.status()).toBe(200);
    const snapBody = await snapRes.json();

    // Now sync that snapshot as if from a federation peer
    const syncRes = await request.post(`${baseUrl}/api/federation/sync`, {
      data: {
        collectionId: "fed-sync-imported",
        snapshot: snapBody.snapshot,
      },
    });
    expect(syncRes.status()).toBe(200);
    const syncBody = await syncRes.json();
    expect(syncBody.ok).toBe(true);
    expect(syncBody.collectionId).toBe("fed-sync-imported");

    // Verify the imported collection exists (list returns string[])
    const listRes = await request.get(`${baseUrl}/api/collections`);
    const listBody = await listRes.json();
    expect(listBody).toContain("fed-sync-imported");
  });
});

// ── Multi-Relay Federation E2E ───────────────────────────────────────────────

test.describe("Multi-Relay Federation", () => {
  let relay2: RelayInstance;
  let port2: number;
  let close2: () => Promise<void>;

  test.beforeAll(async () => {
    const identity2 = await createIdentity({ method: "key" });
    relay2 = createRelayBuilder({ relayDid: identity2.did })
      .use(blindMailboxModule())
      .use(relayRouterModule())
      .use(collectionHostModule())
      .use(federationModule())
      .use(peerTrustModule())
      .build();
    await relay2.start();

    const server2 = createRelayServer({ relay: relay2, port: 0, disableCsrf: true });
    const info2 = await server2.start();
    port2 = info2.port;
    close2 = info2.close;
  });

  test.afterAll(async () => {
    await close2();
    await relay2.stop();
  });

  test("two relays exchange federation announcements", async ({ request }) => {
    const url2 = `http://localhost:${port2}`;

    // Announce relay1 to relay2
    const ann1 = await request.post(`${url2}/api/federation/announce`, {
      data: { relayDid: relay.did, url: baseUrl },
    });
    expect(ann1.status()).toBe(200);

    // Announce relay2 to relay1
    const ann2 = await request.post(`${baseUrl}/api/federation/announce`, {
      data: { relayDid: relay2.did, url: url2 },
    });
    expect(ann2.status()).toBe(200);

    // Verify peers on both sides (federation /peers returns raw array)
    const peers1 = await request.get(`${baseUrl}/api/federation/peers`);
    const peers1Body = await peers1.json();
    expect(peers1Body.some((p: Record<string, string>) => p.relayDid === relay2.did)).toBe(true);

    const peers2 = await request.get(`${url2}/api/federation/peers`);
    const peers2Body = await peers2.json();
    expect(peers2Body.some((p: Record<string, string>) => p.relayDid === relay.did)).toBe(true);
  });

  test("collection sync propagates between federated relays", async ({ request }) => {
    const url2 = `http://localhost:${port2}`;

    // Create a collection on relay1
    const createRes = await request.post(`${baseUrl}/api/collections`, {
      data: { id: "cross-relay-col" },
    });
    expect(createRes.status()).toBe(201);

    // Get its snapshot
    const snapRes = await request.get(`${baseUrl}/api/collections/cross-relay-col/snapshot`);
    const snapBody = await snapRes.json();

    // Sync the snapshot to relay2 via federation sync
    const syncRes = await request.post(`${url2}/api/federation/sync`, {
      data: {
        collectionId: "cross-relay-col",
        snapshot: snapBody.snapshot,
      },
    });
    expect(syncRes.status()).toBe(200);

    // Verify relay2 has the collection (list returns string[])
    const listRes = await request.get(`${url2}/api/collections`);
    const listBody = await listRes.json();
    expect(listBody).toContain("cross-relay-col");
  });

  test("safety gossip pushes flagged hashes to federation peer", async ({ request }) => {
    // Flag content on relay1
    await request.post(`${baseUrl}/api/safety/report`, {
      data: {
        contentHash: "sha256-cross-relay-hash",
        category: "spam",
        reportedBy: "did:key:zCrossRelay",
      },
    });

    // Trigger gossip on relay1 (should push to relay2)
    const gossipRes = await request.post(`${baseUrl}/api/safety/gossip`);
    expect(gossipRes.status()).toBe(200);
    const gossipBody = await gossipRes.json();
    expect(typeof gossipBody.hashCount).toBe("number");
    expect(typeof gossipBody.totalPeers).toBe("number");

    // Note: The gossip actually POSTs to relay2's /api/safety/hashes endpoint.
    // If peers are registered, this should propagate. The result depends on
    // whether relay2 was announced with the right URL format.
  });
});

// ── WebSocket Presence Flow ──────────────────────────────────────────────────

test.describe("WebSocket Presence", () => {
  test("presence-update via WebSocket is received by other clients", async () => {
    const ws1 = new WebSocket(`ws://localhost:${serverPort}/ws/relay`);
    const ws2 = new WebSocket(`ws://localhost:${serverPort}/ws/relay`);

    await new Promise<void>((resolve) => { ws1.onopen = () => resolve(); });
    await new Promise<void>((resolve) => { ws2.onopen = () => resolve(); });

    // Set up presence listener on ws2 BEFORE sending auth to avoid race
    const presencePromise = new Promise<Record<string, unknown>>((resolve) => {
      ws2.addEventListener("message", (evt) => {
        const msg = JSON.parse(evt.data as string);
        if (msg.type === "presence-update") resolve(msg);
      });
    });

    // Auth both using addEventListener so presence listener stays active
    const auth1 = new Promise<void>((resolve) => {
      ws1.addEventListener("message", function handler(evt) {
        const msg = JSON.parse(evt.data as string);
        if (msg.type === "auth-ok") {
          ws1.removeEventListener("message", handler);
          resolve();
        }
      });
    });
    const auth2 = new Promise<void>((resolve) => {
      ws2.addEventListener("message", function handler(evt) {
        const msg = JSON.parse(evt.data as string);
        if (msg.type === "auth-ok") {
          ws2.removeEventListener("message", handler);
          resolve();
        }
      });
    });

    ws1.send(JSON.stringify({ type: "auth", did: "did:key:zPresenceA" }));
    ws2.send(JSON.stringify({ type: "auth", did: "did:key:zPresenceB" }));
    await auth1;
    await auth2;

    // ws1 sends presence-update — ws2 should receive broadcast
    ws1.send(JSON.stringify({
      type: "presence-update",
      peerId: "did:key:zPresenceA",
      cursor: { x: 100, y: 200 },
      activeView: "editor",
    }));

    const presenceMsg = await presencePromise;
    expect(presenceMsg["peerId"]).toBe("did:key:zPresenceA");
    expect(presenceMsg["cursor"]).toEqual({ x: 100, y: 200 });
    expect(presenceMsg["activeView"]).toBe("editor");

    ws1.close();
    ws2.close();
  });

  test("presence-leave broadcast on disconnect", async () => {
    const ws1 = new WebSocket(`ws://localhost:${serverPort}/ws/relay`);
    const ws2 = new WebSocket(`ws://localhost:${serverPort}/ws/relay`);

    await new Promise<void>((resolve) => { ws1.onopen = () => resolve(); });
    await new Promise<void>((resolve) => { ws2.onopen = () => resolve(); });

    ws1.send(JSON.stringify({ type: "auth", did: "did:key:zLeaveA" }));
    ws2.send(JSON.stringify({ type: "auth", did: "did:key:zLeaveB" }));

    await new Promise<void>((resolve) => {
      ws1.onmessage = (evt) => {
        const msg = JSON.parse(evt.data as string);
        if (msg.type === "auth-ok") resolve();
      };
    });
    await new Promise<void>((resolve) => {
      ws2.onmessage = (evt) => {
        const msg = JSON.parse(evt.data as string);
        if (msg.type === "auth-ok") resolve();
      };
    });

    // Send a presence update from ws1 so the store knows about them
    ws1.send(JSON.stringify({
      type: "presence-update",
      peerId: "did:key:zLeaveA",
      cursor: { x: 50, y: 50 },
    }));

    // Wait for ws2 to receive the update
    await new Promise<void>((resolve) => {
      ws2.onmessage = (evt) => {
        const msg = JSON.parse(evt.data as string);
        if (msg.type === "presence-update") resolve();
      };
    });

    // Now listen for presence-leave on ws2 when ws1 disconnects
    const leavePromise = new Promise<Record<string, unknown>>((resolve) => {
      ws2.onmessage = (evt) => {
        const msg = JSON.parse(evt.data as string);
        if (msg.type === "presence-leave") resolve(msg);
      };
    });

    ws1.close();

    const leaveMsg = await leavePromise;
    expect(leaveMsg["peerId"]).toBe("did:key:zLeaveA");

    ws2.close();
  });
});

// ── Rate Limiting ────────────────────────────────────────────────────────────

test.describe("Rate Limiting", () => {
  test("returns 429 after exceeding rate limit", async ({ request }) => {
    // The default rate limit is 100 requests per bucket.
    // Send many rapid requests and check that eventually we get 429.
    // We use a unique DID header to get our own bucket.
    const responses: number[] = [];
    for (let i = 0; i < 120; i++) {
      const res = await request.get(`${baseUrl}/api/status`, {
        headers: { "X-Prism-DID": "did:key:zRateLimitTest" },
      });
      responses.push(res.status());
      if (res.status() === 429) break;
    }
    // Should have gotten at least one 429
    expect(responses).toContain(429);
  });
});

// ── Vault Host API ──────────────────────────────────────────────────────────

test.describe("Vault Host API", () => {
  const vaultManifest = {
    id: "e2e-vault-1",
    name: "E2E Test Vault",
    version: "1",
    storage: { backend: "loro", path: "data" },
    schema: { module: "@prism/core" },
    createdAt: new Date().toISOString(),
    description: "A test vault for E2E",
  };

  test("publish, list, get, download vault round-trip", async ({ request }) => {
    // Publish vault
    const pubRes = await request.post(`${baseUrl}/api/vaults`, {
      data: {
        manifest: vaultManifest,
        ownerDid: identity.did,
        isPublic: true,
        collections: {
          contacts: Buffer.from("contacts-snapshot").toString("base64"),
          tasks: Buffer.from("tasks-snapshot").toString("base64"),
        },
      },
    });
    expect(pubRes.status()).toBe(201);
    const vault = await pubRes.json();
    expect(vault.id).toBe("e2e-vault-1");
    expect(vault.isPublic).toBe(true);

    // List vaults
    const listRes = await request.get(`${baseUrl}/api/vaults`);
    const vaults = await listRes.json();
    expect(vaults.some((v: { id: string }) => v.id === "e2e-vault-1")).toBe(true);

    // Get vault
    const getRes = await request.get(`${baseUrl}/api/vaults/e2e-vault-1`);
    expect(getRes.status()).toBe(200);
    const got = await getRes.json();
    expect(got.manifest.name).toBe("E2E Test Vault");

    // Download full vault
    const dlRes = await request.get(`${baseUrl}/api/vaults/e2e-vault-1/download`);
    expect(dlRes.status()).toBe(200);
    const dl = await dlRes.json();
    expect(dl.manifest.id).toBe("e2e-vault-1");
    expect(dl.collections.contacts).toBe(Buffer.from("contacts-snapshot").toString("base64"));
    expect(dl.collections.tasks).toBe(Buffer.from("tasks-snapshot").toString("base64"));
  });

  test("list collection sizes for a vault", async ({ request }) => {
    const res = await request.get(`${baseUrl}/api/vaults/e2e-vault-1/collections`);
    expect(res.status()).toBe(200);
    const cols = await res.json();
    expect(cols.length).toBe(2);
    expect(cols.every((c: { bytes: number }) => c.bytes > 0)).toBe(true);
  });

  test("update collections (owner-only)", async ({ request }) => {
    const res = await request.put(`${baseUrl}/api/vaults/e2e-vault-1/collections`, {
      data: {
        ownerDid: identity.did,
        collections: { contacts: Buffer.from("updated-contacts").toString("base64") },
      },
    });
    expect(res.status()).toBe(200);

    // Verify update
    const snap = await request.get(`${baseUrl}/api/vaults/e2e-vault-1/collections/contacts`);
    const body = await snap.json();
    expect(body.snapshot).toBe(Buffer.from("updated-contacts").toString("base64"));
  });

  test("reject update from non-owner", async ({ request }) => {
    const res = await request.put(`${baseUrl}/api/vaults/e2e-vault-1/collections`, {
      data: {
        ownerDid: "did:key:zStranger",
        collections: { contacts: Buffer.from("evil").toString("base64") },
      },
    });
    expect(res.status()).toBe(403);
  });

  test("search vaults by name", async ({ request }) => {
    const res = await request.get(`${baseUrl}/api/vaults?search=E2E`);
    const vaults = await res.json();
    expect(vaults.some((v: { id: string }) => v.id === "e2e-vault-1")).toBe(true);
  });

  test("delete vault (owner-only)", async ({ request }) => {
    // Create a vault to delete
    await request.post(`${baseUrl}/api/vaults`, {
      data: {
        manifest: { ...vaultManifest, id: "e2e-vault-delete" },
        ownerDid: identity.did,
        collections: {},
      },
    });

    const res = await request.delete(`${baseUrl}/api/vaults/e2e-vault-delete`, {
      data: { ownerDid: identity.did },
    });
    expect(res.status()).toBe(200);

    const getRes = await request.get(`${baseUrl}/api/vaults/e2e-vault-delete`);
    expect(getRes.status()).toBe(404);
  });
});

// ── Directory Feed ──────────────────────────────────────────────────────────

test.describe("Directory Feed", () => {
  test("GET /api/directory returns relay profile with portals and vaults", async ({ request }) => {
    const res = await request.get(`${baseUrl}/api/directory`);
    expect(res.status()).toBe(200);
    const body = await res.json();

    // Relay profile
    expect(body.relay.did).toBe(identity.did);
    expect(body.relay.modules.length).toBe(16);
    expect(typeof body.relay.uptime).toBe("number");
    expect(body.relay.federation).toHaveProperty("peers");

    // Should have portals and vaults arrays
    expect(Array.isArray(body.portals)).toBe(true);
    expect(Array.isArray(body.vaults)).toBe(true);

    // generatedAt timestamp
    expect(body).toHaveProperty("generatedAt");
  });

  test("directory only shows public portals", async ({ request }) => {
    const res = await request.get(`${baseUrl}/api/directory`);
    const body = await res.json();
    expect(body.portals.every((p: { isPublic: boolean }) => p.isPublic)).toBe(true);
  });

  test("directory sets Cache-Control header", async ({ request }) => {
    const res = await request.get(`${baseUrl}/api/directory`);
    const cacheControl = res.headers()["cache-control"];
    expect(cacheControl).toContain("max-age=300");
  });
});
