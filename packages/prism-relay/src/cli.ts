#!/usr/bin/env node
/**
 * Prism Relay CLI — manage and run relay servers.
 *
 * Commands:
 *   start (default)           Start the relay server
 *   init                      Generate a starter config file
 *   status                    Check health of a running relay
 *   identity show             Display relay DID and public key
 *   identity regenerate       Generate a new identity
 *   modules list              List available relay modules
 *   config validate           Validate config without starting
 *   config show               Show fully resolved config
 *
 * Usage:
 *   prism-relay [COMMAND] [OPTIONS]
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
  webrtcSignalingModule,
} from "@prism/core/relay";
import type { RelayModule, FederationRegistry, WebhookHttpClient, BlindMailbox, AcmeCertificateManager, SignalingHub } from "@prism/core/relay";
import { RELAY_CAPABILITIES } from "@prism/core/relay";
import { createRelayServer } from "./server/relay-server.js";
import {
  parseArgs,
  printHelp,
  resolveConfig,
  createLogger,
  ALL_MODULES,
} from "./config/index.js";
import type {
  RelayConfigFile,
  ResolvedRelayConfig,
  RelayLogger,
} from "./config/index.js";
import type { ExportedIdentity } from "@prism/core/identity";
import { createFileStore } from "./persistence/file-store.js";
import { createLogBuffer } from "./routes/logs-routes.js";

// ── Remote API Helper ───────────────────────────────────────────────────────

function relayUrl(parsed: ReturnType<typeof parseArgs>): string {
  const host = parsed.overrides.host ?? "127.0.0.1";
  const port = parsed.overrides.port ?? 4444;
  return `http://${host}:${port}`;
}

async function apiFetch(
  base: string,
  path: string,
  init?: RequestInit,
): Promise<Response> {
  const url = `${base}${path}`;
  try {
    return await fetch(url, {
      ...init,
      headers: { "X-Prism-CSRF": "1", ...init?.headers },
      signal: AbortSignal.timeout(10_000),
    });
  } catch (e) {
    process.stderr.write(`Cannot reach relay at ${base}: ${String(e)}\n`);
    process.exit(1);
  }
}

// ── Config File Loading ─────────────────────────────────────────────────────

function loadConfigFile(configPath: string | undefined): RelayConfigFile {
  if (configPath) {
    if (!fs.existsSync(configPath)) {
      process.stderr.write(`Config file not found: ${configPath}\n`);
      process.exit(1);
    }
    return JSON.parse(fs.readFileSync(configPath, "utf-8")) as RelayConfigFile;
  }

  const candidates = ["./relay.config.json", "./prism-relay.json"];
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

  const dir = path.dirname(identityPath);
  fs.mkdirSync(dir, { recursive: true });

  const exported = await exportIdentity(identity);
  fs.writeFileSync(identityPath, JSON.stringify(exported, null, 2), "utf-8");
  log.info("Identity saved", { did: identity.did, path: identityPath });

  return identity;
}

// ── Webhook HTTP Client ─────────────────────────────────────────────────────

function createWebhookHttpClient(): WebhookHttpClient {
  return {
    async post(url, body, headers) {
      const res = await fetch(url, {
        method: "POST",
        headers,
        body,
        signal: AbortSignal.timeout(10_000),
      });
      return { status: res.status };
    },
  };
}

// ── Module Wiring ───────────────────────────────────────────────────────────

function createModules(
  names: string[],
  identity: PrismIdentity,
  config: ResolvedRelayConfig,
): RelayModule[] {
  const httpClient = createWebhookHttpClient();

  const factories: Record<string, () => RelayModule> = {
    "blind-mailbox": () => blindMailboxModule(),
    "relay-router": () => relayRouterModule(),
    "relay-timestamp": () => relayTimestampModule(identity),
    "blind-ping": () => blindPingModule(),
    "capability-tokens": () => capabilityTokenModule(identity),
    "webhooks": () => webhookModule(httpClient),
    "sovereign-portals": () => sovereignPortalModule(),
    "collection-host": () => collectionHostModule(),
    "hashcash": () => hashcashModule({ bits: config.hashcashBits }),
    "peer-trust": () => peerTrustModule(),
    "escrow": () => escrowModule(),
    "federation": () => federationModule(),
    "acme-certificates": () => acmeCertificateModule(),
    "portal-templates": () => portalTemplateModule(),
    "webrtc-signaling": () => webrtcSignalingModule(),
  };

  const modules: RelayModule[] = [];
  for (const name of names) {
    const factory = factories[name];
    if (!factory) {
      process.stderr.write(`Unknown module: ${name}\nAvailable: ${Object.keys(factories).join(", ")}\n`);
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

    if (config.federation.publicUrl) {
      try {
        const res = await fetch(`${peer.url}/api/federation/announce`, {
          method: "POST",
          headers: { "Content-Type": "application/json", "X-Prism-CSRF": "1" },
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

// ── Subcommand: init ────────────────────────────────────────────────────────

function cmdInit(parsed: ReturnType<typeof parseArgs>): void {
  const mode = parsed.overrides.mode ?? "dev";
  const output = parsed.initOutput ?? "./relay.config.json";

  if (fs.existsSync(output)) {
    process.stderr.write(`Config file already exists: ${output}\n`);
    process.stderr.write(`Use a different path with --output or remove the existing file.\n`);
    process.exit(1);
  }

  const config: RelayConfigFile = {
    mode,
    port: parsed.overrides.port ?? 4444,
    host: mode === "dev" ? "127.0.0.1" : "0.0.0.0",
    dataDir: "~/.prism/relay",
    didMethod: parsed.overrides.didMethod ?? "key",
    logging: {
      level: mode === "dev" ? "debug" : "info",
      format: mode === "server" ? "json" : "text",
    },
  };

  if (mode === "p2p") {
    config.federation = {
      enabled: true,
      publicUrl: parsed.overrides.federation?.publicUrl ?? "https://your-relay.example.com",
      bootstrapPeers: [],
    };
  }

  if (mode === "dev") {
    config.corsOrigins = ["*"];
  }

  fs.writeFileSync(output, JSON.stringify(config, null, 2) + "\n", "utf-8");
  process.stdout.write(`Created ${mode} config: ${output}\n`);
}

// ── Subcommand: status ──────────────────────────────────────────────────────

async function cmdStatus(parsed: ReturnType<typeof parseArgs>): Promise<void> {
  const host = parsed.overrides.host ?? "127.0.0.1";
  const port = parsed.overrides.port ?? 4444;
  const url = `http://${host}:${port}/api/health`;

  try {
    const res = await fetch(url, { signal: AbortSignal.timeout(5_000) });
    if (!res.ok) {
      process.stderr.write(`Relay responded with status ${res.status}\n`);
      process.exit(1);
    }
    const data = await res.json() as Record<string, unknown>;
    process.stdout.write(JSON.stringify(data, null, 2) + "\n");
  } catch (e) {
    process.stderr.write(`Cannot reach relay at ${url}: ${String(e)}\n`);
    process.exit(1);
  }
}

// ── Subcommand: identity show ───────────────────────────────────────────────

async function cmdIdentityShow(parsed: ReturnType<typeof parseArgs>): Promise<void> {
  const fileConfig = loadConfigFile(parsed.configPath);
  const config = resolveConfig(fileConfig, parsed.overrides);
  const identityPath = config.identityFile;

  if (!fs.existsSync(identityPath)) {
    process.stderr.write(`No identity found at ${identityPath}\n`);
    process.stderr.write(`Run 'prism-relay start' to generate one, or specify --data-dir.\n`);
    process.exit(1);
  }

  const data = JSON.parse(fs.readFileSync(identityPath, "utf-8")) as ExportedIdentity;
  const identity = await importIdentity(data);
  process.stdout.write(`DID:      ${identity.did}\n`);
  process.stdout.write(`Method:   ${identity.did.startsWith("did:web:") ? "web" : "key"}\n`);
  process.stdout.write(`Key file: ${identityPath}\n`);
}

// ── Subcommand: identity regenerate ─────────────────────────────────────────

async function cmdIdentityRegenerate(parsed: ReturnType<typeof parseArgs>): Promise<void> {
  const fileConfig = loadConfigFile(parsed.configPath);
  const config = resolveConfig(fileConfig, parsed.overrides);
  const identityPath = config.identityFile;

  // Back up existing identity
  if (fs.existsSync(identityPath)) {
    const backupPath = `${identityPath}.backup-${Date.now()}`;
    fs.copyFileSync(identityPath, backupPath);
    process.stdout.write(`Backed up existing identity to ${backupPath}\n`);
  }

  const opts: { method: "key" | "web"; domain?: string } = { method: config.didMethod };
  if (config.didWebDomain !== undefined) opts.domain = config.didWebDomain;
  const identity = await createIdentity(opts);

  const dir = path.dirname(identityPath);
  fs.mkdirSync(dir, { recursive: true });

  const exported = await exportIdentity(identity);
  fs.writeFileSync(identityPath, JSON.stringify(exported, null, 2), "utf-8");
  process.stdout.write(`New identity generated.\n`);
  process.stdout.write(`DID: ${identity.did}\n`);
  process.stdout.write(`Saved to: ${identityPath}\n`);
}

// ── Subcommand: modules list ────────────────────────────────────────────────

function cmdModulesList(): void {
  const descriptions: Record<string, string> = {
    "blind-mailbox": "E2EE store-and-forward for offline peers",
    "relay-router": "Zero-knowledge envelope routing",
    "relay-timestamp": "Cryptographic proof-of-when (Ed25519 signatures)",
    "blind-ping": "Content-free push notifications (APNs/FCM)",
    "capability-tokens": "Scoped access tokens with Ed25519 verification",
    "webhooks": "Outgoing HTTP webhooks on CRDT changes",
    "sovereign-portals": "HTML rendering (Levels 1-4) with SEO",
    "collection-host": "CRDT collection hosting + sync protocol",
    "hashcash": "Proof-of-work spam protection",
    "peer-trust": "Reputation graph, peer bans, content flagging",
    "escrow": "Blind escrow key recovery deposits",
    "federation": "Peer discovery + cross-relay envelope forwarding",
    "acme-certificates": "Let's Encrypt ACME HTTP-01 certificate management",
    "portal-templates": "Reusable portal HTML template blueprints",
    "webrtc-signaling": "P2P/SFU WebRTC connection negotiation",
  };

  process.stdout.write("Available Relay Modules:\n\n");
  for (const name of ALL_MODULES) {
    const desc = descriptions[name] ?? "";
    process.stdout.write(`  ${name.padEnd(22)} ${desc}\n`);
  }
  process.stdout.write(`\n${ALL_MODULES.length} modules total.\n`);
  process.stdout.write(`\nUse --modules to select which modules to enable:\n`);
  process.stdout.write(`  prism-relay start --modules blind-mailbox,relay-router,federation\n`);
}

// ── Subcommand: config validate ─────────────────────────────────────────────

function cmdConfigValidate(parsed: ReturnType<typeof parseArgs>): void {
  try {
    const fileConfig = loadConfigFile(parsed.configPath);
    const config = resolveConfig(fileConfig, parsed.overrides);

    // Validate module names
    const knownModules = new Set(ALL_MODULES);
    const unknownModules = config.modules.filter((m) => !knownModules.has(m));
    if (unknownModules.length > 0) {
      process.stderr.write(`Unknown modules: ${unknownModules.join(", ")}\n`);
      process.stderr.write(`Available: ${ALL_MODULES.join(", ")}\n`);
      process.exit(1);
    }

    // Validate federation config
    if (config.federation.enabled && !config.federation.publicUrl) {
      process.stderr.write(`Warning: federation enabled but no --public-url set. Other relays cannot announce back.\n`);
    }

    // Validate did:web requires domain
    if (config.didMethod === "web" && !config.didWebDomain) {
      process.stderr.write(`Error: --did-method web requires --did-web-domain.\n`);
      process.exit(1);
    }

    // Validate port range
    if (config.port < 1 || config.port > 65535) {
      process.stderr.write(`Error: port must be between 1 and 65535, got ${config.port}.\n`);
      process.exit(1);
    }

    process.stdout.write(`Config is valid.\n`);
    process.stdout.write(`  Mode: ${config.mode}\n`);
    process.stdout.write(`  Listen: ${config.host}:${config.port}\n`);
    process.stdout.write(`  Modules: ${config.modules.length} enabled\n`);
    process.stdout.write(`  Federation: ${config.federation.enabled ? "enabled" : "disabled"}\n`);
    process.stdout.write(`  Data dir: ${config.dataDir}\n`);
  } catch (e) {
    process.stderr.write(`Config validation failed: ${String(e)}\n`);
    process.exit(1);
  }
}

// ── Subcommand: config show ─────────────────────────────────────────────────

function cmdConfigShow(parsed: ReturnType<typeof parseArgs>): void {
  const fileConfig = loadConfigFile(parsed.configPath);
  const config = resolveConfig(fileConfig, parsed.overrides);
  process.stdout.write(JSON.stringify(config, null, 2) + "\n");
}

// ── Management Commands (remote, connect via HTTP API) ──────────────────────

async function cmdPeersList(parsed: ReturnType<typeof parseArgs>): Promise<void> {
  const base = relayUrl(parsed);
  const res = await apiFetch(base, "/api/trust");
  const peers = await res.json() as unknown[];
  if (peers.length === 0) {
    process.stdout.write("No peers registered.\n");
    return;
  }
  process.stdout.write(JSON.stringify(peers, null, 2) + "\n");
}

async function cmdPeersBan(parsed: ReturnType<typeof parseArgs>): Promise<void> {
  const did = parsed.positionalArg;
  if (!did) {
    process.stderr.write("Usage: prism-relay peers ban <did>\n");
    process.exit(1);
  }
  const base = relayUrl(parsed);
  const res = await apiFetch(base, `/api/trust/${encodeURIComponent(did)}/ban`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ reason: "Banned via CLI" }),
  });
  if (res.ok) {
    process.stdout.write(`Banned: ${did}\n`);
  } else {
    process.stderr.write(`Failed to ban peer: ${res.status}\n`);
    process.exit(1);
  }
}

async function cmdPeersUnban(parsed: ReturnType<typeof parseArgs>): Promise<void> {
  const did = parsed.positionalArg;
  if (!did) {
    process.stderr.write("Usage: prism-relay peers unban <did>\n");
    process.exit(1);
  }
  const base = relayUrl(parsed);
  const res = await apiFetch(base, `/api/trust/${encodeURIComponent(did)}/unban`, {
    method: "POST",
  });
  if (res.ok) {
    process.stdout.write(`Unbanned: ${did}\n`);
  } else {
    process.stderr.write(`Failed to unban peer: ${res.status}\n`);
    process.exit(1);
  }
}

async function cmdCollectionsList(parsed: ReturnType<typeof parseArgs>): Promise<void> {
  const base = relayUrl(parsed);
  const res = await apiFetch(base, "/api/collections");
  const ids = await res.json() as string[];
  if (ids.length === 0) {
    process.stdout.write("No collections hosted.\n");
    return;
  }
  process.stdout.write("Hosted collections:\n");
  for (const id of ids) {
    process.stdout.write(`  ${id}\n`);
  }
  process.stdout.write(`\n${ids.length} collection(s) total.\n`);
}

async function cmdCollectionsInspect(parsed: ReturnType<typeof parseArgs>): Promise<void> {
  const id = parsed.positionalArg;
  if (!id) {
    process.stderr.write("Usage: prism-relay collections inspect <id>\n");
    process.exit(1);
  }
  const base = relayUrl(parsed);
  const res = await apiFetch(base, `/api/collections/${encodeURIComponent(id)}/snapshot`);
  if (!res.ok) {
    process.stderr.write(`Collection not found: ${id}\n`);
    process.exit(1);
  }
  const data = await res.json() as { snapshot: string };
  const bytes = Buffer.from(data.snapshot, "base64");
  process.stdout.write(`Collection: ${id}\n`);
  process.stdout.write(`  Snapshot size: ${bytes.length} bytes\n`);
}

async function cmdCollectionsExport(parsed: ReturnType<typeof parseArgs>): Promise<void> {
  const id = parsed.positionalArg;
  if (!id) {
    process.stderr.write("Usage: prism-relay collections export <id> [--output <path>]\n");
    process.exit(1);
  }
  const base = relayUrl(parsed);
  const res = await apiFetch(base, `/api/collections/${encodeURIComponent(id)}/snapshot`);
  if (!res.ok) {
    process.stderr.write(`Collection not found: ${id}\n`);
    process.exit(1);
  }
  const data = await res.json() as { snapshot: string };
  const output = parsed.outputFile ?? parsed.initOutput ?? `${id}.snapshot.json`;
  fs.writeFileSync(output, JSON.stringify(data, null, 2), "utf-8");
  process.stdout.write(`Exported collection "${id}" to ${output}\n`);
}

async function cmdCollectionsImport(parsed: ReturnType<typeof parseArgs>): Promise<void> {
  const id = parsed.positionalArg;
  if (!id) {
    process.stderr.write("Usage: prism-relay collections import <id> --input <path>\n");
    process.exit(1);
  }
  const inputPath = parsed.inputFile;
  if (!inputPath) {
    process.stderr.write("--input <path> is required for import.\n");
    process.exit(1);
  }
  if (!fs.existsSync(inputPath)) {
    process.stderr.write(`Input file not found: ${inputPath}\n`);
    process.exit(1);
  }
  const base = relayUrl(parsed);
  // Ensure collection exists
  await apiFetch(base, "/api/collections", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ id }),
  });
  const fileData = JSON.parse(fs.readFileSync(inputPath, "utf-8")) as { snapshot?: string; data?: string };
  const snapshotData = fileData.snapshot ?? fileData.data;
  if (!snapshotData) {
    process.stderr.write("Input file must contain a 'snapshot' or 'data' field.\n");
    process.exit(1);
  }
  const res = await apiFetch(base, `/api/collections/${encodeURIComponent(id)}/import`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ data: snapshotData }),
  });
  if (res.ok) {
    process.stdout.write(`Imported collection "${id}" from ${inputPath}\n`);
  } else {
    process.stderr.write(`Import failed: ${res.status}\n`);
    process.exit(1);
  }
}

async function cmdCollectionsDelete(parsed: ReturnType<typeof parseArgs>): Promise<void> {
  const id = parsed.positionalArg;
  if (!id) {
    process.stderr.write("Usage: prism-relay collections delete <id>\n");
    process.exit(1);
  }
  const base = relayUrl(parsed);
  const res = await apiFetch(base, `/api/collections/${encodeURIComponent(id)}`, {
    method: "DELETE",
  });
  if (res.ok) {
    process.stdout.write(`Deleted collection: ${id}\n`);
  } else {
    process.stderr.write(`Collection not found: ${id}\n`);
    process.exit(1);
  }
}

async function cmdPortalsList(parsed: ReturnType<typeof parseArgs>): Promise<void> {
  const base = relayUrl(parsed);
  const res = await apiFetch(base, "/api/portals");
  const portals = await res.json() as Array<{ portalId: string; name: string; level: number; basePath: string; isPublic: boolean }>;
  if (portals.length === 0) {
    process.stdout.write("No portals published.\n");
    return;
  }
  process.stdout.write("Published portals:\n\n");
  for (const p of portals) {
    process.stdout.write(`  ${p.name} (${p.portalId})\n`);
    process.stdout.write(`    Level: ${p.level}  Path: ${p.basePath}  Public: ${p.isPublic}\n`);
    process.stdout.write(`    URL: ${base}/portals/${p.portalId}\n\n`);
  }
  process.stdout.write(`${portals.length} portal(s) total.\n`);
}

async function cmdPortalsInspect(parsed: ReturnType<typeof parseArgs>): Promise<void> {
  const id = parsed.positionalArg;
  if (!id) {
    process.stderr.write("Usage: prism-relay portals inspect <id>\n");
    process.exit(1);
  }
  const base = relayUrl(parsed);
  const res = await apiFetch(base, "/api/portals");
  const portals = await res.json() as Array<Record<string, unknown>>;
  const portal = portals.find((p) => p["portalId"] === id);
  if (!portal) {
    process.stderr.write(`Portal not found: ${id}\n`);
    process.exit(1);
  }
  process.stdout.write(JSON.stringify(portal, null, 2) + "\n");
}

async function cmdPortalsDelete(parsed: ReturnType<typeof parseArgs>): Promise<void> {
  const id = parsed.positionalArg;
  if (!id) {
    process.stderr.write("Usage: prism-relay portals delete <id>\n");
    process.exit(1);
  }
  const base = relayUrl(parsed);
  const res = await apiFetch(base, `/api/portals/${encodeURIComponent(id)}`, {
    method: "DELETE",
  });
  if (res.ok) {
    process.stdout.write(`Deleted portal: ${id}\n`);
  } else {
    process.stderr.write(`Portal not found: ${id}\n`);
    process.exit(1);
  }
}

async function cmdWebhooksList(parsed: ReturnType<typeof parseArgs>): Promise<void> {
  const base = relayUrl(parsed);
  const res = await apiFetch(base, "/api/webhooks");
  const webhooks = await res.json() as Array<{ id: string; url: string; events: string[]; active: boolean }>;
  if (webhooks.length === 0) {
    process.stdout.write("No webhooks registered.\n");
    return;
  }
  process.stdout.write("Registered webhooks:\n\n");
  for (const w of webhooks) {
    process.stdout.write(`  ${w.id}\n`);
    process.stdout.write(`    URL: ${w.url}  Active: ${w.active}  Events: ${w.events.join(", ")}\n\n`);
  }
  process.stdout.write(`${webhooks.length} webhook(s) total.\n`);
}

async function cmdWebhooksDelete(parsed: ReturnType<typeof parseArgs>): Promise<void> {
  const id = parsed.positionalArg;
  if (!id) {
    process.stderr.write("Usage: prism-relay webhooks delete <id>\n");
    process.exit(1);
  }
  const base = relayUrl(parsed);
  const res = await apiFetch(base, `/api/webhooks/${encodeURIComponent(id)}`, {
    method: "DELETE",
  });
  if (res.ok) {
    process.stdout.write(`Deleted webhook: ${id}\n`);
  } else {
    process.stderr.write(`Webhook not found: ${id}\n`);
    process.exit(1);
  }
}

async function cmdWebhooksTest(parsed: ReturnType<typeof parseArgs>): Promise<void> {
  const id = parsed.positionalArg;
  if (!id) {
    process.stderr.write("Usage: prism-relay webhooks test <id>\n");
    process.exit(1);
  }
  const base = relayUrl(parsed);
  const res = await apiFetch(base, `/api/webhooks/${encodeURIComponent(id)}/test`, {
    method: "POST",
  });
  if (res.ok) {
    const data = await res.json() as { deliveredTo: string };
    process.stdout.write(`Test event sent to: ${data.deliveredTo}\n`);
  } else {
    process.stderr.write(`Webhook not found: ${id}\n`);
    process.exit(1);
  }
}

async function cmdTokensList(parsed: ReturnType<typeof parseArgs>): Promise<void> {
  const base = relayUrl(parsed);
  const res = await apiFetch(base, "/api/tokens");
  const tokens = await res.json() as unknown[];
  if (tokens.length === 0) {
    process.stdout.write("No active tokens.\n");
    return;
  }
  process.stdout.write(JSON.stringify(tokens, null, 2) + "\n");
}

async function cmdTokensRevoke(parsed: ReturnType<typeof parseArgs>): Promise<void> {
  const id = parsed.positionalArg;
  if (!id) {
    process.stderr.write("Usage: prism-relay tokens revoke <id>\n");
    process.exit(1);
  }
  const base = relayUrl(parsed);
  const res = await apiFetch(base, "/api/tokens/revoke", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ tokenId: id }),
  });
  if (res.ok) {
    process.stdout.write(`Revoked token: ${id}\n`);
  } else {
    process.stderr.write(`Failed to revoke token: ${res.status}\n`);
    process.exit(1);
  }
}

async function cmdCertsList(parsed: ReturnType<typeof parseArgs>): Promise<void> {
  const base = relayUrl(parsed);
  const res = await apiFetch(base, "/api/acme/certificates");
  const certs = await res.json() as Array<{ domain: string; expiresAt: string; issuedAt: string }>;
  if (certs.length === 0) {
    process.stdout.write("No certificates managed.\n");
    return;
  }
  process.stdout.write("ACME certificates:\n\n");
  for (const c of certs) {
    const expires = new Date(c.expiresAt);
    const daysLeft = Math.ceil((expires.getTime() - Date.now()) / 86_400_000);
    process.stdout.write(`  ${c.domain}\n`);
    process.stdout.write(`    Issued: ${c.issuedAt}  Expires: ${c.expiresAt} (${daysLeft} days)\n\n`);
  }
  process.stdout.write(`${certs.length} certificate(s) total.\n`);
}

async function cmdCertsRenew(parsed: ReturnType<typeof parseArgs>): Promise<void> {
  const domain = parsed.positionalArg;
  if (!domain) {
    process.stderr.write("Usage: prism-relay certs renew <domain>\n");
    process.exit(1);
  }
  const base = relayUrl(parsed);
  const res = await apiFetch(base, "/api/acme/certificates", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ domain }),
  });
  if (res.ok) {
    process.stdout.write(`Certificate renewal initiated for: ${domain}\n`);
  } else {
    const text = await res.text();
    process.stderr.write(`Renewal failed: ${res.status} ${text}\n`);
    process.exit(1);
  }
}

async function cmdBackup(parsed: ReturnType<typeof parseArgs>): Promise<void> {
  const base = relayUrl(parsed);
  const res = await apiFetch(base, "/api/backup");
  if (!res.ok) {
    process.stderr.write(`Backup failed: ${res.status}\n`);
    process.exit(1);
  }
  const data = await res.text();
  const output = parsed.outputFile ?? parsed.initOutput ?? "relay-backup.json";
  fs.writeFileSync(output, data, "utf-8");
  process.stdout.write(`Relay state backed up to: ${output}\n`);
}

async function cmdRestore(parsed: ReturnType<typeof parseArgs>): Promise<void> {
  const inputPath = parsed.inputFile;
  if (!inputPath) {
    process.stderr.write("Usage: prism-relay restore --input <path>\n");
    process.exit(1);
  }
  if (!fs.existsSync(inputPath)) {
    process.stderr.write(`Input file not found: ${inputPath}\n`);
    process.exit(1);
  }
  const base = relayUrl(parsed);
  const data = fs.readFileSync(inputPath, "utf-8");
  const res = await apiFetch(base, "/api/backup", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: data,
  });
  if (res.ok) {
    const result = await res.json() as { restored: Record<string, number> };
    process.stdout.write(`Relay state restored from: ${inputPath}\n`);
    for (const [key, count] of Object.entries(result.restored)) {
      if (count > 0) process.stdout.write(`  ${key}: ${count}\n`);
    }
  } else {
    process.stderr.write(`Restore failed: ${res.status}\n`);
    process.exit(1);
  }
}

async function cmdLogs(parsed: ReturnType<typeof parseArgs>): Promise<void> {
  const base = relayUrl(parsed);
  const level = parsed.logLevelFilter;
  const qs = new URLSearchParams();
  if (level) qs.set("level", level);
  qs.set("limit", "200");

  async function fetchAndPrint(): Promise<void> {
    const res = await apiFetch(base, `/api/logs?${qs.toString()}`);
    if (!res.ok) {
      process.stderr.write(`Failed to fetch logs: ${res.status}\n`);
      process.exit(1);
    }
    const entries = await res.json() as Array<{ ts: string; level: string; msg: string; data?: Record<string, unknown> }>;
    for (const e of entries) {
      const data = e.data && Object.keys(e.data).length > 0
        ? " " + Object.entries(e.data).map(([k, v]) => `${k}=${JSON.stringify(v)}`).join(" ")
        : "";
      process.stdout.write(`${e.ts} [${e.level.toUpperCase().padEnd(5)}] ${e.msg}${data}\n`);
    }
  }

  await fetchAndPrint();

  if (parsed.follow) {
    // Poll every 2 seconds
    const interval = setInterval(async () => {
      try {
        await fetchAndPrint();
      } catch {
        clearInterval(interval);
      }
    }, 2000);

    process.on("SIGINT", () => {
      clearInterval(interval);
      process.exit(0);
    });

    // Keep alive
    await new Promise(() => {});
  }
}

// ── Subcommand: start ───────────────────────────────────────────────────────

async function cmdStart(parsed: ReturnType<typeof parseArgs>): Promise<void> {
  const fileConfig = loadConfigFile(parsed.configPath);
  const config = resolveConfig(fileConfig, parsed.overrides);
  const logBuffer = createLogBuffer(2000);
  const baseLog = createLogger(config.logging);
  // Wrap logger to also push into the ring buffer for /api/logs
  const log: typeof baseLog = {
    debug(msg, data) { logBuffer.push({ ts: new Date().toISOString(), level: "debug", msg, ...(data ? { data } : {}) }); baseLog.debug(msg, data); },
    info(msg, data) { logBuffer.push({ ts: new Date().toISOString(), level: "info", msg, ...(data ? { data } : {}) }); baseLog.info(msg, data); },
    warn(msg, data) { logBuffer.push({ ts: new Date().toISOString(), level: "warn", msg, ...(data ? { data } : {}) }); baseLog.warn(msg, data); },
    error(msg, data) { logBuffer.push({ ts: new Date().toISOString(), level: "error", msg, ...(data ? { data } : {}) }); baseLog.error(msg, data); },
  };

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

  // ── Background Jobs ────────────────────────────────────────────────────
  const evictionTimers: ReturnType<typeof setInterval>[] = [];
  const mailbox = relay.getCapability<BlindMailbox>(RELAY_CAPABILITIES.MAILBOX);
  if (mailbox) {
    const timer = setInterval(() => {
      const evicted = mailbox.evict();
      if (evicted > 0) log.debug("Mailbox eviction", { evicted });
    }, config.relay.evictionIntervalMs);
    evictionTimers.push(timer);
  }
  const acme = relay.getCapability<AcmeCertificateManager>(RELAY_CAPABILITIES.ACME);
  if (acme) {
    const timer = setInterval(() => {
      acme.evictExpiredChallenges();
    }, config.relay.evictionIntervalMs);
    evictionTimers.push(timer);
  }
  const signaling = relay.getCapability<SignalingHub>(RELAY_CAPABILITIES.SIGNALING);
  if (signaling) {
    const timer = setInterval(() => {
      signaling.evictEmptyRooms();
    }, config.relay.evictionIntervalMs * 5); // Less frequent for rooms
    evictionTimers.push(timer);
  }

  // ── Persistence ───────────────────────────────────────────────────────
  const fileStore = createFileStore({ dataDir: config.dataDir });
  fileStore.load(relay);
  fileStore.startAutoSave(relay);
  log.info("Persistence loaded", { dataDir: config.dataDir });

  // ── Start Server ──────────────────────────────────────────────────────
  const publicUrl = config.federation.publicUrl;
  const serverOpts: import("./server/relay-server.js").RelayServerOptions = {
    relay,
    port: config.port,
    host: config.host,
    corsOrigins: config.corsOrigins,
    maxBodySize: config.relay.maxEnvelopeSizeBytes,
    disableCsrf: config.mode === "dev",
    logBuffer,
  };
  if (publicUrl !== undefined) serverOpts.publicUrl = publicUrl;
  const server = createRelayServer(serverOpts);
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
    for (const timer of evictionTimers) clearInterval(timer);
    fileStore.save(relay);
    fileStore.dispose();
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

  switch (parsed.command) {
    case "start":
      await cmdStart(parsed);
      break;
    case "init":
      cmdInit(parsed);
      break;
    case "status":
      await cmdStatus(parsed);
      break;
    case "identity-show":
      await cmdIdentityShow(parsed);
      break;
    case "identity-regenerate":
      await cmdIdentityRegenerate(parsed);
      break;
    case "modules-list":
      cmdModulesList();
      break;
    case "config-validate":
      cmdConfigValidate(parsed);
      break;
    case "config-show":
      cmdConfigShow(parsed);
      break;
    case "peers-list":
      await cmdPeersList(parsed);
      break;
    case "peers-ban":
      await cmdPeersBan(parsed);
      break;
    case "peers-unban":
      await cmdPeersUnban(parsed);
      break;
    case "collections-list":
      await cmdCollectionsList(parsed);
      break;
    case "collections-inspect":
      await cmdCollectionsInspect(parsed);
      break;
    case "collections-export":
      await cmdCollectionsExport(parsed);
      break;
    case "collections-import":
      await cmdCollectionsImport(parsed);
      break;
    case "collections-delete":
      await cmdCollectionsDelete(parsed);
      break;
    case "portals-list":
      await cmdPortalsList(parsed);
      break;
    case "portals-inspect":
      await cmdPortalsInspect(parsed);
      break;
    case "portals-delete":
      await cmdPortalsDelete(parsed);
      break;
    case "webhooks-list":
      await cmdWebhooksList(parsed);
      break;
    case "webhooks-delete":
      await cmdWebhooksDelete(parsed);
      break;
    case "webhooks-test":
      await cmdWebhooksTest(parsed);
      break;
    case "tokens-list":
      await cmdTokensList(parsed);
      break;
    case "tokens-revoke":
      await cmdTokensRevoke(parsed);
      break;
    case "certs-list":
      await cmdCertsList(parsed);
      break;
    case "certs-renew":
      await cmdCertsRenew(parsed);
      break;
    case "backup":
      await cmdBackup(parsed);
      break;
    case "restore":
      await cmdRestore(parsed);
      break;
    case "logs":
      await cmdLogs(parsed);
      break;
  }
}

main().catch((e: unknown) => {
  process.stderr.write(`Failed to start relay: ${String(e)}\n`);
  process.exit(1);
});
