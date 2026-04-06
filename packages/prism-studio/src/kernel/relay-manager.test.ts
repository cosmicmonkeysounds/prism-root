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
