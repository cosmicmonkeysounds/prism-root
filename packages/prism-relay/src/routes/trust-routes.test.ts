import { describe, it, expect, beforeAll } from "vitest";
import { createIdentity } from "@prism/core/identity";
import type { PrismIdentity } from "@prism/core/identity";
import { createRelayBuilder, blindMailboxModule, peerTrustModule } from "@prism/core/relay";
import type { RelayInstance } from "@prism/core/relay";
import { createTrustRoutes } from "./trust-routes.js";

let relay: RelayInstance;
let relayNoTrust: RelayInstance;
let identity: PrismIdentity;

beforeAll(async () => {
  identity = await createIdentity({ method: "key" });
  relay = createRelayBuilder({ relayDid: identity.did })
    .use(peerTrustModule())
    .build();
  await relay.start();

  relayNoTrust = createRelayBuilder({ relayDid: identity.did })
    .use(blindMailboxModule())
    .build();
  await relayNoTrust.start();
});

// Hono decodes %-encoded path params, so we must encode colons in DID strings
const PEER_DID = "did:key:zBannedPeer";
const PEER_DID_ENCODED = encodeURIComponent(PEER_DID);
const UNKNOWN_DID_ENCODED = encodeURIComponent("did:key:zUnknownPeer");

describe("trust-routes", () => {
  it("returns 404 when trust module not installed", async () => {
    const app = createTrustRoutes(relayNoTrust);
    const res = await app.request("/");
    expect(res.status).toBe(404);
  });

  it("list returns empty initially", async () => {
    const app = createTrustRoutes(relay);
    const res = await app.request("/");
    expect(res.status).toBe(200);
    const body = (await res.json()) as unknown[];
    expect(Array.isArray(body)).toBe(true);
  });

  it("ban a peer then list shows banned peer", async () => {
    const app = createTrustRoutes(relay);

    // Ban the peer
    const banRes = await app.request(`/${PEER_DID_ENCODED}/ban`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ reason: "spam" }),
    });
    expect(banRes.status).toBe(200);
    const banBody = (await banRes.json()) as { ok: boolean };
    expect(banBody.ok).toBe(true);

    // List should include the banned peer
    const listRes = await app.request("/");
    expect(listRes.status).toBe(200);
    const list = (await listRes.json()) as Array<Record<string, unknown>>;
    expect(list.some((p) => p["peerId"] === PEER_DID)).toBe(true);
  });

  it("get banned peer returns peer info", async () => {
    const app = createTrustRoutes(relay);

    const res = await app.request(`/${PEER_DID_ENCODED}`);
    expect(res.status).toBe(200);
    const body = (await res.json()) as Record<string, unknown>;
    expect(body["peerId"]).toBe(PEER_DID);
    expect(body["banned"]).toBe(true);
  });

  it("unban a peer then get returns updated state", async () => {
    const app = createTrustRoutes(relay);

    // Unban
    const unbanRes = await app.request(`/${PEER_DID_ENCODED}/unban`, {
      method: "POST",
    });
    expect(unbanRes.status).toBe(200);

    // Get peer info
    const res = await app.request(`/${PEER_DID_ENCODED}`);
    expect(res.status).toBe(200);
    const body = (await res.json()) as Record<string, unknown>;
    expect(body["peerId"]).toBe(PEER_DID);
    expect(body["banned"]).toBe(false);
  });

  it("get unknown peer returns 404", async () => {
    const app = createTrustRoutes(relay);
    const res = await app.request(`/${UNKNOWN_DID_ENCODED}`);
    expect(res.status).toBe(404);
  });
});
