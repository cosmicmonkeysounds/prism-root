import { describe, it, expect, beforeEach } from "vitest";
import { Hono } from "hono";
import {
  createMetricsRegistry,
  metricsMiddleware,
  DEFAULT_LATENCY_BUCKETS_SECONDS,
} from "./metrics.js";

describe("createMetricsRegistry", () => {
  let registry: ReturnType<typeof createMetricsRegistry>;

  beforeEach(() => {
    registry = createMetricsRegistry();
  });

  it("counts requests by method/route/status", () => {
    registry.recordRequest("GET", "/api/status", 200, 5);
    registry.recordRequest("GET", "/api/status", 200, 5);
    registry.recordRequest("POST", "/api/portals", 201, 12);

    const snap = registry.snapshot();
    const status = snap.requestsTotal.find(
      (s) => s.labels.route === "/api/status" && s.labels.method === "GET",
    );
    const portals = snap.requestsTotal.find(
      (s) => s.labels.route === "/api/portals" && s.labels.method === "POST",
    );
    expect(status?.value).toBe(2);
    expect(portals?.value).toBe(1);
  });

  it("buckets latency into the histogram in seconds", () => {
    registry.recordRequest("GET", "/api/status", 200, 3); // 0.003s → first bucket
    registry.recordRequest("GET", "/api/status", 200, 30); // 0.030s
    registry.recordRequest("GET", "/api/status", 200, 800); // 0.8s

    const hist = registry
      .snapshot()
      .requestDurationSeconds.find((h) => h.labels.route === "/api/status");
    if (!hist) throw new Error("histogram entry missing for /api/status");
    expect(hist.count).toBe(3);
    expect(hist.sumSeconds).toBeCloseTo(0.003 + 0.03 + 0.8, 5);

    const bucketAt = (boundary: number): number => {
      const idx = DEFAULT_LATENCY_BUCKETS_SECONDS.indexOf(boundary);
      return hist.bucketCounts[idx] ?? -1;
    };

    // 0.005s bucket: only the 3ms request fits.
    expect(bucketAt(0.005)).toBe(1);
    // 0.05s bucket: 3ms + 30ms.
    expect(bucketAt(0.05)).toBe(2);
    // 1s bucket: all three.
    expect(bucketAt(1)).toBe(3);
  });

  it("renders Prometheus exposition format with required headers", () => {
    registry.recordRequest("GET", "/api/health", 200, 2);
    registry.setGauge("relay_modules_total", 7);
    registry.setGauge("relay_peers_online", 3);

    const text = registry.exposition();

    // HELP + TYPE lines for the counter and histogram.
    expect(text).toContain("# HELP relay_requests_total");
    expect(text).toContain("# TYPE relay_requests_total counter");
    expect(text).toContain("# HELP relay_request_duration_seconds");
    expect(text).toContain("# TYPE relay_request_duration_seconds histogram");

    // Counter sample with sorted labels.
    expect(text).toMatch(
      /relay_requests_total\{method="GET",route="\/api\/health",status="200"\} 1/,
    );

    // Histogram +Inf bucket and sum/count. Labels render in alphabetical
    // order, so `le` precedes `method` and `route`.
    expect(text).toContain('relay_request_duration_seconds_bucket{le="+Inf",method="GET",route="/api/health"} 1');
    expect(text).toMatch(/relay_request_duration_seconds_sum\{method="GET",route="\/api\/health"\}/);
    expect(text).toMatch(/relay_request_duration_seconds_count\{method="GET",route="\/api\/health"\} 1/);

    // Gauges rendered as simple `name{labels} value` lines.
    expect(text).toContain("# TYPE relay_modules_total gauge");
    expect(text).toContain("relay_modules_total 7");
    expect(text).toContain("relay_peers_online 3");

    // Spec requires trailing newline.
    expect(text.endsWith("\n")).toBe(true);
  });

  it("escapes special characters in label values", () => {
    registry.recordRequest("GET", '/api/"odd"\\path', 200, 1);
    const text = registry.exposition();
    expect(text).toContain('route="/api/\\"odd\\"\\\\path"');
  });

  it("caps label cardinality to bound memory under attack", () => {
    const small = createMetricsRegistry({ maxLabelSets: 3 });
    for (let i = 0; i < 100; i += 1) {
      small.recordRequest("GET", `/route-${i}`, 200, 1);
    }
    const snap = small.snapshot();
    expect(snap.requestsTotal.length).toBe(3);
  });

  it("setGauge replaces previous value for the same label set", () => {
    registry.setGauge("relay_peers_online", 1);
    registry.setGauge("relay_peers_online", 5);
    const samples = registry.snapshot().gauges.get("relay_peers_online");
    expect(samples).toHaveLength(1);
    expect(samples?.[0]?.value).toBe(5);
  });

  it("reset() clears all state", () => {
    registry.recordRequest("GET", "/x", 200, 1);
    registry.setGauge("relay_modules_total", 4);
    registry.reset();
    const snap = registry.snapshot();
    expect(snap.requestsTotal).toHaveLength(0);
    expect(snap.requestDurationSeconds).toHaveLength(0);
    expect(snap.gauges.size).toBe(0);
  });
});

describe("metricsMiddleware", () => {
  it("records each request through the Hono pipeline", async () => {
    const registry = createMetricsRegistry();
    const app = new Hono();
    app.use("/*", metricsMiddleware(registry));
    app.get("/ok", (c) => c.text("hi"));
    app.get("/boom", (c) => c.json({ error: "x" }, 500));

    await app.request("/ok");
    await app.request("/ok");
    await app.request("/boom");

    const snap = registry.snapshot();
    const ok = snap.requestsTotal.find((s) => s.labels.route === "/ok");
    const boom = snap.requestsTotal.find((s) => s.labels.route === "/boom");
    expect(ok?.value).toBe(2);
    expect(ok?.labels.status).toBe("200");
    expect(boom?.value).toBe(1);
    expect(boom?.labels.status).toBe("500");
  });

  it("collapses parameterised routes via routePath", async () => {
    const registry = createMetricsRegistry();
    const app = new Hono();
    app.use("/*", metricsMiddleware(registry));
    app.get("/items/:id", (c) => c.text(c.req.param("id")));

    await app.request("/items/abc");
    await app.request("/items/def");
    await app.request("/items/ghi");

    const snap = registry.snapshot();
    const items = snap.requestsTotal.filter((s) => s.labels.route === "/items/:id");
    expect(items).toHaveLength(1);
    expect(items[0]?.value).toBe(3);
  });
});
