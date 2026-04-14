/**
 * Daemon-backed AdminDataSource.
 *
 * Talks HTTP to a running Prism Daemon's HTTP transport (axum):
 *   GET  /healthz           — "ok" text
 *   GET  /capabilities      — list of command names
 *   POST /invoke/daemon.admin — full admin snapshot JSON
 *
 * Falls back to assembling a partial snapshot from /healthz + /capabilities
 * if the daemon doesn't have the admin module installed.
 */

import type {
  AdminDataSource,
  AdminSnapshot,
  HealthLevel,
  Metric,
  Service,
} from "../types.js";

export interface DaemonDataSourceOptions {
  id?: string;
  label?: string;
  /** Base URL of the daemon's HTTP transport (e.g. "http://localhost:3000"). */
  url: string;
  /** Injectable fetch for tests. */
  fetch?: typeof fetch;
  /** Clock override for tests. */
  now?: () => number;
}

interface DaemonAdminPayload {
  health: { level: HealthLevel; label: string; detail?: string };
  uptimeSeconds: number;
  metrics: Metric[];
  services: Service[];
  activity: Array<{ id: string; timestamp: string; kind: string; message: string; level?: HealthLevel }>;
}

export function createDaemonDataSource(options: DaemonDataSourceOptions): AdminDataSource {
  const id = options.id ?? `daemon:${options.url}`;
  const label = options.label ?? `Daemon @ ${options.url.replace(/^https?:\/\//, "")}`;
  const baseUrl = options.url.replace(/\/$/, "");
  const doFetch = options.fetch ?? globalThis.fetch;
  const now = options.now ?? Date.now;

  async function fetchJson<T>(path: string, method = "GET", body?: unknown): Promise<T | null> {
    try {
      const init: RequestInit = { method };
      if (body !== undefined) {
        init.headers = { "Content-Type": "application/json" };
        init.body = JSON.stringify(body);
      }
      const res = await doFetch(`${baseUrl}${path}`, init);
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
    // Try the admin module first — it returns a complete snapshot.
    const adminResult = await fetchJson<DaemonAdminPayload>(
      "/invoke/daemon.admin",
      "POST",
      {},
    );

    if (adminResult) {
      return {
        sourceId: id,
        sourceLabel: label,
        capturedAt: new Date(now()).toISOString(),
        health: adminResult.health,
        uptimeSeconds: adminResult.uptimeSeconds,
        metrics: adminResult.metrics,
        services: adminResult.services,
        activity: adminResult.activity,
      };
    }

    // Fallback: assemble from /healthz + /capabilities.
    const [healthText, capabilities] = await Promise.all([
      fetchText("/healthz"),
      fetchJson<string[]>("/capabilities"),
    ]);

    const reachable = healthText !== null;
    const level: HealthLevel = reachable ? "ok" : "error";

    const metrics: Metric[] = [];
    if (capabilities) {
      metrics.push({ id: "commands", label: "Commands", value: capabilities.length });
    }

    const services: Service[] = [];
    if (capabilities) {
      const moduleSet = new Set<string>();
      for (const cmd of capabilities) {
        const dot = cmd.indexOf(".");
        if (dot > 0) moduleSet.add(cmd.substring(0, dot));
      }
      for (const mod of moduleSet) {
        services.push({ id: mod, name: mod, health: "ok" });
      }
    }

    return {
      sourceId: id,
      sourceLabel: label,
      capturedAt: new Date(now()).toISOString(),
      health: {
        level,
        label: reachable ? "Healthy" : "Unreachable",
        ...(reachable ? {} : { detail: "Cannot reach daemon HTTP transport" }),
      },
      uptimeSeconds: -1,
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
