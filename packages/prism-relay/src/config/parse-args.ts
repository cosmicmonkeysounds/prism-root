/**
 * CLI argument parser for Prism Relay.
 *
 * Supports:
 *   prism-relay [--mode server|p2p|dev] [--port N] [--host H]
 *               [--config path] [--data-dir path] [--identity path]
 *               [--modules mod1,mod2,...] [--cors origin1,origin2]
 *               [--public-url URL] [--log-level debug|info|warn|error]
 *               [--log-format text|json] [--hashcash-bits N]
 *               [--did-method key|web] [--did-web-domain D]
 *               [--bootstrap-peer did@url,...]
 *               [--help] [--version]
 */

import type { DeploymentMode, RelayConfigFile } from "./relay-config.js";

export interface ParsedArgs {
  configPath: string | undefined;
  help: boolean;
  version: boolean;
  overrides: Partial<RelayConfigFile>;
}

export function parseArgs(argv: string[]): ParsedArgs {
  let configPath: string | undefined;
  let help = false;
  let version = false;
  const overrides: Partial<RelayConfigFile> = {};

  for (let i = 0; i < argv.length; i++) {
    const arg = argv[i];
    const next = argv[i + 1];

    switch (arg) {
      case "--help":
      case "-h":
        help = true;
        break;
      case "--version":
      case "-v":
        version = true;
        break;
      case "--config":
      case "-c":
        configPath = next;
        i++;
        break;
      case "--mode":
        overrides.mode = next as DeploymentMode;
        i++;
        break;
      case "--port":
        overrides.port = parseInt(next ?? "", 10);
        i++;
        break;
      case "--host":
        if (next) overrides.host = next;
        i++;
        break;
      case "--data-dir":
        if (next) overrides.dataDir = next;
        i++;
        break;
      case "--identity":
        if (next) overrides.identityFile = next;
        i++;
        break;
      case "--modules":
        overrides.modules = (next ?? "").split(",").map((s) => s.trim());
        i++;
        break;
      case "--cors":
        overrides.corsOrigins = (next ?? "").split(",").map((s) => s.trim());
        i++;
        break;
      case "--hashcash-bits":
        overrides.hashcashBits = parseInt(next ?? "", 10);
        i++;
        break;
      case "--did-method":
        overrides.didMethod = next as "key" | "web";
        i++;
        break;
      case "--did-web-domain":
        if (next) overrides.didWebDomain = next;
        i++;
        break;
      case "--public-url":
        if (next) {
          if (!overrides.federation) overrides.federation = {};
          overrides.federation.publicUrl = next;
        }
        i++;
        break;
      case "--bootstrap-peer": {
        if (!overrides.federation) overrides.federation = {};
        if (!overrides.federation.bootstrapPeers) overrides.federation.bootstrapPeers = [];
        // Format: did@url,did@url,...
        const peers = (next ?? "").split(",");
        for (const peer of peers) {
          const atIdx = peer.indexOf("@");
          if (atIdx > 0) {
            overrides.federation.bootstrapPeers.push({
              relayDid: peer.slice(0, atIdx) as `did:${string}:${string}`,
              url: peer.slice(atIdx + 1),
            });
          }
        }
        i++;
        break;
      }
      case "--log-level":
        if (!overrides.logging) overrides.logging = {};
        overrides.logging.level = next as "debug" | "info" | "warn" | "error";
        i++;
        break;
      case "--log-format":
        if (!overrides.logging) overrides.logging = {};
        overrides.logging.format = next as "text" | "json";
        i++;
        break;
    }
  }

  return { configPath, help, version, overrides };
}

export function printHelp(): string {
  return `
Prism Relay — distributed relay server for the Prism framework.

USAGE:
  prism-relay [OPTIONS]

DEPLOYMENT MODES:
  --mode server    Always-on relay server (all modules, hashcash=16, no CORS)
  --mode p2p       Federated peer relay (minimal modules, federation enabled)
  --mode dev       Local development (all modules, hashcash=4, CORS=*, debug logging)

OPTIONS:
  -c, --config <path>        Config file path (default: ./relay.config.json)
  --port <number>            Listen port (default: 4444)
  --host <address>           Bind address (default: varies by mode)
  --data-dir <path>          Data directory (default: ~/.prism/relay)
  --identity <path>          Identity key file (default: <data-dir>/identity.json)
  --modules <list>           Comma-separated module names to enable
  --cors <origins>           Comma-separated CORS allowed origins
  --hashcash-bits <number>   Proof-of-work difficulty
  --did-method <key|web>     DID method for identity generation
  --did-web-domain <domain>  Domain for did:web method
  --public-url <url>         Public URL for federation announce
  --bootstrap-peer <list>    Comma-separated did@url pairs for federation
  --log-level <level>        debug, info, warn, or error
  --log-format <format>      text or json
  -h, --help                 Show this help
  -v, --version              Show version

ENVIRONMENT VARIABLES:
  PRISM_RELAY_MODE           Deployment mode
  PRISM_RELAY_HOST           Bind address
  PRISM_RELAY_PORT           Listen port
  PRISM_RELAY_DATA_DIR       Data directory
  PRISM_RELAY_PUBLIC_URL     Public URL for federation
  PRISM_RELAY_LOG_LEVEL      Log level

EXAMPLES:
  # Local development
  prism-relay --mode dev

  # Production server
  prism-relay --mode server --port 443 --host 0.0.0.0

  # P2P peer with federation
  prism-relay --mode p2p --public-url https://my-relay.example.com \\
    --bootstrap-peer did:key:zPeer1@https://peer1.example.com

  # From config file
  prism-relay --config ./my-relay.json
`.trim();
}
