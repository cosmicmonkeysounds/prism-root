/**
 * Metrics — Prometheus-compatible instrumentation for the relay HTTP layer.
 *
 * Tracks request counts and latency per (method, route, status), plus a small
 * set of process-level gauges. The registry is plain in-memory state; scraping
 * happens via the `/metrics` route, which renders the Prometheus text exposition
 * format (version 0.0.4) — no external client library required.
 *
 * Cardinality safety: the middleware uses Hono's `c.req.routePath` (e.g.
 * `/portals/:id`) instead of the raw URL, so high-volume parameterised routes
 * collapse to a single label set.
 */

import type { Context, Next } from "hono";

// ── Types ───────────────────────────────────────────────────────────────────

/** A scalar metric value with optional labels. */
export interface MetricSample {
  labels: Record<string, string>;
  value: number;
}

/** Histogram bucket boundaries in seconds. */
export const DEFAULT_LATENCY_BUCKETS_SECONDS = [
  0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1, 2.5, 5, 10,
];

interface HistogramState {
  /** Cumulative bucket counts (one per boundary, plus +Inf). */
  bucketCounts: number[];
  count: number;
  sumSeconds: number;
}

/** Snapshot of all metrics, used to render exposition output. */
export interface MetricsSnapshot {
  requestsTotal: MetricSample[];
  requestDurationSeconds: Array<{
    labels: Record<string, string>;
    bucketCounts: number[];
    count: number;
    sumSeconds: number;
  }>;
  gauges: Map<string, MetricSample[]>;
}

/** Metrics registry. */
export interface MetricsRegistry {
  /** Record a completed HTTP request. */
  recordRequest(method: string, route: string, status: number, durationMs: number): void;
  /**
   * Set or update a gauge value (e.g. `relay_modules_total`). Gauges replace
   * the previous value for a given label set.
   */
  setGauge(name: string, value: number, labels?: Record<string, string>): void;
  /** Reset all state — primarily for tests. */
  reset(): void;
  /** Take a snapshot of current state for rendering. */
  snapshot(): MetricsSnapshot;
  /** Render the registry as a Prometheus text exposition payload. */
  exposition(): string;
}

// ── Registry implementation ────────────────────────────────────────────────

interface RegistryOptions {
  /** Latency histogram bucket boundaries (seconds). */
  buckets?: readonly number[];
  /**
   * Hard cap on the number of distinct (method, route, status) label sets
   * tracked. Above this, additional series are dropped on the floor to keep
   * memory bounded under attack. Default: 5000.
   */
  maxLabelSets?: number;
}

export function createMetricsRegistry(options?: RegistryOptions): MetricsRegistry {
  const buckets = options?.buckets ?? DEFAULT_LATENCY_BUCKETS_SECONDS;
  const maxLabelSets = options?.maxLabelSets ?? 5000;

  // Map keyed by serialized labels — value is the raw count / histogram.
  const requestCounts = new Map<string, { labels: Record<string, string>; value: number }>();
  const requestHistograms = new Map<string, { labels: Record<string, string>; state: HistogramState }>();
  const gauges = new Map<string, Map<string, MetricSample>>();

  function newHistogramState(): HistogramState {
    return {
      bucketCounts: new Array<number>(buckets.length).fill(0),
      count: 0,
      sumSeconds: 0,
    };
  }

  return {
    recordRequest(method, route, status, durationMs) {
      const upperMethod = method.toUpperCase();
      const labels: Record<string, string> = {
        method: upperMethod,
        route,
        status: String(status),
      };
      const key = serializeLabels(labels);

      // Counter
      const countEntry = requestCounts.get(key);
      if (countEntry) {
        countEntry.value += 1;
      } else if (requestCounts.size < maxLabelSets) {
        requestCounts.set(key, { labels, value: 1 });
      }

      // Histogram (uses the same key so the cardinality cap is consistent)
      const histLabels: Record<string, string> = { method: upperMethod, route };
      const histKey = serializeLabels(histLabels);
      let histEntry = requestHistograms.get(histKey);
      if (!histEntry) {
        if (requestHistograms.size >= maxLabelSets) return;
        histEntry = { labels: histLabels, state: newHistogramState() };
        requestHistograms.set(histKey, histEntry);
      }
      const seconds = durationMs / 1000;
      histEntry.state.count += 1;
      histEntry.state.sumSeconds += seconds;
      for (let i = 0; i < buckets.length; i += 1) {
        const boundary = buckets[i];
        if (boundary === undefined) continue;
        if (seconds <= boundary) {
          histEntry.state.bucketCounts[i] = (histEntry.state.bucketCounts[i] ?? 0) + 1;
        }
      }
    },

    setGauge(name, value, labels = {}) {
      let series = gauges.get(name);
      if (!series) {
        series = new Map<string, MetricSample>();
        gauges.set(name, series);
      }
      const key = serializeLabels(labels);
      series.set(key, { labels, value });
    },

    reset() {
      requestCounts.clear();
      requestHistograms.clear();
      gauges.clear();
    },

    snapshot() {
      return {
        requestsTotal: [...requestCounts.values()].map(({ labels, value }) => ({
          labels,
          value,
        })),
        requestDurationSeconds: [...requestHistograms.values()].map(({ labels, state }) => ({
          labels,
          bucketCounts: [...state.bucketCounts],
          count: state.count,
          sumSeconds: state.sumSeconds,
        })),
        gauges: new Map(
          [...gauges.entries()].map(([name, series]) => [name, [...series.values()]]),
        ),
      };
    },

    exposition() {
      return renderExposition(this.snapshot(), buckets);
    },
  };
}

// ── Middleware ─────────────────────────────────────────────────────────────

/**
 * Hono middleware that records request latency and outcomes into the supplied
 * registry. Mount this *after* CORS but *before* anything that may short-circuit
 * with a 4xx, so rejected-but-counted requests are still measured.
 */
export function metricsMiddleware(registry: MetricsRegistry) {
  return async (c: Context, next: Next) => {
    const start = performance.now();
    let status = 0;
    try {
      await next();
      status = c.res.status;
    } catch (err) {
      status = 500;
      throw err;
    } finally {
      const durationMs = performance.now() - start;
      // routePath collapses parameterised routes — keeps cardinality bounded.
      const route = c.req.routePath || c.req.path;
      registry.recordRequest(c.req.method, route, status, durationMs);
    }
  };
}

// ── Exposition rendering ───────────────────────────────────────────────────

function serializeLabels(labels: Record<string, string>): string {
  // Stable, deterministic serialization for map keys.
  const entries = Object.entries(labels).sort(([a], [b]) => a.localeCompare(b));
  return entries.map(([k, v]) => `${k}=${v}`).join(",");
}

function escapeLabelValue(value: string): string {
  return value.replace(/\\/g, "\\\\").replace(/\n/g, "\\n").replace(/"/g, '\\"');
}

function formatLabels(labels: Record<string, string>, extra?: Record<string, string>): string {
  const merged = { ...labels, ...(extra ?? {}) };
  const entries = Object.entries(merged).sort(([a], [b]) => a.localeCompare(b));
  if (entries.length === 0) return "";
  const inner = entries.map(([k, v]) => `${k}="${escapeLabelValue(v)}"`).join(",");
  return `{${inner}}`;
}

function renderExposition(snapshot: MetricsSnapshot, buckets: readonly number[]): string {
  const lines: string[] = [];

  // ── relay_requests_total ──
  lines.push("# HELP relay_requests_total Total number of HTTP requests handled by the relay.");
  lines.push("# TYPE relay_requests_total counter");
  for (const sample of snapshot.requestsTotal) {
    lines.push(`relay_requests_total${formatLabels(sample.labels)} ${sample.value}`);
  }

  // ── relay_request_duration_seconds ──
  lines.push("# HELP relay_request_duration_seconds Histogram of HTTP request latency in seconds.");
  lines.push("# TYPE relay_request_duration_seconds histogram");
  for (const hist of snapshot.requestDurationSeconds) {
    for (let i = 0; i < buckets.length; i += 1) {
      const boundary = buckets[i];
      if (boundary === undefined) continue;
      const cumulative = hist.bucketCounts[i] ?? 0;
      lines.push(
        `relay_request_duration_seconds_bucket${formatLabels(hist.labels, {
          le: formatBucketBoundary(boundary),
        })} ${cumulative}`,
      );
    }
    // +Inf bucket equals total count.
    lines.push(
      `relay_request_duration_seconds_bucket${formatLabels(hist.labels, { le: "+Inf" })} ${hist.count}`,
    );
    lines.push(`relay_request_duration_seconds_sum${formatLabels(hist.labels)} ${hist.sumSeconds}`);
    lines.push(`relay_request_duration_seconds_count${formatLabels(hist.labels)} ${hist.count}`);
  }

  // ── Custom gauges ──
  for (const [name, samples] of snapshot.gauges) {
    lines.push(`# HELP ${name} ${gaugeHelp(name)}`);
    lines.push(`# TYPE ${name} gauge`);
    for (const sample of samples) {
      lines.push(`${name}${formatLabels(sample.labels)} ${sample.value}`);
    }
  }

  // Trailing newline is required by the Prometheus exposition spec.
  return lines.join("\n") + "\n";
}

function formatBucketBoundary(boundary: number): string {
  // Prometheus bucket labels must be plain decimals — no scientific notation.
  if (Number.isInteger(boundary)) return `${boundary}`;
  return boundary.toString();
}

function gaugeHelp(name: string): string {
  switch (name) {
    case "relay_modules_total":
      return "Number of relay modules currently installed.";
    case "relay_peers_online":
      return "Number of peers currently connected via the router.";
    case "relay_federation_peers":
      return "Number of federation peers known to this relay.";
    case "relay_websocket_connections":
      return "Number of currently open WebSocket connections.";
    case "relay_uptime_seconds":
      return "Seconds since the relay process started.";
    default:
      return name;
  }
}
