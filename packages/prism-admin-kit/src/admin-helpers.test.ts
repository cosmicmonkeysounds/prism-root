import { describe, expect, it } from "vitest";
import {
  formatUptime,
  formatBytes,
  formatMetricValue,
  formatRelativeTime,
  rollupHealth,
  HEALTH_COLORS,
} from "./admin-helpers.js";
import type { Metric } from "./types.js";

describe("formatUptime", () => {
  it("returns seconds under a minute", () => {
    expect(formatUptime(0)).toBe("0s");
    expect(formatUptime(45)).toBe("45s");
  });

  it("returns minutes+seconds", () => {
    expect(formatUptime(90)).toBe("1m 30s");
    expect(formatUptime(59 * 60 + 59)).toBe("59m 59s");
  });

  it("returns hours+minutes", () => {
    expect(formatUptime(3600)).toBe("1h 0m");
    expect(formatUptime(3 * 3600 + 14 * 60)).toBe("3h 14m");
  });

  it("returns days+hours", () => {
    expect(formatUptime(2 * 24 * 3600 + 5 * 3600)).toBe("2d 5h");
  });

  it("renders em-dash for invalid input", () => {
    expect(formatUptime(-1)).toBe("—");
    expect(formatUptime(Number.NaN)).toBe("—");
  });
});

describe("formatBytes", () => {
  it("formats bytes, KB, MB, GB", () => {
    expect(formatBytes(0)).toBe("0 B");
    expect(formatBytes(512)).toBe("512 B");
    expect(formatBytes(2048)).toBe("2.0 KB");
    expect(formatBytes(5 * 1024 * 1024)).toBe("5.0 MB");
    expect(formatBytes(3 * 1024 * 1024 * 1024)).toBe("3.00 GB");
  });

  it("renders em-dash for invalid input", () => {
    expect(formatBytes(-5)).toBe("—");
  });
});

describe("formatMetricValue", () => {
  it("passes string values through", () => {
    const m: Metric = { id: "v", label: "Version", value: "v0.1.0" };
    expect(formatMetricValue(m)).toBe("v0.1.0");
  });

  it("suffixes k/M for large numbers", () => {
    expect(formatMetricValue({ id: "a", label: "", value: 1500 })).toBe("1.5k");
    expect(formatMetricValue({ id: "a", label: "", value: 2_500_000 })).toBe("2.5M");
  });

  it("keeps integers as integers and applies units", () => {
    expect(formatMetricValue({ id: "a", label: "", value: 42 })).toBe("42");
    expect(formatMetricValue({ id: "a", label: "", value: 42, unit: "ms" })).toBe("42ms");
  });

  it("renders fractional values to two decimals", () => {
    expect(formatMetricValue({ id: "a", label: "", value: 3.14159 })).toBe("3.14");
  });
});

describe("rollupHealth", () => {
  it("error wins", () => {
    expect(rollupHealth(["ok", "error", "warn"])).toBe("error");
  });

  it("warn beats ok", () => {
    expect(rollupHealth(["ok", "warn", "ok"])).toBe("warn");
  });

  it("all-ok stays ok", () => {
    expect(rollupHealth(["ok", "ok"])).toBe("ok");
  });

  it("empty is unknown", () => {
    expect(rollupHealth([])).toBe("unknown");
  });

  it("unknown mixed falls back to unknown", () => {
    expect(rollupHealth(["ok", "unknown"])).toBe("unknown");
  });
});

describe("formatRelativeTime", () => {
  const NOW = 1_000_000_000_000;

  it("renders recent events as 'just now'", () => {
    const iso = new Date(NOW - 2000).toISOString();
    expect(formatRelativeTime(iso, NOW)).toBe("just now");
  });

  it("renders seconds", () => {
    const iso = new Date(NOW - 30_000).toISOString();
    expect(formatRelativeTime(iso, NOW)).toBe("30s ago");
  });

  it("renders minutes, hours, days", () => {
    expect(formatRelativeTime(new Date(NOW - 5 * 60_000).toISOString(), NOW)).toBe("5m ago");
    expect(formatRelativeTime(new Date(NOW - 3 * 3_600_000).toISOString(), NOW)).toBe("3h ago");
    expect(formatRelativeTime(new Date(NOW - 2 * 86_400_000).toISOString(), NOW)).toBe("2d ago");
  });

  it("returns em-dash for invalid input", () => {
    expect(formatRelativeTime("not a date", NOW)).toBe("—");
  });
});

describe("HEALTH_COLORS", () => {
  it("has an entry per health level", () => {
    expect(HEALTH_COLORS.ok.fg).toMatch(/^#/);
    expect(HEALTH_COLORS.warn.fg).toMatch(/^#/);
    expect(HEALTH_COLORS.error.fg).toMatch(/^#/);
    expect(HEALTH_COLORS.unknown.fg).toMatch(/^#/);
  });
});
