/**
 * Relay configuration — loaded from file, env vars, and CLI flags.
 *
 * Priority (highest wins): CLI flags > env vars > config file > defaults.
 */

import type { DID } from "@prism/core/identity";

// ── Config Types ──────────────────────────────────────────────────────────

export type DeploymentMode = "server" | "p2p" | "dev";

export interface RelayConfigFile {
  /** Deployment mode. "server" = always-on, "p2p" = federated peer, "dev" = local testing. */
  mode?: DeploymentMode;

  /** Network settings. */
  host?: string;
  port?: number;

  /** Path to persistent data directory. Default: ~/.prism/relay */
  dataDir?: string;

  /** Path to identity key file. Default: <dataDir>/identity.json */
  identityFile?: string;

  /** DID method for new identity generation. Default: "key". */
  didMethod?: "key" | "web";
  /** Domain for did:web. Required if didMethod is "web". */
  didWebDomain?: string;

  /** Modules to enable. Default: all for "server", minimal for "p2p", all for "dev". */
  modules?: string[];

  /** Hashcash difficulty (leading zero bits). Default: 16 for server, 0 for dev. */
  hashcashBits?: number;

  /** CORS allowed origins. Default: ["*"] for dev, [] for server. */
  corsOrigins?: string[];

  /** Federation settings. */
  federation?: {
    /** Enable federation. Default: true for p2p, false for server/dev. */
    enabled?: boolean;
    /** Peers to announce to on startup. */
    bootstrapPeers?: Array<{ relayDid: DID; url: string }>;
    /** Public URL where this relay is reachable. Required for federation. */
    publicUrl?: string;
  };

  /** Relay config overrides (ttl, max envelope size, eviction). */
  relay?: {
    defaultTtlMs?: number;
    maxEnvelopeSizeBytes?: number;
    evictionIntervalMs?: number;
  };

  /** Structured logging. */
  logging?: {
    level?: "debug" | "info" | "warn" | "error";
    /** Output format. "text" for human-readable, "json" for structured. */
    format?: "text" | "json";
  };
}

// ── Resolved Config ──────────────────────────────────────────────────────

/** Fully resolved config with all defaults applied. */
export interface ResolvedRelayConfig {
  mode: DeploymentMode;
  host: string;
  port: number;
  dataDir: string;
  identityFile: string;
  didMethod: "key" | "web";
  didWebDomain: string | undefined;
  modules: string[];
  hashcashBits: number;
  corsOrigins: string[];
  federation: {
    enabled: boolean;
    bootstrapPeers: Array<{ relayDid: DID; url: string }>;
    publicUrl: string | undefined;
  };
  relay: {
    defaultTtlMs: number;
    maxEnvelopeSizeBytes: number;
    evictionIntervalMs: number;
  };
  logging: {
    level: "debug" | "info" | "warn" | "error";
    format: "text" | "json";
  };
}

// ── Module Presets ────────────────────────────────────────────────────────

const ALL_MODULES = [
  "blind-mailbox",
  "relay-router",
  "relay-timestamp",
  "blind-ping",
  "capability-tokens",
  "webhooks",
  "sovereign-portals",
  "collection-host",
  "hashcash",
  "peer-trust",
  "escrow",
  "federation",
];

/** Minimal modules for p2p mode — routing + federation + trust. */
const P2P_MODULES = [
  "blind-mailbox",
  "relay-router",
  "relay-timestamp",
  "capability-tokens",
  "hashcash",
  "peer-trust",
  "federation",
];

export { ALL_MODULES, P2P_MODULES };

// ── Default Resolution ───────────────────────────────────────────────────

function defaultDataDir(): string {
  const home = process.env["HOME"] ?? process.env["USERPROFILE"] ?? ".";
  return `${home}/.prism/relay`;
}

function modeDefaults(mode: DeploymentMode): Partial<ResolvedRelayConfig> {
  switch (mode) {
    case "server":
      return {
        host: "0.0.0.0",
        port: 4444,
        hashcashBits: 16,
        corsOrigins: [],
        modules: ALL_MODULES,
        federation: { enabled: false, bootstrapPeers: [], publicUrl: undefined },
        logging: { level: "info", format: "json" },
      };
    case "p2p":
      return {
        host: "0.0.0.0",
        port: 4444,
        hashcashBits: 12,
        corsOrigins: [],
        modules: P2P_MODULES,
        federation: { enabled: true, bootstrapPeers: [], publicUrl: undefined },
        logging: { level: "info", format: "text" },
      };
    case "dev":
      return {
        host: "127.0.0.1",
        port: 4444,
        hashcashBits: 4,
        corsOrigins: ["*"],
        modules: ALL_MODULES,
        federation: { enabled: false, bootstrapPeers: [], publicUrl: undefined },
        logging: { level: "debug", format: "text" },
      };
  }
}

export function resolveConfig(
  file: RelayConfigFile,
  cliOverrides: Partial<RelayConfigFile> = {},
): ResolvedRelayConfig {
  // CLI flags override file config
  const merged = { ...file, ...stripUndefined(cliOverrides) };
  const mode = envStr("PRISM_RELAY_MODE", merged.mode) as DeploymentMode ?? "dev";
  const defaults = modeDefaults(mode);
  const dataDir = envStr("PRISM_RELAY_DATA_DIR", merged.dataDir) ?? defaultDataDir();

  return {
    mode,
    host: envStr("PRISM_RELAY_HOST", merged.host) ?? (defaults.host as string),
    port: envInt("PRISM_RELAY_PORT", merged.port) ?? (defaults.port as number),
    dataDir,
    identityFile: merged.identityFile ?? `${dataDir}/identity.json`,
    didMethod: merged.didMethod ?? "key",
    didWebDomain: merged.didWebDomain,
    modules: merged.modules ?? (defaults.modules as string[]),
    hashcashBits: merged.hashcashBits ?? (defaults.hashcashBits as number),
    corsOrigins: merged.corsOrigins ?? (defaults.corsOrigins as string[]),
    federation: {
      enabled: merged.federation?.enabled ?? (defaults.federation as ResolvedRelayConfig["federation"]).enabled,
      bootstrapPeers: merged.federation?.bootstrapPeers ?? [],
      publicUrl: envStr("PRISM_RELAY_PUBLIC_URL", merged.federation?.publicUrl),
    },
    relay: {
      defaultTtlMs: merged.relay?.defaultTtlMs ?? 7 * 24 * 60 * 60 * 1000,
      maxEnvelopeSizeBytes: merged.relay?.maxEnvelopeSizeBytes ?? 1_048_576,
      evictionIntervalMs: merged.relay?.evictionIntervalMs ?? 60_000,
    },
    logging: {
      level: envStr("PRISM_RELAY_LOG_LEVEL", merged.logging?.level) as ResolvedRelayConfig["logging"]["level"] ?? (defaults.logging as ResolvedRelayConfig["logging"]).level,
      format: merged.logging?.format ?? (defaults.logging as ResolvedRelayConfig["logging"]).format,
    },
  };
}

// ── Helpers ──────────────────────────────────────────────────────────────

function envStr<T extends string>(key: string, fallback: T | undefined): T | undefined {
  const val = process.env[key];
  return (val !== undefined && val !== "" ? val : fallback) as T | undefined;
}

function envInt(key: string, fallback: number | undefined): number | undefined {
  const val = process.env[key];
  if (val !== undefined && val !== "") {
    const parsed = parseInt(val, 10);
    if (!Number.isNaN(parsed)) return parsed;
  }
  return fallback;
}

function stripUndefined<T extends Record<string, unknown>>(obj: T): Partial<T> {
  const result: Record<string, unknown> = {};
  for (const [key, value] of Object.entries(obj)) {
    if (value !== undefined) result[key] = value;
  }
  return result as Partial<T>;
}
