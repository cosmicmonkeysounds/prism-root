/**
 * Prism Relay CLI — start a relay server from the command line.
 *
 * Supports three deployment modes:
 *   --mode server   Always-on relay with all modules (production)
 *   --mode p2p      Federated peer with minimal modules
 *   --mode dev      Local development with debug logging
 *
 * Usage:
 *   npx tsx packages/prism-relay/src/cli.ts [OPTIONS]
 *   prism-relay --help
 */

import * as fs from "node:fs";
import * as path from "node:path";
import {
  createIdentity,
  exportIdentity,
  importIdentity,
} from "@prism/core/identity";
import type { PrismIdentity } from "@prism/core/identity";
import {
  createRelayBuilder,
  blindMailboxModule,
  relayRouterModule,
  relayTimestampModule,
  blindPingModule,
  capabilityTokenModule,
  webhookModule,
  sovereignPortalModule,
  collectionHostModule,
  hashcashModule,
  peerTrustModule,
  escrowModule,
  federationModule,
  acmeCertificateModule,
  portalTemplateModule,
} from "@prism/core/relay";
import type { RelayModule, FederationRegistry } from "@prism/core/relay";
import { RELAY_CAPABILITIES } from "@prism/core/relay";
import { createRelayServer } from "./server/relay-server.js";
import {
  parseArgs,
  printHelp,
  resolveConfig,
  createLogger,
} from "./config/index.js";
import type {
  RelayConfigFile,
  ResolvedRelayConfig,
  RelayLogger,
} from "./config/index.js";
import type { ExportedIdentity } from "@prism/core/identity";

// ── Config File Loading ─────────────────────────────────────────────────────

function loadConfigFile(configPath: string | undefined): RelayConfigFile {
  // Try explicit path first
  if (configPath) {
    if (!fs.existsSync(configPath)) {
      process.stderr.write(`Config file not found: ${configPath}\n`);
      process.exit(1);
    }
    return JSON.parse(fs.readFileSync(configPath, "utf-8")) as RelayConfigFile;
  }

  // Try default locations
  const candidates = [
    "./relay.config.json",
    "./prism-relay.json",
  ];
  for (const candidate of candidates) {
    if (fs.existsSync(candidate)) {
      return JSON.parse(fs.readFileSync(candidate, "utf-8")) as RelayConfigFile;
    }
  }

  return {};
}

// ── Identity Persistence ────────────────────────────────────────────────────

async function loadOrCreateIdentity(
  config: ResolvedRelayConfig,
  log: RelayLogger,
): Promise<PrismIdentity> {
  const identityPath = config.identityFile;

  if (fs.existsSync(identityPath)) {
    log.info("Loading existing identity", { path: identityPath });
    const data = JSON.parse(fs.readFileSync(identityPath, "utf-8")) as ExportedIdentity;
    const identity = await importIdentity(data);
    log.info("Identity loaded", { did: identity.did });
    return identity;
  }

  log.info("Generating new identity", { method: config.didMethod });
  const opts: { method: "key" | "web"; domain?: string } = { method: config.didMethod };
  if (config.didWebDomain !== undefined) opts.domain = config.didWebDomain;
  const identity = await createIdentity(opts);

  // Ensure data directory exists
  const dir = path.dirname(identityPath);
  fs.mkdirSync(dir, { recursive: true });

  // Save identity for next time
  const exported = await exportIdentity(identity);
  fs.writeFileSync(identityPath, JSON.stringify(exported, null, 2), "utf-8");
  log.info("Identity saved", { did: identity.did, path: identityPath });

  return identity;
}

// ── Module Wiring ───────────────────────────────────────────────────────────

function createModules(
  names: string[],
  identity: PrismIdentity,
  config: ResolvedRelayConfig,
): RelayModule[] {
  const factories: Record<string, () => RelayModule> = {
    "blind-mailbox": () => blindMailboxModule(),
    "relay-router": () => relayRouterModule(),
    "relay-timestamp": () => relayTimestampModule(identity),
    "blind-ping": () => blindPingModule(),
    "capability-tokens": () => capabilityTokenModule(identity),
    "webhooks": () => webhookModule(),
    "sovereign-portals": () => sovereignPortalModule(),
    "collection-host": () => collectionHostModule(),
    "hashcash": () => hashcashModule({ bits: config.hashcashBits }),
    "peer-trust": () => peerTrustModule(),
    "escrow": () => escrowModule(),
    "federation": () => federationModule(),
    "acme-certificates": () => acmeCertificateModule(),
    "portal-templates": () => portalTemplateModule(),
  };

  const modules: RelayModule[] = [];
  for (const name of names) {
    const factory = factories[name];
    if (!factory) {
      process.stderr.write(`Unknown module: ${name}\n`);
      process.exit(1);
    }
    modules.push(factory());
  }
  return modules;
}

// ── Federation Bootstrap ────────────────────────────────────────────────────

async function bootstrapFederation(
  config: ResolvedRelayConfig,
  relay: { getCapability<T>(name: string): T | undefined; did: string },
  log: RelayLogger,
): Promise<void> {
  if (!config.federation.enabled) return;

  const registry = relay.getCapability<FederationRegistry>(RELAY_CAPABILITIES.FEDERATION);
  if (!registry) {
    log.warn("Federation enabled but federation module not installed");
    return;
  }

  for (const peer of config.federation.bootstrapPeers) {
    log.info("Announcing to bootstrap peer", { relayDid: peer.relayDid, url: peer.url });
    registry.announce(peer.relayDid, peer.url);

    // Also announce ourselves to the peer if we have a public URL
    if (config.federation.publicUrl) {
      try {
        const res = await fetch(`${peer.url}/api/federation/announce`, {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify({
            relayDid: relay.did,
            url: config.federation.publicUrl,
          }),
        });
        if (res.ok) {
          log.info("Announced to peer successfully", { peer: peer.relayDid });
        } else {
          log.warn("Peer announce failed", { peer: peer.relayDid, status: res.status });
        }
      } catch (e) {
        log.warn("Peer unreachable", { peer: peer.relayDid, error: String(e) });
      }
    }
  }
}

// ── Main ────────────────────────────────────────────────────────────────────

async function main(): Promise<void> {
  const parsed = parseArgs(process.argv.slice(2));

  if (parsed.help) {
    process.stdout.write(printHelp() + "\n");
    process.exit(0);
  }
  if (parsed.version) {
    process.stdout.write("prism-relay 0.1.0\n");
    process.exit(0);
  }

  const fileConfig = loadConfigFile(parsed.configPath);
  const config = resolveConfig(fileConfig, parsed.overrides);
  const log = createLogger(config.logging);

  log.info("Prism Relay starting", { mode: config.mode });

  // ── Identity ──────────────────────────────────────────────────────────
  const identity = await loadOrCreateIdentity(config, log);

  // ── Build Relay ───────────────────────────────────────────────────────
  const modules = createModules(config.modules, identity, config);
  let builder = createRelayBuilder({
    relayDid: identity.did,
    config: config.relay,
  });
  for (const mod of modules) {
    builder = builder.use(mod);
  }
  const relay = builder.build();
  await relay.start();

  // ── Start Server ──────────────────────────────────────────────────────
  const server = createRelayServer({
    relay,
    port: config.port,
    host: config.host,
    corsOrigins: config.corsOrigins,
  });
  const info = await server.start();

  log.info("Relay listening", {
    did: relay.did,
    http: `http://${config.host}:${info.port}`,
    ws: `ws://${config.host}:${info.port}/ws/relay`,
    modules: relay.modules,
    mode: config.mode,
  });

  // ── Federation Bootstrap ──────────────────────────────────────────────
  await bootstrapFederation(config, relay, log);

  if (config.federation.enabled && config.federation.publicUrl) {
    log.info("Federation active", {
      publicUrl: config.federation.publicUrl,
      bootstrapPeers: config.federation.bootstrapPeers.length,
    });
  }

  // ── Shutdown ──────────────────────────────────────────────────────────
  function shutdown(): void {
    log.info("Shutting down...");
    info.close()
      .then(() => relay.stop())
      .then(() => {
        log.info("Relay stopped.");
        process.exit(0);
      })
      .catch((e) => {
        log.error("Shutdown error", { error: String(e) });
        process.exit(1);
      });
  }

  process.on("SIGINT", shutdown);
  process.on("SIGTERM", shutdown);
}

main().catch((e: unknown) => {
  process.stderr.write(`Failed to start relay: ${String(e)}\n`);
  process.exit(1);
});
