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
  | "config-show";

export interface ParsedArgs {
  command: SubCommand;
  configPath: string | undefined;
  help: boolean;
  version: boolean;
  overrides: Partial<RelayConfigFile>;
  /** Output path for `init` command. */
  initOutput: string | undefined;
}

/**
 * Extract the subcommand from argv (first non-flag argument).
 * Returns the command and the remaining argv with the command stripped.
 */
function extractCommand(argv: string[]): { command: SubCommand; rest: string[] } {
  if (argv.length === 0) return { command: "start", rest: [] };

  const first = argv[0] as string;

  // Two-word subcommands
  if (first === "identity" && argv.length > 1) {
    const sub = argv[1];
    if (sub === "show") return { command: "identity-show", rest: argv.slice(2) };
    if (sub === "regenerate") return { command: "identity-regenerate", rest: argv.slice(2) };
  }
  if (first === "config" && argv.length > 1) {
    const sub = argv[1];
    if (sub === "validate") return { command: "config-validate", rest: argv.slice(2) };
    if (sub === "show") return { command: "config-show", rest: argv.slice(2) };
  }
  if (first === "modules" && argv.length > 1 && argv[1] === "list") {
    return { command: "modules-list", rest: argv.slice(2) };
  }

  // Single-word subcommands
  if (first === "start") return { command: "start", rest: argv.slice(1) };
  if (first === "init") return { command: "init", rest: argv.slice(1) };
  if (first === "status") return { command: "status", rest: argv.slice(1) };
  if (first === "identity") return { command: "identity-show", rest: argv.slice(1) };
  if (first === "config") return { command: "config-show", rest: argv.slice(1) };
  if (first === "modules") return { command: "modules-list", rest: argv.slice(1) };

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
    }
  }

  return { command, configPath, help, version, overrides, initOutput };
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

DEPLOYMENT MODES:
  --mode server    Always-on relay server (all modules, hashcash=16, no CORS)
  --mode p2p       Federated peer relay (minimal modules, federation enabled)
  --mode dev       Local development (all modules, hashcash=4, CORS=*, debug logging)

OPTIONS:
  -c, --config <path>        Config file path (default: ./relay.config.json)
  -o, --output <path>        Output path for init command (default: ./relay.config.json)
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

  # Generate a production config file
  prism-relay init --mode server -o relay.config.json

  # Validate config before deploying
  prism-relay config validate -c relay.config.json

  # Show what the resolved config looks like
  prism-relay config show --mode p2p --public-url https://my-relay.example.com

  # Check a running relay
  prism-relay status --port 4444

  # Show relay identity
  prism-relay identity show

  # Production server
  prism-relay start --mode server --port 443 --host 0.0.0.0

  # P2P peer with federation
  prism-relay start --mode p2p --public-url https://my-relay.example.com \\
    --bootstrap-peer did:key:zPeer1@https://peer1.example.com

  # From config file
  prism-relay -c ./my-relay.json
`.trim();
}
