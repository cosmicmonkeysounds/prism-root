import { describe, it, expect, afterEach } from "vitest";
import { resolveConfig, ALL_MODULES, P2P_MODULES } from "./relay-config.js";

// Save and restore env vars across tests
const envBackup = new Map<string, string | undefined>();

function setEnv(key: string, value: string): void {
  envBackup.set(key, process.env[key]);
  process.env[key] = value;
}

afterEach(() => {
  for (const [key, value] of envBackup) {
    if (value === undefined) {
      process.env[key] = "";
    } else {
      process.env[key] = value;
    }
  }
  envBackup.clear();
});

describe("resolveConfig", () => {
  it("defaults to dev mode with all modules", () => {
    const config = resolveConfig({});
    expect(config.mode).toBe("dev");
    expect(config.modules).toEqual(ALL_MODULES);
    expect(config.hashcashBits).toBe(4);
    expect(config.corsOrigins).toEqual(["*"]);
    expect(config.host).toBe("127.0.0.1");
    expect(config.logging.level).toBe("debug");
    expect(config.logging.format).toBe("text");
  });

  it("server mode has strict defaults", () => {
    const config = resolveConfig({ mode: "server" });
    expect(config.mode).toBe("server");
    expect(config.hashcashBits).toBe(16);
    expect(config.corsOrigins).toEqual([]);
    expect(config.host).toBe("0.0.0.0");
    expect(config.federation.enabled).toBe(false);
    expect(config.logging.format).toBe("json");
  });

  it("p2p mode enables federation and uses minimal modules", () => {
    const config = resolveConfig({ mode: "p2p" });
    expect(config.mode).toBe("p2p");
    expect(config.modules).toEqual(P2P_MODULES);
    expect(config.federation.enabled).toBe(true);
    expect(config.hashcashBits).toBe(12);
  });

  it("CLI overrides take precedence over file config", () => {
    const config = resolveConfig(
      { port: 3000, host: "localhost" },
      { port: 9999 },
    );
    expect(config.port).toBe(9999);
    expect(config.host).toBe("localhost");
  });

  it("env vars override file config", () => {
    setEnv("PRISM_RELAY_PORT", "8080");
    setEnv("PRISM_RELAY_HOST", "10.0.0.1");
    const config = resolveConfig({ port: 3000, host: "localhost" });
    expect(config.port).toBe(8080);
    expect(config.host).toBe("10.0.0.1");
  });

  it("env var sets deployment mode", () => {
    setEnv("PRISM_RELAY_MODE", "server");
    const config = resolveConfig({});
    expect(config.mode).toBe("server");
    expect(config.hashcashBits).toBe(16);
  });

  it("identity file defaults to dataDir/identity.json", () => {
    const config = resolveConfig({ dataDir: "/tmp/test-relay" });
    expect(config.identityFile).toBe("/tmp/test-relay/identity.json");
  });

  it("explicit identityFile overrides default", () => {
    const config = resolveConfig({
      dataDir: "/tmp/test-relay",
      identityFile: "/custom/key.json",
    });
    expect(config.identityFile).toBe("/custom/key.json");
  });

  it("federation bootstrap peers are preserved", () => {
    const config = resolveConfig({
      mode: "p2p",
      federation: {
        enabled: true,
        bootstrapPeers: [
          { relayDid: "did:key:zPeer1", url: "http://peer1:4444" },
        ],
        publicUrl: "http://me:4444",
      },
    });
    expect(config.federation.bootstrapPeers).toHaveLength(1);
    expect(config.federation.publicUrl).toBe("http://me:4444");
  });

  it("custom module list overrides mode defaults", () => {
    const config = resolveConfig({
      mode: "server",
      modules: ["blind-mailbox", "relay-router"],
    });
    expect(config.modules).toEqual(["blind-mailbox", "relay-router"]);
  });

  it("relay config overrides are applied", () => {
    const config = resolveConfig({
      relay: { defaultTtlMs: 1000, maxEnvelopeSizeBytes: 512 },
    });
    expect(config.relay.defaultTtlMs).toBe(1000);
    expect(config.relay.maxEnvelopeSizeBytes).toBe(512);
    expect(config.relay.evictionIntervalMs).toBe(60_000); // default
  });
});
