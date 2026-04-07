import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import * as os from "node:os";
import * as path from "node:path";
import * as fs from "node:fs";
import { createIdentity } from "@prism/core/identity";
import {
  createRelayBuilder,
  sovereignPortalModule,
  webhookModule,
  portalTemplateModule,
  RELAY_CAPABILITIES,
} from "@prism/core/relay";
import { peerTrustModule, federationModule } from "@prism/core/relay";
import type {
  RelayInstance,
  RelayModule,
  PortalRegistry,
  WebhookEmitter,
  PortalTemplateRegistry,
  FederationRegistry,
} from "@prism/core/relay";
import type { PeerTrustGraph } from "@prism/core/trust";
import { createFileStore } from "./file-store.js";

function makeTempDir(): string {
  return fs.mkdtempSync(path.join(os.tmpdir(), "relay-test-"));
}

function buildRelay(modules: RelayModule[]): RelayInstance {
  const identity = createIdentity();
  let builder = createRelayBuilder({ relayDid: identity.did });
  for (const mod of modules) {
    builder = builder.use(mod);
  }
  return builder.build();
}

describe("file-store", () => {
  let tmpDir: string;

  beforeEach(() => {
    tmpDir = makeTempDir();
  });

  afterEach(() => {
    fs.rmSync(tmpDir, { recursive: true, force: true });
  });

  it("creates dataDir and state file on load then save", () => {
    const dataDir = path.join(tmpDir, "nested", "data");
    const relay = buildRelay([]);
    const store = createFileStore({ dataDir });

    // dataDir doesn't exist yet
    expect(fs.existsSync(dataDir)).toBe(false);

    // load creates the directory
    store.load(relay);
    expect(fs.existsSync(dataDir)).toBe(true);

    // save writes the state file
    store.save(relay);
    expect(fs.existsSync(path.join(dataDir, "relay-state.json"))).toBe(true);
  });

  it("round-trips portals via save/load", () => {
    const dataDir = path.join(tmpDir, "portals");
    const store = createFileStore({ dataDir });

    const relay1 = buildRelay([sovereignPortalModule()]);
    const portals1 = relay1.getCapability<PortalRegistry>(RELAY_CAPABILITIES.PORTALS)!;
    portals1.register({
      name: "Test Portal",
      level: 1,
      collectionId: "col-1",
      basePath: "/test",
      isPublic: true,
    });

    store.load(relay1); // ensures dir exists
    store.save(relay1);

    const relay2 = buildRelay([sovereignPortalModule()]);
    store.load(relay2);

    const portals2 = relay2.getCapability<PortalRegistry>(RELAY_CAPABILITIES.PORTALS)!;
    const list = portals2.list();
    expect(list.length).toBe(1);
    expect(list[0]!.name).toBe("Test Portal");
    expect(list[0]!.level).toBe(1);
    expect(list[0]!.collectionId).toBe("col-1");
    expect(list[0]!.isPublic).toBe(true);
  });

  it("round-trips webhooks via save/load", () => {
    const dataDir = path.join(tmpDir, "webhooks");
    const store = createFileStore({ dataDir });

    const relay1 = buildRelay([webhookModule()]);
    const webhooks1 = relay1.getCapability<WebhookEmitter>(RELAY_CAPABILITIES.WEBHOOKS)!;
    webhooks1.register({
      url: "https://example.com/hook",
      events: ["object.created"],
      active: true,
    });

    store.load(relay1);
    store.save(relay1);

    const relay2 = buildRelay([webhookModule()]);
    store.load(relay2);

    const webhooks2 = relay2.getCapability<WebhookEmitter>(RELAY_CAPABILITIES.WEBHOOKS)!;
    const list = webhooks2.list();
    expect(list.length).toBe(1);
    expect(list[0]!.url).toBe("https://example.com/hook");
    expect(list[0]!.events).toEqual(["object.created"]);
    expect(list[0]!.active).toBe(true);
  });

  it("round-trips templates via save/load", () => {
    const dataDir = path.join(tmpDir, "templates");
    const store = createFileStore({ dataDir });

    const relay1 = buildRelay([portalTemplateModule()]);
    const templates1 = relay1.getCapability<PortalTemplateRegistry>(RELAY_CAPABILITIES.TEMPLATES)!;
    templates1.register({
      name: "Blog Template",
      description: "A simple blog layout",
      css: "body { margin: 0; }",
      headerHtml: "<h1>Blog</h1>",
      footerHtml: "<footer>End</footer>",
      objectCardHtml: "<div>{{title}}</div>",
    });

    store.load(relay1);
    store.save(relay1);

    const relay2 = buildRelay([portalTemplateModule()]);
    store.load(relay2);

    const templates2 = relay2.getCapability<PortalTemplateRegistry>(RELAY_CAPABILITIES.TEMPLATES)!;
    const list = templates2.list();
    expect(list.length).toBe(1);
    expect(list[0]!.name).toBe("Blog Template");
    expect(list[0]!.css).toBe("body { margin: 0; }");
  });

  it("returns empty state when file does not exist", () => {
    const dataDir = path.join(tmpDir, "missing");
    const store = createFileStore({ dataDir });

    // Load into an empty relay — no crash, no data
    const relay = buildRelay([sovereignPortalModule()]);
    store.load(relay);

    const portals = relay.getCapability<PortalRegistry>(RELAY_CAPABILITIES.PORTALS)!;
    expect(portals.list()).toEqual([]);
  });

  it("returns empty state when JSON is corrupted", () => {
    const dataDir = path.join(tmpDir, "corrupt");
    fs.mkdirSync(dataDir, { recursive: true });
    fs.writeFileSync(path.join(dataDir, "relay-state.json"), "NOT VALID JSON{{{", "utf-8");

    const store = createFileStore({ dataDir });
    const relay = buildRelay([sovereignPortalModule()]);
    store.load(relay);

    const portals = relay.getCapability<PortalRegistry>(RELAY_CAPABILITIES.PORTALS)!;
    expect(portals.list()).toEqual([]);
  });

  it("round-trips federation peers via save/load", () => {
    const dataDir = path.join(tmpDir, "federation");
    const store = createFileStore({ dataDir });

    const relay1 = buildRelay([federationModule()]);
    const fed1 = relay1.getCapability<FederationRegistry>(RELAY_CAPABILITIES.FEDERATION)!;
    fed1.announce("did:key:z6MkPeer1", "https://peer1.example.com");

    store.load(relay1);
    store.save(relay1);

    const relay2 = buildRelay([federationModule()]);
    store.load(relay2);

    const fed2 = relay2.getCapability<FederationRegistry>(RELAY_CAPABILITIES.FEDERATION)!;
    const peers = fed2.getPeers();
    expect(peers.length).toBe(1);
    expect(peers[0]!.relayDid).toBe("did:key:z6MkPeer1");
    expect(peers[0]!.url).toBe("https://peer1.example.com");
  });

  it("round-trips flagged hashes via save/load", () => {
    const dataDir = path.join(tmpDir, "trust");
    const store = createFileStore({ dataDir });

    const relay1 = buildRelay([peerTrustModule()]);
    const trust1 = relay1.getCapability<PeerTrustGraph>(RELAY_CAPABILITIES.TRUST)!;
    trust1.flagContent("sha256-abc123", "csam", "did:key:z6MkReporter");

    store.load(relay1);
    store.save(relay1);

    const relay2 = buildRelay([peerTrustModule()]);
    store.load(relay2);

    const trust2 = relay2.getCapability<PeerTrustGraph>(RELAY_CAPABILITIES.TRUST)!;
    const flagged = [...trust2.flaggedContent()];
    expect(flagged.length).toBe(1);
    expect(flagged[0]!.hash).toBe("sha256-abc123");
    expect(flagged[0]!.category).toBe("csam");
  });

  it("startAutoSave triggers periodic saves", () => {
    vi.useFakeTimers();
    try {
      const dataDir = path.join(tmpDir, "autosave");
      fs.mkdirSync(dataDir, { recursive: true });
      const store = createFileStore({ dataDir, saveIntervalMs: 100 });
      const relay = buildRelay([sovereignPortalModule()]);

      store.startAutoSave(relay);

      // No save yet
      expect(fs.existsSync(path.join(dataDir, "relay-state.json"))).toBe(false);

      vi.advanceTimersByTime(100);
      expect(fs.existsSync(path.join(dataDir, "relay-state.json"))).toBe(true);

      // Mutate and advance again to confirm periodic saving
      const portals = relay.getCapability<PortalRegistry>(RELAY_CAPABILITIES.PORTALS)!;
      portals.register({
        name: "Auto Portal",
        level: 1,
        collectionId: "col-auto",
        basePath: "/auto",
        isPublic: false,
      });

      vi.advanceTimersByTime(100);
      const content = fs.readFileSync(path.join(dataDir, "relay-state.json"), "utf-8");
      const state = JSON.parse(content) as { portals: Array<{ name: string }> };
      expect(state.portals.length).toBe(1);
      expect(state.portals[0]!.name).toBe("Auto Portal");

      store.dispose();
    } finally {
      vi.useRealTimers();
    }
  });

  it("dispose stops auto-save timer", () => {
    vi.useFakeTimers();
    try {
      const dataDir = path.join(tmpDir, "dispose");
      fs.mkdirSync(dataDir, { recursive: true });
      const store = createFileStore({ dataDir, saveIntervalMs: 100 });
      const relay = buildRelay([]);

      store.startAutoSave(relay);
      store.dispose();

      // Advancing past the interval should NOT create the file
      vi.advanceTimersByTime(200);
      expect(fs.existsSync(path.join(dataDir, "relay-state.json"))).toBe(false);
    } finally {
      vi.useRealTimers();
    }
  });
});
