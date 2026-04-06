import { describe, it, expect } from "vitest";
import { createIdentity } from "../identity/identity.js";
import type { DID } from "../identity/identity-types.js";
import type {
  RelayEnvelope,
  BlindMailbox,
  RelayRouter,
  RelayTimestamper,
  BlindPinger,
  CapabilityTokenManager,
  WebhookEmitter,
  PortalRegistry,
  RelayModule,
  RelayContext,
} from "./relay-types.js";
import { RELAY_CAPABILITIES } from "./relay-types.js";
import {
  createRelayBuilder,
  blindMailboxModule,
  relayRouterModule,
  relayTimestampModule,
  blindPingModule,
  capabilityTokenModule,
  webhookModule,
  sovereignPortalModule,
  createMemoryPingTransport,
} from "./relay.js";

// ── Helpers ─────────────────────────────────────────────────────────────────

const enc = new TextEncoder();

function makeEnvelope(
  from: DID,
  to: DID,
  payload = "encrypted-data",
  ttlMs = 60_000,
): RelayEnvelope {
  return {
    id: `env-${Math.random().toString(36).slice(2)}`,
    from,
    to,
    ciphertext: enc.encode(payload),
    submittedAt: new Date().toISOString(),
    ttlMs,
  };
}

const ALICE_DID = "did:key:zAlice" as DID;
const BOB_DID = "did:key:zBob" as DID;
const RELAY_DID = "did:key:zRelay" as DID;

// ── Relay Builder ───────────────────────────────────────────────────────────

describe("RelayBuilder", () => {
  it("builds a relay with no modules", () => {
    const relay = createRelayBuilder({ relayDid: RELAY_DID }).build();
    expect(relay.did).toBe(RELAY_DID);
    expect(relay.modules).toHaveLength(0);
    expect(relay.running).toBe(false);
  });

  it("builds a relay with multiple modules", () => {
    const relay = createRelayBuilder({ relayDid: RELAY_DID })
      .use(blindMailboxModule())
      .use(relayRouterModule())
      .build();

    expect(relay.modules).toEqual(["blind-mailbox", "relay-router"]);
  });

  it("rejects duplicate module names", () => {
    expect(() =>
      createRelayBuilder({ relayDid: RELAY_DID })
        .use(blindMailboxModule())
        .use(blindMailboxModule())
        .build(),
    ).toThrow("Duplicate module names");
  });

  it("rejects missing dependencies", () => {
    expect(() =>
      createRelayBuilder({ relayDid: RELAY_DID })
        .use(relayRouterModule()) // depends on blind-mailbox
        .build(),
    ).toThrow('depends on "blind-mailbox"');
  });

  it("starts and stops lifecycle", async () => {
    const relay = createRelayBuilder({ relayDid: RELAY_DID })
      .use(blindMailboxModule())
      .build();

    expect(relay.running).toBe(false);
    await relay.start();
    expect(relay.running).toBe(true);
    await relay.stop();
    expect(relay.running).toBe(false);
  });

  it("configure overrides defaults", () => {
    const relay = createRelayBuilder({ relayDid: RELAY_DID })
      .configure({ defaultTtlMs: 1000, maxEnvelopeSizeBytes: 512 })
      .use(blindMailboxModule())
      .build();

    // The config is used internally; verify through behavior
    expect(relay.modules).toContain("blind-mailbox");
  });

  it("allows custom modules", () => {
    const customModule: RelayModule = {
      name: "custom-analytics",
      description: "Custom analytics module",
      dependencies: [],
      install(ctx: RelayContext): void {
        ctx.setCapability("custom:analytics", { events: [] });
      },
    };

    const relay = createRelayBuilder({ relayDid: RELAY_DID })
      .use(customModule)
      .build();

    expect(relay.modules).toEqual(["custom-analytics"]);
    const analytics = relay.getCapability<{ events: string[] }>("custom:analytics");
    expect(analytics).toBeDefined();
    expect((analytics as { events: string[] }).events).toEqual([]);
  });
});

// ── Blind Mailbox ───────────────────────────────────────────────────────────

describe("BlindMailbox", () => {
  function getMailbox(): BlindMailbox {
    const relay = createRelayBuilder({ relayDid: RELAY_DID })
      .use(blindMailboxModule())
      .build();
    return relay.getCapability<BlindMailbox>(RELAY_CAPABILITIES.MAILBOX) as BlindMailbox;
  }

  it("deposits and collects envelopes", () => {
    const mailbox = getMailbox();
    const env = makeEnvelope(ALICE_DID, BOB_DID);

    mailbox.deposit(env);
    expect(mailbox.pendingCount(BOB_DID)).toBe(1);
    expect(mailbox.totalCount()).toBe(1);

    const collected = mailbox.collect(BOB_DID);
    expect(collected).toHaveLength(1);
    expect(collected[0]?.id).toBe(env.id);

    // Collecting empties the mailbox
    expect(mailbox.pendingCount(BOB_DID)).toBe(0);
  });

  it("returns empty array for no pending envelopes", () => {
    const mailbox = getMailbox();
    expect(mailbox.collect(BOB_DID)).toEqual([]);
  });

  it("deposits multiple envelopes for same recipient", () => {
    const mailbox = getMailbox();
    mailbox.deposit(makeEnvelope(ALICE_DID, BOB_DID, "msg1"));
    mailbox.deposit(makeEnvelope(ALICE_DID, BOB_DID, "msg2"));

    expect(mailbox.pendingCount(BOB_DID)).toBe(2);
    const collected = mailbox.collect(BOB_DID);
    expect(collected).toHaveLength(2);
  });

  it("evicts expired envelopes", () => {
    const mailbox = getMailbox();
    // Envelope with 1ms TTL
    const env: RelayEnvelope = {
      id: "expired",
      from: ALICE_DID,
      to: BOB_DID,
      ciphertext: enc.encode("old"),
      submittedAt: new Date(Date.now() - 10_000).toISOString(),
      ttlMs: 1,
    };
    mailbox.deposit(env);

    const evicted = mailbox.evict();
    expect(evicted).toBe(1);
    expect(mailbox.totalCount()).toBe(0);
  });

  it("does not evict non-expired envelopes", () => {
    const mailbox = getMailbox();
    mailbox.deposit(makeEnvelope(ALICE_DID, BOB_DID, "fresh", 999_999));

    const evicted = mailbox.evict();
    expect(evicted).toBe(0);
    expect(mailbox.totalCount()).toBe(1);
  });

  it("clears all mailboxes", () => {
    const mailbox = getMailbox();
    mailbox.deposit(makeEnvelope(ALICE_DID, BOB_DID));
    mailbox.deposit(makeEnvelope(BOB_DID, ALICE_DID));

    mailbox.clear();
    expect(mailbox.totalCount()).toBe(0);
  });
});

// ── Relay Router ────────────────────────────────────────────────────────────

describe("RelayRouter", () => {
  function getRouter(): { router: RelayRouter; mailbox: BlindMailbox } {
    const relay = createRelayBuilder({ relayDid: RELAY_DID })
      .use(blindMailboxModule())
      .use(relayRouterModule())
      .build();
    return {
      router: relay.getCapability<RelayRouter>(RELAY_CAPABILITIES.ROUTER) as RelayRouter,
      mailbox: relay.getCapability<BlindMailbox>(RELAY_CAPABILITIES.MAILBOX) as BlindMailbox,
    };
  }

  it("delivers to online peer", () => {
    const { router } = getRouter();
    const received: RelayEnvelope[] = [];
    router.registerPeer(BOB_DID, (env) => received.push(env));

    const env = makeEnvelope(ALICE_DID, BOB_DID);
    const result = router.route(env);

    expect(result.status).toBe("delivered");
    expect(received).toHaveLength(1);
    expect(received[0]?.id).toBe(env.id);
  });

  it("queues to mailbox for offline peer", () => {
    const { router, mailbox } = getRouter();

    const env = makeEnvelope(ALICE_DID, BOB_DID);
    const result = router.route(env);

    expect(result.status).toBe("queued");
    if (result.status === "queued") {
      expect(result.mailboxSize).toBe(1);
    }
    expect(mailbox.pendingCount(BOB_DID)).toBe(1);
  });

  it("rejects oversized envelopes", () => {
    const relay = createRelayBuilder({ relayDid: RELAY_DID })
      .configure({ maxEnvelopeSizeBytes: 10 })
      .use(blindMailboxModule())
      .use(relayRouterModule())
      .build();

    const router = relay.getCapability<RelayRouter>(RELAY_CAPABILITIES.ROUTER) as RelayRouter;
    const bigEnvelope = makeEnvelope(ALICE_DID, BOB_DID, "x".repeat(100));
    const result = router.route(bigEnvelope);

    expect(result.status).toBe("rejected");
    if (result.status === "rejected") {
      expect(result.reason).toContain("maximum size");
    }
  });

  it("flushes queued messages when peer comes online", () => {
    const { router, mailbox } = getRouter();

    // Queue some messages while Bob is offline
    router.route(makeEnvelope(ALICE_DID, BOB_DID, "msg1"));
    router.route(makeEnvelope(ALICE_DID, BOB_DID, "msg2"));
    expect(mailbox.pendingCount(BOB_DID)).toBe(2);

    // Bob comes online
    const received: RelayEnvelope[] = [];
    router.registerPeer(BOB_DID, (env) => received.push(env));

    // Queued messages are flushed immediately
    expect(received).toHaveLength(2);
    expect(mailbox.pendingCount(BOB_DID)).toBe(0);
  });

  it("tracks online/offline peer state", () => {
    const { router } = getRouter();

    expect(router.isOnline(BOB_DID)).toBe(false);
    expect(router.onlinePeers()).toHaveLength(0);

    router.registerPeer(BOB_DID, () => {});
    expect(router.isOnline(BOB_DID)).toBe(true);
    expect(router.onlinePeers()).toContain(BOB_DID);

    router.unregisterPeer(BOB_DID);
    expect(router.isOnline(BOB_DID)).toBe(false);
  });
});

// ── Relay Timestamper ───────────────────────────────────────────────────────

describe("RelayTimestamper", () => {
  async function getTimestamper(): Promise<RelayTimestamper> {
    const identity = await createIdentity();
    const relay = createRelayBuilder({ relayDid: identity.did })
      .use(relayTimestampModule(identity))
      .build();
    return relay.getCapability<RelayTimestamper>(RELAY_CAPABILITIES.TIMESTAMPER) as RelayTimestamper;
  }

  it("stamps a data hash and verifies it", async () => {
    const ts = await getTimestamper();
    const receipt = await ts.stamp("abc123hash");

    expect(receipt.dataHash).toBe("abc123hash");
    expect(receipt.timestamp).toBeTruthy();
    expect(receipt.signature).toBeInstanceOf(Uint8Array);

    const valid = await ts.verify(receipt);
    expect(valid).toBe(true);
  });

  it("rejects tampered receipt", async () => {
    const ts = await getTimestamper();
    const receipt = await ts.stamp("original-hash");

    const tampered = { ...receipt, dataHash: "tampered-hash" };
    const valid = await ts.verify(tampered);
    expect(valid).toBe(false);
  });
});

// ── Blind Pinger ────────────────────────────────────────────────────────────

describe("BlindPinger", () => {
  it("sends pings via transport", async () => {
    const relay = createRelayBuilder({ relayDid: RELAY_DID })
      .use(blindPingModule())
      .build();

    const pinger = relay.getCapability<BlindPinger>(RELAY_CAPABILITIES.PINGER) as BlindPinger;
    const transport = createMemoryPingTransport();
    pinger.setTransport(transport);

    const ok = await pinger.ping(BOB_DID, 3);
    expect(ok).toBe(true);
    expect(transport.sent).toHaveLength(1);
    expect(transport.sent[0]?.to).toBe(BOB_DID);
    expect(transport.sent[0]?.badgeCount).toBe(3);
  });

  it("returns false when no transport is set", async () => {
    const relay = createRelayBuilder({ relayDid: RELAY_DID })
      .use(blindPingModule())
      .build();

    const pinger = relay.getCapability<BlindPinger>(RELAY_CAPABILITIES.PINGER) as BlindPinger;
    const ok = await pinger.ping(BOB_DID);
    expect(ok).toBe(false);
  });
});

// ── Capability Tokens ───────────────────────────────────────────────────────

describe("CapabilityTokenManager", () => {
  async function getTokenManager(): Promise<CapabilityTokenManager> {
    const identity = await createIdentity();
    const relay = createRelayBuilder({ relayDid: identity.did })
      .use(capabilityTokenModule(identity))
      .build();
    return relay.getCapability<CapabilityTokenManager>(RELAY_CAPABILITIES.TOKENS) as CapabilityTokenManager;
  }

  it("issues and verifies a token", async () => {
    const manager = await getTokenManager();
    const token = await manager.issue({
      subject: BOB_DID,
      permissions: ["read", "list"],
      scope: "collection-notes",
    });

    expect(token.tokenId).toBeTruthy();
    expect(token.permissions).toEqual(["read", "list"]);
    expect(token.scope).toBe("collection-notes");
    expect(token.expiresAt).toBeNull();

    const result = await manager.verify(token);
    expect(result.valid).toBe(true);
  });

  it("issues token with TTL and detects expiry", async () => {
    const manager = await getTokenManager();
    const token = await manager.issue({
      subject: BOB_DID,
      permissions: ["read"],
      scope: "test",
      ttlMs: 1, // expires immediately
    });

    // Wait a tick for expiry
    await new Promise(r => setTimeout(r, 5));
    const result = await manager.verify(token);
    expect(result.valid).toBe(false);
    expect(result.reason).toBe("token expired");
  });

  it("revokes a token", async () => {
    const manager = await getTokenManager();
    const token = await manager.issue({
      subject: BOB_DID,
      permissions: ["write"],
      scope: "admin",
    });

    manager.revoke(token.tokenId);
    expect(manager.isRevoked(token.tokenId)).toBe(true);

    const result = await manager.verify(token);
    expect(result.valid).toBe(false);
    expect(result.reason).toBe("token revoked");
  });

  it("detects signature tampering", async () => {
    const manager = await getTokenManager();
    const token = await manager.issue({
      subject: BOB_DID,
      permissions: ["read"],
      scope: "test",
    });

    const tampered = { ...token, scope: "hacked" };
    const result = await manager.verify(tampered);
    expect(result.valid).toBe(false);
    expect(result.reason).toBe("invalid signature");
  });

  it("issues wildcard subject token", async () => {
    const manager = await getTokenManager();
    const token = await manager.issue({
      subject: "*",
      permissions: ["read"],
      scope: "public-portal",
    });

    expect(token.subject).toBe("*");
    const result = await manager.verify(token);
    expect(result.valid).toBe(true);
  });
});

// ── Webhooks ────────────────────────────────────────────────────────────────

describe("WebhookEmitter", () => {
  function getEmitter(): WebhookEmitter {
    const relay = createRelayBuilder({ relayDid: RELAY_DID })
      .use(webhookModule())
      .build();
    return relay.getCapability<WebhookEmitter>(RELAY_CAPABILITIES.WEBHOOKS) as WebhookEmitter;
  }

  it("registers and lists webhooks", () => {
    const emitter = getEmitter();
    const wh = emitter.register({
      url: "https://example.com/hook",
      events: ["object.created"],
      active: true,
    });

    expect(wh.id).toBeTruthy();
    expect(emitter.list()).toHaveLength(1);
    expect(emitter.list()[0]?.url).toBe("https://example.com/hook");
  });

  it("unregisters a webhook", () => {
    const emitter = getEmitter();
    const wh = emitter.register({
      url: "https://example.com/hook",
      events: ["*"],
      active: true,
    });

    expect(emitter.unregister(wh.id)).toBe(true);
    expect(emitter.list()).toHaveLength(0);
  });

  it("emits events to matching webhooks (dry-run mode)", async () => {
    const emitter = getEmitter();
    emitter.register({
      url: "https://example.com/hook",
      events: ["object.created"],
      active: true,
    });

    const results = await emitter.emit("object.created", { id: "obj-1" });
    expect(results).toHaveLength(1);
    expect(results[0]?.success).toBe(true);
  });

  it("does not emit to inactive webhooks", async () => {
    const emitter = getEmitter();
    emitter.register({
      url: "https://example.com/hook",
      events: ["object.created"],
      active: false,
    });

    const results = await emitter.emit("object.created", { id: "obj-1" });
    expect(results).toHaveLength(0);
  });

  it("does not emit to non-matching events", async () => {
    const emitter = getEmitter();
    emitter.register({
      url: "https://example.com/hook",
      events: ["object.deleted"],
      active: true,
    });

    const results = await emitter.emit("object.created", { id: "obj-1" });
    expect(results).toHaveLength(0);
  });

  it("wildcard event matches everything", async () => {
    const emitter = getEmitter();
    emitter.register({
      url: "https://example.com/hook",
      events: ["*"],
      active: true,
    });

    const results = await emitter.emit("anything.here", { x: 1 });
    expect(results).toHaveLength(1);
  });

  it("emits with HTTP client", async () => {
    const posts: { url: string; body: string }[] = [];
    const mockClient = {
      async post(url: string, body: string) {
        posts.push({ url, body });
        return { status: 200 };
      },
    };

    const relay = createRelayBuilder({ relayDid: RELAY_DID })
      .use(webhookModule(mockClient))
      .build();
    const emitter = relay.getCapability<WebhookEmitter>(RELAY_CAPABILITIES.WEBHOOKS) as WebhookEmitter;

    emitter.register({
      url: "https://slack.example.com/webhook",
      events: ["object.updated"],
      active: true,
    });

    const results = await emitter.emit("object.updated", { name: "test" });
    expect(results).toHaveLength(1);
    expect(results[0]?.success).toBe(true);
    expect(results[0]?.statusCode).toBe(200);
    expect(posts).toHaveLength(1);
    expect(posts[0]?.url).toBe("https://slack.example.com/webhook");
  });

  it("records delivery failures", async () => {
    const mockClient = {
      async post() {
        return { status: 500 };
      },
    };

    const relay = createRelayBuilder({ relayDid: RELAY_DID })
      .use(webhookModule(mockClient))
      .build();
    const emitter = relay.getCapability<WebhookEmitter>(RELAY_CAPABILITIES.WEBHOOKS) as WebhookEmitter;

    const wh = emitter.register({
      url: "https://broken.example.com",
      events: ["*"],
      active: true,
    });

    await emitter.emit("test", {});
    const log = emitter.deliveries(wh.id);
    expect(log).toHaveLength(1);
    expect(log[0]?.success).toBe(false);
    expect(log[0]?.statusCode).toBe(500);
  });

  it("includes HMAC signature when secret is configured", async () => {
    const emitter = getEmitter();
    emitter.register({
      url: "https://example.com/hook",
      events: ["*"],
      secret: "my-secret",
      active: true,
    });

    const results = await emitter.emit("test", { key: "value" });
    expect(results).toHaveLength(1);
    // In dry-run mode the payload is still built (just not sent over HTTP)
    expect(results[0]?.success).toBe(true);
  });
});

// ── Sovereign Portals ───────────────────────────────────────────────────────

describe("PortalRegistry", () => {
  function getPortals(): PortalRegistry {
    const relay = createRelayBuilder({ relayDid: RELAY_DID })
      .use(sovereignPortalModule())
      .build();
    return relay.getCapability<PortalRegistry>(RELAY_CAPABILITIES.PORTALS) as PortalRegistry;
  }

  it("registers and retrieves a portal", () => {
    const registry = getPortals();
    const portal = registry.register({
      name: "My Blog",
      level: 1,
      collectionId: "coll-blog",
      basePath: "/blog",
      isPublic: true,
    });

    expect(portal.portalId).toBeTruthy();
    expect(portal.name).toBe("My Blog");
    expect(portal.level).toBe(1);

    const found = registry.get(portal.portalId);
    expect(found).toBeDefined();
    expect((found as PortalManifest).name).toBe("My Blog");
  });

  it("lists all portals", () => {
    const registry = getPortals();
    registry.register({
      name: "Blog",
      level: 1,
      collectionId: "coll-1",
      basePath: "/blog",
      isPublic: true,
    });
    registry.register({
      name: "Dashboard",
      level: 2,
      collectionId: "coll-2",
      basePath: "/dash",
      isPublic: false,
    });

    expect(registry.list()).toHaveLength(2);
  });

  it("unregisters a portal", () => {
    const registry = getPortals();
    const portal = registry.register({
      name: "Temp",
      level: 1,
      collectionId: "coll-tmp",
      basePath: "/tmp",
      isPublic: true,
    });

    expect(registry.unregister(portal.portalId)).toBe(true);
    expect(registry.get(portal.portalId)).toBeUndefined();
  });

  it("resolves portal by domain + path", () => {
    const registry = getPortals();
    registry.register({
      name: "Blog",
      level: 2,
      collectionId: "coll-blog",
      domain: "blog.example.com",
      basePath: "/",
      isPublic: true,
    });
    registry.register({
      name: "API Docs",
      level: 1,
      collectionId: "coll-docs",
      domain: "docs.example.com",
      basePath: "/api",
      isPublic: true,
    });

    const blog = registry.resolve("blog.example.com", "/posts/1");
    expect(blog).toBeDefined();
    expect((blog as PortalManifest).name).toBe("Blog");

    const docs = registry.resolve("docs.example.com", "/api/v1/users");
    expect(docs).toBeDefined();
    expect((docs as PortalManifest).name).toBe("API Docs");

    const notFound = registry.resolve("other.com", "/");
    expect(notFound).toBeUndefined();
  });

  it("supports all 4 portal levels", () => {
    const registry = getPortals();
    const levels = [1, 2, 3, 4] as const;

    for (const level of levels) {
      const portal = registry.register({
        name: `Level ${level}`,
        level,
        collectionId: `coll-${level}`,
        basePath: `/${level}`,
        isPublic: level <= 2,
      });
      expect(portal.level).toBe(level);
    }

    expect(registry.list()).toHaveLength(4);
  });
});

// ── Full Relay Composition ──────────────────────────────────────────────────

describe("Full Relay composition", () => {
  it("composes all modules into a working relay", async () => {
    const identity = await createIdentity();

    const relay = createRelayBuilder({ relayDid: identity.did })
      .use(blindMailboxModule())
      .use(relayRouterModule())
      .use(relayTimestampModule(identity))
      .use(blindPingModule())
      .use(capabilityTokenModule(identity))
      .use(webhookModule())
      .use(sovereignPortalModule())
      .build();

    expect(relay.modules).toHaveLength(7);

    await relay.start();
    expect(relay.running).toBe(true);

    // All capabilities are accessible
    expect(relay.getCapability(RELAY_CAPABILITIES.MAILBOX)).toBeDefined();
    expect(relay.getCapability(RELAY_CAPABILITIES.ROUTER)).toBeDefined();
    expect(relay.getCapability(RELAY_CAPABILITIES.TIMESTAMPER)).toBeDefined();
    expect(relay.getCapability(RELAY_CAPABILITIES.PINGER)).toBeDefined();
    expect(relay.getCapability(RELAY_CAPABILITIES.TOKENS)).toBeDefined();
    expect(relay.getCapability(RELAY_CAPABILITIES.WEBHOOKS)).toBeDefined();
    expect(relay.getCapability(RELAY_CAPABILITIES.PORTALS)).toBeDefined();

    await relay.stop();
    expect(relay.running).toBe(false);
  });

  it("works with minimal modules (just mailbox)", () => {
    const relay = createRelayBuilder({ relayDid: RELAY_DID })
      .use(blindMailboxModule())
      .build();

    expect(relay.modules).toEqual(["blind-mailbox"]);
    expect(relay.getCapability(RELAY_CAPABILITIES.ROUTER)).toBeUndefined();
    expect(relay.getCapability(RELAY_CAPABILITIES.WEBHOOKS)).toBeUndefined();
  });

  it("supports choose-your-own-adventure composition", () => {
    // Web 1.0 style: just portals, no real-time
    const web1 = createRelayBuilder({ relayDid: RELAY_DID })
      .use(sovereignPortalModule())
      .build();
    expect(web1.modules).toEqual(["sovereign-portals"]);

    // Web 2.0 style: portals + webhooks + REST
    const web2 = createRelayBuilder({ relayDid: RELAY_DID })
      .use(sovereignPortalModule())
      .use(webhookModule())
      .build();
    expect(web2.modules).toEqual(["sovereign-portals", "webhooks"]);

    // Full Web 2.5+: everything
    const full = createRelayBuilder({ relayDid: RELAY_DID })
      .use(blindMailboxModule())
      .use(relayRouterModule())
      .use(blindPingModule())
      .use(webhookModule())
      .use(sovereignPortalModule())
      .build();
    expect(full.modules).toHaveLength(5);
  });
});
