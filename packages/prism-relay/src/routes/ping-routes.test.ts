import { describe, it, expect, beforeAll, afterAll } from "vitest";
import { createIdentity } from "@prism/core/identity";
import type { PrismIdentity } from "@prism/core/identity";
import {
  createRelayBuilder,
  blindMailboxModule,
  relayRouterModule,
  blindPingModule,
  createMemoryPingTransport,
} from "@prism/core/relay";
import type { RelayInstance, BlindPinger } from "@prism/core/relay";
import { RELAY_CAPABILITIES } from "@prism/core/relay";
import { createRelayServer } from "../server/relay-server.js";

let relay: RelayInstance;
let identity: PrismIdentity;
let port: number;
let close: () => Promise<void>;
let transport: ReturnType<typeof createMemoryPingTransport>;

beforeAll(async () => {
  identity = await createIdentity({ method: "key" });
  relay = createRelayBuilder({ relayDid: identity.did })
    .use(blindMailboxModule())
    .use(relayRouterModule())
    .use(blindPingModule())
    .build();
  await relay.start();

  // Wire in memory transport for testing
  transport = createMemoryPingTransport();
  const pinger = relay.getCapability<BlindPinger>(RELAY_CAPABILITIES.PINGER);
  if (!pinger) throw new Error("pinger not available");
  pinger.setTransport(transport);

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

describe("ping-routes", () => {
  it("POST /api/pings/register registers a device", async () => {
    const res = await fetch(url("/api/pings/register"), {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        did: "did:key:mobile-user",
        platform: "fcm",
        token: "fcm-token-123",
      }),
    });
    expect(res.status).toBe(201);
    const body = await res.json() as { ok: boolean };
    expect(body.ok).toBe(true);
  });

  it("GET /api/pings/devices lists registered devices", async () => {
    const res = await fetch(url("/api/pings/devices"));
    expect(res.status).toBe(200);
    const body = await res.json() as { devices: unknown[]; count: number };
    expect(body.count).toBeGreaterThanOrEqual(1);
  });

  it("POST /api/pings/send sends a blind ping", async () => {
    const res = await fetch(url("/api/pings/send"), {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ recipientDid: "did:key:mobile-user" }),
    });
    expect(res.status).toBe(200);
    const body = await res.json() as { ok: boolean; sent: boolean };
    expect(body.ok).toBe(true);
    expect(body.sent).toBe(true);
    expect(transport.sent.length).toBeGreaterThanOrEqual(1);
  });

  it("validates required fields", async () => {
    const res = await fetch(url("/api/pings/register"), {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ did: "test" }),
    });
    expect(res.status).toBe(400);
  });

  it("validates platform enum", async () => {
    const res = await fetch(url("/api/pings/register"), {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ did: "test", platform: "invalid", token: "t" }),
    });
    expect(res.status).toBe(400);
  });

  it("DELETE /api/pings/register/:did removes device", async () => {
    const res = await fetch(url("/api/pings/register/did:key:mobile-user"), {
      method: "DELETE",
    });
    expect(res.status).toBe(200);
    const body = await res.json() as { removed: number };
    expect(body.removed).toBeGreaterThanOrEqual(1);
  });
});
