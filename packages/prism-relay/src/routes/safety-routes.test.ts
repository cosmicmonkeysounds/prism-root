import { describe, it, expect, beforeAll, afterAll } from "vitest";
import { createIdentity } from "@prism/core/identity";
import type { PrismIdentity } from "@prism/core/identity";
import {
  createRelayBuilder,
  blindMailboxModule,
  relayRouterModule,
  peerTrustModule,
  federationModule,
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
    .use(peerTrustModule())
    .use(federationModule())
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

describe("safety-routes", () => {
  it("POST /api/safety/report flags content", async () => {
    const res = await fetch(url("/api/safety/report"), {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        contentHash: "abc123",
        category: "malware",
        reportedBy: "did:key:reporter",
      }),
    });
    expect(res.status).toBe(201);
    const body = await res.json() as { ok: boolean; flagged: boolean };
    expect(body.ok).toBe(true);
    expect(body.flagged).toBe(true);
  });

  it("GET /api/safety/hashes lists flagged hashes", async () => {
    const res = await fetch(url("/api/safety/hashes"));
    expect(res.status).toBe(200);
    const body = await res.json() as { hashes: unknown[]; count: number };
    expect(body.count).toBeGreaterThanOrEqual(1);
    expect(body.hashes).toEqual(
      expect.arrayContaining([
        expect.objectContaining({ hash: "abc123", category: "malware" }),
      ]),
    );
  });

  it("POST /api/safety/check verifies hash status", async () => {
    const res = await fetch(url("/api/safety/check"), {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ hashes: ["abc123", "clean456"] }),
    });
    expect(res.status).toBe(200);
    const body = await res.json() as { results: Record<string, boolean> };
    expect(body.results["abc123"]).toBe(true);
    expect(body.results["clean456"]).toBe(false);
  });

  it("POST /api/safety/hashes imports hashes from federated peer", async () => {
    const res = await fetch(url("/api/safety/hashes"), {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        hashes: [
          { hash: "fed789", category: "csam", reportedBy: "did:key:fed-relay" },
        ],
        sourceRelay: "did:key:peer-relay",
      }),
    });
    expect(res.status).toBe(200);
    const body = await res.json() as { ok: boolean; imported: number };
    expect(body.ok).toBe(true);
    expect(body.imported).toBe(1);
  });

  it("POST /api/safety/report validates required fields", async () => {
    const res = await fetch(url("/api/safety/report"), {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ contentHash: "only-hash" }),
    });
    expect(res.status).toBe(400);
  });
});
