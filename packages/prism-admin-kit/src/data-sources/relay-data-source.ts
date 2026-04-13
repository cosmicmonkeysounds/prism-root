/**
 * Relay-backed AdminDataSource.
 *
 * Talks HTTP to a running @prism/relay server's documented admin endpoints:
 *   GET /api/health   — uptime, memory, peer count
 *   GET /api/modules  — list of installed modules
 *   GET /metrics      — Prometheus text exposition
 */

import type {
  AdminDataSource,
  AdminSnapshot,
  HealthLevel,
  Metric,
  Service,
} from "../types.js";
import { parsePrometheus, findSample } from "./prometheus-parse.js";

export interface RelayDataSourceOptions {
  id?: string;
  label?: string;
  url: string;
  /** Injectable fetch for tests. */
  fetch?: typeof fetch;
  /** Clock override for tests. */
  now?: () => number;
}

interface RelayHealthPayload {
  status?: string;
  uptime?: number;
  memory?: {
    rss?: number;
    heapUsed?: number;
    heapTotal?: number;
  };
  peers?: number;
  connections?: number;
  version?: string;
}

interface RelayModulesPayload {
  modules?: Array<{ id?: string; name?: string; description?: string } | string>;
}

export function createRelayDataSource(options: RelayDataSourceOptions): AdminDataSource {
  const id = options.id ?? `relay:${options.url}`;
  const label = options.label ?? `Relay @ ${stripProtocol(options.url)}`;
  const baseUrl = options.url.replace(/\/$/, "");
  const doFetch = options.fetch ?? globalThis.fetch;
  const now = options.now ?? Date.now;

  async function fetchJson<T>(path: string): Promise<T | null> {
    try {
      const res = await doFetch(`${baseUrl}${path}`);
      if (!res.ok) return null;
      return (await res.json()) as T;
    } catch {
      return null;
    }
  }

  async function fetchText(path: string): Promise<string | null> {
    try {
      const res = await doFetch(`${baseUrl}${path}`);
      if (!res.ok) return null;
      return await res.text();
    } catch {
      return null;
    }
  }

  async function buildSnapshot(): Promise<AdminSnapshot> {
    const [health, modules, metricsText] = await Promise.all([
      fetchJson<RelayHealthPayload>("/api/health"),
      fetchJson<RelayModulesPayload>("/api/modules"),
      fetchText("/metrics"),
    ]);

    const reachable = health !== null;
    const samples = metricsText ? parsePrometheus(metricsText) : [];

    const uptimeSeconds = health?.uptime
      ?? findSample(samples, "relay_uptime_seconds")?.value
      ?? -1;

    const connections = health?.connections
      ?? findSample(samples, "relay_websocket_connections")?.value
      ?? 0;

    const peersOnline = health?.peers
      ?? findSample(samples, "relay_peers_online")?.value
      ?? 0;

    const federationPeers = findSample(samples, "relay_federation_peers")?.value ?? 0;
    const moduleCount = modules?.modules?.length
      ?? findSample(samples, "relay_modules_total")?.value
      ?? 0;

    const services: Service[] = [];
    if (modules?.modules) {
      for (const m of modules.modules) {
        const name = typeof m === "string" ? m : (m.name ?? m.id ?? "unknown");
        const description = typeof m === "string" ? undefined : m.description;
        services.push({
          id: `module:${name}`,
          name,
          kind: "relay-module",
          health: "ok",
          ...(description !== undefined ? { status: description } : {}),
        });
      }
    }

    const metrics: Metric[] = [
      { id: "uptime", label: "Uptime", value: uptimeSeconds, unit: "s" },
      { id: "modules", label: "Modules", value: moduleCount },
      { id: "peers-online", label: "Peers Online", value: peersOnline },
      { id: "federation-peers", label: "Federation Peers", value: federationPeers },
      { id: "ws-connections", label: "WebSocket Connections", value: connections },
    ];

    if (health?.memory?.rss !== undefined) {
      metrics.push({ id: "mem-rss", label: "Memory (RSS)", value: health.memory.rss, unit: " B" });
    }
    if (health?.version) {
      metrics.push({ id: "version", label: "Version", value: health.version });
    }

    const healthLevel: HealthLevel = reachable ? "ok" : "error";

    return {
      sourceId: id,
      sourceLabel: label,
      capturedAt: new Date(now()).toISOString(),
      uptimeSeconds,
      health: reachable
        ? { level: healthLevel, label: "Reachable", detail: `${moduleCount} modules loaded` }
        : { level: "error", label: "Unreachable", detail: `Could not reach ${baseUrl}/api/health` },
      metrics,
      services,
      activity: [],
    };
  }

  return {
    id,
    label,
    snapshot: buildSnapshot,
  };
}

function stripProtocol(url: string): string {
  return url.replace(/^https?:\/\//, "");
}
