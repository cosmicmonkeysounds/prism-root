import { describe, it, expect, vi, beforeEach } from "vitest";
import { createRelayManager } from "./relay-manager.js";
import type { RelayManager, RelayHttpClient } from "./relay-manager.js";
import type { RelayClient, RelayClientEvents, RelayClientOptions } from "@prism/core/relay";
import type { DID } from "@prism/core/identity";

// ── Mock HTTP client ──────────────────────────────────────────────────────

function createMockHttpClient(): RelayHttpClient & { responses: Map<string, { status: number; body: unknown }> } {
  const responses = new Map<string, { status: number; body: unknown }>();

  return {
    responses,
    fetch: vi.fn(async (url: string, init?: RequestInit) => {
      const method = init?.method ?? "GET";
      const key = `${method} ${url}`;

      // Check for exact match first, then pattern match
      const match = responses.get(key) ?? responses.get(url);
      if (match) {
        return {
          ok: match.status >= 200 && match.status < 300,
          status: match.status,
          text: async () => JSON.stringify(match.body),
          json: async () => match.body,
        } as Response;
      }

      return {
        ok: true,
        status: 200,
        text: async () => "{}",
        json: async () => ({}),
      } as Response;
    }),
  };
}

// ── Mock WebSocket client ─────────────────────────────────────────────────

type EventHandler<K extends keyof RelayClientEvents> = (data: RelayClientEvents[K]) => void;

function createMockWsClientFactory() {
  const clients: MockRelayClient[] = [];

  function factory(options: RelayClientOptions): RelayClient {
    const handlers = new Map<string, Set<(...args: unknown[]) => void>>();
    let state: RelayClient["state"] = "disconnected";

    const client: MockRelayClient = {
      get state() { return state; },
      get relayDid() { return "did:key:mock-relay" as DID; },
      get modules() { return ["blind-mailbox", "sovereign-portals"]; },
      options,

      connect: vi.fn(async () => {
        state = "connected";
        const set = handlers.get("connected");
        if (set) {
          for (const h of set) h({ relayDid: "did:key:mock-relay" as DID, modules: ["blind-mailbox", "sovereign-portals"] });
        }
      }),
      close: vi.fn(() => {
        state = "disconnected";
      }),
      send: vi.fn(async () => ({ status: "delivered" as const, recipientDid: "did:key:test" as `did:${string}:${string}` })),
      syncRequest: vi.fn(async () => new Uint8Array()),
      syncUpdate: vi.fn(),
      on: vi.fn(<K extends keyof RelayClientEvents>(event: K, handler: EventHandler<K>) => {
        let set = handlers.get(event);
        if (!set) {
          set = new Set();
          handlers.set(event, set);
        }
        set.add(handler as (...args: unknown[]) => void);
      }),
      off: vi.fn(<K extends keyof RelayClientEvents>(event: K, handler: EventHandler<K>) => {
        handlers.get(event)?.delete(handler as (...args: unknown[]) => void);
      }),

      // Test helpers
      _emit<K extends keyof RelayClientEvents>(event: K, data: RelayClientEvents[K]): void {
        const set = handlers.get(event);
        if (set) {
          for (const h of set) h(data);
        }
      },
      _setState(s: RelayClient["state"]) { state = s; },
    };

    clients.push(client);
    return client;
  }

  return { factory, clients };
}

interface MockRelayClient extends RelayClient {
  options: RelayClientOptions;
  _emit<K extends keyof RelayClientEvents>(event: K, data: RelayClientEvents[K]): void;
  _setState(s: RelayClient["state"]): void;
}

// ── Tests ─────────────────────────────────────────────────────────────────

describe("RelayManager", () => {
  let manager: RelayManager;
  let http: ReturnType<typeof createMockHttpClient>;
  let ws: ReturnType<typeof createMockWsClientFactory>;

  beforeEach(() => {
    http = createMockHttpClient();
    ws = createMockWsClientFactory();
    manager = createRelayManager({
      httpClient: http,
      createWsClient: ws.factory,
    });
  });

  // ── Relay CRUD ────────────────────────────────────────────────────────

  describe("relay CRUD", () => {
    it("adds a relay", () => {
      const entry = manager.addRelay("Test", "http://localhost:4444");
      expect(entry.name).toBe("Test");
      expect(entry.url).toBe("http://localhost:4444");
      expect(entry.status).toBe("disconnected");
      expect(manager.listRelays()).toHaveLength(1);
    });

    it("strips trailing slashes from URL", () => {
      const entry = manager.addRelay("Test", "http://localhost:4444///");
      expect(entry.url).toBe("http://localhost:4444");
    });

    it("removes a relay", () => {
      const entry = manager.addRelay("Test", "http://localhost:4444");
      expect(manager.removeRelay(entry.id)).toBe(true);
      expect(manager.listRelays()).toHaveLength(0);
    });

    it("returns false for unknown relay removal", () => {
      expect(manager.removeRelay("unknown")).toBe(false);
    });

    it("gets a relay by ID", () => {
      const entry = manager.addRelay("Test", "http://localhost:4444");
      const found = manager.getRelay(entry.id);
      expect(found).toBeDefined();
      expect(found?.name).toBe("Test");
    });

    it("returns undefined for unknown relay", () => {
      expect(manager.getRelay("unknown")).toBeUndefined();
    });
  });

  // ── Subscriptions ─────────────────────────────────────────────────────

  describe("subscriptions", () => {
    it("notifies on add/remove", () => {
      const listener = vi.fn();
      manager.subscribe(listener);

      const entry = manager.addRelay("Test", "http://localhost:4444");
      expect(listener).toHaveBeenCalled();

      const countAfterAdd = listener.mock.calls.length;
      manager.removeRelay(entry.id);
      // Remove triggers disconnect (which updates entry) + delete + notify
      expect(listener.mock.calls.length).toBeGreaterThan(countAfterAdd);
    });

    it("unsubscribes", () => {
      const listener = vi.fn();
      const unsub = manager.subscribe(listener);
      unsub();

      manager.addRelay("Test", "http://localhost:4444");
      expect(listener).not.toHaveBeenCalled();
    });
  });

  // ── WebSocket connection ──────────────────────────────────────────────

  describe("connect/disconnect", () => {
    it("connects to a relay via WebSocket", async () => {
      const entry = manager.addRelay("Test", "http://localhost:4444");
      const identity = { did: "did:key:test" } as unknown as Parameters<typeof manager.connect>[1];

      await manager.connect(entry.id, identity);

      expect(ws.clients).toHaveLength(1);
      expect(ws.clients[0]?.options.url).toBe("ws://localhost:4444/ws/relay");

      const updated = manager.getRelay(entry.id);
      expect(updated?.status).toBe("connected");
      expect(updated?.relayDid).toBe("did:key:mock-relay");
      expect(updated?.modules).toContain("blind-mailbox");
    });

    it("uses wss for https URLs", async () => {
      const entry = manager.addRelay("Prod", "https://relay.example.com");
      const identity = { did: "did:key:test" } as unknown as Parameters<typeof manager.connect>[1];

      await manager.connect(entry.id, identity);

      expect(ws.clients[0]?.options.url).toBe("wss://relay.example.com/ws/relay");
    });

    it("disconnects from a relay", async () => {
      const entry = manager.addRelay("Test", "http://localhost:4444");
      const identity = { did: "did:key:test" } as unknown as Parameters<typeof manager.connect>[1];

      await manager.connect(entry.id, identity);
      manager.disconnect(entry.id);

      const updated = manager.getRelay(entry.id);
      expect(updated?.status).toBe("disconnected");
      expect(ws.clients[0]?.close).toHaveBeenCalled();
    });

    it("throws on connect to unknown relay", async () => {
      const identity = { did: "did:key:test" } as unknown as Parameters<typeof manager.connect>[1];
      await expect(manager.connect("unknown", identity)).rejects.toThrow("Unknown relay");
    });

    it("handles connection error", async () => {
      manager.addRelay("Bad", "http://localhost:9999");
      const identity = { did: "did:key:test" } as unknown as Parameters<typeof manager.connect>[1];

      // Override the mock to fail
      const badFactory = createMockWsClientFactory();
      const badManager = createRelayManager({
        httpClient: http,
        createWsClient: (opts) => {
          const client = badFactory.factory(opts);
          (client.connect as ReturnType<typeof vi.fn>).mockRejectedValue(new Error("Connection refused"));
          return client;
        },
      });

      const badEntry = badManager.addRelay("Bad", "http://localhost:9999");
      await expect(badManager.connect(badEntry.id, identity)).rejects.toThrow("Connection refused");
      expect(badManager.getRelay(badEntry.id)?.status).toBe("error");
    });
  });

  // ── Portal management ─────────────────────────────────────────────────

  describe("portal management", () => {
    it("publishes a portal via HTTP POST", async () => {
      const entry = manager.addRelay("Test", "http://localhost:4444");

      const mockManifest = {
        portalId: "portal-1",
        name: "My Portal",
        level: 1,
        collectionId: "default",
        basePath: "/",
        isPublic: true,
        createdAt: "2026-04-06T00:00:00.000Z",
      };

      http.responses.set("POST http://localhost:4444/api/portals", {
        status: 201,
        body: mockManifest,
      });

      const result = await manager.publishPortal({
        relayId: entry.id,
        collectionId: "default",
        name: "My Portal",
        level: 1,
        basePath: "/",
        isPublic: true,
      });

      expect(result.manifest.portalId).toBe("portal-1");
      expect(result.manifest.name).toBe("My Portal");
      expect(result.viewUrl).toBe("http://localhost:4444/portals/portal-1");
      expect(http.fetch).toHaveBeenCalledWith(
        "http://localhost:4444/api/portals",
        expect.objectContaining({ method: "POST" }),
      );
    });

    it("throws on publish failure", async () => {
      const entry = manager.addRelay("Test", "http://localhost:4444");

      http.responses.set("POST http://localhost:4444/api/portals", {
        status: 500,
        body: "Internal error",
      });

      await expect(
        manager.publishPortal({
          relayId: entry.id,
          collectionId: "default",
          name: "Bad",
          level: 1,
        }),
      ).rejects.toThrow("Failed to publish portal");
    });

    it("unpublishes a portal via HTTP DELETE", async () => {
      const entry = manager.addRelay("Test", "http://localhost:4444");

      http.responses.set("DELETE http://localhost:4444/api/portals/portal-1", {
        status: 204,
        body: null,
      });

      const result = await manager.unpublishPortal(entry.id, "portal-1");
      expect(result).toBe(true);
    });

    it("lists portals from a relay", async () => {
      const entry = manager.addRelay("Test", "http://localhost:4444");

      http.responses.set("http://localhost:4444/api/portals", {
        status: 200,
        body: [
          {
            portalId: "p1",
            name: "Portal One",
            level: 1,
            collectionId: "c1",
            basePath: "/one",
            isPublic: true,
            createdAt: "2026-04-06T00:00:00.000Z",
          },
          {
            portalId: "p2",
            name: "Portal Two",
            level: 2,
            collectionId: "c2",
            basePath: "/two",
            isPublic: true,
            createdAt: "2026-04-06T00:00:00.000Z",
          },
        ],
      });

      const portals = await manager.listPortals(entry.id);
      expect(portals).toHaveLength(2);
      expect(portals[0]?.manifest.name).toBe("Portal One");
      expect(portals[0]?.viewUrl).toBe("http://localhost:4444/portals/p1");
      expect(portals[1]?.manifest.name).toBe("Portal Two");
    });

    it("throws on portal listing for unknown relay", async () => {
      await expect(manager.listPortals("unknown")).rejects.toThrow("Unknown relay");
    });
  });

  // ── Status ────────────────────────────────────────────────────────────

  describe("fetchStatus", () => {
    it("fetches relay status via HTTP", async () => {
      const entry = manager.addRelay("Test", "http://localhost:4444");

      http.responses.set("http://localhost:4444/api/status", {
        status: 200,
        body: {
          did: "did:key:relay-1",
          modules: ["blind-mailbox", "sovereign-portals"],
          uptime: 3600,
          connections: 5,
          mode: "server",
        },
      });

      const status = await manager.fetchStatus(entry.id);
      expect(status.did).toBe("did:key:relay-1");
      expect(status.modules).toContain("sovereign-portals");
      expect(status.connections).toBe(5);
    });

    it("throws on status fetch failure", async () => {
      const entry = manager.addRelay("Test", "http://localhost:4444");
      http.responses.set("http://localhost:4444/api/status", { status: 503, body: "unavailable" });
      await expect(manager.fetchStatus(entry.id)).rejects.toThrow("Failed to fetch status");
    });
  });

  // ── Collections management ────────────────────────────────────────────

  describe("collections management", () => {
    it("lists collections", async () => {
      const entry = manager.addRelay("Test", "http://localhost:4444");
      http.responses.set("http://localhost:4444/api/collections", {
        status: 200,
        body: ["col-1", "col-2", "col-3"],
      });

      const collections = await manager.listCollections(entry.id);
      expect(collections).toEqual(["col-1", "col-2", "col-3"]);
    });

    it("inspects a collection", async () => {
      const entry = manager.addRelay("Test", "http://localhost:4444");
      http.responses.set("http://localhost:4444/api/collections/col-1/snapshot", {
        status: 200,
        body: { snapshot: "abc123" },
      });

      const result = await manager.inspectCollection(entry.id, "col-1");
      expect(result.snapshot).toBe("abc123");
    });

    it("deletes a collection", async () => {
      const entry = manager.addRelay("Test", "http://localhost:4444");
      http.responses.set("DELETE http://localhost:4444/api/collections/col-1", {
        status: 204,
        body: null,
      });

      const result = await manager.deleteCollection(entry.id, "col-1");
      expect(result).toBe(true);
    });

    it("throws for unknown relay on listCollections", async () => {
      await expect(manager.listCollections("unknown")).rejects.toThrow("Unknown relay");
    });
  });

  // ── Webhooks management ──────────────────────────────────────────────

  describe("webhooks management", () => {
    it("lists webhooks", async () => {
      const entry = manager.addRelay("Test", "http://localhost:4444");
      const mockWebhooks = [
        { id: "wh-1", url: "https://example.com/hook", events: ["portal.created"], active: true },
        { id: "wh-2", url: "https://example.com/hook2", events: ["collection.updated"], active: false },
      ];
      http.responses.set("http://localhost:4444/api/webhooks", {
        status: 200,
        body: mockWebhooks,
      });

      const webhooks = await manager.listWebhooks(entry.id);
      expect(webhooks).toHaveLength(2);
      expect(webhooks[0]?.id).toBe("wh-1");
      expect(webhooks[0]?.active).toBe(true);
      expect(webhooks[1]?.events).toContain("collection.updated");
    });

    it("deletes a webhook", async () => {
      const entry = manager.addRelay("Test", "http://localhost:4444");
      http.responses.set("DELETE http://localhost:4444/api/webhooks/wh-1", {
        status: 204,
        body: null,
      });

      const result = await manager.deleteWebhook(entry.id, "wh-1");
      expect(result).toBe(true);
    });
  });

  // ── Federation/peers ─────────────────────────────────────────────────

  describe("federation/peers", () => {
    it("lists peers", async () => {
      const entry = manager.addRelay("Test", "http://localhost:4444");
      const mockPeers = [
        { relayDid: "did:key:peer-1", url: "https://peer1.example.com" },
        { relayDid: "did:key:peer-2", url: "https://peer2.example.com" },
      ];
      http.responses.set("http://localhost:4444/api/federation/peers", {
        status: 200,
        body: mockPeers,
      });

      const peers = await manager.listPeers(entry.id);
      expect(peers).toHaveLength(2);
      expect(peers[0]?.relayDid).toBe("did:key:peer-1");
    });

    it("bans a peer", async () => {
      const entry = manager.addRelay("Test", "http://localhost:4444");
      http.responses.set("POST http://localhost:4444/api/federation/peers/did%3Akey%3Abad-peer/ban", {
        status: 200,
        body: { ok: true },
      });

      const result = await manager.banPeer(entry.id, "did:key:bad-peer");
      expect(result).toBe(true);
    });

    it("unbans a peer", async () => {
      const entry = manager.addRelay("Test", "http://localhost:4444");
      http.responses.set("POST http://localhost:4444/api/federation/peers/did%3Akey%3Abad-peer/unban", {
        status: 200,
        body: { ok: true },
      });

      const result = await manager.unbanPeer(entry.id, "did:key:bad-peer");
      expect(result).toBe(true);
    });

    it("gets trust graph", async () => {
      const entry = manager.addRelay("Test", "http://localhost:4444");
      const mockGraph = [
        { from: "did:key:a", to: "did:key:b", level: "trusted" },
      ];
      http.responses.set("http://localhost:4444/api/federation/trust-graph", {
        status: 200,
        body: mockGraph,
      });

      const graph = await manager.getTrustGraph(entry.id);
      expect(graph).toHaveLength(1);
    });
  });

  // ── Certificates ─────────────────────────────────────────────────────

  describe("certificates", () => {
    it("lists certificates", async () => {
      const entry = manager.addRelay("Test", "http://localhost:4444");
      const mockCerts = [
        { domain: "relay.example.com", expiresAt: "2027-01-01T00:00:00Z", issuedAt: "2026-01-01T00:00:00Z" },
      ];
      http.responses.set("http://localhost:4444/api/certificates", {
        status: 200,
        body: mockCerts,
      });

      const certs = await manager.listCertificates(entry.id);
      expect(certs).toHaveLength(1);
      expect(certs[0]?.domain).toBe("relay.example.com");
    });
  });

  // ── Backup/restore ───────────────────────────────────────────────────

  describe("backup/restore", () => {
    it("backs up relay state", async () => {
      const entry = manager.addRelay("Test", "http://localhost:4444");
      const mockBackup = { collections: 5, portals: 2, webhooks: 1 };
      http.responses.set("http://localhost:4444/api/backup", {
        status: 200,
        body: mockBackup,
      });

      const backup = await manager.backupRelay(entry.id);
      expect(backup["collections"]).toBe(5);
      expect(backup["portals"]).toBe(2);
    });

    it("restores relay state", async () => {
      const entry = manager.addRelay("Test", "http://localhost:4444");
      http.responses.set("POST http://localhost:4444/api/restore", {
        status: 200,
        body: { restored: { collections: 5, portals: 2 } },
      });

      const result = await manager.restoreRelay(entry.id, { collections: [], portals: [] });
      expect(result.restored["collections"]).toBe(5);
      expect(result.restored["portals"]).toBe(2);
    });
  });

  // ── Health ───────────────────────────────────────────────────────────

  describe("health", () => {
    it("fetches health data", async () => {
      const entry = manager.addRelay("Test", "http://localhost:4444");
      http.responses.set("http://localhost:4444/api/health", {
        status: 200,
        body: { uptime: 7200, memoryMb: 128, connections: 10 },
      });

      const health = await manager.fetchHealth(entry.id);
      expect(health["uptime"]).toBe(7200);
      expect(health["memoryMb"]).toBe(128);
      expect(health["connections"]).toBe(10);
    });
  });

  // ── Discovery ────────────────────────────────────────────────────────

  describe("discovery", () => {
    it("discovers a relay at URL", async () => {
      http.responses.set("http://relay.example.com/api/health", {
        status: 200,
        body: { did: "did:key:discovered", modules: ["blind-mailbox"], mode: "server" },
      });

      const result = await manager.discoverRelay("http://relay.example.com");
      expect(result).not.toBeNull();
      expect(result?.did).toBe("did:key:discovered");
      expect(result?.modules).toContain("blind-mailbox");
      expect(result?.mode).toBe("server");
    });

    it("returns null for non-relay URL", async () => {
      http.responses.set("http://not-a-relay.example.com/api/health", {
        status: 404,
        body: "Not found",
      });

      const result = await manager.discoverRelay("http://not-a-relay.example.com");
      expect(result).toBeNull();
    });
  });

  // ── Dispose ───────────────────────────────────────────────────────────

  describe("dispose", () => {
    it("disconnects all clients and clears state", async () => {
      const entry = manager.addRelay("Test", "http://localhost:4444");
      const identity = { did: "did:key:test" } as unknown as Parameters<typeof manager.connect>[1];

      await manager.connect(entry.id, identity);
      manager.dispose();

      expect(manager.listRelays()).toHaveLength(0);
      expect(ws.clients[0]?.close).toHaveBeenCalled();
    });
  });
});
