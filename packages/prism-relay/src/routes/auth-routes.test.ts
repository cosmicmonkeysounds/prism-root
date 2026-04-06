import { describe, it, expect, beforeAll, afterAll } from "vitest";
import { createIdentity } from "@prism/core/identity";
import type { PrismIdentity } from "@prism/core/identity";
import {
  createRelayBuilder,
  blindMailboxModule,
  relayRouterModule,
  capabilityTokenModule,
  escrowModule,
} from "@prism/core/relay";
import type { RelayInstance } from "@prism/core/relay";
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
    .use(capabilityTokenModule(identity))
    .use(escrowModule())
    .build();
  await relay.start();

  const server = createRelayServer({ relay, port: 0, disableCsrf: true });
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

describe("auth-routes", () => {
  it("GET /api/auth/providers lists available providers", async () => {
    const res = await fetch(url("/api/auth/providers"));
    expect(res.status).toBe(200);
    const body = await res.json() as { providers: string[] };
    // No providers configured in test
    expect(body.providers).toEqual([]);
  });

  it("GET /api/auth/google returns 404 when not configured", async () => {
    const res = await fetch(url("/api/auth/google"), { redirect: "manual" });
    expect(res.status).toBe(404);
  });

  it("GET /api/auth/github returns 404 when not configured", async () => {
    const res = await fetch(url("/api/auth/github"), { redirect: "manual" });
    expect(res.status).toBe(404);
  });

  it("POST /api/auth/escrow/derive creates escrow deposit", async () => {
    const res = await fetch(url("/api/auth/escrow/derive"), {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        depositorId: "did:key:test-user",
        password: "hunter2",
        oauthSalt: "google-salt-abc123",
        encryptedVaultKey: "encrypted-key-data-here",
      }),
    });
    expect(res.status).toBe(201);
    const body = await res.json() as { ok: boolean; depositId: string };
    expect(body.ok).toBe(true);
    expect(body.depositId).toBeTruthy();
  });

  it("POST /api/auth/escrow/recover finds matching deposit", async () => {
    const res = await fetch(url("/api/auth/escrow/recover"), {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        depositorId: "did:key:test-user",
        password: "hunter2",
        oauthSalt: "google-salt-abc123",
      }),
    });
    expect(res.status).toBe(200);
    const body = await res.json() as { ok: boolean; encryptedVaultKey: string };
    expect(body.ok).toBe(true);
    expect(body.encryptedVaultKey).toBe("encrypted-key-data-here");
  });

  it("POST /api/auth/escrow/recover rejects wrong password", async () => {
    const res = await fetch(url("/api/auth/escrow/recover"), {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        depositorId: "did:key:test-user",
        password: "wrong-password",
        oauthSalt: "google-salt-abc123",
      }),
    });
    expect(res.status).toBe(403);
  });

  it("validates required fields on escrow derive", async () => {
    const res = await fetch(url("/api/auth/escrow/derive"), {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ depositorId: "test" }),
    });
    expect(res.status).toBe(400);
  });
});
