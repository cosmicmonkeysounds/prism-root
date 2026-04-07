/**
 * CLI argument parser for Prism Relay.
 *
 * Supports subcommands:
 *   prism-relay [start] [--mode server|p2p|dev] [--port N] [--host H] ...
 *   prism-relay init [--mode server|p2p|dev] [--output path]
 *   prism-relay status [--port N] [--host H]
 *   prism-relay identity show [--data-dir path]
 *   prism-relay identity regenerate [--data-dir path] [--did-method key|web]
 *   prism-relay modules list
 *   prism-relay config validate [--config path]
 *   prism-relay config show [--config path] [--mode ...]
 *   prism-relay [--help] [--version]
 */

import type { DeploymentMode, RelayConfigFile } from "./relay-config.js";

export type SubCommand =
  | "start"
  | "init"
  | "status"
  | "identity-show"
  | "identity-regenerate"
  | "modules-list"
  | "config-validate"
  | "config-show"
  | "peers-list"
  | "peers-ban"
  | "peers-unban"
  | "collections-list"
  | "collections-inspect"
  | "collections-export"
  | "collections-import"
  | "collections-delete"
  | "portals-list"
  | "portals-inspect"
  | "portals-delete"
  | "webhooks-list"
  | "webhooks-delete"
  | "webhooks-test"
  | "tokens-list"
  | "tokens-revoke"
  | "certs-list"
  | "certs-renew"
  | "backup"
  | "restore"
  | "logs";

export interface ParsedArgs {
  command: SubCommand;
  configPath: string | undefined;
  help: boolean;
  version: boolean;
  overrides: Partial<RelayConfigFile>;
  /** Output path for `init` command. */
  initOutput: string | undefined;
  /** Positional argument (e.g. DID for ban/unban, ID for inspect/delete). */
  positionalArg: string | undefined;
  /** Input file path for import/restore. */
  inputFile: string | undefined;
  /** Output file path for export/backup. */
  outputFile: string | undefined;
  /** Log level filter for `logs` command. */
  logLevelFilter: string | undefined;
  /** Follow mode for `logs` command. */
  follow: boolean;
}

/**
 * Extract the subcommand from argv (first non-flag argument).
 * Returns the command and the remaining argv with the command stripped.
 */
function extractCommand(argv: string[]): { command: SubCommand; rest: string[] } {
  if (argv.length === 0) return { command: "start", rest: [] };

  const first = argv[0] as string;
  const second = argv[1];

  // Two-word subcommands
  if (first === "identity" && argv.length > 1) {
    if (second === "show") return { command: "identity-show", rest: argv.slice(2) };
    if (second === "regenerate") return { command: "identity-regenerate", rest: argv.slice(2) };
  }
  if (first === "config" && argv.length > 1) {
    if (second === "validate") return { command: "config-validate", rest: argv.slice(2) };
    if (second === "show") return { command: "config-show", rest: argv.slice(2) };
  }
  if (first === "modules" && argv.length > 1 && second === "list") {
    return { command: "modules-list", rest: argv.slice(2) };
  }
  if (first === "peers" && argv.length > 1) {
    if (second === "list") return { command: "peers-list", rest: argv.slice(2) };
    if (second === "ban") return { command: "peers-ban", rest: argv.slice(2) };
    if (second === "unban") return { command: "peers-unban", rest: argv.slice(2) };
  }
  if (first === "collections" && argv.length > 1) {
    if (second === "list") return { command: "collections-list", rest: argv.slice(2) };
    if (second === "inspect") return { command: "collections-inspect", rest: argv.slice(2) };
    if (second === "export") return { command: "collections-export", rest: argv.slice(2) };
    if (second === "import") return { command: "collections-import", rest: argv.slice(2) };
    if (second === "delete") return { command: "collections-delete", rest: argv.slice(2) };
  }
  if (first === "portals" && argv.length > 1) {
    if (second === "list") return { command: "portals-list", rest: argv.slice(2) };
    if (second === "inspect") return { command: "portals-inspect", rest: argv.slice(2) };
    if (second === "delete") return { command: "portals-delete", rest: argv.slice(2) };
  }
  if (first === "webhooks" && argv.length > 1) {
    if (second === "list") return { command: "webhooks-list", rest: argv.slice(2) };
    if (second === "delete") return { command: "webhooks-delete", rest: argv.slice(2) };
    if (second === "test") return { command: "webhooks-test", rest: argv.slice(2) };
  }
  if (first === "tokens" && argv.length > 1) {
    if (second === "list") return { command: "tokens-list", rest: argv.slice(2) };
    if (second === "revoke") return { command: "tokens-revoke", rest: argv.slice(2) };
  }
  if (first === "certs" && argv.length > 1) {
    if (second === "list") return { command: "certs-list", rest: argv.slice(2) };
    if (second === "renew") return { command: "certs-renew", rest: argv.slice(2) };
  }

  // Single-word subcommands
  if (first === "start") return { command: "start", rest: argv.slice(1) };
  if (first === "init") return { command: "init", rest: argv.slice(1) };
  if (first === "status") return { command: "status", rest: argv.slice(1) };
  if (first === "backup") return { command: "backup", rest: argv.slice(1) };
  if (first === "restore") return { command: "restore", rest: argv.slice(1) };
  if (first === "logs") return { command: "logs", rest: argv.slice(1) };
  if (first === "identity") return { command: "identity-show", rest: argv.slice(1) };
  if (first === "config") return { command: "config-show", rest: argv.slice(1) };
  if (first === "modules") return { command: "modules-list", rest: argv.slice(1) };
  if (first === "peers") return { command: "peers-list", rest: argv.slice(1) };
  if (first === "collections") return { command: "collections-list", rest: argv.slice(1) };
  if (first === "portals") return { command: "portals-list", rest: argv.slice(1) };
  if (first === "webhooks") return { command: "webhooks-list", rest: argv.slice(1) };
  if (first === "tokens") return { command: "tokens-list", rest: argv.slice(1) };
  if (first === "certs") return { command: "certs-list", rest: argv.slice(1) };

  // If first arg is a flag, default to "start"
  if (first.startsWith("-")) return { command: "start", rest: argv };

  // Unknown word — treat as start with the full argv
  return { command: "start", rest: argv };
}

export function parseArgs(argv: string[]): ParsedArgs {
  const { command, rest } = extractCommand(argv);

  let configPath: string | undefined;
  let help = false;
  let version = false;
  let initOutput: string | undefined;
  const overrides: Partial<RelayConfigFile> = {};

  for (let i = 0; i < rest.length; i++) {
    const arg = rest[i];
    const next = rest[i + 1];

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
      case "--output":
      case "-o":
        initOutput = next;
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
      case "--apns-key-path":
        if (next) {
          if (!overrides.push) overrides.push = {};
          if (!overrides.push.apns) {
            overrides.push.apns = { keyId: "", teamId: "", privateKey: next, bundleId: "" };
          } else {
            overrides.push.apns.privateKey = next;
          }
        }
        i++;
        break;
      case "--apns-key-id":
        if (next) {
          if (!overrides.push) overrides.push = {};
          if (!overrides.push.apns) {
            overrides.push.apns = { keyId: next, teamId: "", privateKey: "", bundleId: "" };
          } else {
            overrides.push.apns.keyId = next;
          }
        }
        i++;
        break;
      case "--apns-team-id":
        if (next) {
          if (!overrides.push) overrides.push = {};
          if (!overrides.push.apns) {
            overrides.push.apns = { keyId: "", teamId: next, privateKey: "", bundleId: "" };
          } else {
            overrides.push.apns.teamId = next;
          }
        }
        i++;
        break;
      case "--apns-bundle-id":
        if (next) {
          if (!overrides.push) overrides.push = {};
          if (!overrides.push.apns) {
            overrides.push.apns = { keyId: "", teamId: "", privateKey: "", bundleId: next };
          } else {
            overrides.push.apns.bundleId = next;
          }
        }
        i++;
        break;
      case "--fcm-key-path":
        if (next) {
          if (!overrides.push) overrides.push = {};
          if (!overrides.push.fcm) {
            overrides.push.fcm = { projectId: "", serviceAccountKey: next };
          } else {
            overrides.push.fcm.serviceAccountKey = next;
          }
        }
        i++;
        break;
      case "--fcm-project-id":
        if (next) {
          if (!overrides.push) overrides.push = {};
          if (!overrides.push.fcm) {
            overrides.push.fcm = { projectId: next, serviceAccountKey: "" };
          } else {
            overrides.push.fcm.projectId = next;
          }
        }
        i++;
        break;
    }
  }

  // Extract positional arg (first non-flag argument in rest, after flags are consumed)
  let positionalArg: string | undefined;
  let inputFile: string | undefined;
  let outputFile: string | undefined;
  let logLevelFilter: string | undefined;
  let follow = false;

  // Re-scan for new flags and positional arg
  const remaining = rest.filter((_, idx) => {
    // Already consumed by the switch above
    return true;
  });

  for (let j = 0; j < rest.length; j++) {
    const a = rest[j];
    if (a === "--input" || a === "-i") {
      inputFile = rest[j + 1];
      j++;
    } else if (a === "--output" || a === "-o") {
      // Already handled for init, also capture for export/backup
      if (!initOutput) outputFile = rest[j + 1];
      j++;
    } else if (a === "--level" && command === "logs") {
      logLevelFilter = rest[j + 1];
      j++;
    } else if (a === "--follow" || a === "-f") {
      follow = true;
    } else if (!a.startsWith("-") && positionalArg === undefined) {
      // Positional argument — skip if it was consumed by a prior flag
      const prev = j > 0 ? rest[j - 1] : "";
      const flagsThatConsumeNext = [
        "--config", "-c", "--output", "-o", "--mode", "--port", "--host",
        "--data-dir", "--identity", "--modules", "--cors", "--hashcash-bits",
        "--did-method", "--did-web-domain", "--public-url", "--bootstrap-peer",
        "--log-level", "--log-format", "--apns-key-path", "--apns-key-id",
        "--apns-team-id", "--apns-bundle-id", "--fcm-key-path", "--fcm-project-id",
        "--input", "-i", "--level",
      ];
      if (!flagsThatConsumeNext.includes(prev)) {
        positionalArg = a;
      }
    }
  }

  return { command, configPath, help, version, overrides, initOutput, positionalArg, inputFile, outputFile, logLevelFilter, follow };
}

export function printHelp(): string {
  return `
Prism Relay — distributed relay server for the Prism framework.

USAGE:
  prism-relay [COMMAND] [OPTIONS]

COMMANDS:
  start                    Start the relay server (default if no command given)
  init                     Generate a starter config file
  status                   Check health of a running relay

  identity show            Display the relay's DID and public key
  identity regenerate      Generate a new identity (backs up the old one)
  modules list             List all available relay modules
  config validate          Validate a config file without starting
  config show              Show the fully resolved config (with defaults applied)

MANAGEMENT (connect to a running relay via HTTP):
  peers list               List federation peers
  peers ban <did>          Ban a peer by DID
  peers unban <did>        Unban a peer

  collections list         List hosted collections
  collections inspect <id> Show collection details (object count, size)
  collections export <id>  Export collection to file (--output <path>)
  collections import <id>  Import collection from file (--input <path>)
  collections delete <id>  Delete a hosted collection

  portals list             List published portals
  portals inspect <id>     Show portal details
  portals delete <id>      Delete a portal

  webhooks list            List registered webhooks
  webhooks delete <id>     Delete a webhook
  webhooks test <id>       Send a test delivery

  tokens list              List active capability tokens
  tokens revoke <id>       Revoke a token

  certs list               List ACME certificates
  certs renew <domain>     Renew a certificate

  backup                   Export full relay state to file (--output <path>)
  restore                  Import relay state from file (--input <path>)
  logs                     View recent log entries (--level, --follow)

DEPLOYMENT MODES:
  --mode server    Always-on relay server (all modules, hashcash=16, no CORS)
  --mode p2p       Federated peer relay (minimal modules, federation enabled)
  --mode dev       Local development (all modules, hashcash=4, CORS=*, debug logging)

OPTIONS:
  -c, --config <path>        Config file path (default: ./relay.config.json)
  -o, --output <path>        Output path for init/export/backup
  -i, --input <path>         Input path for import/restore
  --port <number>            Listen port / target relay port (default: 4444)
  --host <address>           Bind address / target relay host (default: varies)
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
  --level <level>            Log level filter for 'logs' command
  --follow, -f               Follow mode for 'logs' command (poll every 2s)
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
  prism-relay start --mode server --port 443 --host 0.0.0.0

  # Check a running relay
  prism-relay status --port 4444

  # List collections on a relay
  prism-relay collections list --port 4444

  # Export a collection
  prism-relay collections export my-collection --output ./backup.json

  # Ban a peer
  prism-relay peers ban did:key:z6MkPeer1 --port 4444

  # Backup full relay state
  prism-relay backup --output relay-backup.json

  # View logs filtered to errors
  prism-relay logs --level error --follow

  # P2P peer with federation
  prism-relay start --mode p2p --public-url https://my-relay.example.com \\
    --bootstrap-peer did:key:zPeer1@https://peer1.example.com
`.trim();
}
