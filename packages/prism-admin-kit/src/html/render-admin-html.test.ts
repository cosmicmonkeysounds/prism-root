import { describe, expect, it } from "vitest";
import type { AdminSnapshot } from "../types.js";
import {
  renderAdminHtml,
  renderSnapshotBody,
  renderHealthBadge,
  renderMetricCard,
  renderUptimeCard,
  renderServiceList,
  renderActivityTail,
} from "./render-admin-html.js";

function makeSnapshot(overrides: Partial<AdminSnapshot> = {}): AdminSnapshot {
  return {
    sourceId: "test",
    sourceLabel: "Test Runtime",
    capturedAt: new Date().toISOString(),
    health: { level: "ok", label: "Healthy" },
    uptimeSeconds: 3661,
    metrics: [
      { id: "objects", label: "Objects", value: 42 },
      { id: "memory", label: "Memory", value: 128.5, unit: "MB" },
    ],
    services: [
      { id: "store", name: "Object Store", health: "ok", status: "running" },
      { id: "relay", name: "Relay", health: "warn", status: "degraded" },
    ],
    activity: [
      { id: "a1", timestamp: new Date(Date.now() - 5000).toISOString(), kind: "object.created", message: "Created page Home" },
    ],
    ...overrides,
  };
}

describe("renderAdminHtml", () => {
  it("returns a complete HTML document", () => {
    const html = renderAdminHtml({ title: "Test Admin" });
    expect(html).toContain("<!DOCTYPE html>");
    expect(html).toContain("<title>Test Admin</title>");
    expect(html).toContain("admin-shell");
    expect(html).toContain("/admin/api/snapshot");
  });

  it("embeds the runtime label in the header", () => {
    const html = renderAdminHtml({ runtimeLabel: "Relay v1.0" });
    expect(html).toContain("Relay v1.0");
  });

  it("uses custom snapshot URL", () => {
    const html = renderAdminHtml({ snapshotUrl: "/custom/endpoint" });
    expect(html).toContain("/custom/endpoint");
  });

  it("sets the poll interval", () => {
    const html = renderAdminHtml({ pollMs: 10000 });
    expect(html).toContain("var POLL_MS = 10000");
  });

  it("embeds initial snapshot as seed data", () => {
    const snap = makeSnapshot();
    const html = renderAdminHtml({ initialSnapshot: snap });
    expect(html).toContain("Test Runtime");
    expect(html).toContain(snap.sourceId);
  });

  it("escapes script-closing tags in seed data", () => {
    const snap = makeSnapshot({ sourceLabel: "</script><evil>" });
    const html = renderAdminHtml({ initialSnapshot: snap });
    expect(html).not.toContain("</script><evil>");
    expect(html).toContain("<\\/script>");
  });
});

describe("renderHealthBadge", () => {
  it("renders ok badge with green styling", () => {
    const html = renderHealthBadge("ok", "Healthy");
    expect(html).toContain("health-badge");
    expect(html).toContain("Healthy");
    expect(html).toContain("#4ade80");
  });

  it("renders error badge with red styling", () => {
    const html = renderHealthBadge("error", "Down");
    expect(html).toContain("Down");
    expect(html).toContain("#f87171");
  });
});

describe("renderMetricCard", () => {
  it("renders a numeric metric", () => {
    const html = renderMetricCard({ id: "objects", label: "Objects", value: 42 });
    expect(html).toContain("Objects");
    expect(html).toContain("42");
  });

  it("renders a metric with unit", () => {
    const html = renderMetricCard({ id: "mem", label: "Memory", value: 128.5, unit: "MB" });
    expect(html).toContain("128.50MB");
  });

  it("renders a positive delta", () => {
    const html = renderMetricCard({ id: "x", label: "X", value: 10, delta: 3 });
    expect(html).toContain("+3");
    expect(html).toContain("pos");
  });

  it("renders a negative delta", () => {
    const html = renderMetricCard({ id: "x", label: "X", value: 10, delta: -2 });
    expect(html).toContain("-2");
    expect(html).toContain("neg");
  });

  it("renders a string metric value", () => {
    const html = renderMetricCard({ id: "ver", label: "Version", value: "v0.1.0" });
    expect(html).toContain("v0.1.0");
  });

  it("renders hint text", () => {
    const html = renderMetricCard({ id: "x", label: "X", value: 5, hint: "across 3 peers" });
    expect(html).toContain("across 3 peers");
    expect(html).toContain("metric-hint");
  });
});

describe("renderUptimeCard", () => {
  it("formats hours and minutes", () => {
    const html = renderUptimeCard(3661);
    expect(html).toContain("1h 1m");
  });

  it("handles unknown uptime", () => {
    const html = renderUptimeCard(-1);
    expect(html).toContain("\u2014");
  });
});

describe("renderServiceList", () => {
  it("renders services with health dots", () => {
    const html = renderServiceList([
      { id: "s1", name: "Store", health: "ok" },
      { id: "s2", name: "Relay", health: "error", status: "offline" },
    ]);
    expect(html).toContain("Store");
    expect(html).toContain("Relay");
    expect(html).toContain("offline");
    expect(html).toContain("service-dot");
  });

  it("shows empty message when no services", () => {
    const html = renderServiceList([]);
    expect(html).toContain("No services reported");
  });
});

describe("renderActivityTail", () => {
  const now = Date.now();

  it("renders activity items with relative time", () => {
    const html = renderActivityTail([
      { id: "a1", timestamp: new Date(now - 30000).toISOString(), kind: "object.created", message: "Created page" },
    ], now);
    expect(html).toContain("object.created");
    expect(html).toContain("Created page");
    expect(html).toContain("30s ago");
  });

  it("caps at 20 items", () => {
    const items = Array.from({ length: 25 }, (_, i) => ({
      id: `a${i}`,
      timestamp: new Date(now - i * 1000).toISOString(),
      kind: "test",
      message: `Item ${i}`,
    }));
    const html = renderActivityTail(items, now);
    expect(html).toContain("Item 19");
    expect(html).not.toContain("Item 20");
  });

  it("shows empty message when no activity", () => {
    const html = renderActivityTail([], now);
    expect(html).toContain("No recent activity");
  });
});

describe("renderSnapshotBody", () => {
  it("renders all sections in order", () => {
    const snap = makeSnapshot();
    const html = renderSnapshotBody(snap, Date.now());
    expect(html).toContain("Test Runtime");
    expect(html).toContain("Healthy");
    expect(html).toContain("Uptime");
    expect(html).toContain("Objects");
    expect(html).toContain("Object Store");
    expect(html).toContain("object.created");
  });
});
