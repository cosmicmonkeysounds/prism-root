/**
 * Pure formatting helpers shared by admin widgets.
 * Extracted so vitest can exercise them without React.
 */

import type { HealthLevel, Metric } from "./types.js";

export function formatUptime(seconds: number): string {
  if (!Number.isFinite(seconds) || seconds < 0) return "—";
  const s = Math.floor(seconds);
  if (s < 60) return `${s}s`;
  const m = Math.floor(s / 60);
  if (m < 60) return `${m}m ${s % 60}s`;
  const h = Math.floor(m / 60);
  if (h < 24) return `${h}h ${m % 60}m`;
  const d = Math.floor(h / 24);
  return `${d}d ${h % 24}h`;
}

export function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes < 0) return "—";
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(2)} GB`;
}

export function formatMetricValue(metric: Metric): string {
  if (typeof metric.value === "string") return metric.value;
  const abs = Math.abs(metric.value);
  let rendered: string;
  if (abs >= 1000000) rendered = `${(metric.value / 1000000).toFixed(1)}M`;
  else if (abs >= 1000) rendered = `${(metric.value / 1000).toFixed(1)}k`;
  else if (Number.isInteger(metric.value)) rendered = `${metric.value}`;
  else rendered = metric.value.toFixed(2);
  return metric.unit ? `${rendered}${metric.unit}` : rendered;
}

export const HEALTH_COLORS: Record<HealthLevel, { bg: string; fg: string; border: string }> = {
  ok: { bg: "#0e3b1f", fg: "#4ade80", border: "#166534" },
  warn: { bg: "#3b2911", fg: "#f59e0b", border: "#854d0e" },
  error: { bg: "#3b1111", fg: "#f87171", border: "#7f1d1d" },
  unknown: { bg: "#1f1f1f", fg: "#888888", border: "#333333" },
};

/** Fold individual service healths into an overall roll-up. */
export function rollupHealth(levels: HealthLevel[]): HealthLevel {
  if (levels.length === 0) return "unknown";
  if (levels.includes("error")) return "error";
  if (levels.includes("warn")) return "warn";
  if (levels.every((l) => l === "ok")) return "ok";
  return "unknown";
}

/** Relative-time formatter — short form ("3s ago", "2h ago"). */
export function formatRelativeTime(iso: string, now: number = Date.now()): string {
  const then = Date.parse(iso);
  if (Number.isNaN(then)) return "—";
  const delta = Math.max(0, Math.floor((now - then) / 1000));
  if (delta < 5) return "just now";
  if (delta < 60) return `${delta}s ago`;
  const m = Math.floor(delta / 60);
  if (m < 60) return `${m}m ago`;
  const h = Math.floor(m / 60);
  if (h < 24) return `${h}h ago`;
  const d = Math.floor(h / 24);
  return `${d}d ago`;
}
