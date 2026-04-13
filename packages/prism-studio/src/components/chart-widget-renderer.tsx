/**
 * ChartWidgetRenderer — bar/line/pie/area chart of aggregated kernel data,
 * powered by recharts.
 *
 * Pure aggregation lives in `./chart-data.ts` (re-exported here so existing
 * imports keep working). The visual layer mounts recharts <ResponsiveContainer>
 * around the chosen chart shape, with a shared palette + dark surface.
 */

import { useMemo } from "react";
import {
  ResponsiveContainer,
  BarChart,
  Bar,
  LineChart,
  Line,
  PieChart,
  Pie,
  Cell,
  AreaChart,
  Area,
  XAxis,
  YAxis,
  CartesianGrid,
  Tooltip,
  Legend,
} from "recharts";
import type { GraphObject } from "@prism/core/object-model";
import {
  aggregateObjects,
  CHART_PALETTE,
  type ChartType,
  type ChartAggregation,
  type ChartDataPoint,
} from "./chart-data.js";

export {
  aggregateObjects,
  CHART_PALETTE,
  type ChartType,
  type ChartAggregation,
  type ChartDataPoint,
};

export interface ChartWidgetProps {
  objects: GraphObject[];
  chartType: ChartType;
  groupField: string;
  valueField?: string | undefined;
  aggregation: ChartAggregation;
  width?: number;
  height?: number;
}

export function ChartWidgetRenderer(props: ChartWidgetProps) {
  const {
    objects,
    chartType,
    groupField,
    valueField,
    aggregation,
    height = 260,
  } = props;

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
      <div
        style={{
          fontSize: 11,
          fontWeight: 600,
          color: "#a855f7",
          textTransform: "uppercase",
          letterSpacing: "0.05em",
          marginBottom: 6,
        }}
      >
        {chartType} chart — {aggregation}
      </div>
      {points.length === 0 ? (
        <div
          style={{
            padding: 24,
            color: "#94a3b8",
            fontSize: 12,
            textAlign: "center",
          }}
        >
          No data to display.
        </div>
      ) : (
        <div style={{ width: "100%", height }} data-testid="chart-surface">
          <ResponsiveContainer width="100%" height="100%">
            {renderChart(chartType, points)}
          </ResponsiveContainer>
        </div>
      )}
    </div>
  );
}

function renderChart(type: ChartType, points: ChartDataPoint[]) {
  const tooltipStyle = {
    background: "#0f172a",
    border: "1px solid #334155",
    borderRadius: 4,
    color: "#e2e8f0",
    fontSize: 12,
  } as const;
  const axisColor = "#94a3b8";

  if (type === "pie") {
    return (
      <PieChart>
        <Pie
          data={points}
          dataKey="value"
          nameKey="label"
          innerRadius="35%"
          outerRadius="75%"
          paddingAngle={2}
          stroke="#0f172a"
        >
          {points.map((p, i) => (
            <Cell
              key={p.label}
              fill={CHART_PALETTE[i % CHART_PALETTE.length] ?? "#a855f7"}
            />
          ))}
        </Pie>
        <Tooltip contentStyle={tooltipStyle} />
        <Legend wrapperStyle={{ fontSize: 11, color: "#94a3b8" }} />
      </PieChart>
    );
  }

  if (type === "line") {
    return (
      <LineChart data={points} margin={{ top: 8, right: 12, left: 0, bottom: 8 }}>
        <CartesianGrid stroke="#1e293b" strokeDasharray="3 3" />
        <XAxis dataKey="label" stroke={axisColor} fontSize={10} />
        <YAxis stroke={axisColor} fontSize={10} />
        <Tooltip contentStyle={tooltipStyle} />
        <Line
          type="monotone"
          dataKey="value"
          stroke="#a855f7"
          strokeWidth={2}
          dot={{ fill: "#a855f7", r: 3 }}
          activeDot={{ r: 5 }}
        />
      </LineChart>
    );
  }

  if (type === "area") {
    return (
      <AreaChart data={points} margin={{ top: 8, right: 12, left: 0, bottom: 8 }}>
        <defs>
          <linearGradient id="prism-area-fill" x1="0" y1="0" x2="0" y2="1">
            <stop offset="0%" stopColor="#a855f7" stopOpacity={0.55} />
            <stop offset="100%" stopColor="#a855f7" stopOpacity={0.05} />
          </linearGradient>
        </defs>
        <CartesianGrid stroke="#1e293b" strokeDasharray="3 3" />
        <XAxis dataKey="label" stroke={axisColor} fontSize={10} />
        <YAxis stroke={axisColor} fontSize={10} />
        <Tooltip contentStyle={tooltipStyle} />
        <Area
          type="monotone"
          dataKey="value"
          stroke="#a855f7"
          strokeWidth={2}
          fill="url(#prism-area-fill)"
        />
      </AreaChart>
    );
  }

  // bar (default)
  return (
    <BarChart data={points} margin={{ top: 8, right: 12, left: 0, bottom: 8 }}>
      <CartesianGrid stroke="#1e293b" strokeDasharray="3 3" />
      <XAxis dataKey="label" stroke={axisColor} fontSize={10} />
      <YAxis stroke={axisColor} fontSize={10} />
      <Tooltip contentStyle={tooltipStyle} cursor={{ fill: "#1e293b" }} />
      <Bar dataKey="value" radius={[3, 3, 0, 0]}>
        {points.map((p, i) => (
          <Cell
            key={p.label}
            fill={CHART_PALETTE[i % CHART_PALETTE.length] ?? "#a855f7"}
          />
        ))}
      </Bar>
    </BarChart>
  );
}
