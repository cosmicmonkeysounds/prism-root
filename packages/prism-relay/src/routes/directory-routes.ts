import { Hono } from "hono";
import type { RelayInstance, PortalRegistry, VaultHost } from "@prism/core/relay";
import { RELAY_CAPABILITIES } from "@prism/core/relay";
import type { ResolvedRelayConfig } from "../config/relay-config.js";

export interface DirectoryRoutesOptions {
  relay: RelayInstance;
  publicUrl: string | undefined;
  config: ResolvedRelayConfig;
}

export function createDirectoryRoutes(opts: DirectoryRoutesOptions): Hono {
  const { relay, publicUrl, config } = opts;
  const app = new Hono();
  const startTime = Date.now();

  app.get("/", (c) => {
    if (!config.directory.listed) {
      return c.json({ error: "directory not listed" }, 404);
    }

    // ── Relay profile ──────────────────────────────────────────────────
    const modules = relay.modules;
    const federation = relay.getCapability(RELAY_CAPABILITIES.FEDERATION);
    const federationPeers = federation
      ? (federation as { getPeers(): unknown[] }).getPeers().length
      : 0;

    const relayProfile = {
      did: relay.did,
      name: config.directory.name ?? undefined,
      description: config.directory.description ?? undefined,
      publicUrl: publicUrl ?? undefined,
      version: "0.1.0",
      uptime: Math.floor((Date.now() - startTime) / 1000),
      modules,
      federation: {
        peers: federationPeers,
        accepts: config.federation.enabled,
      },
    };

    // ── Public portals ─────────────────────────────────────────────────
    const portalRegistry = relay.getCapability<PortalRegistry>(RELAY_CAPABILITIES.PORTALS);
    const portals = portalRegistry
      ? portalRegistry.list()
          .filter((p) => p.isPublic)
          .map((p) => ({
            id: p.portalId,
            name: p.name,
            level: p.level,
            url: `/portals/${p.portalId}`,
            isPublic: true,
            basePath: p.basePath,
            createdAt: p.createdAt,
          }))
      : [];

    // ── Public vaults ──────────────────────────────────────────────────
    const vaultHost = relay.getCapability<VaultHost>(RELAY_CAPABILITIES.VAULT_HOST);
    const vaults = vaultHost
      ? vaultHost.list({ publicOnly: true }).map((v) => ({
          id: v.id,
          name: v.manifest.name,
          ownerDid: v.ownerDid,
          collectionCount: Object.keys(vaultHost.getAllSnapshots(v.id) ?? {}).length,
          totalBytes: v.totalBytes,
          isPublic: true,
          hostedAt: v.hostedAt,
        }))
      : [];

    const body = {
      relay: relayProfile,
      portals,
      vaults,
      generatedAt: new Date().toISOString(),
    };

    c.header("Cache-Control", "public, max-age=300");
    return c.json(body);
  });

  return app;
}
