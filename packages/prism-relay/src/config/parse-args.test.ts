import { describe, it, expect } from "vitest";
import { parseArgs } from "./parse-args.js";

describe("parseArgs", () => {
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
});
