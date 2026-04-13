/**
 * Pure aggregation helpers for the chart widget. Lives in a separate
 * file from `chart-widget-renderer.tsx` so vitest (node env) can import
 * the data layer without pulling in recharts and its DOM dependencies.
 */

import type { GraphObject } from "@prism/core/object-model";

export type ChartType = "bar" | "line" | "pie" | "area";
export type ChartAggregation = "count" | "sum" | "avg" | "min" | "max";

export interface ChartDataPoint {
  label: string;
  value: number;
}

export function aggregateObjects(
  objects: GraphObject[],
  groupField: string,
  valueField: string | undefined,
  aggregation: ChartAggregation,
): ChartDataPoint[] {
  const groups = new Map<string, number[]>();
  for (const obj of objects) {
    const data = obj.data as Record<string, unknown>;
    const rawGroup = data[groupField];
    const key = rawGroup == null || rawGroup === "" ? "—" : String(rawGroup);
    const bucket = groups.get(key) ?? [];
    if (aggregation === "count" || !valueField) {
      bucket.push(1);
    } else {
      const raw = data[valueField];
      const num = typeof raw === "number" ? raw : Number(raw);
      if (Number.isFinite(num)) bucket.push(num);
    }
    groups.set(key, bucket);
  }

  const points: ChartDataPoint[] = [];
  for (const [label, values] of groups) {
    if (values.length === 0) {
      points.push({ label, value: 0 });
      continue;
    }
    let value = 0;
    switch (aggregation) {
      case "count":
        value = values.length;
        break;
      case "sum":
        value = values.reduce((s, v) => s + v, 0);
        break;
      case "avg":
        value = values.reduce((s, v) => s + v, 0) / values.length;
        break;
      case "min":
        value = Math.min(...values);
        break;
      case "max":
        value = Math.max(...values);
        break;
    }
    points.push({ label, value });
  }
  return points;
}

export const CHART_PALETTE: readonly string[] = [
  "#a855f7",
  "#ec4899",
  "#0ea5e9",
  "#22c55e",
  "#f59e0b",
  "#ef4444",
  "#14b8a6",
  "#6366f1",
];
