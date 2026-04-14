import { describe, expect, it } from "vitest";
import { createDaemonDataSource } from "./daemon-data-source.js";
import type { AdminSnapshot } from "../types.js";

function createFakeFetch(
  responses: Record<string, { status: number; body: unknown }>,
): typeof fetch {
  const fake: typeof fetch = async (input, init) => {
    const url =
      typeof input === "string"
        ? input
        : input instanceof URL
          ? input.toString()
          : input.url;
    const path = new URL(url).pathname;
    const method = init?.method ?? "GET";
    const key = `${method} ${path}`;
    const match = responses[key] ?? responses[path];
    if (!match) {
      return { ok: false, status: 404, json: async () => null, text: async () => "" } as Response;
    }
    return {
      ok: match.status >= 200 && match.status < 300,
      status: match.status,
      json: async () => match.body,
      text: async () => (typeof match.body === "string" ? match.body : JSON.stringify(match.body)),
    } as Response;
  };
  return fake;
}

describe("createDaemonDataSource", () => {
  it("uses admin module when available", async () => {
    const adminPayload = {
      health: { level: "ok", label: "Healthy", detail: "2 modules" },
      uptimeSeconds: 120,
      metrics: [{ id: "commands", label: "Commands", value: 5 }],
      services: [{ id: "crdt", name: "crdt", health: "ok", status: "loaded" }],
      activity: [],
    };

    const source = createDaemonDataSource({
      url: "http://localhost:3000",
      fetch: createFakeFetch({
        "POST /invoke/daemon.admin": { status: 200, body: adminPayload },
      }),
      now: () => 1700000000000,
    });

    expect(source.id).toBe("daemon:http://localhost:3000");
    expect(source.label).toContain("localhost:3000");

    const snap = await source.snapshot();
    expect(snap.sourceId).toBe("daemon:http://localhost:3000");
    expect(snap.health.level).toBe("ok");
    expect(snap.health.label).toBe("Healthy");
    expect(snap.uptimeSeconds).toBe(120);
    expect(snap.metrics).toHaveLength(1);
    expect(snap.services).toHaveLength(1);
  });

  it("falls back to /healthz + /capabilities when admin module not installed", async () => {
    const source = createDaemonDataSource({
      url: "http://localhost:3000",
      fetch: createFakeFetch({
        "POST /invoke/daemon.admin": { status: 404, body: null },
        "GET /healthz": { status: 200, body: "ok" },
        "/healthz": { status: 200, body: "ok" },
        "GET /capabilities": { status: 200, body: ["crdt.read", "crdt.write", "luau.exec"] },
        "/capabilities": { status: 200, body: ["crdt.read", "crdt.write", "luau.exec"] },
      }),
    });

    const snap = await source.snapshot();
    expect(snap.health.level).toBe("ok");
    expect(snap.health.label).toBe("Healthy");
    expect(snap.uptimeSeconds).toBe(-1); // unknown without admin module

    // Should derive commands count and module services
    const cmdMetric = snap.metrics.find((m) => m.id === "commands");
    expect(cmdMetric?.value).toBe(3);

    // Should derive crdt and luau as services
    expect(snap.services.some((s) => s.id === "crdt")).toBe(true);
    expect(snap.services.some((s) => s.id === "luau")).toBe(true);
  });

  it("reports unreachable when daemon is down", async () => {
    const source = createDaemonDataSource({
      url: "http://localhost:3000",
      fetch: async () => { throw new Error("connection refused"); },
    });

    const snap = await source.snapshot();
    expect(snap.health.level).toBe("error");
    expect(snap.health.label).toBe("Unreachable");
    expect(snap.metrics).toHaveLength(0);
    expect(snap.services).toHaveLength(0);
  });

  it("accepts custom id and label", async () => {
    const source = createDaemonDataSource({
      url: "http://localhost:3000",
      id: "my-daemon",
      label: "Production Daemon",
      fetch: async () => { throw new Error("down"); },
    });

    expect(source.id).toBe("my-daemon");
    expect(source.label).toBe("Production Daemon");
    const snap = await source.snapshot();
    expect(snap.sourceId).toBe("my-daemon");
    expect(snap.sourceLabel).toBe("Production Daemon");
  });
});
