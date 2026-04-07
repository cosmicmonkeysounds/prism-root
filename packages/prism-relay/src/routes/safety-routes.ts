/**
 * Safety Routes — whistleblower packets, toxic hash gossip, content checks.
 *
 * Implements the Trust & Safety primitives from the spec:
 * - Whistleblower Packet: submit a report with a one-time decryption key
 *   exposing only the offending CRDT node for policy evaluation
 * - Toxic Hash Gossip: share/receive verified toxic hashes between relays
 * - Content Check: verify a file hash against the relay's flagged content DB
 */

import { Hono } from "hono";
import type { RelayInstance, FederationRegistry, FederationPeer } from "@prism/core/relay";
import { RELAY_CAPABILITIES } from "@prism/core/relay";
import type { PeerTrustGraph, EscrowManager } from "@prism/core/trust";

/** Result of gossiping to a single peer. */
export interface GossipPeerResult {
  relayDid: string;
  url: string;
  success: boolean;
  error?: string;
}

/** Summary returned by the gossip endpoint. */
export interface GossipSummary {
  ok: boolean;
  hashCount: number;
  totalPeers: number;
  successCount: number;
  failedCount: number;
  peers: GossipPeerResult[];
}

/** Report body for whistleblower packets. */
export interface WhistleblowerReport {
  contentHash: string;
  category: string;
  reportedBy: string;
  evidence?: string;
}

/**
 * Gossip all flagged hashes to every federation peer.
 * Resilient: catches per-peer errors and continues.
 */
async function gossipToAllPeers(
  trust: PeerTrustGraph,
  federation: FederationRegistry,
  sourceRelayDid: string,
): Promise<GossipSummary> {
  const flagged = trust.flaggedContent();
  if (flagged.length === 0) {
    return { ok: true, hashCount: 0, totalPeers: 0, successCount: 0, failedCount: 0, peers: [] };
  }

  const peers: ReadonlyArray<FederationPeer> = federation.getPeers();
  const payload = {
    hashes: flagged.map((f) => ({
      hash: f.hash,
      category: f.category,
      reportedBy: f.reportedBy,
    })),
    sourceRelay: sourceRelayDid,
  };
  const body = JSON.stringify(payload);

  const results: GossipPeerResult[] = [];

  for (const peer of peers) {
    try {
      const res = await fetch(`${peer.url}/api/safety/hashes`, {
        method: "POST",
        headers: { "Content-Type": "application/json", "X-Prism-CSRF": "1" },
        body,
      });
      if (res.ok) {
        results.push({ relayDid: peer.relayDid, url: peer.url, success: true });
      } else {
        results.push({
          relayDid: peer.relayDid,
          url: peer.url,
          success: false,
          error: `HTTP ${String(res.status)}`,
        });
      }
    } catch (err: unknown) {
      results.push({
        relayDid: peer.relayDid,
        url: peer.url,
        success: false,
        error: err instanceof Error ? err.message : String(err),
      });
    }
  }

  const successCount = results.filter((r) => r.success).length;
  return {
    ok: true,
    hashCount: flagged.length,
    totalPeers: peers.length,
    successCount,
    failedCount: peers.length - successCount,
    peers: results,
  };
}

export function createSafetyRoutes(relay: RelayInstance): Hono {
  const app = new Hono();

  function getTrust(): PeerTrustGraph | undefined {
    return relay.getCapability<PeerTrustGraph>(RELAY_CAPABILITIES.TRUST);
  }

  function getFederation(): FederationRegistry | undefined {
    return relay.getCapability<FederationRegistry>(RELAY_CAPABILITIES.FEDERATION);
  }

  function getEscrow(): EscrowManager | undefined {
    return relay.getCapability<EscrowManager>(RELAY_CAPABILITIES.ESCROW);
  }

  // POST /api/safety/report — submit a whistleblower packet
  app.post("/report", async (c) => {
    const trust = getTrust();
    if (!trust) return c.json({ error: "trust module not installed" }, 404);

    const body = await c.req.json<WhistleblowerReport>();

    if (!body.contentHash || !body.category || !body.reportedBy) {
      return c.json({ error: "contentHash, category, and reportedBy are required" }, 400);
    }

    const validCategories = ["spam", "csam", "malware", "other"];
    if (!validCategories.includes(body.category)) {
      return c.json({ error: `category must be one of: ${validCategories.join(", ")}` }, 400);
    }

    // Flag the content in the trust graph
    trust.flagContent(body.contentHash, body.category, body.reportedBy);

    // Record positive reputation for the reporter (they are helping the network)
    if (body.reportedBy !== "anonymous") {
      trust.recordPositive(body.reportedBy);
    }

    // Store evidence as a one-time escrow deposit if provided
    let escrowId: string | null = null;
    if (body.evidence) {
      const escrow = getEscrow();
      if (escrow) {
        // Evidence expires in 30 days — one-time claim
        const expiry = new Date(Date.now() + 30 * 24 * 60 * 60 * 1000).toISOString();
        const deposit = escrow.deposit(body.reportedBy, body.evidence, expiry);
        escrowId = deposit.id;
      }
    }

    // Auto-trigger gossip if federation is enabled
    let gossip: GossipSummary | null = null;
    const federation = getFederation();
    if (federation) {
      gossip = await gossipToAllPeers(trust, federation, relay.did);
    }

    return c.json({
      ok: true,
      contentHash: body.contentHash,
      flagged: true,
      escrowId,
      gossip,
    }, 201);
  });

  // GET /api/safety/hashes — list all flagged toxic hashes
  app.get("/hashes", (c) => {
    const trust = getTrust();
    if (!trust) return c.json({ error: "trust module not installed" }, 404);

    const hashes = trust.flaggedContent();
    return c.json({ hashes, count: hashes.length });
  });

  // POST /api/safety/hashes — receive toxic hashes from a federated peer
  app.post("/hashes", async (c) => {
    const trust = getTrust();
    if (!trust) return c.json({ error: "trust module not installed" }, 404);

    const body = await c.req.json<{
      hashes: Array<{ hash: string; category: string; reportedBy: string }>;
      sourceRelay: string;
    }>();

    if (!body.hashes || !Array.isArray(body.hashes)) {
      return c.json({ error: "hashes array is required" }, 400);
    }

    if (!body.sourceRelay) {
      return c.json({ error: "sourceRelay is required" }, 400);
    }

    let imported = 0;
    for (const entry of body.hashes) {
      if (entry.hash && entry.category && entry.reportedBy) {
        if (!trust.isContentFlagged(entry.hash)) {
          trust.flagContent(entry.hash, entry.category, entry.reportedBy);
          imported++;
        }
      }
    }

    return c.json({ ok: true, imported, total: trust.flaggedContent().length });
  });

  // POST /api/safety/check — check if a content hash is flagged
  app.post("/check", async (c) => {
    const trust = getTrust();
    if (!trust) return c.json({ error: "trust module not installed" }, 404);

    const body = await c.req.json<{ hashes: string[] }>();
    if (!body.hashes || !Array.isArray(body.hashes)) {
      return c.json({ error: "hashes array is required" }, 400);
    }

    const results: Record<string, boolean> = {};
    for (const hash of body.hashes) {
      results[hash] = trust.isContentFlagged(hash);
    }

    return c.json({ results });
  });

  // POST /api/safety/gossip — push toxic hashes to all federated peers
  app.post("/gossip", async (c) => {
    const trust = getTrust();
    const federation = getFederation();
    if (!trust) return c.json({ error: "trust module not installed" }, 404);
    if (!federation) return c.json({ error: "federation module not installed" }, 404);

    const summary = await gossipToAllPeers(trust, federation, relay.did);
    return c.json(summary);
  });

  return app;
}
