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

// ── Subcommand: start ───────────────────────────────────────────────────────

async function cmdStart(parsed: ReturnType<typeof parseArgs>): Promise<void> {
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
  }
}

main().catch((e: unknown) => {
  process.stderr.write(`Failed to start relay: ${String(e)}\n`);
  process.exit(1);
});
