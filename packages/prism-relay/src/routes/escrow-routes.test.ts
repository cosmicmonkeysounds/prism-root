import { describe, it, expect, beforeAll } from "vitest";
import { createIdentity } from "@prism/core/identity";
import type { PrismIdentity } from "@prism/core/identity";
import { createRelayBuilder, blindMailboxModule, escrowModule } from "@prism/core/relay";
import type { RelayInstance } from "@prism/core/relay";
import { createEscrowRoutes } from "./escrow-routes.js";

let relay: RelayInstance;
let relayNoEscrow: RelayInstance;
let identity: PrismIdentity;

beforeAll(async () => {
  identity = await createIdentity({ method: "key" });
  relay = createRelayBuilder({ relayDid: identity.did })
    .use(escrowModule())
    .build();
  await relay.start();

  relayNoEscrow = createRelayBuilder({ relayDid: identity.did })
    .use(blindMailboxModule())
    .build();
  await relayNoEscrow.start();
});

describe("escrow-routes", () => {
  it("returns 404 when escrow module not installed", async () => {
    const app = createEscrowRoutes(relayNoEscrow);
    const res = await app.request("/deposit", { method: "POST" });
    expect(res.status).toBe(404);
  });

  it("deposit creates and returns deposit", async () => {
    const app = createEscrowRoutes(relay);
    const res = await app.request("/deposit", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        depositorId: "did:key:alice",
        encryptedPayload: "encrypted-secret-data",
      }),
    });
    expect(res.status).toBe(201);
    const body = (await res.json()) as Record<string, unknown>;
    expect(typeof body["id"]).toBe("string");
    expect(body["depositorId"]).toBe("did:key:alice");
    expect(body["encryptedPayload"]).toBe("encrypted-secret-data");
  });

  it("list deposits returns the deposited item", async () => {
    const app = createEscrowRoutes(relay);

    // Deposit first
    const depositRes = await app.request("/deposit", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        depositorId: "did:key:bob",
        encryptedPayload: "bob-secret",
      }),
    });
    expect(depositRes.status).toBe(201);

    // List deposits for bob
    const res = await app.request("/did:key:bob");
    expect(res.status).toBe(200);
    const body = (await res.json()) as Array<Record<string, unknown>>;
    expect(body.length).toBeGreaterThanOrEqual(1);
    expect(body.some((d) => d["encryptedPayload"] === "bob-secret")).toBe(true);
  });

  it("claim returns the deposit", async () => {
    const app = createEscrowRoutes(relay);

    // Deposit
    const depositRes = await app.request("/deposit", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        depositorId: "did:key:charlie",
        encryptedPayload: "charlie-secret",
      }),
    });
    const deposit = (await depositRes.json()) as Record<string, unknown>;

    // Claim
    const res = await app.request("/claim", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ depositId: deposit["id"] }),
    });
    expect(res.status).toBe(200);
    const body = (await res.json()) as Record<string, unknown>;
    expect(body["encryptedPayload"]).toBe("charlie-secret");
  });

  it("claim same deposit again returns 404 (already claimed)", async () => {
    const app = createEscrowRoutes(relay);

    // Deposit
    const depositRes = await app.request("/deposit", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        depositorId: "did:key:dave",
        encryptedPayload: "dave-secret",
      }),
    });
    const deposit = (await depositRes.json()) as Record<string, unknown>;

    // First claim
    await app.request("/claim", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ depositId: deposit["id"] }),
    });

    // Second claim
    const res = await app.request("/claim", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ depositId: deposit["id"] }),
    });
    expect(res.status).toBe(404);
  });

  it("list deposits for unknown depositor returns empty array", async () => {
    const app = createEscrowRoutes(relay);
    const res = await app.request("/did:key:unknown-depositor");
    expect(res.status).toBe(200);
    const body = (await res.json()) as unknown[];
    expect(body).toEqual([]);
  });
});
