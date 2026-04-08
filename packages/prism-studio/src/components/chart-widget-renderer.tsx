/**
 * ChartWidgetRenderer — bar/line/pie/area chart of aggregated kernel data.
 *
 * Pure SVG rendering — no chart library dependency. Groups objects by
 * `groupField`, aggregates `valueField` via the chosen function, plots
 * the result in the selected chart shape.
 */

import { useMemo } from "react";
import type { GraphObject } from "@prism/core/object-model";

export type ChartType = "bar" | "line" | "pie" | "area";
export type ChartAggregation = "count" | "sum" | "avg" | "min" | "max";

export interface ChartWidgetProps {
  objects: GraphObject[];
  chartType: ChartType;
  groupField: string;
  valueField?: string | undefined;
  aggregation: ChartAggregation;
  width?: number;
  height?: number;
}

export interface ChartDataPoint {
  label: string;
  value: number;
}

/** Aggregate objects into chart data points. */
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

const PALETTE = [
  "#a855f7",
  "#ec4899",
  "#0ea5e9",
  "#22c55e",
  "#f59e0b",
  "#ef4444",
  "#14b8a6",
  "#6366f1",
];

export function ChartWidgetRenderer(props: ChartWidgetProps) {
  const { objects, chartType, groupField, valueField, aggregation, width = 480, height = 240 } = props;

  const points = useMemo(
    () => aggregateObjects(objects, groupField, valueField, aggregation),
    [objects, groupField, valueField, aggregation],
  );

  return (
    <div
      data-testid="chart-widget"
      style={{
        border: "1px solid #a855f7",
        borderRadius: 6,
        background: "#0f172a",
        padding: 8,
        color: "#e2e8f0",
      }}
    >
      <div style={{ fontSize: 11, fontWeight: 600, color: "#a855f7", textTransform: "uppercase", marginBottom: 6 }}>
        {chartType} chart — {aggregation}
      </div>
      {points.length === 0 ? (
        <div style={{ padding: 24, color: "#94a3b8", fontSize: 12, textAlign: "center" }}>
          No data to display.
        </div>
      ) : (
        <svg width={width} height={height} viewBox={`0 0 ${width} ${height}`} data-testid="chart-svg">
          {renderChart(chartType, points, width, height)}
        </svg>
      )}
    </div>
  );
}

function renderChart(
  type: ChartType,
  points: ChartDataPoint[],
  width: number,
  height: number,
) {
  const pad = { top: 12, right: 12, bottom: 32, left: 40 };
  const innerW = width - pad.left - pad.right;
  const innerH = height - pad.top - pad.bottom;
  const maxV = Math.max(...points.map((p) => p.value), 0) || 1;
  const minV = Math.min(...points.map((p) => p.value), 0);
  const range = maxV - minV || 1;

  if (type === "pie") {
    const total = points.reduce((s, p) => s + Math.max(0, p.value), 0) || 1;
    const cx = width / 2;
    const cy = height / 2;
    const r = Math.min(innerW, innerH) / 2;
    let angle = -Math.PI / 2;
    return (
      <g>
        {points.map((p, i) => {
          const slice = (Math.max(0, p.value) / total) * Math.PI * 2;
          const x1 = cx + r * Math.cos(angle);
          const y1 = cy + r * Math.sin(angle);
          const x2 = cx + r * Math.cos(angle + slice);
          const y2 = cy + r * Math.sin(angle + slice);
          const large = slice > Math.PI ? 1 : 0;
          const d = `M ${cx} ${cy} L ${x1} ${y1} A ${r} ${r} 0 ${large} 1 ${x2} ${y2} Z`;
          angle += slice;
          return <path key={p.label} d={d} fill={PALETTE[i % PALETTE.length]} />;
        })}
      </g>
    );
  }

  const barW = innerW / points.length;
  const yFor = (v: number): number => pad.top + innerH - ((v - minV) / range) * innerH;

  if (type === "bar") {
    return (
      <g>
        <line x1={pad.left} y1={pad.top + innerH} x2={pad.left + innerW} y2={pad.top + innerH} stroke="#334155" />
        {points.map((p, i) => {
          const x = pad.left + i * barW + barW * 0.15;
          const w = barW * 0.7;
          const y = yFor(Math.max(0, p.value));
          const h = pad.top + innerH - y;
          return (
            <g key={p.label}>
              <rect x={x} y={y} width={w} height={h} fill={PALETTE[i % PALETTE.length]} />
              <text
                x={x + w / 2}
                y={pad.top + innerH + 14}
                fill="#94a3b8"
                fontSize="10"
                textAnchor="middle"
              >
                {p.label.length > 10 ? `${p.label.slice(0, 9)}…` : p.label}
              </text>
            </g>
          );
        })}
      </g>
    );
  }

  // line + area share path building
  const lineD = points
    .map((p, i) => {
      const x = pad.left + i * barW + barW / 2;
      const y = yFor(p.value);
      return `${i === 0 ? "M" : "L"} ${x} ${y}`;
    })
    .join(" ");

  if (type === "area") {
    const first = pad.left + barW / 2;
    const last = pad.left + (points.length - 1) * barW + barW / 2;
    const areaD = `${lineD} L ${last} ${pad.top + innerH} L ${first} ${pad.top + innerH} Z`;
    return (
      <g>
        <path d={areaD} fill="#a855f744" />
        <path d={lineD} stroke="#a855f7" strokeWidth={2} fill="none" />
      </g>
    );
  }

  // line
  return (
    <g>
      <line x1={pad.left} y1={pad.top + innerH} x2={pad.left + innerW} y2={pad.top + innerH} stroke="#334155" />
      <path d={lineD} stroke="#a855f7" strokeWidth={2} fill="none" />
      {points.map((p, i) => {
        const x = pad.left + i * barW + barW / 2;
        const y = yFor(p.value);
        return <circle key={p.label} cx={x} cy={y} r={3} fill="#a855f7" />;
      })}
    </g>
  );
}
