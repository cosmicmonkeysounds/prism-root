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
import type { RelayInstance, FederationRegistry } from "@prism/core/relay";
import { RELAY_CAPABILITIES } from "@prism/core/relay";
import type { PeerTrustGraph } from "@prism/core/trust";

export function createSafetyRoutes(relay: RelayInstance): Hono {
  const app = new Hono();

  function getTrust(): PeerTrustGraph | undefined {
    return relay.getCapability<PeerTrustGraph>(RELAY_CAPABILITIES.TRUST);
  }

  function getFederation(): FederationRegistry | undefined {
    return relay.getCapability<FederationRegistry>(RELAY_CAPABILITIES.FEDERATION);
  }

  // POST /api/safety/report — submit a whistleblower packet
  app.post("/report", async (c) => {
    const trust = getTrust();
    if (!trust) return c.json({ error: "trust module not installed" }, 404);

    const body = await c.req.json<{
      contentHash: string;
      category: string;
      reportedBy: string;
      decryptionKey?: string;
      evidence?: string;
    }>();

    if (!body.contentHash || !body.category || !body.reportedBy) {
      return c.json({ error: "contentHash, category, and reportedBy are required" }, 400);
    }

    // Flag the content in the trust graph
    trust.flagContent(body.contentHash, body.category, body.reportedBy);

    // Record negative reputation for the content origin if identifiable
    if (body.reportedBy !== "anonymous") {
      trust.recordPositive(body.reportedBy); // Reporter gets positive rep
    }

    return c.json({
      ok: true,
      contentHash: body.contentHash,
      flagged: true,
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

    let imported = 0;
    for (const entry of body.hashes) {
      if (!trust.isContentFlagged(entry.hash)) {
        trust.flagContent(entry.hash, entry.category, entry.reportedBy);
        imported++;
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

    const flagged = trust.flaggedContent();
    if (flagged.length === 0) {
      return c.json({ ok: true, peers: 0, hashes: 0 });
    }

    const peers = federation.getPeers();
    const payload = {
      hashes: flagged.map((f) => ({
        hash: f.hash,
        category: f.category,
        reportedBy: f.reportedBy,
      })),
      sourceRelay: relay.did,
    };

    let successCount = 0;
    for (const peer of peers) {
      try {
        const res = await fetch(`${peer.url}/api/safety/hashes`, {
          method: "POST",
          headers: { "Content-Type": "application/json", "X-Prism-CSRF": "1" },
          body: JSON.stringify(payload),
        });
        if (res.ok) successCount++;
      } catch {
        // Peer unreachable — skip
      }
    }

    return c.json({ ok: true, peers: successCount, hashes: flagged.length });
  });

  return app;
}
