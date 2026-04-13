import { describe, expect, it, vi } from "vitest";
import { createRelayDataSource } from "./relay-data-source.js";

function mockFetch(handlers: Record<string, () => Response>): typeof fetch {
  return vi.fn(async (input: RequestInfo | URL) => {
    const url = typeof input === "string" ? input : input.toString();
    for (const [path, handler] of Object.entries(handlers)) {
      if (url.endsWith(path)) return handler();
    }
    return new Response(null, { status: 404 });
  }) as unknown as typeof fetch;
}

describe("createRelayDataSource", () => {
  it("exposes id and label defaults derived from the URL", () => {
    const src = createRelayDataSource({
      url: "https://relay.example.com",
      fetch: mockFetch({}),
    });
    expect(src.id).toBe("relay:https://relay.example.com");
    expect(src.label).toBe("Relay @ relay.example.com");
  });

  it("honors custom id and label", () => {
    const src = createRelayDataSource({
      id: "prod",
      label: "Production Relay",
      url: "http://localhost:3000",
      fetch: mockFetch({}),
    });
    expect(src.id).toBe("prod");
    expect(src.label).toBe("Production Relay");
  });

  it("composes a snapshot from /api/health, /api/modules, and /metrics", async () => {
    const fetchImpl = mockFetch({
      "/api/health": () =>
        new Response(
          JSON.stringify({
            status: "ok",
            uptime: 120,
            peers: 4,
            connections: 9,
            memory: { rss: 10_485_760 },
            version: "0.2.0",
          }),
          { status: 200, headers: { "content-type": "application/json" } },
        ),
      "/api/modules": () =>
        new Response(
          JSON.stringify({
            modules: [
              { id: "mailbox", name: "Blind Mailbox", description: "store and forward" },
              { id: "router", name: "Router" },
            ],
          }),
          { status: 200, headers: { "content-type": "application/json" } },
        ),
      "/metrics": () =>
        new Response("relay_federation_peers 2\nrelay_uptime_seconds 120\n", { status: 200 }),
    });

    const src = createRelayDataSource({
      url: "http://relay.local",
      fetch: fetchImpl,
      now: () => 1_700_000_000_000,
    });
    const snap = await src.snapshot();

    expect(snap.health.level).toBe("ok");
    expect(snap.health.label).toBe("Reachable");
    expect(snap.uptimeSeconds).toBe(120);
    expect(snap.services.map((s) => s.name)).toEqual(["Blind Mailbox", "Router"]);
    const ids = snap.metrics.map((m) => m.id);
    expect(ids).toContain("uptime");
    expect(ids).toContain("modules");
    expect(ids).toContain("peers-online");
    expect(ids).toContain("federation-peers");
    expect(ids).toContain("ws-connections");
    expect(ids).toContain("mem-rss");
    expect(ids).toContain("version");
    expect(snap.capturedAt).toBe(new Date(1_700_000_000_000).toISOString());
  });

  it("marks unreachable when /api/health fails", async () => {
    const fetchImpl = mockFetch({
      "/api/health": () => new Response(null, { status: 500 }),
      "/api/modules": () => new Response(null, { status: 500 }),
      "/metrics": () => new Response("", { status: 500 }),
    });
    const src = createRelayDataSource({
      url: "http://nope",
      fetch: fetchImpl,
    });
    const snap = await src.snapshot();
    expect(snap.health.level).toBe("error");
    expect(snap.health.label).toBe("Unreachable");
    expect(snap.services).toEqual([]);
  });

  it("falls back to Prometheus samples when health fields are missing", async () => {
    const fetchImpl = mockFetch({
      "/api/health": () => new Response(JSON.stringify({ status: "ok" }), { status: 200 }),
      "/api/modules": () => new Response(JSON.stringify({ modules: [] }), { status: 200 }),
      "/metrics": () =>
        new Response(
          [
            "relay_uptime_seconds 99",
            "relay_peers_online 3",
            "relay_websocket_connections 5",
            "relay_federation_peers 1",
          ].join("\n"),
          { status: 200 },
        ),
    });
    const src = createRelayDataSource({ url: "http://relay", fetch: fetchImpl });
    const snap = await src.snapshot();
    const byId = Object.fromEntries(snap.metrics.map((m) => [m.id, m.value]));
    expect(byId["uptime"]).toBe(99);
    expect(byId["peers-online"]).toBe(3);
    expect(byId["ws-connections"]).toBe(5);
    expect(byId["federation-peers"]).toBe(1);
  });

  it("handles fetch throwing (e.g. CORS) gracefully", async () => {
    const fetchImpl = vi.fn(async () => {
      throw new Error("network down");
    }) as unknown as typeof fetch;
    const src = createRelayDataSource({ url: "http://offline", fetch: fetchImpl });
    const snap = await src.snapshot();
    expect(snap.health.level).toBe("error");
  });
});
