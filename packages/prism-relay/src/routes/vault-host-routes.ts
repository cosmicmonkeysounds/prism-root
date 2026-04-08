import { Hono } from "hono";
import type { RelayInstance, VaultHost } from "@prism/core/relay";
import { RELAY_CAPABILITIES } from "@prism/core/relay";
import { encodeBase64, decodeBase64 } from "../protocol/relay-protocol.js";

type DID = `did:${string}:${string}`;

function asDid(s: string): DID {
  return s as DID;
}

export function createVaultHostRoutes(relay: RelayInstance): Hono {
  const app = new Hono();

  function host(): VaultHost {
    return relay.getCapability<VaultHost>(RELAY_CAPABILITIES.VAULT_HOST) as VaultHost;
  }

  app.use("/*", async (c, next) => {
    if (!relay.getCapability(RELAY_CAPABILITIES.VAULT_HOST)) {
      return c.json({ error: "vault-host module not installed" }, 404);
    }
    await next();
  });

  // ── List vaults ────────────────────────────────────────────────────────

  app.get("/", (c) => {
    const publicOnly = c.req.query("public") === "true";
    const search = c.req.query("search");

    if (search) {
      return c.json(host().search(search));
    }
    return c.json(host().list(publicOnly ? { publicOnly: true } : undefined));
  });

  // ── Publish vault ──────────────────────────────────────────────────────

  app.post("/", async (c) => {
    const body = await c.req.json<{
      manifest: Record<string, unknown>;
      ownerDid: string;
      isPublic?: boolean;
      collections: Record<string, string>; // id -> base64 snapshot
    }>();

    if (!body.manifest || !body.ownerDid) {
      return c.json({ error: "manifest and ownerDid required" }, 400);
    }

    const collections: Record<string, Uint8Array> = {};
    for (const [id, b64] of Object.entries(body.collections ?? {})) {
      collections[id] = decodeBase64(b64);
    }

    const vault = host().publish({
      manifest: body.manifest as unknown as Parameters<VaultHost["publish"]>[0]["manifest"],
      ownerDid: asDid(body.ownerDid),
      ...(body.isPublic !== undefined && { isPublic: body.isPublic }),
      collections,
    });

    return c.json(vault, 201);
  });

  // ── Get vault metadata ─────────────────────────────────────────────────

  app.get("/:id", (c) => {
    const vault = host().get(c.req.param("id"));
    if (!vault) return c.json({ error: "vault not found" }, 404);
    return c.json(vault);
  });

  // ── List collection IDs with sizes ─────────────────────────────────────

  app.get("/:id/collections", (c) => {
    const vaultId = c.req.param("id");
    const all = host().getAllSnapshots(vaultId);
    if (!all) return c.json({ error: "vault not found" }, 404);

    const result = Object.entries(all).map(([id, data]) => ({
      id,
      bytes: data.byteLength,
    }));
    return c.json(result);
  });

  // ── Download single collection snapshot ────────────────────────────────

  app.get("/:id/collections/:cid", (c) => {
    const snap = host().getSnapshot(c.req.param("id"), c.req.param("cid"));
    if (!snap) return c.json({ error: "collection not found" }, 404);
    return c.json({ snapshot: encodeBase64(snap) });
  });

  // ── Bulk download entire vault ─────────────────────────────────────────

  app.get("/:id/download", (c) => {
    const vaultId = c.req.param("id");
    const vault = host().get(vaultId);
    if (!vault) return c.json({ error: "vault not found" }, 404);

    const all = host().getAllSnapshots(vaultId);
    const collections: Record<string, string> = {};
    if (all) {
      for (const [id, data] of Object.entries(all)) {
        collections[id] = encodeBase64(data);
      }
    }

    return c.json({
      manifest: vault.manifest,
      collections,
    });
  });

  // ── Update collection snapshots (owner-only) ───────────────────────────

  app.put("/:id/collections", async (c) => {
    const vaultId = c.req.param("id");
    const body = await c.req.json<{
      ownerDid: string;
      collections: Record<string, string>; // id -> base64
    }>();

    if (!body.ownerDid) {
      return c.json({ error: "ownerDid required" }, 400);
    }

    const updates: Record<string, Uint8Array> = {};
    for (const [id, b64] of Object.entries(body.collections ?? {})) {
      updates[id] = decodeBase64(b64);
    }

    const ok = host().updateCollections(vaultId, asDid(body.ownerDid), updates);
    if (!ok) return c.json({ error: "vault not found or not owner" }, 403);
    return c.json({ ok: true });
  });

  // ── Remove vault (owner-only) ──────────────────────────────────────────

  app.delete("/:id", async (c) => {
    const vaultId = c.req.param("id");
    const body = await c.req.json<{ ownerDid: string }>();

    if (!body.ownerDid) {
      return c.json({ error: "ownerDid required" }, 400);
    }

    const ok = host().remove(vaultId, asDid(body.ownerDid));
    if (!ok) return c.json({ error: "vault not found or not owner" }, 403);
    return c.json({ ok: true });
  });

  return app;
}
