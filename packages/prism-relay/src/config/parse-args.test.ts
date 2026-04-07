import { describe, it, expect } from "vitest";
import { parseArgs } from "./parse-args.js";

describe("parseArgs", () => {
  // ── Subcommand extraction ──────────────────────────────────────────

  it("defaults to start command when no args", () => {
    const result = parseArgs([]);
    expect(result.command).toBe("start");
  });

  it("defaults to start command when first arg is a flag", () => {
    const result = parseArgs(["--mode", "dev"]);
    expect(result.command).toBe("start");
    expect(result.overrides.mode).toBe("dev");
  });

  it("parses explicit start command", () => {
    const result = parseArgs(["start", "--port", "8080"]);
    expect(result.command).toBe("start");
    expect(result.overrides.port).toBe(8080);
  });

  it("parses init command", () => {
    const result = parseArgs(["init", "--mode", "server"]);
    expect(result.command).toBe("init");
    expect(result.overrides.mode).toBe("server");
  });

  it("parses init with --output", () => {
    const result = parseArgs(["init", "--output", "/tmp/relay.json"]);
    expect(result.command).toBe("init");
    expect(result.initOutput).toBe("/tmp/relay.json");
  });

  it("parses init with -o shorthand", () => {
    const result = parseArgs(["init", "-o", "my-config.json"]);
    expect(result.command).toBe("init");
    expect(result.initOutput).toBe("my-config.json");
  });

  it("parses status command", () => {
    const result = parseArgs(["status", "--port", "5555"]);
    expect(result.command).toBe("status");
    expect(result.overrides.port).toBe(5555);
  });

  it("parses identity show command", () => {
    const result = parseArgs(["identity", "show"]);
    expect(result.command).toBe("identity-show");
  });

  it("parses identity regenerate command", () => {
    const result = parseArgs(["identity", "regenerate", "--did-method", "web"]);
    expect(result.command).toBe("identity-regenerate");
    expect(result.overrides.didMethod).toBe("web");
  });

  it("defaults identity to show", () => {
    const result = parseArgs(["identity"]);
    expect(result.command).toBe("identity-show");
  });

  it("parses modules list command", () => {
    const result = parseArgs(["modules", "list"]);
    expect(result.command).toBe("modules-list");
  });

  it("defaults modules to list", () => {
    const result = parseArgs(["modules"]);
    expect(result.command).toBe("modules-list");
  });

  it("parses config validate command", () => {
    const result = parseArgs(["config", "validate", "-c", "relay.json"]);
    expect(result.command).toBe("config-validate");
    expect(result.configPath).toBe("relay.json");
  });

  it("parses config show command", () => {
    const result = parseArgs(["config", "show", "--mode", "p2p"]);
    expect(result.command).toBe("config-show");
    expect(result.overrides.mode).toBe("p2p");
  });

  it("defaults config to show", () => {
    const result = parseArgs(["config"]);
    expect(result.command).toBe("config-show");
  });

  // ── Flag parsing (preserved from original) ─────────────────────────

  it("parses --help flag", () => {
    const result = parseArgs(["--help"]);
    expect(result.help).toBe(true);
  });

  it("parses -h flag", () => {
    const result = parseArgs(["-h"]);
    expect(result.help).toBe(true);
  });

  it("parses --version flag", () => {
    const result = parseArgs(["--version"]);
    expect(result.version).toBe(true);
  });

  it("parses --config path", () => {
    const result = parseArgs(["--config", "/path/to/config.json"]);
    expect(result.configPath).toBe("/path/to/config.json");
  });

  it("parses -c shorthand", () => {
    const result = parseArgs(["-c", "relay.json"]);
    expect(result.configPath).toBe("relay.json");
  });

  it("parses --mode", () => {
    const result = parseArgs(["--mode", "server"]);
    expect(result.overrides.mode).toBe("server");
  });

  it("parses --port", () => {
    const result = parseArgs(["--port", "8080"]);
    expect(result.overrides.port).toBe(8080);
  });

  it("parses --host", () => {
    const result = parseArgs(["--host", "0.0.0.0"]);
    expect(result.overrides.host).toBe("0.0.0.0");
  });

  it("parses --modules as comma-separated list", () => {
    const result = parseArgs(["--modules", "blind-mailbox,relay-router,federation"]);
    expect(result.overrides.modules).toEqual(["blind-mailbox", "relay-router", "federation"]);
  });

  it("parses --cors as comma-separated list", () => {
    const result = parseArgs(["--cors", "http://localhost:3000,http://example.com"]);
    expect(result.overrides.corsOrigins).toEqual(["http://localhost:3000", "http://example.com"]);
  });

  it("parses --data-dir", () => {
    const result = parseArgs(["--data-dir", "/var/prism"]);
    expect(result.overrides.dataDir).toBe("/var/prism");
  });

  it("parses --identity", () => {
    const result = parseArgs(["--identity", "/keys/relay.json"]);
    expect(result.overrides.identityFile).toBe("/keys/relay.json");
  });

  it("parses --hashcash-bits", () => {
    const result = parseArgs(["--hashcash-bits", "20"]);
    expect(result.overrides.hashcashBits).toBe(20);
  });

  it("parses --public-url", () => {
    const result = parseArgs(["--public-url", "https://relay.example.com"]);
    expect(result.overrides.federation?.publicUrl).toBe("https://relay.example.com");
  });

  it("parses --bootstrap-peer", () => {
    const result = parseArgs([
      "--bootstrap-peer",
      "did:key:zPeer1@http://peer1:4444,did:key:zPeer2@http://peer2:4444",
    ]);
    const peers = result.overrides.federation?.bootstrapPeers ?? [];
    expect(peers).toHaveLength(2);
    expect(peers[0]).toEqual({ relayDid: "did:key:zPeer1", url: "http://peer1:4444" });
    expect(peers[1]).toEqual({ relayDid: "did:key:zPeer2", url: "http://peer2:4444" });
  });

  it("parses --log-level", () => {
    const result = parseArgs(["--log-level", "warn"]);
    expect(result.overrides.logging?.level).toBe("warn");
  });

  it("parses --log-format", () => {
    const result = parseArgs(["--log-format", "json"]);
    expect(result.overrides.logging?.format).toBe("json");
  });

  it("combines multiple flags", () => {
    const result = parseArgs([
      "--mode", "p2p",
      "--port", "5555",
      "--public-url", "http://me:5555",
      "--log-level", "info",
    ]);
    expect(result.overrides.mode).toBe("p2p");
    expect(result.overrides.port).toBe(5555);
    expect(result.overrides.federation?.publicUrl).toBe("http://me:5555");
    expect(result.overrides.logging?.level).toBe("info");
  });

  it("returns empty overrides for no args", () => {
    const result = parseArgs([]);
    expect(result.help).toBe(false);
    expect(result.version).toBe(false);
    expect(result.configPath).toBeUndefined();
    expect(Object.keys(result.overrides)).toHaveLength(0);
  });

  // ── Subcommand + flags combined ────────────────────────────────────

  it("passes flags through to subcommands", () => {
    const result = parseArgs(["start", "--mode", "server", "--port", "443", "--host", "0.0.0.0"]);
    expect(result.command).toBe("start");
    expect(result.overrides.mode).toBe("server");
    expect(result.overrides.port).toBe(443);
    expect(result.overrides.host).toBe("0.0.0.0");
  });

  it("identity regenerate with did-web options", () => {
    const result = parseArgs(["identity", "regenerate", "--did-method", "web", "--did-web-domain", "relay.example.com"]);
    expect(result.command).toBe("identity-regenerate");
    expect(result.overrides.didMethod).toBe("web");
    expect(result.overrides.didWebDomain).toBe("relay.example.com");
  });

  // ── Peers subcommands ───────────────────────────────────────────────

  it("parses peers list", () => {
    const result = parseArgs(["peers", "list"]);
    expect(result.command).toBe("peers-list");
  });

  it("parses peers ban with DID", () => {
    const result = parseArgs(["peers", "ban", "did:key:z123"]);
    expect(result.command).toBe("peers-ban");
    expect(result.positionalArg).toBe("did:key:z123");
  });

  it("parses peers unban with DID", () => {
    const result = parseArgs(["peers", "unban", "did:key:z123"]);
    expect(result.command).toBe("peers-unban");
    expect(result.positionalArg).toBe("did:key:z123");
  });

  it("defaults bare peers to peers-list", () => {
    const result = parseArgs(["peers"]);
    expect(result.command).toBe("peers-list");
  });

  // ── Collections subcommands ─────────────────────────────────────────

  it("parses collections list", () => {
    const result = parseArgs(["collections", "list"]);
    expect(result.command).toBe("collections-list");
  });

  it("parses collections inspect with positional arg", () => {
    const result = parseArgs(["collections", "inspect", "myCol"]);
    expect(result.command).toBe("collections-inspect");
    expect(result.positionalArg).toBe("myCol");
  });

  it("parses collections export with positional arg and --output", () => {
    const result = parseArgs(["collections", "export", "myCol", "--output", "out.json"]);
    expect(result.command).toBe("collections-export");
    expect(result.positionalArg).toBe("myCol");
    expect(result.outputFile).toBe("out.json");
  });

  it("parses collections import with positional arg and --input", () => {
    const result = parseArgs(["collections", "import", "myCol", "--input", "in.json"]);
    expect(result.command).toBe("collections-import");
    expect(result.positionalArg).toBe("myCol");
    expect(result.inputFile).toBe("in.json");
  });

  it("parses collections delete with positional arg", () => {
    const result = parseArgs(["collections", "delete", "myCol"]);
    expect(result.command).toBe("collections-delete");
    expect(result.positionalArg).toBe("myCol");
  });

  it("defaults bare collections to collections-list", () => {
    const result = parseArgs(["collections"]);
    expect(result.command).toBe("collections-list");
  });

  // ── Portals subcommands ─────────────────────────────────────────────

  it("parses portals list", () => {
    const result = parseArgs(["portals", "list"]);
    expect(result.command).toBe("portals-list");
  });

  it("parses portals delete with positional arg", () => {
    const result = parseArgs(["portals", "delete", "p1"]);
    expect(result.command).toBe("portals-delete");
    expect(result.positionalArg).toBe("p1");
  });

  it("defaults bare portals to portals-list", () => {
    const result = parseArgs(["portals"]);
    expect(result.command).toBe("portals-list");
  });

  // ── Webhooks subcommands ────────────────────────────────────────────

  it("parses webhooks list", () => {
    const result = parseArgs(["webhooks", "list"]);
    expect(result.command).toBe("webhooks-list");
  });

  it("parses webhooks test with positional arg", () => {
    const result = parseArgs(["webhooks", "test", "w1"]);
    expect(result.command).toBe("webhooks-test");
    expect(result.positionalArg).toBe("w1");
  });

  // ── Tokens subcommands ──────────────────────────────────────────────

  it("parses tokens list", () => {
    const result = parseArgs(["tokens", "list"]);
    expect(result.command).toBe("tokens-list");
  });

  it("parses tokens revoke with positional arg", () => {
    const result = parseArgs(["tokens", "revoke", "t1"]);
    expect(result.command).toBe("tokens-revoke");
    expect(result.positionalArg).toBe("t1");
  });

  // ── Certs subcommands ───────────────────────────────────────────────

  it("parses certs list", () => {
    const result = parseArgs(["certs", "list"]);
    expect(result.command).toBe("certs-list");
  });

  it("parses certs renew with positional arg", () => {
    const result = parseArgs(["certs", "renew", "example.com"]);
    expect(result.command).toBe("certs-renew");
    expect(result.positionalArg).toBe("example.com");
  });

  // ── Backup / Restore / Logs ─────────────────────────────────────────

  it("parses backup command", () => {
    const result = parseArgs(["backup"]);
    expect(result.command).toBe("backup");
  });

  it("parses backup with --output", () => {
    const result = parseArgs(["backup", "--output", "b.json"]);
    expect(result.command).toBe("backup");
    expect(result.outputFile).toBe("b.json");
  });

  it("parses restore with --input", () => {
    const result = parseArgs(["restore", "--input", "b.json"]);
    expect(result.command).toBe("restore");
    expect(result.inputFile).toBe("b.json");
  });

  it("parses logs command", () => {
    const result = parseArgs(["logs"]);
    expect(result.command).toBe("logs");
  });

  it("parses logs with --level and --follow", () => {
    const result = parseArgs(["logs", "--level", "error", "--follow"]);
    expect(result.command).toBe("logs");
    expect(result.logLevelFilter).toBe("error");
    expect(result.follow).toBe(true);
  });
});
