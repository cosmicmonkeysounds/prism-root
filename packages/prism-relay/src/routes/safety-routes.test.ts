import { describe, it, expect, beforeAll, afterAll } from "vitest";
import { createIdentity } from "@prism/core/identity";
import {
  createRelayBuilder,
  blindMailboxModule,
  relayRouterModule,
  peerTrustModule,
  federationModule,
  escrowModule,
  RELAY_CAPABILITIES,
} from "@prism/core/relay";
import type { RelayInstance, FederationRegistry } from "@prism/core/relay";
import type { EscrowManager } from "@prism/core/trust";
import { createRelayServer } from "../server/relay-server.js";
import type { GossipSummary } from "./safety-routes.js";

/** Spin up a relay with trust + federation + escrow, return test helpers. */
async function createTestRelay(): Promise<{
  relay: RelayInstance;
  port: number;
  close: () => Promise<void>;
  url: (path: string) => string;
}> {
  const identity = await createIdentity({ method: "key" });
  const relay = createRelayBuilder({ relayDid: identity.did })
    .use(blindMailboxModule())
    .use(relayRouterModule())
    .use(peerTrustModule())
    .use(federationModule())
    .use(escrowModule())
    .build();
  await relay.start();

  const server = createRelayServer({ relay, port: 0, disableCsrf: true });
  const info = await server.start();
  return {
    relay,
    port: info.port,
    close: info.close,
    url: (path: string) => `http://127.0.0.1:${info.port}${path}`,
  };
}

// ── Primary relay (the one we test against) ─────────────────────────────────

let relay: RelayInstance;
let close: () => Promise<void>;
let url: (path: string) => string;

// ── Peer relay (receives gossip) ────────────────────────────────────────────

let peerRelay: RelayInstance;
let peerPort: number;
let peerClose: () => Promise<void>;
let peerUrl: (path: string) => string;

beforeAll(async () => {
  const main = await createTestRelay();
  relay = main.relay;
  close = main.close;
  url = main.url;

  const peer = await createTestRelay();
  peerRelay = peer.relay;
  peerPort = peer.port;
  peerClose = peer.close;
  peerUrl = peer.url;

  // Register the peer relay in the main relay's federation registry
  const federation = relay.getCapability<FederationRegistry>(RELAY_CAPABILITIES.FEDERATION);
  if (federation) {
    federation.announce(peerRelay.did, `http://127.0.0.1:${peerPort}`);
  }
});

afterAll(async () => {
  await close();
  await relay.stop();
  await peerClose();
  await peerRelay.stop();
});

// ── Whistleblower Packet Tests ──────────────────────────────────────────────

describe("whistleblower packets", () => {
  it("POST /api/safety/report flags content and returns 201", async () => {
    const res = await fetch(url("/api/safety/report"), {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        contentHash: "sha256-abc123",
        category: "malware",
        reportedBy: "did:key:reporter",
      }),
    });
    expect(res.status).toBe(201);
    const body = await res.json() as { ok: boolean; flagged: boolean; contentHash: string; escrowId: string | null };
    expect(body.ok).toBe(true);
    expect(body.flagged).toBe(true);
    expect(body.contentHash).toBe("sha256-abc123");
    expect(body.escrowId).toBeNull();
  });

  it("POST /api/safety/report validates required fields", async () => {
    const res = await fetch(url("/api/safety/report"), {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ contentHash: "only-hash" }),
    });
    expect(res.status).toBe(400);
  });

  it("POST /api/safety/report rejects invalid category", async () => {
    const res = await fetch(url("/api/safety/report"), {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        contentHash: "sha256-bad",
        category: "invalid-category",
        reportedBy: "did:key:reporter",
      }),
    });
    expect(res.status).toBe(400);
    const body = await res.json() as { error: string };
    expect(body.error).toContain("category must be one of");
  });

  it("POST /api/safety/report stores evidence as escrow deposit", async () => {
    const evidenceBlob = "base64-encrypted-evidence-payload";
    const res = await fetch(url("/api/safety/report"), {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        contentHash: "sha256-with-evidence",
        category: "csam",
        reportedBy: "did:key:whistleblower",
        evidence: evidenceBlob,
      }),
    });
    expect(res.status).toBe(201);
    const body = await res.json() as { ok: boolean; escrowId: string | null };
    expect(body.ok).toBe(true);
    expect(body.escrowId).toBeTruthy();

    // Verify the escrow deposit exists and can be claimed once
    const escrow = relay.getCapability<EscrowManager>(RELAY_CAPABILITIES.ESCROW);
    expect(escrow).toBeDefined();
    const deposit = escrow?.get(body.escrowId as string);
    expect(deposit).toBeDefined();
    expect(deposit?.encryptedPayload).toBe(evidenceBlob);
    expect(deposit?.claimed).toBe(false);

    // Claim the evidence (one-time access)
    const claimed = escrow?.claim(body.escrowId as string);
    expect(claimed).toBeDefined();
    expect(claimed?.claimed).toBe(true);

    // Second claim should fail (one-time)
    const secondClaim = escrow?.claim(body.escrowId as string);
    expect(secondClaim).toBeNull();
  });

  it("POST /api/safety/report auto-triggers gossip to federation peers", async () => {
    const res = await fetch(url("/api/safety/report"), {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        contentHash: "sha256-auto-gossip",
        category: "spam",
        reportedBy: "did:key:reporter2",
      }),
    });
    expect(res.status).toBe(201);
    const body = await res.json() as { ok: boolean; gossip: GossipSummary | null };
    expect(body.ok).toBe(true);
    // Gossip should have fired since federation is enabled
    expect(body.gossip).toBeTruthy();
    expect(body.gossip?.totalPeers).toBe(1);
    expect(body.gossip?.successCount).toBe(1);
    expect(body.gossip?.hashCount).toBeGreaterThanOrEqual(1);
  });

  it("POST /api/safety/report accepts all valid categories", async () => {
    for (const category of ["spam", "csam", "malware", "other"]) {
      const res = await fetch(url("/api/safety/report"), {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          contentHash: `sha256-cat-${category}`,
          category,
          reportedBy: "did:key:category-tester",
        }),
      });
      expect(res.status).toBe(201);
    }
  });
});

// ── Toxic Hash Gossip Tests ─────────────────────────────────────────────────

describe("toxic hash gossip", () => {
  it("POST /api/safety/gossip sends hashes to all peers", async () => {
    const res = await fetch(url("/api/safety/gossip"), {
      method: "POST",
      headers: { "Content-Type": "application/json" },
    });
    expect(res.status).toBe(200);
    const body = await res.json() as GossipSummary;
    expect(body.ok).toBe(true);
    expect(body.totalPeers).toBe(1);
    expect(body.successCount).toBe(1);
    expect(body.failedCount).toBe(0);
    expect(body.hashCount).toBeGreaterThanOrEqual(1);
    expect(body.peers).toHaveLength(1);
    expect(body.peers[0]?.success).toBe(true);
    expect(body.peers[0]?.relayDid).toBe(peerRelay.did);
  });

  it("POST /api/safety/gossip returns per-peer results with errors for unreachable peers", async () => {
    // Add a fake unreachable peer
    const federation = relay.getCapability<FederationRegistry>(RELAY_CAPABILITIES.FEDERATION);
    federation?.announce("did:key:dead-peer" as ReturnType<typeof relay.did extends string ? never : never>, "http://127.0.0.1:1");

    const res = await fetch(url("/api/safety/gossip"), {
      method: "POST",
      headers: { "Content-Type": "application/json" },
    });
    expect(res.status).toBe(200);
    const body = await res.json() as GossipSummary;
    expect(body.ok).toBe(true);
    expect(body.totalPeers).toBe(2);
    // At least one should succeed (the real peer), at least one should fail (the fake peer)
    expect(body.successCount).toBeGreaterThanOrEqual(1);
    expect(body.failedCount).toBeGreaterThanOrEqual(1);

    const failedPeer = body.peers.find((p) => !p.success);
    expect(failedPeer).toBeDefined();
    expect(failedPeer?.error).toBeTruthy();
  });

  it("POST /api/safety/gossip returns empty when no hashes are flagged", async () => {
    // Create a fresh relay with no flagged content
    const freshIdentity = await createIdentity({ method: "key" });
    const freshRelay = createRelayBuilder({ relayDid: freshIdentity.did })
      .use(peerTrustModule())
      .use(federationModule())
      .build();
    await freshRelay.start();

    const server = createRelayServer({ relay: freshRelay, port: 0, disableCsrf: true });
    const info = await server.start();

    try {
      const res = await fetch(`http://127.0.0.1:${info.port}/api/safety/gossip`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
      });
      expect(res.status).toBe(200);
      const body = await res.json() as GossipSummary;
      expect(body.ok).toBe(true);
      expect(body.hashCount).toBe(0);
      expect(body.totalPeers).toBe(0);
      expect(body.peers).toHaveLength(0);
    } finally {
      await info.close();
      await freshRelay.stop();
    }
  });
});

// ── Hash Import Tests ───────────────────────────────────────────────────────

describe("hash import from peers", () => {
  it("POST /api/safety/hashes imports new hashes", async () => {
    const res = await fetch(url("/api/safety/hashes"), {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        hashes: [
          { hash: "sha256-imported-1", category: "spam", reportedBy: "did:key:remote-relay" },
          { hash: "sha256-imported-2", category: "malware", reportedBy: "did:key:remote-relay" },
        ],
        sourceRelay: "did:key:peer-relay",
      }),
    });
    expect(res.status).toBe(200);
    const body = await res.json() as { ok: boolean; imported: number; total: number };
    expect(body.ok).toBe(true);
    expect(body.imported).toBe(2);
  });

  it("POST /api/safety/hashes skips already-flagged hashes", async () => {
    // Import the same hash again
    const res = await fetch(url("/api/safety/hashes"), {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        hashes: [
          { hash: "sha256-imported-1", category: "spam", reportedBy: "did:key:remote-relay" },
        ],
        sourceRelay: "did:key:peer-relay",
      }),
    });
    expect(res.status).toBe(200);
    const body = await res.json() as { ok: boolean; imported: number };
    expect(body.ok).toBe(true);
    expect(body.imported).toBe(0);
  });

  it("POST /api/safety/hashes validates hashes array", async () => {
    const res = await fetch(url("/api/safety/hashes"), {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ sourceRelay: "did:key:peer" }),
    });
    expect(res.status).toBe(400);
  });

  it("POST /api/safety/hashes validates sourceRelay", async () => {
    const res = await fetch(url("/api/safety/hashes"), {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        hashes: [{ hash: "x", category: "spam", reportedBy: "did:key:a" }],
      }),
    });
    expect(res.status).toBe(400);
    const body = await res.json() as { error: string };
    expect(body.error).toContain("sourceRelay");
  });

  it("POST /api/safety/hashes skips entries with missing fields", async () => {
    const res = await fetch(url("/api/safety/hashes"), {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        hashes: [
          { hash: "sha256-valid-entry", category: "spam", reportedBy: "did:key:a" },
          { hash: "sha256-missing-category" },
          { category: "spam", reportedBy: "did:key:a" },
        ],
        sourceRelay: "did:key:peer",
      }),
    });
    expect(res.status).toBe(200);
    const body = await res.json() as { ok: boolean; imported: number };
    expect(body.ok).toBe(true);
    // Only the first valid entry should be imported
    expect(body.imported).toBe(1);
  });
});

// ── Content Check Tests ─────────────────────────────────────────────────────

describe("content hash checks", () => {
  it("GET /api/safety/hashes lists all flagged hashes", async () => {
    const res = await fetch(url("/api/safety/hashes"));
    expect(res.status).toBe(200);
    const body = await res.json() as { hashes: unknown[]; count: number };
    expect(body.count).toBeGreaterThanOrEqual(1);
    expect(body.hashes).toEqual(
      expect.arrayContaining([
        expect.objectContaining({ hash: "sha256-abc123", category: "malware" }),
      ]),
    );
  });

  it("POST /api/safety/check verifies hash status", async () => {
    const res = await fetch(url("/api/safety/check"), {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ hashes: ["sha256-abc123", "clean-hash-never-flagged"] }),
    });
    expect(res.status).toBe(200);
    const body = await res.json() as { results: Record<string, boolean> };
    expect(body.results["sha256-abc123"]).toBe(true);
    expect(body.results["clean-hash-never-flagged"]).toBe(false);
  });

  it("POST /api/safety/check validates hashes array", async () => {
    const res = await fetch(url("/api/safety/check"), {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({}),
    });
    expect(res.status).toBe(400);
  });
});

// ── Cross-relay gossip verification ─────────────────────────────────────────

describe("cross-relay gossip verification", () => {
  it("peer relay received flagged hashes via gossip", async () => {
    // The auto-gossip from the report tests should have pushed hashes to the peer
    const res = await fetch(peerUrl("/api/safety/hashes"));
    expect(res.status).toBe(200);
    const body = await res.json() as { hashes: unknown[]; count: number };
    // The peer should have received at least some hashes from gossip
    expect(body.count).toBeGreaterThanOrEqual(1);
  });

  it("peer relay can check imported hashes", async () => {
    const res = await fetch(peerUrl("/api/safety/check"), {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ hashes: ["sha256-abc123"] }),
    });
    expect(res.status).toBe(200);
    const body = await res.json() as { results: Record<string, boolean> };
    expect(body.results["sha256-abc123"]).toBe(true);
  });
});
